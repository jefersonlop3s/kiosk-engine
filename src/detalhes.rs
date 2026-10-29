//! Detalhe do log forense como código estável mais parâmetros (ADR-010).
//!
//! O log é append-only e guarda o detalhe como um objeto JSON de uma linha, por exemplo
//! `{"code":"APP_TERMINATED","process":"jogo.exe","pid":4242,"reason":"APP_DAILY_LIMIT"}`. Quem lê o
//! log (`kioskctl log`) traduz na hora, no idioma dele; linhas antigas, ou eventos que não usam
//! este formato, aparecem como foram gravados.

use serde::{Deserialize, Serialize};

use crate::i18n::{Idioma, Msg};
use crate::motor::Motivo;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum Detalhe {
    WarningShown {
        marker: u64,
        remaining: u64,
        reason: String,
        app: Option<String>,
    },
    WarningFailed {
        reason: String,
        error: String,
    },
    AppTerminated {
        process: String,
        pid: u32,
        reason: String,
    },
    TerminateFailed {
        process: String,
        pid: u32,
        attempt: u32,
        error: String,
    },
    ProcessWontTerminate {
        process: String,
        pid: u32,
        attempts: u32,
    },
    SessionLocked {
        reason: String,
    },
    SessionLoggedOff {
        locks: u32,
        reason: String,
    },
    LockFailed {
        reason: String,
        error: String,
    },
    LogoffFailed {
        reason: String,
        error: String,
    },
    SuspiciousUsageInterval {
        from: String,
        to: String,
        seconds: i64,
        max: i64,
    },
    /// Intervalo sem agente rodando (E-004 F4): `clean` se o serviço foi parado de propósito
    /// (parada normal, desligamento), `false` se o agente simplesmente sumiu.
    AgentWasDown {
        from: String,
        to: String,
        seconds: i64,
        clean: bool,
    },
    HeartbeatNotRecorded {
        error: String,
    },
    /// Conexão ao pipe da UI recusada (ADR-004): `INVALID_MESSAGE`, `WRONG_IMAGE` ou `RATE_LIMIT`.
    UiRejected {
        reason: String,
    },
    PairingCodeNotAdmin,
    /// Comando administrativo tentado sem elevação (ADR-011). `caller_sid` só quando a conta não
    /// é um perfil de criança; a API dos pais o omite.
    AdminRequired {
        command: String,
        admin_account: bool,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        caller_sid: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        suppressed: Option<u64>,
    },
    RetentionPurged {
        removed: u64,
    },
    ListProcessesFailed {
        error: String,
    },
    ListSessionsFailed {
        error: String,
    },
    UsageNotRecorded {
        error: String,
    },
    SessionTimeNotRecorded {
        error: String,
    },
    SessionDecisionFailed {
        error: String,
    },
    AppDecisionFailed {
        error: String,
    },
    ProfileNotQueried {
        error: String,
    },
    AppNotQueried {
        error: String,
    },
    EventNotRecorded {
        event_type: String,
        error: String,
    },
}

/// O motivo, escrito para o adulto; um código desconhecido aparece como veio.
fn motivo_em(codigo: &str, idioma: Idioma) -> String {
    match Motivo::de_codigo(codigo) {
        Some(Motivo::AppBloqueado) => Msg::MotivoAppBloqueado,
        Some(Motivo::LimiteDiarioApp) => Msg::MotivoLimiteDoApp,
        Some(Motivo::LimiteDiarioTotal) => Msg::MotivoLimiteTotal,
        Some(Motivo::ForaDaJanelaApp) => Msg::MotivoForaDaJanelaDoApp,
        Some(Motivo::ForaDaJanelaGeral) => Msg::MotivoForaDaJanelaGeral,
        None => return codigo.to_owned(),
    }
    .em(idioma)
}

impl Detalhe {
    /// O texto que vai para o log: JSON de uma linha.
    pub fn codigo(&self) -> String {
        serde_json::to_string(self).expect("o detalhe sempre serializa")
    }

    /// Lê um detalhe gravado; `None` se não for deste formato (linha antiga ou de outra origem).
    pub fn de_texto(texto: &str) -> Option<Self> {
        if !texto.starts_with("{\"code\":") {
            return None;
        }
        serde_json::from_str(texto).ok()
    }

