//! Camada 3 da arquitetura: validação dos parâmetros dos comandos `POST /api/windows/{ação}`.
//! Pura (sem banco): devolve o comando tipado ou o motivo exato, que vai só para o log forense.
//! Esquema fechado: campo desconhecido é recusado.

use chrono::{DateTime, NaiveTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::database_schema::{ActionType, TimeWindow};

/// Limite diário máximo aceito (um dia).
pub const MAX_LIMITE_SEGUNDOS: u64 = 86_400;

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
    ActivateRule { rule_id: Uuid },
    RevokeRule { rule_id: Uuid },
}

impl Comando {
    pub fn acao(&self) -> ActionType {
        match self {
            Self::AddRule(_) => ActionType::AddRule,
            Self::ActivateRule { .. } => ActionType::ActivateRule,
            Self::RevokeRule { .. } => ActionType::Revoke,
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
        outra => Err(format!(
            "ação desconhecida: {}",
            outra.chars().take(64).collect::<String>()
        )),
    }
}
