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

/// Evento do log forense (`security_log`).
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityEvent {
    pub event_id: Uuid,
    /// Momento exato do evento (não o da escrita).
    pub timestamp: DateTime<Utc>,
    /// Origem do comando; `"local"` para eventos gerados pelo próprio agente.
    pub client_ip: String,
    pub action_attempted: ActionType,
    pub event_type: EventType,
    pub transaction_status: TransactionStatus,
    /// Razão exata, só para o log: nunca vai na resposta externa (arquitetura, Camada 5).
    pub rejection_reason_detail: String,
}

impl SecurityEvent {
    pub fn new(
        client_ip: &str,
        action_attempted: ActionType,
        event_type: EventType,
        transaction_status: TransactionStatus,
        rejection_reason_detail: &str,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            client_ip: client_ip.to_owned(),
            action_attempted,
            event_type,
            transaction_status,
            rejection_reason_detail: rejection_reason_detail.to_owned(),
        }
    }
}

/// Intenção registrada.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ActionType {
    AddRule,
    ModifyLimit,
    Revoke,
    ActivateRule,
    AddApp,
    AddProfile,
    SetProfileWindow,
    /// Evento gerado pelo próprio agente, sem comando externo.
    System,
}

impl ActionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AddRule => "ADD_RULE",
            Self::ModifyLimit => "MODIFY_LIMIT",
            Self::Revoke => "REVOKE",
            Self::ActivateRule => "ACTIVATE_RULE",
            Self::AddApp => "ADD_APP",
            Self::AddProfile => "ADD_PROFILE",
            Self::SetProfileWindow => "SET_PROFILE_WINDOW",
            Self::System => "SYSTEM",
        }
    }
}

/// Classificação do evento: camadas da API, motor e enforcement (ADR-001).
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum EventType {
    Success,
    AuthFail,
    AuthzFail,
    ValidationFail,
    InternalError,
    StaleRule,
    WarningShown,
    AppTerminated,
    SessionLocked,
    AgentStopped,
    TamperSuspected,
}

impl EventType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::AuthFail => "AUTH_FAIL",
            Self::AuthzFail => "AUTHZ_FAIL",
            Self::ValidationFail => "VALIDATION_FAIL",
            Self::InternalError => "INTERNAL_ERROR",
            Self::StaleRule => "STALE_RULE",
            Self::WarningShown => "WARNING_SHOWN",
            Self::AppTerminated => "APP_TERMINATED",
            Self::SessionLocked => "SESSION_LOCKED",
            Self::AgentStopped => "AGENT_STOPPED",
            Self::TamperSuspected => "TAMPER_SUSPECTED",
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TransactionStatus {
    Success,
    Failure,
}

impl TransactionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Failure => "FAILURE",
        }
    }
}