    pub fn msg(&self, idioma: Idioma) -> Msg {
        let m = |c: &str| motivo_em(c, idioma);
        match self.clone() {
            Self::WarningShown {
                marker,
                remaining,
                reason,
                app,
            } => Msg::LogAvisoExibido {
                marco: marker,
                restante: remaining,
                motivo: m(&reason),
                app: app.map(|a| format!(", app {a}")).unwrap_or_default(),
            },
            Self::WarningFailed { reason, error } => Msg::LogAvisoFalhou {
                motivo: m(&reason),
                erro: error,
            },
            Self::AppTerminated {
                process,
                pid,
                reason,
            } => Msg::LogAppEncerrado {
                processo: process,
                pid,
                motivo: m(&reason),
            },
            Self::TerminateFailed {
                process,
                pid,
                attempt,
                error,
            } => Msg::LogFalhaAoEncerrar {
                processo: process,
                pid,
                tentativa: attempt,
                erro: error,
            },
            Self::ProcessWontTerminate {
                process,
                pid,
                attempts,
            } => Msg::LogNaoEncerra {
                processo: process,
                pid,
                tentativas: attempts,
            },
            Self::SessionLocked { reason } => Msg::LogSessaoBloqueada { motivo: m(&reason) },
            Self::SessionLoggedOff { locks, reason } => Msg::LogSessaoEncerrada {
                bloqueios: locks,
                motivo: m(&reason),
            },
            Self::LockFailed { reason, error } => Msg::LogSessaoNaoBloqueada {
                motivo: m(&reason),
                erro: error,
            },
            Self::LogoffFailed { reason, error } => Msg::LogSessaoNaoEncerrada {
                motivo: m(&reason),
                erro: error,
            },
            Self::SuspiciousUsageInterval {
                from,
                to,
                seconds,
                max,
            } => Msg::LogIntervaloSuspeito {
                inicio: from,
                fim: to,
                duracao: seconds,
                max,
            },
            Self::AgentWasDown {
                from,
                to,
                seconds,
                clean,
            } => {
                let duracao = crate::textos::duracao(seconds.max(0) as u64);
                if clean {
                    Msg::LogAgenteParado {
                        inicio: from,
                        fim: to,
                        duracao,
                    }
                } else {
                    Msg::LogAgenteSumiu {
                        inicio: from,
                        fim: to,
                        duracao,
                    }
                }
            }
            Self::HeartbeatNotRecorded { error } => Msg::LogBatimentoNaoGravado { erro: error },
            Self::UiRejected { reason } => Msg::LogUiRecusada {
                motivo: match reason.as_str() {
                    "WRONG_IMAGE" => Msg::UiMotivoOutroPrograma.em(idioma),
                    "INVALID_MESSAGE" => Msg::UiMotivoMensagemInvalida.em(idioma),
                    "RATE_LIMIT" => Msg::UiMotivoExcessoDeMensagens.em(idioma),
                    outro => outro.to_owned(),
                },
            },
            Self::PairingCodeNotAdmin => Msg::LogCodigoSemAdmin,
            Self::AdminRequired {
                command,
                admin_account,
                suppressed,
                ..
            } => Msg::LogTentativaSemPrivilegio {
                comando: command,
                quem: if admin_account {
                    Msg::QuemAdminSemElevar
                } else {
                    Msg::QuemContaComum
                }
                .em(idioma),
                extra: suppressed
                    .map(|n| Msg::ExtraSuprimidas { n }.em(idioma))
                    .unwrap_or_default(),
            },
            Self::RetentionPurged { removed } => Msg::LogRetencao { removidos: removed },
            Self::ListProcessesFailed { error } => Msg::LogFalhaListarProcessos { erro: error },
            Self::ListSessionsFailed { error } => Msg::LogFalhaListarSessoes { erro: error },
            Self::UsageNotRecorded { error } => Msg::LogUsoNaoContabilizado { erro: error },
            Self::SessionTimeNotRecorded { error } => {
                Msg::LogSessaoNaoContabilizada { erro: error }
            }
            Self::SessionDecisionFailed { error } => Msg::LogDecisaoDaSessaoFalhou { erro: error },
            Self::AppDecisionFailed { error } => Msg::LogDecisaoDoAppFalhou { erro: error },
            Self::ProfileNotQueried { error } => Msg::LogPerfilNaoConsultado { erro: error },
            Self::AppNotQueried { error } => Msg::LogAppNaoConsultado { erro: error },
            Self::EventNotRecorded { event_type, error } => Msg::LogEventoNaoGravado {
                tipo: event_type,
                erro: error,
            },
        }
    }

    pub fn texto(&self, idioma: Idioma) -> String {
        self.msg(idioma).em(idioma)
    }
}

/// O detalhe como a API o entrega aos pais: só o que não identifica contas que não são perfis.
pub fn para_os_pais(detalhe: &str) -> Option<String> {
    match Detalhe::de_texto(detalhe)? {
        Detalhe::AdminRequired {
            command,
            admin_account,
            suppressed,
            ..
        } => Some(
            Detalhe::AdminRequired {
                command,
                admin_account,
                caller_sid: None,
                suppressed,
            }
            .codigo(),
        ),
        _ => None,
    }
}

/// Detalhe como o adulto lê: traduzido se for do formato novo, senão como foi gravado.
pub fn legivel(detalhe: &str, idioma: Idioma) -> String {
    Detalhe::de_texto(detalhe).map_or_else(|| detalhe.to_owned(), |d| d.texto(idioma))
}
