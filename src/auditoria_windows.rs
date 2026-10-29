//! Pipe `kiosk-audit` (ADR-011): o servidor no serviço e o cliente no `kioskd`.
//!
//! O serviço (SYSTEM) escuta; qualquer conta autenticada conecta, porque é exatamente a conta da
//! criança que queremos registrar. A entrada é **hostil**: uma linha curta de esquema fechado, com
//! prazo, limite de tamanho e limite de taxa por conta, e o chamador é lido do token da conexão.

use std::io::{Read, Write};
use std::mem::size_of;
use std::os::windows::fs::OpenOptionsExt;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::Mutex;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;
use windows_sys::Win32::Storage::FileSystem::SECURITY_IDENTIFICATION;

use crate::auditoria::{
    Limitador, MAX_BYTES, NOME_DO_PIPE, montar_aviso, montar_resposta, registrar, resposta_ok,
};
use crate::db_manager::KioskDbManager;
use crate::i18n::Msg;
use crate::servico_windows::{Descritor, pid_do_servico};
use crate::sistema_windows::{identidade_do_cliente_do_pipe, pid_do_servidor_do_pipe};

/// DACL do pipe: SYSTEM e Administradores com controle total; os demais usuários autenticados só
/// leem e escrevem dados (`0x12018b` = `FILE_GENERIC_READ` + `FILE_WRITE_DATA` +
/// `FILE_WRITE_ATTRIBUTES`), **sem** `FILE_CREATE_PIPE_INSTANCE` (o mesmo bit de
/// `FILE_APPEND_DATA`, que o `FILE_GENERIC_WRITE` traria): ninguém mais cria instâncias do pipe.
const SDDL_PIPE: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12018b;;;AU)";
/// Conexões atendidas ao mesmo tempo.
const MAX_INSTANCIAS: usize = 16;
/// Quanto o serviço espera a linha de um cliente que conectou e não escreve.
const PRAZO_DO_SERVIDOR: Duration = Duration::from_secs(3);
/// Quanto o `kioskd` espera o serviço.
const PRAZO_DO_CLIENTE: Duration = Duration::from_secs(5);

/// Atributos de segurança do pipe: o ponteiro aponta para o descritor, que vive junto.
struct Atributos {
    attrs: SECURITY_ATTRIBUTES,
    _descritor: Descritor,
}

// SAFETY: o descritor é imutável depois de criado e vive tanto quanto os atributos; o ponteiro só
// é entregue ao Windows na criação de cada instância do pipe.
unsafe impl Send for Atributos {}

