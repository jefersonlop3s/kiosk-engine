//! Motor de decisão (E-003). Função pura: sem SO, sem banco e sem relógio próprio;
//! quem chama traz o instante, o perfil, as regras e o consumo do dia.
//!
//! Semântica adotada (a confirmar pelo dono, ver PENDING):
//! - `WHITELISTED` e `IGNORE`: o app não sofre regras de app (a sessão ainda pode ser bloqueada).
//! - `MONITORED`: só é restringido por regras `ACTIVE` e vigentes do perfil para o app;
//!   sem regra vigente, é permitido. Regra com limite 0 bloqueia o app.
//! - Várias regras no mesmo app: vale a mais restritiva (menor limite; estar dentro de todas as janelas).
//! - Gravidade: `BloquearSessao` > `EncerrarApp` > `Avisar` > `Permitir`.
//! - Dia da semana sem janela cadastrada = sem restrição de horário.

use chrono::{DateTime, NaiveDateTime, NaiveTime, TimeZone, Utc};
use uuid::Uuid;

use crate::database_schema::{ActiveRule, ProcessStatus, RuleState, TimeWindow};

/// Marcos de aviso da ADR-001 (5 min e 1 min). O motor devolve `Avisar` a partir do primeiro
/// marco; quem exibe (E-004) decide quando mostrar cada um.
pub const AVISOS_SEGUNDOS: [u64; 2] = [300, 60];

/// Hora local de parede e instante absoluto do mesmo momento. Quem monta garante que coincidem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instante {
    pub utc: DateTime<Utc>,
    pub local: NaiveDateTime,
}

