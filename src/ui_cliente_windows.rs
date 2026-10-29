//! O lado da UI do pipe `kiosk-ui` (ADR-004): conecta ao serviço, manda batimento e entrega os
//! avisos e o estado que o serviço empurra. Roda dentro do `kiosk-ui.exe`, na conta da criança.
//!
//! Sem o serviço (ainda subindo, ou reiniciando), espera e tenta de novo para sempre: a UI nunca
//! desiste, porque quem decide que algo está errado é o serviço, pelo batimento que deixa de chegar.

use std::fs::File;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::IntoRawHandle;
use std::time::Duration;

use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::net::windows::named_pipe::NamedPipeClient;
use tokio::sync::mpsc::UnboundedReceiver;
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OVERLAPPED, SECURITY_IDENTIFICATION};

use crate::auditoria_windows::DIREITOS_DO_CLIENTE;
use crate::ipc::{DaUi, ParaUi, codificar_da_ui, decodificar_para_ui};
use crate::ui_windows::Linhas;

/// Quanto a UI espera entre uma tentativa de conexão e a seguinte.
const ESPERA_PARA_RECONECTAR: Duration = Duration::from_secs(2);
/// Quanto a UI espera o serviço aceitar uma escrita.
const PRAZO_DA_ESCRITA: Duration = Duration::from_secs(2);

/// Abre o pipe com só os direitos que a DACL concede e o nível de personificação mínimo: o serviço
/// precisa saber quem somos, não agir em nosso nome. Aberto para E/S assíncrona.
fn abrir(nome: &str) -> std::io::Result<NamedPipeClient> {
    let arquivo: File = std::fs::OpenOptions::new()
        .access_mode(DIREITOS_DO_CLIENTE)
        .security_qos_flags(SECURITY_IDENTIFICATION)
        .custom_flags(FILE_FLAG_OVERLAPPED)
        .open(nome)?;
    // SAFETY: o handle é de um pipe aberto com FILE_FLAG_OVERLAPPED e passa a ser do cliente.
    unsafe { NamedPipeClient::from_raw_handle(arquivo.into_raw_handle()) }
}

/// Mantém a conexão com o serviço para sempre. Cada mensagem do serviço vai para `entregar`;
/// cada mensagem em `confirmacoes` (aviso exibido) vai ao serviço. `batimento` é o intervalo do
/// batimento (5 s em produção, menor nos testes).
pub async fn manter(
    nome_do_pipe: &str,
    batimento: Duration,
    entregar: impl Fn(ParaUi),
    confirmacoes: &mut UnboundedReceiver<DaUi>,
) {
    loop {
        if let Ok(cliente) = abrir(nome_do_pipe) {
            sessao(cliente, batimento, &entregar, confirmacoes).await;
        }
        tokio::time::sleep(ESPERA_PARA_RECONECTAR).await;
    }
}

async fn escrever(escritor: &mut WriteHalf<NamedPipeClient>, msg: &DaUi) -> bool {
    let bytes = codificar_da_ui(msg);
    matches!(
        tokio::time::timeout(PRAZO_DA_ESCRITA, escritor.write_all(&bytes)).await,
        Ok(Ok(()))
    )
}

/// Uma conexão: acaba quando o serviço fecha ou deixa de aceitar escritas.
async fn sessao(
    cliente: NamedPipeClient,
    batimento: Duration,
    entregar: &impl Fn(ParaUi),
    confirmacoes: &mut UnboundedReceiver<DaUi>,
) {
    let (mut leitor, mut escritor): (ReadHalf<NamedPipeClient>, WriteHalf<NamedPipeClient>) =
        tokio::io::split(cliente);
    // a primeira mensagem é o batimento: é com ela que o serviço descobre quem somos
    if !escrever(&mut escritor, &DaUi::Batimento {}).await {
        return;
    }
    let mut relogio = tokio::time::interval(batimento);
    relogio.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    relogio.tick().await; // o primeiro tick é imediato; o batimento inicial já foi
    let mut linhas = Linhas::default();
    loop {
        tokio::select! {
            _ = relogio.tick() => {
                if !escrever(&mut escritor, &DaUi::Batimento {}).await {
                    return;
                }
            }
            linha = linhas.proxima(&mut leitor) => match linha {
                Ok(Some(l)) => {
                    // linha fora do esquema: ignora, a UI não mostra o que não entende
                    if let Ok(msg) = decodificar_para_ui(&l) {
                        entregar(msg);
                    }
                }
                _ => return, // o serviço fechou, ou a linha passou do limite
            },
            Some(conf) = confirmacoes.recv() => {
                if !escrever(&mut escritor, &conf).await {
                    return;
                }
            }
        }
    }
}
