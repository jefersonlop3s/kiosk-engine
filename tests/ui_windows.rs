//! Pipe `kiosk-ui` (ADR-004) de ponta a ponta no Windows: a UI conecta, manda batimento, recebe
//! avisos; a entrada hostil é descartada e registrada; só o executável esperado vale como UI.
#![cfg(windows)]

mod comum;

use std::fs::File;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::IntoRawHandle;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use comum::{ContaPadrao, como};
use kiosk_engine::auditoria_windows::DIREITOS_DO_CLIENTE;
use kiosk_engine::db_manager::KioskDbManager;
use kiosk_engine::detalhes::Detalhe;
use kiosk_engine::ipc::ParaUi;
use kiosk_engine::ui_windows::{Configuracao, GerenteUi, servir};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::windows::named_pipe::NamedPipeClient;
use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OVERLAPPED, SECURITY_IDENTIFICATION};

const BATIMENTO: &str = "{\"v\":1,\"tipo\":\"batimento\"}\n";

struct Servidor {
    _dir: TempDir,
    db: Arc<KioskDbManager>,
    gerente: GerenteUi,
    nome: String,
    tarefa: tokio::task::JoinHandle<()>,
}

impl Drop for Servidor {
    fn drop(&mut self) {
        self.tarefa.abort();
    }
}

async fn subir(imagem_esperada: Option<PathBuf>) -> Servidor {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
            .await
            .unwrap(),
    );
    let gerente = GerenteUi::default();
    let nome = format!(r"\\.\pipe\kiosk-ui-teste-{}", uuid::Uuid::new_v4().simple());
    let config = Configuracao {
        nome_do_pipe: nome.clone(),
        imagem_esperada,
    };
    let (g, d) = (gerente.clone(), db.clone());
    let tarefa = tokio::spawn(async move {
        let _ = servir(config, g, d).await;
    });
    Servidor {
        _dir: dir,
        db,
        gerente,
        nome,
        tarefa,
    }
}

enum Chegou {
    Linha(String),
    Fechou,
    Silencio,
}

/// Cliente do pipe como o `kiosk-ui` o abriria: só os direitos que a DACL concede.
///
/// Aberto com `FILE_FLAG_OVERLAPPED`: num handle síncrono, uma leitura pendente trava qualquer
/// escrita no mesmo handle, e o teste ficaria esperando para sempre.
struct Cliente {
    leitor: BufReader<tokio::io::ReadHalf<NamedPipeClient>>,
    escritor: tokio::io::WriteHalf<NamedPipeClient>,
}

impl Cliente {
    /// Abre o pipe (com algumas tentativas se o servidor ainda está recriando a instância).
    fn abrir(nome: &str) -> std::io::Result<File> {
        let fim = Instant::now() + Duration::from_secs(10);
        loop {
            match std::fs::OpenOptions::new()
                .access_mode(DIREITOS_DO_CLIENTE)
                // o serviço só precisa saber quem somos; sem a QoS, o Windows nem usa o token da
                // thread personificada (a conta padrão do teste) e entrega o do processo
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .custom_flags(FILE_FLAG_OVERLAPPED)
                .open(nome)
            {
                Ok(f) => return Ok(f),
                // 231: todas as instâncias ocupadas (o servidor recria a próxima logo em seguida)
                Err(e)
                    if (e.raw_os_error() == Some(231)
                        || e.kind() == std::io::ErrorKind::NotFound)
                        && Instant::now() < fim =>
                {
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e),
            }
        }
    }

    /// Entrega o handle ao tokio. Tem de ser chamado dentro do runtime.
    fn de_arquivo(arquivo: File) -> std::io::Result<Self> {
        // SAFETY: o handle é de um pipe aberto com FILE_FLAG_OVERLAPPED e passa a ser do cliente.
        let cliente = unsafe { NamedPipeClient::from_raw_handle(arquivo.into_raw_handle())? };
        let (leitor, escritor) = tokio::io::split(cliente);
        Ok(Self {
            leitor: BufReader::new(leitor),
            escritor,
        })
    }

    async fn conectar(nome: &str) -> std::io::Result<Self> {
        Self::de_arquivo(Self::abrir(nome)?)
    }

    async fn enviar(&mut self, texto: &str) -> std::io::Result<()> {
        self.escritor.write_all(texto.as_bytes()).await?;
        self.escritor.flush().await
    }

