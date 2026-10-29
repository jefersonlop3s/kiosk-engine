//! Versão e informações da compilação, iguais em qualquer plataforma. A primeira linha de
//! `kioskd version` tem formato fixo (`kioskd <versão> (<commit>)`), para o `kioskd status` e
//! outros programas lerem a versão do executável instalado.

use crate::i18n::Msg;

/// O que foi gravado no binário quando ele foi compilado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Build {
    pub versao: &'static str,
    pub commit: &'static str,
    pub data: &'static str,
    pub alvo: &'static str,
    pub perfil: &'static str,
}

/// A compilação deste executável.
pub const fn atual() -> Build {
    Build {
        versao: env!("CARGO_PKG_VERSION"),
        commit: env!("KIOSK_BUILD_COMMIT"),
        data: env!("KIOSK_BUILD_DATE"),
        alvo: env!("KIOSK_BUILD_TARGET"),
        perfil: env!("KIOSK_BUILD_PROFILE"),
    }
}

impl Build {
    /// `0.0.6 (a1b2c3d)`: o que as mensagens curtas mostram.
    pub fn curta(&self) -> String {
        format!("{} ({})", self.versao, self.commit)
    }

    /// A saída de `<programa> version`: linha fixa, depois os campos no idioma de quem lê.
    pub fn completa(&self, programa: &str) -> String {
        [
            format!("{programa} {}", self.curta()),
            Msg::LblCommit {
                v: self.commit.into(),
            }
            .to_string(),
            Msg::LblDataDaBuild {
                v: self.data.into(),
            }
            .to_string(),
            Msg::LblAlvo {
                v: self.alvo.into(),
            }
            .to_string(),
            Msg::LblPerfilDaBuild {
                v: self.perfil.into(),
            }
            .to_string(),
        ]
        .join("\n")
    }
}

/// Lê a versão da primeira linha de `<programa> version` (`kioskd 0.0.6 (a1b2c3d)` vira
/// `0.0.6 (a1b2c3d)`). `None` se a linha não tem o formato.
pub fn da_primeira_linha(saida: &str, programa: &str) -> Option<String> {
    let linha = saida.lines().next()?.trim();
    let resto = linha.strip_prefix(programa)?.strip_prefix(' ')?;
    (!resto.is_empty()).then(|| resto.to_owned())
}
