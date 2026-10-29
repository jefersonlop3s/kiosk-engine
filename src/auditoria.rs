//! Rastro das tentativas de comando administrativo sem privilégio (ADR-011).
//!
//! Uma conta sem privilégio não abre o banco, então quem registra a tentativa é o serviço: o
//! `kioskd` avisa pelo named pipe local `kiosk-audit` e o serviço grava um `AUTHZ_FAIL`. Aqui ficam
//! as partes que não dependem do Windows, para serem testadas em qualquer sistema: o protocolo (uma
//! linha JSON de esquema fechado), o limite de taxa por conta e a montagem e gravação do evento.
//! O pipe e a leitura do token do chamador estão em `auditoria_windows`.
//!
//! O chamador **nunca** é lido da mensagem: quem diz o SID e se a conta é de administrador é o
//! token da conexão, lido pelo serviço.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database_schema::{ActionType, EventType, SecurityEvent, TransactionStatus};
use crate::db_manager::KioskDbManager;
use crate::detalhes::Detalhe;

pub const NOME_DO_PIPE: &str = r"\\.\pipe\kiosk-audit";
pub const VERSAO: u32 = 1;
const TIPO: &str = "tentativa_sem_privilegio";
/// Maior mensagem aceita (uma linha JSON, sem o `\n`).
pub const MAX_BYTES: usize = 512;
/// Avisos por minuto, por conta, que viram evento; o excedente é descartado e contado.
pub const MAX_AVISOS_POR_MINUTO: u32 = 5;

/// Os comandos do `kioskd` que exigem administrador (em inglês). Qualquer outro nome é recusado.
pub const COMANDOS: &[&str] = &[
    "pairing-code",
    "devices",
    "revoke",
    "profile create",
    "profile list",
    "profile limit",
    "profile window",
    "app create",
    "app list",
    "rule create",
    "rule activate",
    "rule revoke",
    "rule limit",
    "rule list",
    "install",
    "uninstall",
];

/// Ação registrada para o comando, quando o projeto já tem uma; senão `SYSTEM`.
pub fn acao_do_comando(comando: &str) -> ActionType {
    match comando {
        "pairing-code" => ActionType::CreatePairingCode,
        "rule create" => ActionType::AddRule,
        "rule activate" => ActionType::ActivateRule,
        "rule revoke" => ActionType::Revoke,
        "rule limit" => ActionType::ModifyLimit,
        _ => ActionType::System,
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AvisoBruto {
    v: u32,
    tipo: String,
    comando: String,
}

/// A linha que o `kioskd` envia.
pub fn montar_aviso(comando: &str) -> String {
    serde_json::to_string(&AvisoBruto {
        v: VERSAO,
        tipo: TIPO.into(),
        comando: comando.into(),
    })
    .expect("o aviso sempre serializa")
}

/// A resposta do serviço.
pub fn montar_resposta(ok: bool) -> String {
    format!(r#"{{"v":{VERSAO},"ok":{ok}}}"#)
}

/// `true` se a resposta é `{"v":1,"ok":true}`.
pub fn resposta_ok(linha: &str) -> bool {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct R {
        v: u32,
        ok: bool,
    }
    serde_json::from_str::<R>(linha.trim()).is_ok_and(|r| r.v == VERSAO && r.ok)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroAviso {
    Grande,
    NaoEUtf8,
    Json(String),
    Versao(u32),
    Tipo,
    ComandoDesconhecido,
}

impl std::fmt::Display for ErroAviso {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Grande => write!(f, "message above {MAX_BYTES} bytes"),
            Self::NaoEUtf8 => f.write_str("message is not UTF-8"),
            Self::Json(e) => write!(f, "invalid message: {e}"),
            Self::Versao(v) => write!(f, "unsupported protocol version {v}"),
            Self::Tipo => f.write_str("unknown message type"),
            Self::ComandoDesconhecido => f.write_str("command not in the allowed list"),
        }
    }
}

/// Decodifica o aviso como entrada **não confiável**: tamanho, UTF-8, esquema fechado, versão
/// exata, tipo e comando da lista. Devolve o comando.
pub fn decodificar(linha: &[u8]) -> Result<&'static str, ErroAviso> {
    let linha = linha.strip_suffix(b"\n").unwrap_or(linha);
    let linha = linha.strip_suffix(b"\r").unwrap_or(linha);
    if linha.len() > MAX_BYTES {
        return Err(ErroAviso::Grande);
    }
    let texto = std::str::from_utf8(linha).map_err(|_| ErroAviso::NaoEUtf8)?;
    let a: AvisoBruto = serde_json::from_str(texto).map_err(|e| ErroAviso::Json(e.to_string()))?;
    if a.v != VERSAO {
        return Err(ErroAviso::Versao(a.v));
    }
    if a.tipo != TIPO {
        return Err(ErroAviso::Tipo);
    }
    COMANDOS
        .iter()
        .find(|c| **c == a.comando)
        .copied()
        .ok_or(ErroAviso::ComandoDesconhecido)
}

/// Quem chamou, como o serviço leu do token da conexão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chamador {
    pub sid: String,
    /// A conta é de administrador (inclusive sem elevação, com o grupo só para negação).
    pub conta_admin: bool,
}