    async fn proxima(&mut self, prazo: Duration) -> Chegou {
        let mut l = String::new();
        match tokio::time::timeout(prazo, self.leitor.read_line(&mut l)).await {
            Err(_) => Chegou::Silencio,
            Ok(Ok(0)) | Ok(Err(_)) => Chegou::Fechou,
            Ok(Ok(_)) => Chegou::Linha(l),
        }
    }
}

async fn esperar(mut cond: impl FnMut() -> bool) -> bool {
    let fim = Instant::now() + Duration::from_secs(10);
    while Instant::now() < fim {
        if cond() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

fn sid_atual() -> String {
    let saida = std::process::Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&saida.stdout)
        .trim()
        .split(',')
        .nth(1)
        .unwrap()
        .trim_matches('"')
        .to_owned()
}

/// Eventos de recusa da UI no log: (tipo, motivo).
async fn recusas(db: &KioskDbManager) -> Vec<(String, String)> {
    let linhas: Vec<(String, String)> = sqlx::query_as(
        "SELECT event_type, rejection_reason_detail FROM security_log \
         WHERE rejection_reason_detail LIKE '{\"code\":\"UI_REJECTED\"%' ORDER BY rowid",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    linhas
        .into_iter()
        .map(|(tipo, detalhe)| match Detalhe::de_texto(&detalhe) {
            Some(Detalhe::UiRejected { reason }) => (tipo, reason),
            outro => panic!("detalhe inesperado: {outro:?}"),
        })
        .collect()
}

fn aviso() -> ParaUi {
    ParaUi::Aviso {
        restante_segundos: 300,
        motivo: "APP_DAILY_LIMIT".into(),
        app: Some("jogo.exe".into()),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_ui_conecta_manda_batimento_e_recebe_aviso() {
    let s = subir(None).await;
    let sid = sid_atual();
    assert!(!s.gerente.conectada(&sid));
    assert!(!s.gerente.enviar(&sid, aviso()), "sem UI, nada é entregue");

    let mut c = Cliente::conectar(&s.nome).await.unwrap();
    c.enviar(BATIMENTO).await.unwrap();
    assert!(esperar(|| s.gerente.conectada(&sid)).await);
    let primeiro = s.gerente.ultimo_batimento(&sid).unwrap();

    // serviço -> UI
    assert!(s.gerente.enviar(&sid, aviso()));
    let Chegou::Linha(l) = c.proxima(Duration::from_secs(5)).await else {
        panic!("o aviso não chegou");
    };
    assert!(l.starts_with("{\"v\":1,\"tipo\":\"aviso\""), "{l}");
    assert!(l.contains("\"restante_segundos\":300"), "{l}");

    // UI -> serviço: batimento e confirmação do aviso exibido
    tokio::time::sleep(Duration::from_millis(30)).await;
    c.enviar(BATIMENTO).await.unwrap();
    c.enviar("{\"v\":1,\"tipo\":\"aviso_exibido\",\"marco\":300}\n")
        .await
        .unwrap();
    assert!(esperar(|| s.gerente.ultimo_aviso_exibido(&sid) == Some(300)).await);
    assert!(s.gerente.ultimo_batimento(&sid).unwrap() >= primeiro);

    // fechou: sai da lista
    drop(c);
    assert!(esperar(|| !s.gerente.conectada(&sid)).await);
    assert!(recusas(&s.db).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_primeira_mensagem_tem_de_ser_um_batimento() {
    let s = subir(None).await;
    for lixo in [
        "{\"v\":1,\"tipo\":\"aviso_exibido\",\"marco\":300}\n", // válida, mas não é batimento
        "isto nao e json\n",
        "{\"v\":2,\"tipo\":\"batimento\"}\n", // versão errada
        "{\"v\":1,\"tipo\":\"batimento\",\"extra\":1}\n", // campo a mais
    ] {
        let mut c = Cliente::conectar(&s.nome).await.unwrap();
        c.enviar(lixo).await.unwrap();
        assert!(
            matches!(c.proxima(Duration::from_secs(5)).await, Chegou::Fechou),
            "devia ter sido descartada: {lixo}"
        );
        assert!(!s.gerente.conectada(&sid_atual()));
    }
    // a cota é de 5 por minuto por conta: os 4 vieram
    let r = recusas(&s.db).await;
    assert_eq!(r.len(), 4, "{r:?}");
    assert!(
        r.iter()
            .all(|(t, m)| t == "VALIDATION_FAIL" && m == "INVALID_MESSAGE")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lixo_depois_do_batimento_derruba_a_ui() {
    let s = subir(None).await;
    let sid = sid_atual();
    for (lixo, nome) in [
        (
            "{\"v\":1,\"tipo\":\"aviso_exibido\",\"marco\":7}\n".to_owned(),
            "marco inválido",
        ),
        (
            "{\"v\":1,\"tipo\":\"comando\",\"fazer\":\"x\"}\n".to_owned(),
            "tipo desconhecido",
        ),
        ("x".repeat(5000), "linha enorme sem fim"),
    ] {
        let mut c = Cliente::conectar(&s.nome).await.unwrap();
        c.enviar(BATIMENTO).await.unwrap();
        assert!(esperar(|| s.gerente.conectada(&sid)).await, "{nome}");
        c.enviar(&lixo).await.unwrap();
        assert!(
            matches!(c.proxima(Duration::from_secs(5)).await, Chegou::Fechou),
            "{nome}: devia ter caído"
        );
        assert!(esperar(|| !s.gerente.conectada(&sid)).await, "{nome}");
    }
    assert_eq!(recusas(&s.db).await.len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rajada_de_mensagens_derruba_a_ui() {
    let s = subir(None).await;
    let sid = sid_atual();
    let mut c = Cliente::conectar(&s.nome).await.unwrap();
    c.enviar(BATIMENTO).await.unwrap();
    assert!(esperar(|| s.gerente.conectada(&sid)).await);
    // o limite é de 10 por segundo
    let rajada = BATIMENTO.repeat(60);
    let _ = c.enviar(&rajada).await;
    assert!(
        matches!(c.proxima(Duration::from_secs(5)).await, Chegou::Fechou),
        "a rajada devia derrubar a UI"
    );
    assert!(esperar(|| !s.gerente.conectada(&sid)).await);
    let r = recusas(&s.db).await;
    assert_eq!(r, [("VALIDATION_FAIL".to_owned(), "RATE_LIMIT".to_owned())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn so_o_executavel_esperado_vale_como_ui() {
    let sid = sid_atual();
    // outro programa: recusado e registrado
    let s = subir(Some(PathBuf::from(r"C:\Windows\System32\notepad.exe"))).await;
    let mut c = Cliente::conectar(&s.nome).await.unwrap();
    c.enviar(BATIMENTO).await.unwrap();
    assert!(matches!(
        c.proxima(Duration::from_secs(5)).await,
        Chegou::Fechou
    ));
    assert!(!s.gerente.conectada(&sid));
    assert_eq!(
        recusas(&s.db).await,
        [("AUTHZ_FAIL".to_owned(), "WRONG_IMAGE".to_owned())]
    );

    // o executável certo (este mesmo, com o caminho em outra caixa e com barras): aceito
    let este = std::env::current_exe().unwrap();
    let outra_caixa = PathBuf::from(este.to_string_lossy().to_uppercase().replace('\\', "/"));
    let s = subir(Some(outra_caixa)).await;
    let mut c = Cliente::conectar(&s.nome).await.unwrap();
    c.enviar(BATIMENTO).await.unwrap();
    assert!(esperar(|| s.gerente.conectada(&sid)).await);
    assert!(recusas(&s.db).await.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uma_ui_nova_da_mesma_conta_toma_o_lugar_da_anterior() {
    let s = subir(None).await;
    let sid = sid_atual();
    let mut velha = Cliente::conectar(&s.nome).await.unwrap();
    velha.enviar(BATIMENTO).await.unwrap();
    assert!(esperar(|| s.gerente.conectada(&sid)).await);

    let mut nova = Cliente::conectar(&s.nome).await.unwrap();
    nova.enviar(BATIMENTO).await.unwrap();
    assert!(
        matches!(velha.proxima(Duration::from_secs(5)).await, Chegou::Fechou),
        "a UI anterior devia ser largada"
    );
    // a nova segue de pé e recebe
    assert!(s.gerente.conectada(&sid));
    assert!(s.gerente.enviar(&sid, aviso()));
    assert!(matches!(
        nova.proxima(Duration::from_secs(5)).await,
        Chegou::Linha(_)
    ));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conta_padrao_conecta_e_so_recebe_o_que_e_dela() {
    // cria uma conta local de verdade: só no runner descartável, como o teste do serviço
    if std::env::var_os("KIOSK_TESTE_SERVICO").is_none() {
        eprintln!("KIOSK_TESTE_SERVICO não definida: pulando a conta padrão");
        return;
    }
    let s = subir(None).await;
    let conta = ContaPadrao::com_nome("kioskui");
    let sid_conta = kiosk_engine::sistema_windows::sid_de_usuario(&conta.nome).unwrap();
    let sid_runner = sid_atual();
    assert_ne!(sid_conta, sid_runner);

    let nome = s.nome.clone();
    let arquivo = como(&conta, || Cliente::abrir(&nome)).expect("a conta padrão devia conectar");
    let mut c = Cliente::de_arquivo(arquivo).unwrap();
    c.enviar(BATIMENTO).await.unwrap();
    assert!(
        esperar(|| s.gerente.conectada(&sid_conta)).await,
        "esperava {sid_conta}; conectadas: {:?}; recusas: {:?}",
        s.gerente.contas(),
        recusas(&s.db).await
    );
    assert!(
        !s.gerente.conectada(&sid_runner),
        "roteou para a conta errada"
    );

    // o que é de outra conta não chega a ela, e o que é dela chega
    assert!(!s.gerente.enviar(&sid_runner, aviso()));
    assert!(s.gerente.enviar(&sid_conta, aviso()));
    assert!(matches!(
        c.proxima(Duration::from_secs(5)).await,
        Chegou::Linha(_)
    ));
    // a conta padrão não consegue criar outra instância do pipe (servidor falso)
    let falso = como(&conta, || {
        tokio::net::windows::named_pipe::ServerOptions::new().create(&nome)
    });
    assert!(falso.is_err(), "a conta padrão criou uma instância do pipe");
}

/// Um servidor do pipe com runtime próprio: largá-lo derruba todas as conexões, como um serviço
/// que cai de verdade (largar só a tarefa de `servir` deixaria as conexões abertas).
struct ServicoIsolado {
    gerente: GerenteUi,
    _rt: tokio::runtime::Runtime,
    _dir: TempDir,
}

fn subir_isolado(nome: &str) -> ServicoIsolado {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let gerente = GerenteUi::default();
    let (g, n) = (gerente.clone(), nome.to_owned());
    let db_url = format!("sqlite://{}", dir.path().join("k.db").display());
    rt.spawn(async move {
        let db = Arc::new(KioskDbManager::new(&db_url).await.unwrap());
        let config = Configuracao {
            nome_do_pipe: n,
            imagem_esperada: None,
        };
        let _ = servir(config, g, db).await;
    });
    ServicoIsolado {
        gerente,
        _rt: rt,
        _dir: dir,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_cliente_da_ui_espera_o_servico_manda_batimento_entrega_avisos_confirma_e_reconecta() {
    use kiosk_engine::ipc::DaUi;
    use kiosk_engine::ui_cliente_windows::manter;

    let nome = format!(r"\\.\pipe\kiosk-ui-teste-{}", uuid::Uuid::new_v4().simple());
    let sid = sid_atual();
    let (recebido_tx, recebido) = std::sync::mpsc::channel();
    let (confirma_tx, mut confirma_rx) = tokio::sync::mpsc::unbounded_channel();
    let n = nome.clone();
    let cliente = tokio::spawn(async move {
        manter(
            &n,
            Duration::from_millis(200),
            move |m| {
                let _ = recebido_tx.send(m);
            },
            &mut confirma_rx,
        )
        .await;
    });

    // 1. o serviço ainda não existe: a UI espera e conecta quando ele sobe
    tokio::time::sleep(Duration::from_millis(600)).await;
    let servico = subir_isolado(&nome);
    assert!(
        esperar(|| servico.gerente.conectada(&sid)).await,
        "a UI devia conectar quando o serviço subiu"
    );

    // 2. o batimento segue chegando
    let primeiro = servico.gerente.ultimo_batimento(&sid).unwrap();
    assert!(
        esperar(|| servico.gerente.ultimo_batimento(&sid).unwrap() > primeiro).await,
        "o batimento periódico não chegou"
    );

    // 3. serviço -> UI, e a confirmação da UI -> serviço
    assert!(servico.gerente.enviar(&sid, aviso()));
    let chegou = recebido
        .recv_timeout(Duration::from_secs(5))
        .expect("o aviso não chegou");
    assert_eq!(chegou, aviso());
    confirma_tx.send(DaUi::AvisoExibido { marco: 300 }).unwrap();
    assert!(esperar(|| servico.gerente.ultimo_aviso_exibido(&sid) == Some(300)).await);

    // 4. o serviço cai de verdade: a UI não desiste e volta quando ele sobe de novo
    drop(servico);
    tokio::time::sleep(Duration::from_millis(300)).await;
    let novo = subir_isolado(&nome);
    assert!(
        esperar(|| novo.gerente.conectada(&sid)).await,
        "a UI devia reconectar ao serviço reiniciado"
    );
    assert!(novo.gerente.enviar(&sid, aviso()));
    assert_eq!(
        recebido
            .recv_timeout(Duration::from_secs(5))
            .expect("o aviso depois de reconectar"),
        aviso()
    );
    cliente.abort();
}

/// O `kiosk-ui.exe` de verdade, morto ao sair do teste.
struct Programa(std::process::Child);

impl Drop for Programa {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn kiosk_ui(nome: &str, extra: &[(&str, &str)]) -> Programa {
    let mut c = std::process::Command::new(env!("CARGO_BIN_EXE_kiosk-ui"));
    c.env("KIOSK_UI_PIPE", nome)
        .env("KIOSK_LANG", "es-ES")
        .stdin(std::process::Stdio::null());
    for (k, v) in extra {
        c.env(k, v);
    }
    Programa(c.spawn().expect("kiosk-ui.exe devia iniciar"))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_kiosk_ui_mostra_o_estado_e_o_aviso_no_idioma_da_conta_e_confirma_ao_servico() {
    let s = subir(None).await;
    let sid = sid_atual();
    let dir = tempfile::tempdir().unwrap();
    let saida = dir.path().join("saida.txt");
    let _ui = kiosk_ui(
        &s.nome,
        &[
            ("KIOSK_UI_SEM_BANDEJA", "1"),
            ("KIOSK_UI_SAIDA", saida.to_str().unwrap()),
        ],
    );
    assert!(
        esperar(|| s.gerente.conectada(&sid)).await,
        "o kiosk-ui não conectou"
    );
    let lido = || std::fs::read_to_string(&saida).unwrap_or_default();

    // o estado vira a dica do ícone
    assert!(s.gerente.enviar(
        &sid,
        ParaUi::Estado {
            restante_segundos: Some(4_800),
            motivo: None
        }
    ));
    assert!(
        esperar(|| lido().contains("DICA\tControl parental: quedan 1h 20min 00s")).await,
        "{}",
        lido()
    );

    // o aviso chega no idioma da conta (es-ES), e o serviço recebe a confirmação
    assert!(s.gerente.enviar(&sid, aviso()));
    assert!(
        esperar(|| lido().contains("AVISO\tControl parental\tFaltan 5 minutos")).await,
        "{}",
        lido()
    );
    assert!(
        esperar(|| s.gerente.ultimo_aviso_exibido(&sid) == Some(300)).await,
        "o serviço não recebeu a confirmação"
    );

    // motivo fora do esquema: a UI não inventa texto e não confirma
    let antes = lido();
    assert!(s.gerente.enviar(
        &sid,
        ParaUi::Aviso {
            restante_segundos: 60,
            motivo: "<inventado>".into(),
            app: None
        }
    ));
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(lido(), antes, "mostrou algo para um motivo desconhecido");
    assert_eq!(s.gerente.ultimo_aviso_exibido(&sid), Some(300));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn o_kiosk_ui_com_a_bandeja_real_sobe_conecta_e_fica_vivo() {
    // o desenho do ícone só se vê num PC com a barra de tarefas; aqui prova-se que a janela e o
    // laço de mensagens sobem e que a UI conecta e segue viva
    let s = subir(None).await;
    let sid = sid_atual();
    let mut ui = kiosk_ui(&s.nome, &[]);
    assert!(
        esperar(|| s.gerente.conectada(&sid)).await,
        "o kiosk-ui com bandeja não conectou (saiu? {:?})",
        ui.0.try_wait()
    );
    // um aviso não derruba nem faz a UI sair
    assert!(s.gerente.enviar(&sid, aviso()));
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert!(
        ui.0.try_wait().unwrap().is_none(),
        "o kiosk-ui saiu sozinho"
    );
    assert!(s.gerente.conectada(&sid));
    eprintln!(
        "balão aceito pelo Windows neste runner: {}",
        s.gerente.ultimo_aviso_exibido(&sid).is_some()
    );
}
