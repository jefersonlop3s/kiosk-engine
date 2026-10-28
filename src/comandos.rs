//! Camada 3 da arquitetura: validação dos parâmetros dos comandos `POST /api/windows/{ação}`.
//! Pura (sem banco): devolve o comando tipado ou o motivo exato, que vai só para o log forense.
//! Esquema fechado: campo desconhecido é recusado.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::database_schema::{ActionType, TimeWindow};

/// Limite diário máximo aceito (um dia).
pub const MAX_LIMITE_SEGUNDOS: u64 = 86_400;
/// Maior período aceito numa consulta de log, em dias.
pub const MAX_DIAS_DE_LOG: i64 = 31;
/// Eventos devolvidos por consulta de log: padrão e máximo.
pub const LIMITE_PADRAO_LOG: u32 = 100;
pub const LIMITE_MAXIMO_LOG: u32 = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct NovaRegra {
    pub profile_id: Uuid,
    pub app_id: Uuid,
    pub daily_limit_seconds: Option<u64>,
    pub time_window: Option<TimeWindow>,
    pub effective_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Comando {
    AddRule(NovaRegra),
    ActivateRule {
        rule_id: Uuid,
    },
    RevokeRule {
        rule_id: Uuid,
    },
    ModifyLimit {
        rule_id: Uuid,
        daily_limit_seconds: u64,
    },
    Usage {
        profile_id: Uuid,
        dia: NaiveDate,
    },
    Log(ConsultaLog),
}

/// Consulta do log forense: período e, opcionalmente, um perfil.
#[derive(Debug, Clone, PartialEq)]
pub struct ConsultaLog {
    pub de: DateTime<Utc>,
    pub ate: DateTime<Utc>,
    pub profile_id: Option<Uuid>,
    pub limite: u32,
}

impl Comando {
    pub fn acao(&self) -> ActionType {
        match self {
            Self::AddRule(_) => ActionType::AddRule,
            Self::ActivateRule { .. } => ActionType::ActivateRule,
            Self::RevokeRule { .. } => ActionType::Revoke,
            Self::ModifyLimit { .. } => ActionType::ModifyLimit,
            Self::Usage { .. } => ActionType::QueryUsage,
            Self::Log(_) => ActionType::QueryLog,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddRuleBruto {
    profile_id: String,
    app_id: String,
    daily_limit_seconds: Option<u64>,
    window_start: Option<String>,
    window_end: Option<String>,
    effective_until: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModifyLimitBruto {
    rule_id: String,
    daily_limit_seconds: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageBruto {
    profile_id: String,
    date: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogBruto {
    from: String,
    to: String,
    profile_id: Option<String>,
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RegraRef {
    rule_id: String,
}

fn uuid(campo: &str, valor: &str) -> Result<Uuid, String> {
    Uuid::parse_str(valor).map_err(|_| format!("{campo} não é um UUID"))
}

/// Hora no formato estrito `HH:MM` (dois dígitos).
fn hora(campo: &str, valor: &str) -> Result<NaiveTime, String> {
    let ok_formato = valor.len() == 5 && valor.as_bytes()[2] == b':';
    NaiveTime::parse_from_str(valor, "%H:%M")
        .ok()
        .filter(|_| ok_formato)
        .ok_or_else(|| format!("{campo} não está no formato HH:MM"))
}

fn json<'a, T: Deserialize<'a>>(corpo: &'a [u8]) -> Result<T, String> {
    serde_json::from_slice(corpo).map_err(|e| format!("corpo inválido: {e}"))
}

/// Ação pedida pelo nome na URL, quando é conhecida (a autorização, Camada 2, vem antes da validação).
pub fn acao_do_nome(nome: &str) -> Option<ActionType> {
    match nome {
        "add_rule" => Some(ActionType::AddRule),
        "activate_rule" => Some(ActionType::ActivateRule),
        "revoke_rule" => Some(ActionType::Revoke),
        "modify_limit" => Some(ActionType::ModifyLimit),
        "usage" => Some(ActionType::QueryUsage),
        "log" => Some(ActionType::QueryLog),
        _ => None,
    }
}

/// Interpreta e valida o comando. `agora` serve para exigir que `effective_until` esteja no futuro.
pub fn interpretar(acao: &str, corpo: &[u8], agora: DateTime<Utc>) -> Result<Comando, String> {
    match acao {
        "add_rule" => {
            let b: AddRuleBruto = json(corpo)?;
            if b.daily_limit_seconds
                .is_some_and(|l| l > MAX_LIMITE_SEGUNDOS)
            {
                return Err(format!(
                    "daily_limit_seconds acima de {MAX_LIMITE_SEGUNDOS}"
                ));
            }
            let time_window = match (&b.window_start, &b.window_end) {
                (Some(a), Some(z)) => {
                    let (start, end) = (hora("window_start", a)?, hora("window_end", z)?);
                    if start >= end {
                        return Err("window_start deve ser anterior a window_end".into());
                    }
                    Some(TimeWindow { start, end })
                }
                (None, None) => None,
                _ => return Err("window_start e window_end devem vir juntos".into()),
            };
            let effective_until = match &b.effective_until {
                Some(t) => {
                    let t = DateTime::parse_from_rfc3339(t)
                        .map_err(|_| "effective_until não é RFC 3339".to_string())?
                        .with_timezone(&Utc);
                    if t <= agora {
                        return Err("effective_until deve estar no futuro".into());
                    }
                    Some(t)
                }
                None => None,
            };
            Ok(Comando::AddRule(NovaRegra {
                profile_id: uuid("profile_id", &b.profile_id)?,
                app_id: uuid("app_id", &b.app_id)?,
                daily_limit_seconds: b.daily_limit_seconds,
                time_window,
                effective_until,
            }))
        }
        "activate_rule" => Ok(Comando::ActivateRule {
            rule_id: uuid("rule_id", &json::<RegraRef>(corpo)?.rule_id)?,
        }),
        "revoke_rule" => Ok(Comando::RevokeRule {
            rule_id: uuid("rule_id", &json::<RegraRef>(corpo)?.rule_id)?,
        }),
        "modify_limit" => {
            let b: ModifyLimitBruto = json(corpo)?;
            if b.daily_limit_seconds > MAX_LIMITE_SEGUNDOS {
                return Err(format!(
                    "daily_limit_seconds acima de {MAX_LIMITE_SEGUNDOS}"
                ));
            }
            Ok(Comando::ModifyLimit {
                rule_id: uuid("rule_id", &b.rule_id)?,
                daily_limit_seconds: b.daily_limit_seconds,
            })
        }
        "usage" => {
            let b: UsageBruto = json(corpo)?;
            let dia = NaiveDate::parse_from_str(&b.date, "%Y-%m-%d")
                .map_err(|_| "date não está no formato AAAA-MM-DD".to_string())?;
            Ok(Comando::Usage {
                profile_id: uuid("profile_id", &b.profile_id)?,
                dia,
            })
        }
        "log" => {
            let b: LogBruto = json(corpo)?;
            let data = |campo: &str, v: &str| {
                DateTime::parse_from_rfc3339(v)
                    .map(|d| d.with_timezone(&Utc))
                    .map_err(|_| format!("{campo} não é RFC 3339"))
            };
            let (de, ate) = (data("from", &b.from)?, data("to", &b.to)?);
            if de > ate {
                return Err("from deve ser anterior a to".into());
            }
            if ate - de > Duration::days(MAX_DIAS_DE_LOG) {
                return Err(format!("período acima de {MAX_DIAS_DE_LOG} dias"));
            }
            let limite = b.limit.unwrap_or(LIMITE_PADRAO_LOG);
            if !(1..=LIMITE_MAXIMO_LOG).contains(&limite) {
                return Err(format!("limit fora de 1..={LIMITE_MAXIMO_LOG}"));
            }
            let profile_id = b
                .profile_id
                .as_deref()
                .map(|p| uuid("profile_id", p))
                .transpose()?;
            Ok(Comando::Log(ConsultaLog {
                de,
                ate,
                profile_id,
                limite,
            }))
        }
        outra => Err(format!(
            "ação desconhecida: {}",
            outra.chars().take(64).collect::<String>()
        )),
    }
}