impl Instante {
    pub fn em<Tz: TimeZone>(utc: DateTime<Utc>, tz: &Tz) -> Self {
        Self {
            utc,
            local: utc.with_timezone(tz).naive_local(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerfilCtx {
    pub profile_id: Uuid,
    /// `None` = sem limite diário total.
    pub limite_total_segundos: Option<u64>,
    /// Janela geral do dia da semana de hoje; `None` = sem restrição de horário.
    pub janela_geral: Option<TimeWindow>,
}

#[derive(Debug, Clone, Copy)]
pub struct AppCtx<'a> {
    pub app_id: Uuid,
    pub status: ProcessStatus,
    /// Regras conhecidas; o motor ignora as de outro perfil/app, revogadas, pendentes ou expiradas.
    pub regras: &'a [ActiveRule],
}

/// Consumo do dia local, em segundos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Consumo {
    pub app_segundos: u64,
    pub total_segundos: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Motivo {
    /// Regra com limite 0: o app não pode ser usado.
    AppBloqueado,
    LimiteDiarioApp,
    LimiteDiarioTotal,
    ForaDaJanelaApp,
    ForaDaJanelaGeral,
}

impl Motivo {
    /// Texto estável para o log forense.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AppBloqueado => "APP_BLOQUEADO",
            Self::LimiteDiarioApp => "LIMITE_DIARIO_APP",
            Self::LimiteDiarioTotal => "LIMITE_DIARIO_TOTAL",
            Self::ForaDaJanelaApp => "FORA_DA_JANELA_APP",
            Self::ForaDaJanelaGeral => "FORA_DA_JANELA_GERAL",
        }
    }

    /// `true` se o motivo vem do perfil (sessão), `false` se vem de regra de app.
    pub fn e_de_sessao(self) -> bool {
        matches!(self, Self::LimiteDiarioTotal | Self::ForaDaJanelaGeral)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decisao {
    Permitir,
    Avisar {
        restante_segundos: u64,
        motivo: Motivo,
    },
    EncerrarApp {
        motivo: Motivo,
    },
    BloquearSessao {
        motivo: Motivo,
    },
}

#[derive(Clone, Copy)]
enum Nivel {
    Sessao,
    App,
}

enum Avaliacao {
    Folga,
    Aviso(u64),
    Violado,
}

fn avaliar_limite(limite: u64, usado: u64) -> Avaliacao {
    if usado >= limite {
        return Avaliacao::Violado;
    }
    let restante = limite - usado;
    if restante <= AVISOS_SEGUNDOS[0] {
        Avaliacao::Aviso(restante)
    } else {
        Avaliacao::Folga
    }
}

fn avaliar_janela(janela: &TimeWindow, agora: NaiveTime) -> Avaliacao {
    if agora < janela.start || agora >= janela.end {
        return Avaliacao::Violado;
    }
    let restante = (janela.end - agora).num_seconds().max(0) as u64;
    if restante <= AVISOS_SEGUNDOS[0] {
        Avaliacao::Aviso(restante)
    } else {
        Avaliacao::Folga
    }
}

fn para_decisao(a: Avaliacao, nivel: Nivel, motivo: Motivo) -> Decisao {
    match (a, nivel) {
        (Avaliacao::Folga, _) => Decisao::Permitir,
        (Avaliacao::Aviso(restante_segundos), _) => Decisao::Avisar {
            restante_segundos,
            motivo,
        },
        (Avaliacao::Violado, Nivel::Sessao) => Decisao::BloquearSessao { motivo },
        (Avaliacao::Violado, Nivel::App) => Decisao::EncerrarApp { motivo },
    }
}

fn gravidade(d: &Decisao) -> u8 {
    match d {
        Decisao::Permitir => 0,
        Decisao::Avisar { .. } => 1,
        Decisao::EncerrarApp { .. } => 2,
        Decisao::BloquearSessao { .. } => 3,
    }
}

/// Entre duas decisões, a mais grave; entre dois avisos, o de menor tempo restante.
fn pior(a: Decisao, b: Decisao) -> Decisao {
    match (a, b) {
        (
            Decisao::Avisar {
                restante_segundos: x,
                ..
            },
            Decisao::Avisar {
                restante_segundos: y,
                ..
            },
        ) => {
            if y < x {
                b
            } else {
                a
            }
        }
        _ if gravidade(&b) > gravidade(&a) => b,
        _ => a,
    }
}

fn vigente(r: &ActiveRule, perfil: &PerfilCtx, app: &AppCtx, agora: Instante) -> bool {
    r.profile_id == perfil.profile_id
        && r.app_id == app.app_id
        && r.state == RuleState::Active
        && r.effective_until_timestamp
            .is_none_or(|fim| fim > agora.utc)
}

pub fn decidir(perfil: &PerfilCtx, app: &AppCtx, agora: Instante, consumo: Consumo) -> Decisao {
    let mut decisao = Decisao::Permitir;

    if let Some(limite) = perfil.limite_total_segundos {
        let a = avaliar_limite(limite, consumo.total_segundos);
        decisao = pior(
            decisao,
            para_decisao(a, Nivel::Sessao, Motivo::LimiteDiarioTotal),
        );
    }
    if let Some(janela) = &perfil.janela_geral {
        let a = avaliar_janela(janela, agora.local.time());
        decisao = pior(
            decisao,
            para_decisao(a, Nivel::Sessao, Motivo::ForaDaJanelaGeral),
        );
    }

    if app.status != ProcessStatus::Monitored {
        return decisao;
    }
    for regra in app.regras.iter().filter(|r| vigente(r, perfil, app, agora)) {
        if let Some(limite) = regra.daily_limit_seconds {
            let motivo = if limite == 0 {
                Motivo::AppBloqueado
            } else {
                Motivo::LimiteDiarioApp
            };
            let a = avaliar_limite(limite, consumo.app_segundos);
            decisao = pior(decisao, para_decisao(a, Nivel::App, motivo));
        }
        if let Some(janela) = &regra.time_window {
            let a = avaliar_janela(janela, agora.local.time());
            decisao = pior(
                decisao,
                para_decisao(a, Nivel::App, Motivo::ForaDaJanelaApp),
            );
        }
    }
    decisao
}