/// Escuta o pipe até o processo acabar. Erro só se a primeira instância não puder ser criada (por
/// exemplo, já existe outro servidor: um `kioskd serve` rodado à mão com o serviço no ar).
pub async fn servir(db: Arc<KioskDbManager>) -> Result<(), String> {
    let descritor = Descritor::de_sddl(SDDL_PIPE)?;
    let mut atributos = Atributos {
        attrs: SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descritor.0,
            bInheritHandle: 0,
        },
        _descritor: descritor,
    };
    let limitador = Arc::new(Mutex::new(Limitador::default()));
    let mut primeira = true;
    loop {
        let mut opcoes = ServerOptions::new();
        opcoes
            .first_pipe_instance(primeira)
            .reject_remote_clients(true)
            .max_instances(MAX_INSTANCIAS);
        // SAFETY: `attrs` aponta para um SECURITY_ATTRIBUTES válido, com um descritor vivo.
        let servidor = unsafe {
            opcoes.create_with_security_attributes_raw(
                NOME_DO_PIPE,
                (&mut atributos.attrs as *mut SECURITY_ATTRIBUTES).cast(),
            )
        };
        let servidor = match servidor {
            Ok(s) => s,
            Err(e) if primeira => return Err(format!("{NOME_DO_PIPE}: {e}")),
            Err(_) => {
                // todas as instâncias ocupadas: espera uma liberar
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        primeira = false;
        if servidor.connect().await.is_err() {
            continue; // o cliente desistiu antes de a conexão se completar
        }
        let (db, limitador) = (db.clone(), limitador.clone());
        tokio::spawn(async move { atender(servidor, db, limitador).await });
    }
}

/// Lê até o fim da linha, ou um pouco além do máximo (a validação recusa o excesso).
async fn ler_linha(s: &mut NamedPipeServer) -> std::io::Result<Vec<u8>> {
    let mut linha = Vec::with_capacity(128);
    let mut pedaco = [0u8; 128];
    loop {
        let n = s.read(&mut pedaco).await?;
        if n == 0 {
            break;
        }
        linha.extend_from_slice(&pedaco[..n]);
        if linha.contains(&b'\n') || linha.len() > MAX_BYTES {
            break;
        }
    }
    if let Some(i) = linha.iter().position(|b| *b == b'\n') {
        linha.truncate(i);
    }
    Ok(linha)
}

async fn atender(
    mut s: NamedPipeServer,
    db: Arc<KioskDbManager>,
    limitador: Arc<Mutex<Limitador>>,
) {
    let Ok(Ok(linha)) = tokio::time::timeout(PRAZO_DO_SERVIDOR, ler_linha(&mut s)).await else {
        let _ = s.disconnect(); // cliente lento ou quebrado: sem resposta
        return;
    };
    // só depois de ler do pipe a personificação do cliente é permitida; é síncrono, sem `await`
    // no meio, então fica na mesma thread
    let ok = match identidade_do_cliente_do_pipe(&s) {
        Ok(chamador) => {
            let mut l = limitador.lock().await;
            match registrar(&db, &mut l, &chamador, &linha, Utc::now()).await {
                Ok(_) => true,
                Err(e) => {
                    eprintln!("kioskd: auditoria: {e}");
                    false
                }
            }
        }
        Err(e) => {
            eprintln!("kioskd: auditoria: {e}");
            false
        }
    };
    let _ = s.write_all(montar_resposta(ok).as_bytes()).await;
    let _ = s.write_all(b"\n").await;
    let _ = s.flush().await;
    let _ = s.disconnect();
}

/// Abre o pipe, com algumas tentativas se todas as instâncias estiverem ocupadas.
fn abrir() -> Result<std::fs::File, String> {
    const ERROR_PIPE_BUSY: i32 = 231;
    let mut ultimo = String::new();
    for _ in 0..10 {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            // o serviço só precisa saber quem somos, não agir em nosso nome
            .security_qos_flags(SECURITY_IDENTIFICATION)
            .open(NOME_DO_PIPE)
        {
            Ok(f) => return Ok(f),
            Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                ultimo = e.to_string();
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Msg::ServicoNaoResponde.to_string());
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(ultimo)
}

fn trocar(linha: &str) -> Result<(), String> {
    let mut f = abrir()?;
    // só entrega a linha se quem serve o pipe for o processo do serviço: um processo que ocupou o
    // nome antes não recebe nada (conta comum não abre o processo SYSTEM, então a conferência é
    // pelo PID que o gerenciador de serviços informa, e qualquer conta o consulta)
    let servidor = pid_do_servidor_do_pipe(&f)?;
    match pid_do_servico()? {
        Some(pid) if pid == servidor => {}
        Some(_) => return Err(Msg::PipeNaoEDoServico.to_string()),
        None => return Err(Msg::ServicoNaoResponde.to_string()),
    }
    f.write_all(linha.as_bytes()).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())?;
    let mut resposta = Vec::new();
    let mut byte = [0u8; 1];
    while resposta.len() < 64 {
        match f.read(&mut byte) {
            Ok(1) if byte[0] != b'\n' => resposta.push(byte[0]),
            _ => break,
        }
    }
    if resposta_ok(&String::from_utf8_lossy(&resposta)) {
        Ok(())
    } else {
        Err(Msg::ServicoNaoGravou.to_string())
    }
}

/// O `kioskd` avisa o serviço de que `comando` foi tentado sem elevação. `Ok` só se o serviço
/// confirmou que gravou o evento; qualquer outra coisa vira o motivo de **não** ter sido registrado.
pub fn avisar_tentativa(comando: &str) -> Result<(), String> {
    let linha = format!("{}\n", montar_aviso(comando));
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(trocar(&linha));
    });
    rx.recv_timeout(PRAZO_DO_CLIENTE)
        .unwrap_or_else(|_| Err(Msg::ServicoNaoResponde.to_string()))
}
