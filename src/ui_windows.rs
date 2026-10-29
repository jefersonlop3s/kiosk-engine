//! Pipe `kiosk-ui` (ADR-004): o serviço (SYSTEM) fala com o `kiosk-ui.exe` que roda na sessão de
//! cada criança, para mostrar o tempo restante e os avisos e saber que a UI está viva.
//!
//! A UI roda na conta da criança, então é **entrada hostil**: uma linha JSON curta de esquema
//! fechado ([`crate::ipc`]), com prazo, limite de tamanho e de taxa. A identidade de quem conectou
//! vem do **token da conexão** e do **executável do processo**, nunca da mensagem: só o
//! `kiosk-ui.exe` instalado vale como UI (a conta da criança não o troca), e cada UI só recebe as
//! mensagens do próprio SID.
//!
//! Desvio da ADR-004, que previa um pipe por sessão com o SID do perfil na DACL: um pipe só, com a
//! mesma DACL do pipe de auditoria, e o roteamento pelo SID do token. A criança não ganha nada em
//! conectar a mais: só recebe o que é dela e não comanda nada.

use std::collections::HashMap;
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::mpsc;
use windows_sys::Win32::Security::SECURITY_ATTRIBUTES;

use crate::auditoria::{Admissao, Limitador};
use crate::database_schema::{ActionType, EventType, SecurityEvent, TransactionStatus};
use crate::db_manager::KioskDbManager;
use crate::detalhes::Detalhe;
use crate::ipc::{
    DaUi, LimiteDeTaxa, MAX_MENSAGEM_BYTES, ParaUi, codificar_para_ui, decodificar_da_ui,
};
use crate::servico_windows::Descritor;
use crate::sistema_windows::{identidade_do_cliente_do_pipe, imagem_do_cliente_do_pipe};

pub const NOME_DO_PIPE: &str = r"\\.\pipe\kiosk-ui";

/// Mesma DACL do pipe de auditoria: SYSTEM e Administradores com controle total; os demais
/// usuários autenticados leem e escrevem dados, sem `FILE_CREATE_PIPE_INSTANCE`.
const SDDL_PIPE: &str = "D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;0x12018b;;;AU)";
/// Uma UI por sessão de criança, com folga.
const MAX_INSTANCIAS: usize = 32;
/// A UI manda um batimento logo ao conectar: sem a primeira mensagem neste prazo, é descartada.
const PRAZO_DA_PRIMEIRA: Duration = Duration::from_secs(5);
/// Quanto o serviço espera a UI aceitar uma mensagem antes de dar a conexão por perdida.
const PRAZO_DA_ESCRITA: Duration = Duration::from_secs(2);
/// Mensagens do serviço à espera de serem escritas, por UI.
const FILA: usize = 16;

/// O que o serviço sabe de uma UI conectada.
struct Entrada {
    id: u64,
    fila: mpsc::Sender<ParaUi>,
    batimento: DateTime<Utc>,
    aviso_exibido: Option<u64>,
}

/// As UIs conectadas, por SID da conta. O agente usa para empurrar avisos e conferir o batimento.
#[derive(Clone, Default)]
pub struct GerenteUi {
    entradas: Arc<Mutex<HashMap<String, Entrada>>>,
    proximo: Arc<AtomicU64>,
}

impl GerenteUi {
    fn travar(&self) -> std::sync::MutexGuard<'_, HashMap<String, Entrada>> {
        self.entradas.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// SIDs das contas com UI conectada (para diagnóstico).
    pub fn contas(&self) -> Vec<String> {
        self.travar().keys().cloned().collect()
    }

    /// Há uma UI conectada para esta conta.
    pub fn conectada(&self, sid: &str) -> bool {
        self.travar().contains_key(sid)
    }

    /// Entrega `msg` à UI da conta. `false` se não há UI conectada ou a fila dela está cheia:
    /// quem chama decide o que fazer (por exemplo, cair na caixa de mensagem do Windows).
    pub fn enviar(&self, sid: &str, msg: ParaUi) -> bool {
        self.travar()
            .get(sid)
            .is_some_and(|e| e.fila.try_send(msg).is_ok())
    }

    /// Quando a UI da conta mandou o último batimento.
    pub fn ultimo_batimento(&self, sid: &str) -> Option<DateTime<Utc>> {
        self.travar().get(sid).map(|e| e.batimento)
    }

    /// O último marco de aviso (300 ou 60) que a UI da conta confirmou ter exibido.
    pub fn ultimo_aviso_exibido(&self, sid: &str) -> Option<u64> {
        self.travar().get(sid).and_then(|e| e.aviso_exibido)
    }