/// Resultado do limite de taxa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admissao {
    /// Vira evento; `suprimidos` é quantos avisos anteriores foram descartados desde o último.
    Aceito {
        suprimidos: u64,
    },
    Descartado,
}

#[derive(Debug, Default)]
struct Janela {
    inicio: Option<DateTime<Utc>>,
    contados: u32,
    suprimidos: u64,
}

/// No máximo [`MAX_AVISOS_POR_MINUTO`] eventos por minuto por conta; o resto é contado.
#[derive(Debug, Default)]
pub struct Limitador {
    por_conta: HashMap<String, Janela>,
}

const MAX_CONTAS: usize = 1024;

impl Limitador {
    pub fn admitir(&mut self, sid: &str, agora: DateTime<Utc>) -> Admissao {
        // contas paradas há mais de 10 minutos, sem nada a reportar, saem da memória
        if self.por_conta.len() >= MAX_CONTAS {
            self.por_conta.retain(|_, j| {
                j.suprimidos > 0 || j.inicio.is_some_and(|i| agora - i < Duration::minutes(10))
            });
        }
        let j = self.por_conta.entry(sid.to_owned()).or_default();
        if j.inicio.is_none_or(|i| agora - i >= Duration::seconds(60)) {
            j.inicio = Some(agora);
            j.contados = 0;
        }
        if j.contados >= MAX_AVISOS_POR_MINUTO {
            j.suprimidos += 1;
            return Admissao::Descartado;
        }
        j.contados += 1;
        Admissao::Aceito {
            suprimidos: std::mem::take(&mut j.suprimidos),
        }
    }
}

/// O evento de um aviso válido. Se o chamador é um perfil de criança, o evento leva o perfil e o
/// SID fica de fora (o perfil já diz quem é); senão leva o SID, para quem examinar o banco no PC.
pub fn evento(
    comando: &str,
    chamador: &Chamador,
    perfil: Option<Uuid>,
    suprimidos: u64,
    agora: DateTime<Utc>,
) -> SecurityEvent {
    let detalhe = Detalhe::AdminRequired {
        command: comando.to_owned(),
        admin_account: chamador.conta_admin,
        caller_sid: perfil.is_none().then(|| chamador.sid.clone()),
        suppressed: (suprimidos > 0).then_some(suprimidos),
    };
    let mut e = SecurityEvent::new(
        "local",
        acao_do_comando(comando),
        EventType::AuthzFail,
        TransactionStatus::Failure,
        &detalhe.codigo(),
    )
    .with_profile(perfil);
    e.timestamp = agora;
    e
}

/// O que o serviço fez com a conexão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registro {
    Gravado,
    /// Dentro do limite de taxa, mas descartado e contado para o próximo evento.
    Descartado,
}

/// Valida o aviso de `chamador` e grava o evento. Aviso inválido vira `VALIDATION_FAIL` (também
/// sujeito ao limite de taxa) e devolve o erro, para o serviço responder `ok:false`.
pub async fn registrar(
    db: &KioskDbManager,
    limitador: &mut Limitador,
    chamador: &Chamador,
    linha: &[u8],
    agora: DateTime<Utc>,
) -> Result<Registro, String> {
    let comando = match decodificar(linha) {
        Ok(c) => c,
        Err(erro) => {
            if let Admissao::Aceito { .. } = limitador.admitir(&chamador.sid, agora) {
                let e = SecurityEvent::new(
                    "local",
                    ActionType::System,
                    EventType::ValidationFail,
                    TransactionStatus::Failure,
                    &format!("invalid audit notice: {erro}"),
                );
                db.record_event(&e).await.map_err(|e| e.to_string())?;
            }
            return Err(erro.to_string());
        }
    };
    let suprimidos = match limitador.admitir(&chamador.sid, agora) {
        Admissao::Descartado => return Ok(Registro::Descartado),
        Admissao::Aceito { suprimidos } => suprimidos,
    };
    let perfil = db
        .perfil_por_sid(&chamador.sid)
        .await
        .map_err(|e| e.to_string())?
        .map(|p| p.profile_id);
    db.record_event(&evento(comando, chamador, perfil, suprimidos, agora))
        .await
        .map_err(|e| e.to_string())?;
    Ok(Registro::Gravado)
}
