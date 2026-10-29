//! `kiosk-ui`: a bandeja que a criança vê (ADR-001, ADR-004). Roda na sessão dela, conecta ao
//! serviço pelo pipe `kiosk-ui`, mostra o tempo restante na dica do ícone e os avisos de 5 e 1
//! minuto, e confirma ao serviço o que mostrou. Não tem menu, janela nem "sair": o serviço a inicia
//! e a relança se ela morrer, e o batimento que deixa de chegar vira alerta para os pais.
//!
//! Sem console (subsistema Windows): quem a vê é a criança, e uma janela de terminal piscando não
//! ajuda ninguém. Falhas aparecem pelo código de saída e pelo batimento que some.
//!
//! Variáveis, só para diagnóstico e testes:
//! - `KIOSK_UI_PIPE`: outro nome para o pipe (o serviço só aceita o pipe e o `kiosk-ui.exe`
//!   instalados; isto não dá poder a ninguém);
//! - `KIOSK_UI_SEM_BANDEJA=1` com `KIOSK_UI_SAIDA=arquivo`: no lugar do ícone, escreve no arquivo o
//!   que mostraria (uma linha por dica ou aviso) e confirma como se tivesse mostrado.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

#[cfg(not(windows))]
fn main() -> ExitCode {
    eprintln!("kiosk-ui: {}", kiosk_engine::i18n::Msg::SoNoWindows);
    ExitCode::FAILURE
}

#[cfg(windows)]
fn main() -> ExitCode {
    windows::executar()
}

#[cfg(windows)]
mod windows {
    use std::io::Write;
    use std::process::ExitCode;
    use std::time::Duration;

    use kiosk_engine::bandeja::{confirmacao, dica, texto_do_aviso};
    use kiosk_engine::bandeja_windows::{Bandeja, Comando};
    use kiosk_engine::i18n::idioma;
    use kiosk_engine::ipc::{DaUi, INTERVALO_BATIMENTO_SEGUNDOS, ParaUi};
    use kiosk_engine::textos;
    use kiosk_engine::ui_cliente_windows::manter;
    use kiosk_engine::ui_windows::NOME_DO_PIPE;
    use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

    /// Onde o que a UI mostra vai parar: o ícone da bandeja, ou um arquivo (testes).
    enum Tela {
        Icone(kiosk_engine::bandeja_windows::Remetente),
        Arquivo(std::path::PathBuf, UnboundedSender<DaUi>),
    }

    impl Tela {
        fn dica(&self, texto: String) {
            match self {
                Self::Icone(r) => r.enviar(Comando::Dica(texto)),
                Self::Arquivo(caminho, _) => anotar(caminho, &format!("DICA\t{texto}")),
            }
        }

        fn aviso(&self, titulo: String, corpo: String, conf: DaUi) {
            match self {
                // o ícone confirma sozinho, depois de o Windows aceitar o balão
                Self::Icone(r) => r.enviar(Comando::Balao {
                    titulo,
                    corpo,
                    confirmacao: conf,
                }),
                Self::Arquivo(caminho, confirmar) => {
                    anotar(
                        caminho,
                        &format!("AVISO\t{titulo}\t{}", corpo.replace('\n', " ")),
                    );
                    let _ = confirmar.send(conf);
                }
            }
        }
    }

    fn anotar(caminho: &std::path::Path, linha: &str) {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(caminho)
        {
            let _ = writeln!(f, "{linha}");
        }
    }

    /// O que fazer com cada mensagem do serviço.
    fn tratar(tela: &Tela, msg: ParaUi) {
        let idioma = idioma();
        match msg {
            ParaUi::Estado {
                restante_segundos, ..
            } => tela.dica(dica(restante_segundos, idioma)),
            ParaUi::Aviso {
                restante_segundos,
                motivo,
                app,
            } => {
                if let Some((titulo, corpo)) =
                    texto_do_aviso(restante_segundos, &motivo, app.as_deref(), idioma)
                {
                    tela.aviso(titulo, corpo, confirmacao(restante_segundos));
                }
            }
        }
    }

    /// A thread da conexão: um runtime só para o laço de `manter`.
    fn conexao(
        tela: Tela,
        mut confirmacoes: UnboundedReceiver<DaUi>,
    ) -> std::thread::JoinHandle<()> {
        let pipe = std::env::var("KIOSK_UI_PIPE").unwrap_or_else(|_| NOME_DO_PIPE.to_owned());
        std::thread::spawn(move || {
            let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            rt.block_on(manter(
                &pipe,
                Duration::from_secs(INTERVALO_BATIMENTO_SEGUNDOS as u64),
                |msg| tratar(&tela, msg),
                &mut confirmacoes,
            ));
        })
    }

    pub fn executar() -> ExitCode {
        let (confirmar, confirmacoes) = unbounded_channel();
        if std::env::var_os("KIOSK_UI_SEM_BANDEJA").is_some() {
            let Some(saida) = std::env::var_os("KIOSK_UI_SAIDA") else {
                return ExitCode::from(2);
            };
            let tela = Tela::Arquivo(saida.into(), confirmar);
            let _ = conexao(tela, confirmacoes).join();
            return ExitCode::SUCCESS;
        }
        let bandeja = match Bandeja::criar(textos::titulo(idioma()), confirmar) {
            Ok(b) => b,
            Err(_) => return ExitCode::from(2),
        };
        conexao(Tela::Icone(bandeja.remetente()), confirmacoes);
        bandeja.executar();
        ExitCode::SUCCESS
    }
}