    /// Uma UI nova da mesma conta substitui a anterior (a anterior é largada e fecha sozinha).
    fn registrar(&self, sid: &str, fila: mpsc::Sender<ParaUi>, agora: DateTime<Utc>) -> u64 {
        let id = self.proximo.fetch_add(1, Ordering::Relaxed);
        self.travar().insert(
            sid.to_owned(),
            Entrada {
                id,
                fila,
                batimento: agora,
                aviso_exibido: None,
            },
        );
        id
    }

    fn remover(&self, sid: &str, id: u64) {
        let mut m = self.travar();
        if m.get(sid).is_some_and(|e| e.id == id) {
            m.remove(sid);
        }
    }

    fn bater(&self, sid: &str, id: u64, agora: DateTime<Utc>) {
        if let Some(e) = self.travar().get_mut(sid).filter(|e| e.id == id) {
            e.batimento = agora;
        }
    }

    fn exibido(&self, sid: &str, id: u64, marco: u64) {
        if let Some(e) = self.travar().get_mut(sid).filter(|e| e.id == id) {
            e.aviso_exibido = Some(marco);
        }
    }
}

/// Como o servidor escolhe quem vale como UI.
#[derive(Clone)]
pub struct Configuracao {
    pub nome_do_pipe: String,
    /// Executável que o processo do outro lado tem de ser (`kiosk-ui.exe` instalado). `None` só nos
    /// testes, que conectam de dentro do próprio executável de teste.
    pub imagem_esperada: Option<PathBuf>,
}

impl Configuracao {
    /// A de produção: o `kiosk-ui.exe` ao lado do executável do serviço.
    pub fn instalada(pasta_do_programa: &Path) -> Self {
        Self {
            nome_do_pipe: NOME_DO_PIPE.to_owned(),
            imagem_esperada: Some(pasta_do_programa.join("kiosk-ui.exe")),
        }
    }
}

/// Atributos de segurança do pipe: o ponteiro aponta para o descritor, que vive junto.
struct Atributos {
    attrs: SECURITY_ATTRIBUTES,
    _descritor: Descritor,
}

// SAFETY: o descritor é imutável depois de criado e vive tanto quanto os atributos; o ponteiro só
// é entregue ao Windows na criação de cada instância do pipe.
unsafe impl Send for Atributos {}

/// Escuta o pipe até o processo acabar. Erro só se a primeira instância não puder ser criada.
pub async fn servir(
    config: Configuracao,
    gerente: GerenteUi,
    db: Arc<KioskDbManager>,
) -> Result<(), String> {
    let descritor = Descritor::de_sddl(SDDL_PIPE)?;
    let mut atributos = Atributos {
        attrs: SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descritor.0,
            bInheritHandle: 0,
        },
        _descritor: descritor,
    };
    let registro = Arc::new(tokio::sync::Mutex::new(Limitador::default()));
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
                &config.nome_do_pipe,
                (&mut atributos.attrs as *mut SECURITY_ATTRIBUTES).cast(),
            )
        };
        let servidor = match servidor {
            Ok(s) => s,
            Err(e) if primeira => return Err(format!("{}: {e}", config.nome_do_pipe)),
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        primeira = false;
        if servidor.connect().await.is_err() {
            continue;
        }
        let (config, gerente, db, registro) = (
            config.clone(),
            gerente.clone(),
            db.clone(),
            registro.clone(),
        );
        tokio::spawn(async move { atender(servidor, config, gerente, db, registro).await });
    }
}

/// Lê linhas de um fluxo com tamanho máximo, sem perder bytes quando a leitura é cancelada.
#[derive(Default)]
pub(crate) struct Linhas {
    buf: Vec<u8>,
}

pub(crate) enum ErroLinha {
    /// Passou de [`MAX_MENSAGEM_BYTES`] sem fim de linha.
    MuitoLonga,
    Io,
}

impl Linhas {
    /// A próxima linha (sem o `\n`); `Ok(None)` se o outro lado fechou.
    pub(crate) async fn proxima<R: AsyncRead + Unpin>(
        &mut self,
        leitor: &mut R,
    ) -> Result<Option<Vec<u8>>, ErroLinha> {
        loop {
            if let Some(i) = self.buf.iter().position(|b| *b == b'\n') {
                let mut linha: Vec<u8> = self.buf.drain(..=i).collect();
                linha.pop();
                return Ok(Some(linha));
            }
            if self.buf.len() > MAX_MENSAGEM_BYTES {
                return Err(ErroLinha::MuitoLonga);
            }
            let mut pedaco = [0u8; 512];
            let n = leitor.read(&mut pedaco).await.map_err(|_| ErroLinha::Io)?;
            if n == 0 {
                return Ok(None);
            }
            self.buf.extend_from_slice(&pedaco[..n]);
        }
    }
}

