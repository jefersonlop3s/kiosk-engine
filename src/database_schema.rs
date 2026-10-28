// Contrato do dado (arquitetura Seção II + ADR-002). Os nomes de campo espelham as colunas das migrações.

use chrono::{DateTime, NaiveTime, Utc};
use sqlx::{Error, FromRow};
use uuid::Uuid;

/// Formato de hora das janelas no banco.
pub(crate) const HHMM: &str = "%H:%M";

fn decode<T, E: std::error::Error + Send + Sync + 'static>(r: Result<T, E>) -> Result<T, Error> {
    r.map_err(|e| Error::Decode(Box::new(e)))
}

fn bad_value(coluna: &str, valor: &str) -> Error {
    Error::Decode(format!("valor inválido em {coluna}: {valor:?}").into())
}

/// Processos sob vigilância.
#[derive(Debug, Clone, PartialEq)]
pub struct AppMetadata {
    pub app_id: Uuid,
    /// Nome exato do `.exe` monitorado.
    pub process_name: String,
    pub status: ProcessStatus,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ProcessStatus {
    Monitored,
    Whitelisted,
    Ignored,
}

impl ProcessStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monitored => "MONITORED",
            Self::Whitelisted => "WHITELISTED",
            Self::Ignored => "IGNORE",
        }
    }
}

/// Perfil de criança, vinculado ao SID da conta Windows (ADR-002).
#[derive(Debug, Clone, PartialEq)]
pub struct ChildProfile {
    pub profile_id: Uuid,
    pub windows_sid: String,
    pub display_name: String,
    /// `None` = sem limite diário total.
    pub daily_total_limit_seconds: Option<u64>,
}

/// Janela de horário local, `start < end`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeWindow {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

/// Estado vivo da regra (ciclo PENDING -> ACTIVE -> REVOKED).
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveRule {
    pub rule_id: Uuid,
    pub profile_id: Uuid,
    pub app_id: Uuid,
    pub creation_timestamp: DateTime<Utc>,
    pub state: RuleState,
    /// `None` = sem limite diário para este app.
    pub daily_limit_seconds: Option<u64>,
    pub time_window: Option<TimeWindow>,
    /// `None` = sem data de desativação lógica.
    pub effective_until_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum RuleState {
    Pending,
    Active,
    Revoked,
}

impl RuleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Active => "ACTIVE",
            Self::Revoked => "REVOKED",
        }
    }

    fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "PENDING" => Ok(Self::Pending),
            "ACTIVE" => Ok(Self::Active),
            "REVOKED" => Ok(Self::Revoked),
            outro => Err(bad_value("current_state", outro)),
        }
    }
}

/// Linha crua de `active_rules`; a conversão valida cada coluna e falha alto.
#[derive(FromRow)]
pub(crate) struct RuleRow {
    rule_id: String,
    profile_id: String,
    app_id: String,
    creation_timestamp: String,
    current_state: String,
    daily_limit_seconds: Option<i64>,
    time_window_start: Option<String>,
    time_window_end: Option<String>,
    effective_until_timestamp: Option<String>,
}

impl TryFrom<RuleRow> for ActiveRule {
    type Error = Error;

    fn try_from(r: RuleRow) -> Result<Self, Error> {
        let time_window = match (r.time_window_start, r.time_window_end) {
            (Some(a), Some(b)) => Some(TimeWindow {
                start: decode(NaiveTime::parse_from_str(&a, HHMM))?,
                end: decode(NaiveTime::parse_from_str(&b, HHMM))?,
            }),
            (None, None) => None,
            _ => {
                return Err(bad_value(
                    "time_window_*",
                    "apenas um dos limites preenchido",
                ));
            }
        };
        Ok(Self {
            rule_id: decode(Uuid::parse_str(&r.rule_id))?,
            profile_id: decode(Uuid::parse_str(&r.profile_id))?,
            app_id: decode(Uuid::parse_str(&r.app_id))?,
            creation_timestamp: decode(DateTime::parse_from_rfc3339(&r.creation_timestamp))?
                .with_timezone(&Utc),
            state: RuleState::parse(&r.current_state)?,
            daily_limit_seconds: r
                .daily_limit_seconds
                .map(|v| decode(u64::try_from(v)))
                .transpose()?,
            time_window,
            effective_until_timestamp: r
                .effective_until_timestamp
                .map(|t| decode(DateTime::parse_from_rfc3339(&t)).map(|d| d.with_timezone(&Utc)))
                .transpose()?,
        })
    }
}

/// ===================================================
/// 🛡️ TABELA: SecurityLog
/// O LOG FORENSE ABSOLUTO (Abordagem B). Registra o motivo EXATO de cada tentativa.
/// Esta tabela é otimizada para imutabilidade e busca forense, não para leitura rápida de status operacional.
/// ===================================================
#[derive(Debug, Clone)]
pub struct SecurityEvent {
    // log_id (PK): UUID único para este evento/tentativa. Garante a unicidade do registro.
    pub event_id: Uuid,
    // timestamp: Momento EXATO do evento/tentativa. A fonte da verdade forense.
    pub timestamp: DateTime<Utc>,
    // client_ip: IP da origem do comando (ex: "100.x.x.x"). Essencial para rastrear o agente atacante ou administrador.
    pub client_ip: String,
    // action_attempted: A intenção do cliente (Ex: ADD_RULE, MODIFY_LIMIT, REVOKE). O que ele *tentou* fazer?
    pub action_attempted: ActionType,
    // status: Resultado da transação (SUCCESS ou FAILURE). Foi aceito ou rejeitado pelo Daemon Core.
    pub transaction_status: TransactionStatus,
    // REJECTION_REASON_DETAIL (CRÍTICO): Texto livre detalhado com a razão EXATA do bloqueio/falha.
    // Este é o campo que armazena os "porquês" forenses e deve ser robusto para textos longos.
    pub rejection_reason_detail: String,
}

/// Enum para as ações críticas registradas (a intenção).
#[derive(Debug, PartialEq, Clone)]
pub enum ActionType {
    AddRule,
    ModifyLimit,
    Revoke,
    // ... outros comandos como ChangeWhitelistStatus, etc.
}

/// Enum para o resultado da transação (o que realmente aconteceu).
#[derive(Debug, PartialEq, Clone)]
pub enum TransactionStatus {
    Success, // A intenção foi executada com sucesso e alterou o estado do sistema corretamente.
    Failure, // O comando falhou por erro interno ou validação de dados.
}

// Nota: As estruturas para a Camada 0 (Recepção) e Camada 1 (Autenticação Criptográfica) serão criadas em arquivos separados (`api_handler.rs` e `crypto_utils.rs`).
