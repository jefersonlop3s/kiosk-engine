//! Textos em pt-BR mostrados à criança (ADR-001: o agente é visível e explica cada ação).
//! Puros, para serem testados fora do Windows; o adaptador só os exibe.

use crate::agente::Aviso;
use crate::motor::Motivo;

pub const TITULO: &str = "Controle dos pais";

/// `300` -> `5 minutos`, `60` -> `1 minuto`, `45` -> `45 segundos`.
pub fn tempo(segundos: u64) -> String {
    match segundos {
        60 => "1 minuto".into(),
        s if s >= 60 && s % 60 == 0 => format!("{} minutos", s / 60),
        s if s > 60 => format!("{} minutos", s.div_ceil(60)),
        1 => "1 segundo".into(),
        s => format!("{s} segundos"),
    }
}

fn razao(motivo: Motivo) -> &'static str {
    match motivo {
        Motivo::AppBloqueado => "este programa não está liberado",
        Motivo::LimiteDiarioApp => "o tempo de hoje para este programa está acabando",
        Motivo::LimiteDiarioTotal => "o seu tempo de computador de hoje está acabando",
        Motivo::ForaDaJanelaApp => "o horário permitido para este programa está terminando",
        Motivo::ForaDaJanelaGeral => "o horário permitido para usar o computador está terminando",
    }
}

/// Título e mensagem do aviso. Diz quanto tempo resta, por quê e o que vai acontecer.
pub fn aviso(a: &Aviso) -> (String, String) {
    let quando = tempo(a.restante_segundos);
    let corpo = match (&a.app, a.motivo.e_de_sessao()) {
        (Some(app), false) => format!(
            "Faltam {quando}: {}.\nDepois disso, {app} será fechado. Salve o que estiver fazendo.",
            razao(a.motivo)
        ),
        _ => format!(
            "Faltam {quando}: {}.\nDepois disso, a sessão será bloqueada. Salve o que estiver fazendo.",
            razao(a.motivo)
        ),
    };
    (TITULO.into(), corpo)
}
