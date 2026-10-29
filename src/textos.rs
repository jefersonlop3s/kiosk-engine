//! Textos mostrados à criança (ADR-001: o agente é visível e explica cada ação), nos três idiomas
//! (ADR-010). Puros, para serem testados fora do Windows; o adaptador só os exibe, no idioma da
//! conta da criança.

use crate::agente::Aviso;
use crate::i18n::Idioma;
use crate::motor::Motivo;

pub fn titulo(idioma: Idioma) -> &'static str {
    match idioma {
        Idioma::EnUs => "Parental controls",
        Idioma::PtBr => "Controle dos pais",
        Idioma::EsEs => "Control parental",
    }
}

/// `300` -> `5 minutos`, `60` -> `1 minuto`, `45` -> `45 segundos` (e o equivalente em cada idioma).
pub fn tempo(segundos: u64, idioma: Idioma) -> String {
    let (um_min, mins, um_seg, segs) = match idioma {
        Idioma::EnUs => ("1 minute", "minutes", "1 second", "seconds"),
        Idioma::PtBr => ("1 minuto", "minutos", "1 segundo", "segundos"),
        Idioma::EsEs => ("1 minuto", "minutos", "1 segundo", "segundos"),
    };
    match segundos {
        60 => um_min.into(),
        s if s >= 60 && s % 60 == 0 => format!("{} {mins}", s / 60),
        s if s > 60 => format!("{} {mins}", s.div_ceil(60)),
        1 => um_seg.into(),
        s => format!("{s} {segs}"),
    }
}

fn razao(motivo: Motivo, idioma: Idioma) -> &'static str {
    match (motivo, idioma) {
        (Motivo::AppBloqueado, Idioma::EnUs) => "this program is not allowed",
        (Motivo::AppBloqueado, Idioma::PtBr) => "este programa não está liberado",
        (Motivo::AppBloqueado, Idioma::EsEs) => "este programa no está permitido",
        (Motivo::LimiteDiarioApp, Idioma::EnUs) => "today's time for this program is running out",
        (Motivo::LimiteDiarioApp, Idioma::PtBr) => {
            "o tempo de hoje para este programa está acabando"
        }
        (Motivo::LimiteDiarioApp, Idioma::EsEs) => {
            "el tiempo de hoy para este programa se está acabando"
        }
        (Motivo::LimiteDiarioTotal, Idioma::EnUs) => "your computer time for today is running out",
        (Motivo::LimiteDiarioTotal, Idioma::PtBr) => {
            "o seu tempo de computador de hoje está acabando"
        }
        (Motivo::LimiteDiarioTotal, Idioma::EsEs) => {
            "tu tiempo de ordenador de hoy se está acabando"
        }
        (Motivo::ForaDaJanelaApp, Idioma::EnUs) => {
            "the allowed time of day for this program is ending"
        }
        (Motivo::ForaDaJanelaApp, Idioma::PtBr) => {
            "o horário permitido para este programa está terminando"
        }
        (Motivo::ForaDaJanelaApp, Idioma::EsEs) => {
            "el horario permitido para este programa está terminando"
        }
        (Motivo::ForaDaJanelaGeral, Idioma::EnUs) => {
            "the allowed time of day for using the computer is ending"
        }
        (Motivo::ForaDaJanelaGeral, Idioma::PtBr) => {
            "o horário permitido para usar o computador está terminando"
        }
        (Motivo::ForaDaJanelaGeral, Idioma::EsEs) => {
            "el horario permitido para usar el ordenador está terminando"
        }
    }
}

/// Título e mensagem do aviso. Diz quanto tempo resta, por quê e o que vai acontecer.
pub fn aviso(a: &Aviso, idioma: Idioma) -> (String, String) {
    let quando = tempo(a.restante_segundos, idioma);
    let motivo = razao(a.motivo, idioma);
    let corpo = match (&a.app, a.motivo.e_de_sessao(), idioma) {
        (Some(app), false, Idioma::EnUs) => format!(
            "{quando} left: {motivo}.\nAfter that, {app} will be closed. Save what you are doing."
        ),
        (Some(app), false, Idioma::PtBr) => format!(
            "Faltam {quando}: {motivo}.\nDepois disso, {app} será fechado. Salve o que estiver fazendo."
        ),
        (Some(app), false, Idioma::EsEs) => format!(
            "Faltan {quando}: {motivo}.\nDespués, {app} se cerrará. Guarda lo que estés haciendo."
        ),
        (_, _, Idioma::EnUs) => format!(
            "{quando} left: {motivo}.\nAfter that, the session will be locked. Save what you are doing."
        ),
        (_, _, Idioma::PtBr) => format!(
            "Faltam {quando}: {motivo}.\nDepois disso, a sessão será bloqueada. Salve o que estiver fazendo."
        ),
        (_, _, Idioma::EsEs) => format!(
            "Faltan {quando}: {motivo}.\nDespués, la sesión se bloqueará. Guarda lo que estés haciendo."
        ),
    };
    (titulo(idioma).into(), corpo)
}
