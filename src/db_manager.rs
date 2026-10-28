use std::str::FromStr;

use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, TimeZone, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::database_schema::*;
use crate::motor::{AppCtx, Consumo, Decisao, Instante, PerfilCtx, decidir};
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
    pub(crate) async fn finish<T>(
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
        self.security_events_limitado(from, to, profile_id, i64::MAX)
            .await
    }

    /// Como `security_events`, devolvendo no máximo `limite` eventos (os mais antigos do período).
    pub async fn security_events_limitado(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        profile_id: Option<Uuid>,
        limite: i64,
    ) -> Result<Vec<SecurityEvent>, sqlx::Error> {
        let rows: Vec<EventRow> = sqlx::query_as(
            "SELECT log_id, timestamp, client_ip, action_attempted, event_type, transaction_status, \
             rejection_reason_detail, profile_id FROM security_log \
             WHERE timestamp >= ? AND timestamp <= ? AND (? IS NULL OR profile_id = ?) \
             ORDER BY timestamp, rowid LIMIT ?",
        )
        .bind(log_ts(from))
        .bind(log_ts(to))
        .bind(profile_id.map(|p| p.to_string()))
        .bind(profile_id.map(|p| p.to_string()))
        .bind(limite)
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

    /// Valida o intervalo e o divide pelos dias locais. Intervalo negativo ou maior que
    /// `MAX_INTERVALO_SEGUNDOS` indica relógio suspeito: grava `TAMPER_SUSPECTED` e devolve erro,
    /// sem contabilizar nada.
    async fn partes_do_intervalo<Tz: TimeZone>(
        &self,
        ctx: &EventContext,
        profile_id: Uuid,
        inicio: DateTime<Utc>,
        fim: DateTime<Utc>,
        tz: &Tz,
    ) -> Result<Vec<(NaiveDate, i64)>, sqlx::Error> {
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
        dividir_por_dia_local(tz, inicio, fim)
            .map_err(sqlx::Error::Protocol)?
            .into_iter()
            .map(|(dia, s)| {
                Ok((
                    dia,
                    i64::try_from(s).map_err(|e| sqlx::Error::Encode(Box::new(e)))?,
                ))
            })
            .collect()
    }

    /// Soma ao `usage_ledger` o uso de `app` por `profile_id` em `[inicio, fim)`, dividido pelos
    /// dias locais de `tz` (ver `partes_do_intervalo` para a guarda de relógio).
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
        let partes = self
            .partes_do_intervalo(ctx, profile_id, inicio, fim, tz)
            .await?;
        let mut tx = self.begin_transaction().await?;
        for (dia, segundos) in partes {
            sqlx::query(
                "INSERT INTO usage_ledger (profile_id, app_id, usage_date, seconds) VALUES (?, ?, ?, ?) \
                 ON CONFLICT (profile_id, app_id, usage_date) DO UPDATE SET seconds = seconds + excluded.seconds",
            )
            .bind(profile_id.to_string())
            .bind(app_id.to_string())
            .bind(dia.to_string())
            .bind(segundos)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// Soma ao `session_usage` o tempo de sessão ativa do perfil em `[inicio, fim)` (ADR-003).
    pub async fn registrar_sessao<Tz: TimeZone>(
        &self,
        ctx: &EventContext,
        profile_id: Uuid,
        inicio: DateTime<Utc>,
        fim: DateTime<Utc>,
        tz: &Tz,
    ) -> Result<(), sqlx::Error> {
        let partes = self
            .partes_do_intervalo(ctx, profile_id, inicio, fim, tz)
            .await?;
        let mut tx = self.begin_transaction().await?;
        for (dia, segundos) in partes {
            sqlx::query(
                "INSERT INTO session_usage (profile_id, usage_date, seconds) VALUES (?, ?, ?) \
                 ON CONFLICT (profile_id, usage_date) DO UPDATE SET seconds = seconds + excluded.seconds",
            )
            .bind(profile_id.to_string())
            .bind(dia.to_string())
            .bind(segundos)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await
    }

    /// Consumo do dia local `dia`: tempo do app (`usage_ledger`) e tempo de sessão ativa do
    /// perfil (`session_usage`, ADR-003), que não é a soma dos apps.
    pub async fn consumo_do_dia(
        &self,
        profile_id: Uuid,
        app_id: Uuid,
        dia: NaiveDate,
    ) -> Result<Consumo, sqlx::Error> {
        let app: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(seconds), 0) FROM usage_ledger \
             WHERE profile_id = ? AND app_id = ? AND usage_date = ?",
        )
        .bind(profile_id.to_string())
        .bind(app_id.to_string())
        .bind(dia.to_string())
        .fetch_one(&self.pool)
        .await?;
        let total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(seconds), 0) FROM session_usage WHERE profile_id = ? AND usage_date = ?",
        )
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

    pub async fn perfil_por_sid(&self, sid: &str) -> Result<Option<ChildProfile>, sqlx::Error> {
        let row: Option<ProfileRow> = sqlx::query_as(
            "SELECT profile_id, windows_sid, display_name, daily_total_limit_seconds \
             FROM child_profile WHERE windows_sid = ?",
        )
        .bind(sid)
        .fetch_optional(&self.pool)
        .await?;
        row.map(ChildProfile::try_from).transpose()
    }

    /// Procura o app pelo nome do executável, sem diferenciar maiúsculas de minúsculas.
    pub async fn app_por_processo(
        &self,
        process_name: &str,
    ) -> Result<Option<AppMetadata>, sqlx::Error> {
        let row: Option<AppRow> = sqlx::query_as(
            "SELECT app_id, process_name, current_status FROM app_metadata WHERE process_name = ?",
        )
        .bind(process_name)
        .fetch_optional(&self.pool)
        .await?;
        row.map(AppMetadata::try_from).transpose()
    }

    /// Todas as regras do perfil para o app; o motor descarta as não vigentes.
    pub async fn regras_do_app(
        &self,
        profile_id: Uuid,
        app_id: Uuid,
    ) -> Result<Vec<ActiveRule>, sqlx::Error> {
        let rows: Vec<RuleRow> = sqlx::query_as(
            "SELECT rule_id, profile_id, app_id, creation_timestamp, current_state, daily_limit_seconds, \
             time_window_start, time_window_end, effective_until_timestamp \
             FROM active_rules WHERE profile_id = ? AND app_id = ? ORDER BY creation_timestamp, rowid",
        )
        .bind(profile_id.to_string())
        .bind(app_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(ActiveRule::try_from).collect()
    }

    /// Limite total e janela geral do perfil para o dia da semana de `dia` (0 = domingo).
    pub async fn perfil_ctx(
        &self,
        profile_id: Uuid,
        dia: NaiveDate,
    ) -> Result<PerfilCtx, sqlx::Error> {
        let limite: Option<i64> = sqlx::query_scalar(
            "SELECT daily_total_limit_seconds FROM child_profile WHERE profile_id = ?",
        )
        .bind(profile_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
        let janela: Option<(String, String)> = sqlx::query_as(
            "SELECT window_start, window_end FROM profile_time_window WHERE profile_id = ? AND weekday = ?",
        )
        .bind(profile_id.to_string())
        .bind(i64::from(dia.weekday().num_days_from_sunday()))
        .fetch_optional(&self.pool)
        .await?;
        let hora = |t: &str| {
            NaiveTime::parse_from_str(t, HHMM).map_err(|e| sqlx::Error::Decode(Box::new(e)))
        };
        Ok(PerfilCtx {
            profile_id,
            limite_total_segundos: limite
                .map(|v| u64::try_from(v).map_err(|e| sqlx::Error::Decode(Box::new(e))))
                .transpose()?,
            janela_geral: janela
                .map(|(a, b)| {
                    Ok::<_, sqlx::Error>(TimeWindow {
                        start: hora(&a)?,
                        end: hora(&b)?,
                    })
                })
                .transpose()?,
        })
    }

    /// Decisão completa (sessão e app), montada a partir do banco. Com `app = None` avalia só a
    /// sessão do perfil (limite total e janela geral).
    pub async fn decidir_para(
        &self,
        profile_id: Uuid,
        app: Option<&AppMetadata>,
        agora: Instante,
    ) -> Result<Decisao, sqlx::Error> {
        self.decidir_escopo(profile_id, app, agora, true).await
    }

    /// Só as regras do app, sem o limite total nem a janela geral do perfil. O agente usa esta
    /// e `decidir_para(.., None, ..)` em passes separados para que um aviso de sessão não
    /// esconda o aviso de app.
    pub async fn decidir_app(
        &self,
        profile_id: Uuid,
        app: &AppMetadata,
        agora: Instante,
    ) -> Result<Decisao, sqlx::Error> {
        self.decidir_escopo(profile_id, Some(app), agora, false)
            .await
    }

    async fn decidir_escopo(
        &self,
        profile_id: Uuid,
        app: Option<&AppMetadata>,
        agora: Instante,
        com_sessao: bool,
    ) -> Result<Decisao, sqlx::Error> {
        let dia = agora.local.date();
        let mut perfil = self.perfil_ctx(profile_id, dia).await?;
        if !com_sessao {
            perfil.limite_total_segundos = None;
            perfil.janela_geral = None;
        }
        let (app_id, status, regras) = match app {
            Some(a) => (
                a.app_id,
                a.status,
                self.regras_do_app(profile_id, a.app_id).await?,
            ),
            None => (Uuid::nil(), ProcessStatus::Ignored, Vec::new()),
        };
        let consumo = self.consumo_do_dia(profile_id, app_id, dia).await?;
        Ok(decidir(
            &perfil,
            &AppCtx {
                app_id,
                status,
                regras: &regras,
            },
            agora,
            consumo,
        ))
    }

    pub async fn existe_perfil(&self, profile_id: Uuid) -> Result<bool, sqlx::Error> {
        self.existe("child_profile", "profile_id", profile_id).await
    }

    pub async fn existe_app(&self, app_id: Uuid) -> Result<bool, sqlx::Error> {
        self.existe("app_metadata", "app_id", app_id).await
    }

    pub async fn existe_regra(&self, rule_id: Uuid) -> Result<bool, sqlx::Error> {
        self.existe("active_rules", "rule_id", rule_id).await
    }

    /// `tabela` e `coluna` são constantes internas; nunca vêm de entrada externa.
    async fn existe(&self, tabela: &str, coluna: &str, id: Uuid) -> Result<bool, sqlx::Error> {
        let n: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM {tabela} WHERE {coluna} = ?"))
                .bind(id.to_string())
                .fetch_one(&self.pool)
                .await?;
        Ok(n > 0)
    }

    /// Dispositivos pareados (id, nome, criado em, revogado em), do mais antigo ao mais novo.
    pub async fn listar_dispositivos(
        &self,
    ) -> Result<Vec<(String, String, String, Option<String>)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT device_id, display_name, created_at, revoked_at FROM paired_device ORDER BY created_at, rowid",
        )
        .fetch_all(&self.pool)
        .await
    }

    /// Altera o limite diário de uma regra não revogada (auditado como `MODIFY_LIMIT`).
    pub async fn set_rule_limit(
        &self,
        ctx: &EventContext,
        rule_id: Uuid,
        limite_segundos: u64,
    ) -> Result<(), sqlx::Error> {
        let limite =
            i64::try_from(limite_segundos).map_err(|e| sqlx::Error::Encode(Box::new(e)))?;
        let mut tx = self.begin_transaction().await?;
        let profile_id: Option<Uuid> = sqlx::query_scalar::<_, String>(
            "SELECT profile_id FROM active_rules WHERE rule_id = ?",
        )
        .bind(rule_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .and_then(|p| Uuid::parse_str(&p).ok());
        let r = sqlx::query(
            "UPDATE active_rules SET daily_limit_seconds = ? WHERE rule_id = ? AND current_state != 'REVOKED'",
        )
        .bind(limite)
        .bind(rule_id.to_string())
        .execute(&mut *tx)
        .await
        .and_then(|done| match done.rows_affected() {
            0 => Err(sqlx::Error::RowNotFound),
            _ => Ok(()),
        });
        self.finish(tx, r, ctx, ActionType::ModifyLimit, profile_id)
            .await
    }

    /// Uso do dia local `dia`: tempo de sessão ativa do perfil e tempo por app
    /// `(app_id, process_name, segundos)`, em ordem alfabética.
    pub async fn uso_do_dia(
        &self,
        profile_id: Uuid,
        dia: NaiveDate,
    ) -> Result<(u64, Vec<(Uuid, String, u64)>), sqlx::Error> {
        let sessao: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(seconds), 0) FROM session_usage WHERE profile_id = ? AND usage_date = ?",
        )
        .bind(profile_id.to_string())
        .bind(dia.to_string())
        .fetch_one(&self.pool)
        .await?;
        let linhas: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT a.app_id, a.process_name, u.seconds FROM usage_ledger u \
             JOIN app_metadata a ON a.app_id = u.app_id \
             WHERE u.profile_id = ? AND u.usage_date = ? ORDER BY a.process_name",
        )
        .bind(profile_id.to_string())
        .bind(dia.to_string())
        .fetch_all(&self.pool)
        .await?;
        let u = |v: i64| u64::try_from(v).map_err(|e| sqlx::Error::Decode(Box::new(e)));
        let apps = linhas
            .into_iter()
            .map(|(id, nome, s)| {
                Ok((
                    Uuid::parse_str(&id).map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
                    nome,
                    u(s)?,
                ))
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;
        Ok((u(sessao)?, apps))
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
