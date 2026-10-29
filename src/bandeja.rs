//! Textos da bandeja (ADR-004, E-004 F3): o que o ícone mostra e o que cada aviso diz, nos três
//! idiomas. Puros, para serem testados fora do Windows; quem desenha é o `kiosk-ui.exe`, no idioma
//! da conta da criança (é ele que roda na sessão dela).

use crate::agente::Aviso;
use crate::i18n::Idioma;
use crate::ipc::DaUi;
use crate::motor::{AVISOS_SEGUNDOS, Motivo};
use crate::textos;

/// O texto da dica do ícone cabe em 127 caracteres (`NOTIFYICONDATAW::szTip` tem 128 com o zero).
pub const MAX_DICA: usize = 127;

/// A dica do ícone: o tempo restante de hoje, ou que não há limite valendo.
pub fn dica(restante_segundos: Option<u64>, idioma: Idioma) -> String {
    let titulo = textos::titulo(idioma);
    let texto = match (restante_segundos, idioma) {
        (None, Idioma::EnUs) => format!("{titulo}: no limit right now"),
        (None, Idioma::PtBr) => format!("{titulo}: sem limite agora"),
        (None, Idioma::EsEs) => format!("{titulo}: sin límite ahora"),
        (Some(s), Idioma::EnUs) => format!("{titulo}: {} left", textos::duracao(s)),
        (Some(s), Idioma::PtBr) => format!("{titulo}: restam {}", textos::duracao(s)),
        (Some(s), Idioma::EsEs) => format!("{titulo}: quedan {}", textos::duracao(s)),
    };
    texto.chars().take(MAX_DICA).collect()
}

/// Título e texto do aviso que a UI mostra. `None` se o motivo não é um dos conhecidos: a UI não
/// inventa texto para o que o serviço mandou fora do esquema.
pub fn texto_do_aviso(
    restante_segundos: u64,
    motivo: &str,
    app: Option<&str>,
    idioma: Idioma,
) -> Option<(String, String)> {
    let aviso = Aviso {
        sid: String::new(),
        app: app.map(str::to_owned),
        restante_segundos,
        motivo: Motivo::de_codigo(motivo)?,
    };
    Some(textos::aviso(&aviso, idioma))
}

/// O marco de aviso (300 ou 60 segundos) a que um aviso com `restante_segundos` pertence.
pub fn marco_do_aviso(restante_segundos: u64) -> u64 {
    if restante_segundos > AVISOS_SEGUNDOS[1] {
        AVISOS_SEGUNDOS[0]
    } else {
        AVISOS_SEGUNDOS[1]
    }
}

/// A confirmação que a UI manda ao serviço depois de mostrar o aviso.
pub fn confirmacao(restante_segundos: u64) -> DaUi {
    DaUi::AvisoExibido {
        marco: marco_do_aviso(restante_segundos),
    }
}