/// Registra a recusa no log forense (no máximo 5 por minuto por conta), ligada ao perfil da
/// criança quando a conta é de um perfil. Não bloqueia a conexão: ela já foi descartada.
async fn recusar(
    db: &KioskDbManager,
    registro: &tokio::sync::Mutex<Limitador>,
    sid: &str,
    tipo: EventType,
    motivo: &str,
) {
    if let Admissao::Descartado = registro.lock().await.admitir(sid, Utc::now()) {
        return;
    }
    let perfil = db
        .perfil_por_sid(sid)
        .await
        .ok()
        .flatten()
        .map(|p| p.profile_id);
    let detalhe = Detalhe::UiRejected {
        reason: motivo.to_owned(),
    };
    let e = SecurityEvent::new(
        "local",
        ActionType::System,
        tipo,
        TransactionStatus::Failure,
        &detalhe.codigo(),
    )
    .with_profile(perfil);
    if let Err(erro) = db.record_event(&e).await {
        eprintln!("kioskd: ui: {erro}");
    }
}

/// Compara dois caminhos de executável como o Windows: sem diferenciar maiúsculas, `/` e `\`.
fn mesmo_caminho(a: &Path, b: &Path) -> bool {
    let normal = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    normal(a) == normal(b)
}

async fn atender(
    mut servidor: NamedPipeServer,
    config: Configuracao,
    gerente: GerenteUi,
    db: Arc<KioskDbManager>,
    registro: Arc<tokio::sync::Mutex<Limitador>>,
) {
    let mut linhas = Linhas::default();
    // a identidade só é lida depois de o servidor ter lido do pipe: a primeira mensagem vem logo
    let primeira =
        match tokio::time::timeout(PRAZO_DA_PRIMEIRA, linhas.proxima(&mut servidor)).await {
            Ok(Ok(Some(l))) => l,
            // fechou sem enviar ou mandou lixo sem fim de linha
            Ok(_) => return,
            Err(_) => {
                eprintln!("kioskd: ui: conexão sem a primeira mensagem em {PRAZO_DA_PRIMEIRA:?}");
                return;
            }
        };
    let chamador = match identidade_do_cliente_do_pipe(&servidor) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("kioskd: ui: {e}");
            return;
        }
    };
    let sid = chamador.sid;
    // o executável: só o kiosk-ui instalado vale como UI
    if let Some(esperada) = &config.imagem_esperada {
        let confere = imagem_do_cliente_do_pipe(&servidor)
            .map(|imagem| mesmo_caminho(&imagem, esperada))
            .unwrap_or(false);
        if !confere {
            recusar(&db, &registro, &sid, EventType::AuthzFail, "WRONG_IMAGE").await;
            return;
        }
    }
    // a primeira mensagem tem de ser um batimento válido
    if !matches!(decodificar_da_ui(&primeira), Ok(DaUi::Batimento {})) {
        recusar(
            &db,
            &registro,
            &sid,
            EventType::ValidationFail,
            "INVALID_MESSAGE",
        )
        .await;
        return;
    }
    let (fila, mut recebe) = mpsc::channel::<ParaUi>(FILA);
    let id = gerente.registrar(&sid, fila, Utc::now());
    let (mut leitor, mut escritor) = tokio::io::split(servidor);
    let mut taxa = LimiteDeTaxa::default();
    loop {
        tokio::select! {
            linha = linhas.proxima(&mut leitor) => match linha {
                Ok(Some(l)) => {
                    if !taxa.permite(Utc::now()) {
                        recusar(&db, &registro, &sid, EventType::ValidationFail, "RATE_LIMIT").await;
                        break;
                    }
                    match decodificar_da_ui(&l) {
                        Ok(DaUi::Batimento {}) => gerente.bater(&sid, id, Utc::now()),
                        Ok(DaUi::AvisoExibido { marco }) => gerente.exibido(&sid, id, marco),
                        Err(_) => {
                            recusar(&db, &registro, &sid, EventType::ValidationFail, "INVALID_MESSAGE").await;
                            break;
                        }
                    }
                }
                Ok(None) | Err(ErroLinha::Io) => break,
                Err(ErroLinha::MuitoLonga) => {
                    recusar(&db, &registro, &sid, EventType::ValidationFail, "INVALID_MESSAGE").await;
                    break;
                }
            },
            msg = recebe.recv() => match msg {
                Some(m) => {
                    let bytes = codificar_para_ui(&m);
                    let escrita = escritor.write_all(&bytes);
                    if !matches!(tokio::time::timeout(PRAZO_DA_ESCRITA, escrita).await, Ok(Ok(()))) {
                        break;
                    }
                }
                None => break, // outra UI da mesma conta tomou o lugar
            },
        }
    }
    gerente.remover(&sid, id);
}
