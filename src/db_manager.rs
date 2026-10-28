use std::str::FromStr;

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::database_schema::*;
use crate::motor::Consumo;
use crate::security_logger_impl::insert_event;
use crate::uso::{MAX_INTERVALO_SEGUNDOS, dividir_por_dia_local};

/// Pool de conexão para transações atômicas no SQLite local.
pub struct KioskDbManager {
    pool: SqlitePool,
}

impl KioskDbManager {
    /// Abre (e cria, se preciso) o banco em `conn_string` (ex.: `sqlite://kiosk.db`)
    /// e aplica as migrações. Falha em qualquer etapa é devolvida ao chamador.
    pub async fn new(conn_string: &str) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::from_str(conn_string)?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn begin_transaction(&self) -> Result<Transaction<'_, Sqlite>, sqlx::Error> {
        self.pool.begin().await
    }

    /// Grava um evento que não acompanha mudança de estado (AUTH_FAIL, enforcement etc.).
    pub async fn record_event(&self, event: &SecurityEvent) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_transaction().await?;
        insert_event(&mut tx, event).await?;
        tx.commit().await
    }

    /// Fecha uma operação auditada: sucesso grava o evento na mesma transação e confirma;
    /// falha desfaz a transação, grava um evento FAILURE em outra e devolve o erro original.
    async fn finish<T>(
        &self,
        mut tx: Transaction<'_, Sqlite>,
        result: Result<T, sqlx::Error>,
        ctx: &EventContext,
        action: ActionType,
        profile_id: Option<Uuid>,
    ) -> Result<T, sqlx::Error> {
        match result {
            Ok(value) => {
                let ok = SecurityEvent::new(
                    &ctx.client_ip,
                    action,
                    EventType::Success,
                    TransactionStatus::Success,
                    "",
                )
                .with_profile(profile_id);
                insert_event(&mut tx, &ok).await?;
                tx.commit().await?;
                Ok(value)
            }
            Err(err) => {
                drop(tx); // rollback da mudança
                let fail = SecurityEvent::new(
                    &ctx.client_ip,
                    action,
                    EventType::InternalError,
                    TransactionStatus::Failure,
                    &err.to_string(),
                )
                .with_profile(profile_id);
                match self.record_event(&fail).await {
                    Ok(()) => Err(err),
                    Err(log_err) => Err(sqlx::Error::Protocol(format!(
                        "operação falhou ({err}) e o evento de falha não foi gravado ({log_err})"
                    ))),
                }
            }
        }
    }

    pub async fn create_app(
        &self,
        ctx: &EventContext,
        app: &AppMetadata,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_transaction().await?;
        let r = sqlx::query(
            "INSERT INTO app_metadata (app_id, process_name, current_status) VALUES (?, ?, ?)",
        )
        .bind(app.app_id.to_string())
        .bind(&app.process_name)
        .bind(app.status.as_str())
        .execute(&mut *tx)
        .await
        .map(|_| ());
        self.finish(tx, r, ctx, ActionType::AddApp, None).await
    }

    pub async fn create_profile(
        &self,
        ctx: &EventContext,
        profile: &ChildProfile,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_transaction().await?;
        let r = async {
            sqlx::query(
                "INSERT INTO child_profile (profile_id, windows_sid, display_name, daily_total_limit_seconds) \
                 VALUES (?, ?, ?, ?)",
            )
            .bind(profile.profile_id.to_string())
            .bind(&profile.windows_sid)
            .bind(&profile.display_name)
            .bind(opt_secs(profile.daily_total_limit_seconds)?)
            .execute(&mut *tx)
            .await
            .map(|_| ())
        }
        .await;
        self.finish(tx, r, ctx, ActionType::AddProfile, Some(profile.profile_id))
            .await
    }

    /// Define (ou substitui) a janela permitida de um dia da semana do perfil.
    pub async fn set_profile_window(
        &self,
        ctx: &EventContext,
        profile_id: Uuid,
        weekday: u8,
        window: &TimeWindow,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_transaction().await?;
        let r = sqlx::query(
            "INSERT INTO profile_time_window (profile_id, weekday, window_start, window_end) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT (profile_id, weekday) DO UPDATE SET \
             window_start = excluded.window_start, window_end = excluded.window_end",
        )
        .bind(profile_id.to_string())
        .bind(i64::from(weekday))
        .bind(window.start.format(HHMM).to_string())
        .bind(window.end.format(HHMM).to_string())
        .execute(&mut *tx)
        .await
        .map(|_| ());
        self.finish(tx, r, ctx, ActionType::SetProfileWindow, Some(profile_id))
            .await
    }

    /// Cria a regra e grava o evento `ADD_RULE` na mesma transação.
    pub async fn create_active_rule(
        &self,
        ctx: &EventContext,
        rule: &ActiveRule,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_transaction().await?;
        let r = async {
            sqlx::query(
                "INSERT INTO active_rules (rule_id, profile_id, app_id, creation_timestamp, current_state, \
                 daily_limit_seconds, time_window_start, time_window_end, effective_until_timestamp) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(rule.rule_id.to_string())
            .bind(rule.profile_id.to_string())
            .bind(rule.app_id.to_string())
            .bind(rule.creation_timestamp.to_rfc3339())
            .bind(rule.state.as_str())
            .bind(opt_secs(rule.daily_limit_seconds)?)
            .bind(rule.time_window.map(|w| w.start.format(HHMM).to_string()))
            .bind(rule.time_window.map(|w| w.end.format(HHMM).to_string()))
            .bind(rule.effective_until_timestamp.map(|t| t.to_rfc3339()))
            .execute(&mut *tx)
            .await
            .map(|_| ())
        }
        .await;
        self.finish(tx, r, ctx, ActionType::AddRule, Some(rule.profile_id))
            .await
    }

    pub async fn get_active_rule(&self, rule_id: Uuid) -> Result<Option<ActiveRule>, sqlx::Error> {
        let row: Option<RuleRow> = sqlx::query_as(
            "SELECT rule_id, profile_id, app_id, creation_timestamp, current_state, daily_limit_seconds, \
             time_window_start, time_window_end, effective_until_timestamp \
             FROM active_rules WHERE rule_id = ?",
        )
        .bind(rule_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(ActiveRule::try_from).transpose()
    }

    pub async fn set_rule_state(
        &self,
        ctx: &EventContext,
        rule_id: Uuid,
        state: RuleState,
    ) -> Result<(), sqlx::Error> {
        let action = match state {
            RuleState::Revoked => ActionType::Revoke,
            _ => ActionType::ActivateRule,
        };
        let mut tx = self.begin_transaction().await?;
        let profile_id: Option<Uuid> = sqlx::query_scalar::<_, String>(
            "SELECT profile_id FROM active_rules WHERE rule_id = ?",
        )
        .bind(rule_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .and_then(|p| Uuid::parse_str(&p).ok());
        let r = sqlx::query("UPDATE active_rules SET current_state = ? WHERE rule_id = ?")
            .bind(state.as_str())
            .bind(rule_id.to_string())
            .execute(&mut *tx)
            .await
            .and_then(|done| match done.rows_affected() {
                0 => Err(sqlx::Error::RowNotFound),
                _ => Ok(()),
            });
        self.finish(tx, r, ctx, action, profile_id).await
    }

    /// Eventos do período `[from, to]` (inclusive), do mais antigo ao mais novo;
    /// com `profile_id`, só os daquele perfil.
    pub async fn security_events(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        profile_id: Option<Uuid>,
    ) -> Result<Vec<SecurityEvent>, sqlx::Error> {
        let rows: Vec<EventRow> = sqlx::query_as(
            "SELECT log_id, timestamp, client_ip, action_attempted, event_type, transaction_status, \
             rejection_reason_detail, profile_id FROM security_log \
             WHERE timestamp >= ? AND timestamp <= ? AND (? IS NULL OR profile_id = ?) \
             ORDER BY timestamp, rowid",
        )
        .bind(log_ts(from))
        .bind(log_ts(to))
        .bind(profile_id.map(|p| p.to_string()))
        .bind(profile_id.map(|p| p.to_string()))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(SecurityEvent::try_from).collect()
    }

    /// Remove eventos com mais de 90 dias (retenção decidida pelo dono) e registra a limpeza.
    /// O prazo é imposto pelo trigger de DELETE: o banco recusa a remoção de eventos mais novos.
    /// Devolve quantos eventos saíram.
    pub async fn purge_expired_events(&self, ctx: &EventContext) -> Result<u64, sqlx::Error> {
        let mut tx = self.begin_transaction().await?;
        let removed = sqlx::query(
            "DELETE FROM security_log WHERE timestamp < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-90 days')",
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if removed > 0 {
            let e = SecurityEvent::new(
                &ctx.client_ip,
                ActionType::System,
                EventType::Success,
                TransactionStatus::Success,
                &format!("retenção: {removed} eventos com mais de 90 dias removidos"),
            );
            insert_event(&mut tx, &e).await?;
        }
        tx.commit().await?;
        Ok(removed)
    }

    /// Soma ao `usage_ledger` o uso de `app` por `profile_id` em `[inicio, fim)`, dividido pelos
    /// dias locais de `tz`. Intervalo negativo ou maior que `MAX_INTERVALO_SEGUNDOS` indica relógio
    /// suspeito: grava `TAMPER_SUSPECTED` e devolve erro, sem contabilizar nada.
    ///
    /// A contabilização é medição de alta frequência e não grava um evento por chamada; os eventos
    /// do log são as ações de enforcement e as anomalias.
    pub async fn registrar_uso<Tz: TimeZone>(
        &self,
        ctx: &EventContext,
        profile_id: Uuid,
        app_id: Uuid,
        inicio: DateTime<Utc>,
        fim: DateTime<Utc>,
        tz: &Tz,
    ) -> Result<(), sqlx::Error> {
        let duracao = (fim - inicio).num_seconds();
        if !(0..=MAX_INTERVALO_SEGUNDOS).contains(&duracao) {
            let detalhe = format!(
                "intervalo de uso suspeito: {inicio} -> {fim} ({duracao}s; aceito 0..={MAX_INTERVALO_SEGUNDOS}s)"
            );
            let e = SecurityEvent::new(
                &ctx.client_ip,
                ActionType::System,
                EventType::TamperSuspected,
                TransactionStatus::Failure,
                &detalhe,
            )
            .with_profile(Some(profile_id));
            self.record_event(&e).await?;
            return Err(sqlx::Error::Protocol(detalhe));
        }
        let partes = dividir_por_dia_local(tz, inicio, fim).map_err(sqlx::Error::Protocol)?;
        let mut tx = self.begin_transaction().await?;
        for (dia, segundos) in partes {
            sqlx::query(
                "INSERT INTO usage_ledger (profile_id, app_id, usage_date, seconds) VALUES (?, ?, ?, ?) \
                 ON CONFLICT (profile_id, app_id, usage_date) DO UPDATE SET seconds = seconds + excluded.seconds",
            )
            .bind(profile_id.to_string())
            .bind(app_id.to_string())
            .bind(dia.to_string())
            .bind(i64::try_from(segundos).map_err(|e| sqlx::Error::Encode(Box::new(e)))?)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// Consumo do dia local `dia`: do app e total do perfil (soma dos apps).
    pub async fn consumo_do_dia(
        &self,
        profile_id: Uuid,
        app_id: Uuid,
        dia: NaiveDate,
    ) -> Result<Consumo, sqlx::Error> {
        let (app, total): (i64, i64) = sqlx::query_as(
            "SELECT COALESCE(SUM(CASE WHEN app_id = ? THEN seconds END), 0), COALESCE(SUM(seconds), 0) \
             FROM usage_ledger WHERE profile_id = ? AND usage_date = ?",
        )
        .bind(app_id.to_string())
        .bind(profile_id.to_string())
        .bind(dia.to_string())
        .fetch_one(&self.pool)
        .await?;
        let u = |v: i64| u64::try_from(v).map_err(|e| sqlx::Error::Decode(Box::new(e)));
        Ok(Consumo {
            app_segundos: u(app)?,
            total_segundos: u(total)?,
        })
    }
}

/// Quem pediu a operação; vai para o `client_ip` do evento.
pub struct EventContext {
    pub client_ip: String,
}

impl EventContext {
    /// Operação originada no próprio agente/CLI local.
    pub fn local() -> Self {
        Self {
            client_ip: "local".into(),
        }
    }
}

fn opt_secs(v: Option<u64>) -> Result<Option<i64>, sqlx::Error> {
    v.map(|s| i64::try_from(s).map_err(|e| sqlx::Error::Encode(Box::new(e))))
        .transpose()
}
