// Contrato do dado (arquitetura Seção II + ADR-002). Os nomes de campo espelham as colunas das migrações.

use chrono::{DateTime, NaiveTime, SecondsFormat, Utc};
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

    pub(crate) fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "MONITORED" => Ok(Self::Monitored),
            "WHITELISTED" => Ok(Self::Whitelisted),
            "IGNORE" => Ok(Self::Ignored),
            outro => Err(bad_value("current_status", outro)),
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
    /// Limiares de aviso da ADR-001 (GC-08): `[primeiro, segundo]` em segundos,
    /// por exemplo `[300, 60]` para 5 min e 1 min. `None` = usa o padrão do motor.
    /// Lido da coluna `warning_seconds` (TEXT "300,60" ou NULL).
    pub warning_seconds: Option<[u64; 2]>,
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
    warning_seconds: Option<String>,
}

/// GC-08: parseia `"300,60"` → `[300, 60]`, `"300"` → `[300, 60]`,
/// `"600,120"` → `[600, 120]`. O segundo limiar é opcional; quando ausente,
/// vale 60 (1 minuto).
pub fn parse_warning_seconds(s: &str) -> Result<[u64; 2], Error> {
    let partes: Vec<&str> = s.split(',').collect();
    if partes.len() < 1 || partes.len() > 2 {
        return Err(bad_value("warning_seconds", s));
    }
    let p0 = decode(u64::try_from(decode(partes[0].trim().parse::<i64>())?))?;
    let p1 = if partes.len() == 2 {
        decode(u64::try_from(decode(partes[1].trim().parse::<i64>())?))?
    } else {
        60u64
    };
    Ok([p0, p1])
}

/// GC-08: serializa `[300, 60]` → `"300,60"`, `None` → `None`.
pub fn warning_seconds_to_str(w: Option<&[u64; 2]>) -> Option<String> {
    w.map(|t| {
        let [a, b] = *t;
        format!("{a},{b}")
    })
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
            warning_seconds: r
                .warning_seconds
                .map(|s| parse_warning_seconds(&s))
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
    /// Perfil afetado, quando houver. Sem chave estrangeira: o log aceita perfil inexistente.
    pub profile_id: Option<Uuid>,
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
            profile_id: None,
        }
    }

    pub fn with_profile(mut self, profile_id: Option<Uuid>) -> Self {
        self.profile_id = profile_id;
        self
    }
}

/// Formato do timestamp no `security_log`: UTC, microssegundos, largura fixa (ordenável como texto).
pub(crate) fn log_ts(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Micros, true)
}

/// Linha crua de `security_log`; a conversão falha alto em valor desconhecido.
#[derive(FromRow)]
pub(crate) struct EventRow {
    log_id: String,
    timestamp: String,
    client_ip: String,
    action_attempted: String,
    event_type: String,
    transaction_status: String,
    rejection_reason_detail: String,
    profile_id: Option<String>,
}

impl TryFrom<EventRow> for SecurityEvent {
    type Error = Error;

    fn try_from(r: EventRow) -> Result<Self, Error> {
        Ok(Self {
            event_id: decode(Uuid::parse_str(&r.log_id))?,
            timestamp: decode(DateTime::parse_from_rfc3339(&r.timestamp))?.with_timezone(&Utc),
            client_ip: r.client_ip,
            action_attempted: ActionType::parse(&r.action_attempted)?,
            event_type: EventType::parse(&r.event_type)?,
            transaction_status: TransactionStatus::parse(&r.transaction_status)?,
            rejection_reason_detail: r.rejection_reason_detail,
            profile_id: r
                .profile_id
                .map(|p| decode(Uuid::parse_str(&p)))
                .transpose()?,
        })
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
    /// Verificação de assinatura de uma requisição (ADR-006).
    Authenticate,
    CreatePairingCode,
    PairDevice,
    RevokeDevice,
    QueryUsage,
    QueryLog,
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
            Self::Authenticate => "AUTHENTICATE",
            Self::CreatePairingCode => "CREATE_PAIRING_CODE",
            Self::PairDevice => "PAIR_DEVICE",
            Self::RevokeDevice => "REVOKE_DEVICE",
            Self::QueryUsage => "QUERY_USAGE",
            Self::QueryLog => "QUERY_LOG",
        }
    }

    pub(crate) fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "ADD_RULE" => Ok(Self::AddRule),
            "MODIFY_LIMIT" => Ok(Self::ModifyLimit),
            "REVOKE" => Ok(Self::Revoke),
            "ACTIVATE_RULE" => Ok(Self::ActivateRule),
            "ADD_APP" => Ok(Self::AddApp),
            "ADD_PROFILE" => Ok(Self::AddProfile),
            "SET_PROFILE_WINDOW" => Ok(Self::SetProfileWindow),
            "SYSTEM" => Ok(Self::System),
            "AUTHENTICATE" => Ok(Self::Authenticate),
            "CREATE_PAIRING_CODE" => Ok(Self::CreatePairingCode),
            "PAIR_DEVICE" => Ok(Self::PairDevice),
            "REVOKE_DEVICE" => Ok(Self::RevokeDevice),
            "QUERY_USAGE" => Ok(Self::QueryUsage),
            "QUERY_LOG" => Ok(Self::QueryLog),
            outro => Err(bad_value("ActionType", outro)),
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

    pub(crate) fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "SUCCESS" => Ok(Self::Success),
            "AUTH_FAIL" => Ok(Self::AuthFail),
            "AUTHZ_FAIL" => Ok(Self::AuthzFail),
            "VALIDATION_FAIL" => Ok(Self::ValidationFail),
            "INTERNAL_ERROR" => Ok(Self::InternalError),
            "STALE_RULE" => Ok(Self::StaleRule),
            "WARNING_SHOWN" => Ok(Self::WarningShown),
            "APP_TERMINATED" => Ok(Self::AppTerminated),
            "SESSION_LOCKED" => Ok(Self::SessionLocked),
            "AGENT_STOPPED" => Ok(Self::AgentStopped),
            "TAMPER_SUSPECTED" => Ok(Self::TamperSuspected),
            outro => Err(bad_value("EventType", outro)),
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

    pub(crate) fn parse(s: &str) -> Result<Self, Error> {
        match s {
            "SUCCESS" => Ok(Self::Success),
            "FAILURE" => Ok(Self::Failure),
            outro => Err(bad_value("TransactionStatus", outro)),
        }
    }
}

/// Linha crua de `app_metadata`.
#[derive(FromRow)]
pub(crate) struct AppRow {
    app_id: String,
    process_name: String,
    current_status: String,
}

impl TryFrom<AppRow> for AppMetadata {
    type Error = Error;

    fn try_from(r: AppRow) -> Result<Self, Error> {
        Ok(Self {
            app_id: decode(Uuid::parse_str(&r.app_id))?,
            process_name: r.process_name,
            status: ProcessStatus::parse(&r.current_status)?,
        })
    }
}

/// Linha crua de `child_profile`.
#[derive(FromRow)]
pub(crate) struct ProfileRow {
    profile_id: String,
    windows_sid: String,
    display_name: String,
    daily_total_limit_seconds: Option<i64>,
}

impl TryFrom<ProfileRow> for ChildProfile {
    type Error = Error;

    fn try_from(r: ProfileRow) -> Result<Self, Error> {
        Ok(Self {
            profile_id: decode(Uuid::parse_str(&r.profile_id))?,
            windows_sid: r.windows_sid,
            display_name: r.display_name,
            daily_total_limit_seconds: r
                .daily_total_limit_seconds
                .map(|v| decode(u64::try_from(v)))
                .transpose()?,
        })
    }
}
