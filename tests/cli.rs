//! E-005 F3: o serviço (`kioskd`) e o CLI dos pais (`kioskctl`) como processos reais.

use std::net::{SocketAddr, TcpListener as StdListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

use kiosk_engine::cliente::Credenciais;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use tempfile::TempDir;
use uuid::Uuid;

const KIOSKD: &str = env!("CARGO_BIN_EXE_kioskd");
const KIOSKCTL: &str = env!("CARGO_BIN_EXE_kioskctl");

struct Servidor(Child);

impl Drop for Servidor {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn porta_livre() -> u16 {
    StdListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn kioskd(db_url: &str, listen: Option<&str>, args: &[&str]) -> Output {
    let mut c = Command::new(KIOSKD);
    c.args(args)
        .env("KIOSK_DB", db_url)
        .env_remove("KIOSK_LISTEN");
    if let Some(l) = listen {
        c.env("KIOSK_LISTEN", l);
    }
    c.output().unwrap()
}

fn kioskctl(cred: &Path, args: &[&str]) -> Output {
    Command::new(KIOSKCTL)
        .args(args)
        .env("KIOSK_CREDENCIAIS", cred)
        .output()
        .unwrap()
}

fn texto(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn subir_servico(db_url: &str, porta: u16) -> Servidor {
    let filho = Command::new(KIOSKD)
        .arg("servir")
        .env("KIOSK_DB", db_url)
        .env("KIOSK_LISTEN", format!("127.0.0.1:{porta}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let s = Servidor(filho);
    let addr: SocketAddr = format!("127.0.0.1:{porta}").parse().unwrap();
    let limite = Instant::now() + Duration::from_secs(10);
    while Instant::now() < limite {
        if TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok() {
            return s;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("o serviço não abriu a porta {porta} a tempo");
}

struct Mundo {
    dir: TempDir,
    db_url: String,
    perfil: Uuid,
    app: Uuid,
}

async fn mundo() -> Mundo {
    let dir = tempfile::tempdir().unwrap();
    let db_url = format!("sqlite://{}", dir.path().join("k.db").display());
    let db = KioskDbManager::new(&db_url).await.unwrap();
    let ctx = EventContext::local();
    let perfil = ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: "S-1-5-21-1".into(),
        display_name: "Filho".into(),
        daily_total_limit_seconds: None,
    };
    let app = AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: "jogo.exe".into(),
        status: ProcessStatus::Monitored,
    };
    db.create_profile(&ctx, &perfil).await.unwrap();
    db.create_app(&ctx, &app).await.unwrap();
    Mundo {
        db_url,
        perfil: perfil.profile_id,
        app: app.app_id,
        dir,
    }
}

#[tokio::test]
async fn ciclo_completo_pelo_cli_contra_o_servico_real() {
    let m = mundo().await;
    let porta = porta_livre();
    let _srv = subir_servico(&m.db_url, porta);
    let servidor = format!("127.0.0.1:{porta}");
    let cred = m.dir.path().join("pais").join("credenciais.json");

    // código na conta de administrador, pareamento no dispositivo dos pais
    let saida = kioskd(&m.db_url, None, &["codigo-pareamento"]);
    assert!(saida.status.success(), "{}", texto(&saida.stderr));
    let codigo = texto(&saida.stdout).trim().to_owned();
    assert_eq!(codigo.len(), 8);

    let p = kioskctl(
        &cred,
        &[
            "parear",
            "--servidor",
            &servidor,
            "--codigo",
            &codigo,
            "--nome",
            "Celular da mãe",
        ],
    );
    assert!(p.status.success(), "{}", texto(&p.stderr));
    let c = Credenciais::carregar(&cred).unwrap();
    assert!(c.device_id.is_some());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let modo = std::fs::metadata(&cred).unwrap().permissions().mode() & 0o777;
        assert_eq!(
            modo, 0o600,
            "a chave privada não pode ser legível por outros"
        );
    }
    let lista = kioskd(&m.db_url, None, &["dispositivos"]);
    assert!(
        texto(&lista.stdout).contains("Celular da mãe") && texto(&lista.stdout).contains("ativo")
    );

    // criar -> ativar -> revogar
    let perfil = m.perfil.to_string();
    let app = m.app.to_string();
    let criar = kioskctl(
        &cred,
        &[
            "regra",
            "criar",
            "--perfil",
            &perfil,
            "--app",
            &app,
            "--limite",
            "3600",
            "--janela",
            "08:00-20:00",
        ],
    );
    assert!(criar.status.success(), "{}", texto(&criar.stderr));
    let saida = texto(&criar.stdout);
    let id = saida.trim().rsplit(' ').next().unwrap().to_owned();
    let id = Uuid::parse_str(&id).unwrap_or_else(|_| panic!("saída inesperada: {saida}"));

    let db = KioskDbManager::new(&m.db_url).await.unwrap();
    assert_eq!(
        db.get_active_rule(id).await.unwrap().unwrap().state,
        RuleState::Pending
    );
    assert!(
        kioskctl(&cred, &["regra", "ativar", &id.to_string()])
            .status
            .success()
    );
    assert_eq!(
        db.get_active_rule(id).await.unwrap().unwrap().state,
        RuleState::Active
    );
    assert!(
        kioskctl(&cred, &["regra", "revogar", &id.to_string()])
            .status
            .success()
    );
    assert_eq!(
        db.get_active_rule(id).await.unwrap().unwrap().state,
        RuleState::Revoked
    );

    // revogado o dispositivo, o CLI perde o acesso
    let device = c.device_id.unwrap().to_string();
    assert!(
        kioskd(&m.db_url, None, &["revogar", &device])
            .status
            .success()
    );
    let negado = kioskctl(
        &cred,
        &["regra", "criar", "--perfil", &perfil, "--app", &app],
    );
    assert!(!negado.status.success());
    let erro = texto(&negado.stderr);
    assert!(
        erro.contains("recusou") && !erro.contains(&device),
        "{erro}"
    );
}

#[tokio::test]
async fn pareamento_com_codigo_errado_falha_e_nao_grava_credenciais() {
    let m = mundo().await;
    let porta = porta_livre();
    let _srv = subir_servico(&m.db_url, porta);
    let cred = m.dir.path().join("credenciais.json");
    let p = kioskctl(
        &cred,
        &[
            "parear",
            "--servidor",
            &format!("127.0.0.1:{porta}"),
            "--codigo",
            "ZZZZZZZZ",
        ],
    );
    assert!(!p.status.success());
    assert!(texto(&p.stderr).contains("recusado"));
    assert!(!cred.exists());
}

#[tokio::test]
async fn parear_de_novo_sobre_credenciais_existentes_e_recusado() {
    let m = mundo().await;
    let cred = m.dir.path().join("credenciais.json");
    Credenciais::nova("127.0.0.1:1").0.salvar(&cred).unwrap();
    let p = kioskctl(
        &cred,
        &[
            "parear",
            "--servidor",
            "127.0.0.1:1",
            "--codigo",
            "AAAAAAAA",
        ],
    );
    assert!(!p.status.success() && texto(&p.stderr).contains("já existe"));
}

#[tokio::test]
async fn servico_recusa_iniciar_sem_endereco_ou_com_endereco_proibido() {
    let m = mundo().await;
    let sem = kioskd(&m.db_url, None, &["servir"]);
    assert!(!sem.status.success() && texto(&sem.stderr).contains("KIOSK_LISTEN"));

    for (listen, trecho) in [
        ("0.0.0.0:8443", "curinga"),
        ("[::]:8443", "curinga"),
        ("192.168.0.10:8443", "LAN"),
        ("127.0.0.1:8443,10.0.0.5:8443", "LAN"),
        ("isto-nao-e-endereco", "inválido"),
    ] {
        let s = kioskd(&m.db_url, Some(listen), &["servir"]);
        assert!(!s.status.success(), "{listen} deveria ser recusado");
        assert!(
            texto(&s.stderr).contains(trecho),
            "{listen}: {}",
            texto(&s.stderr)
        );
    }
}

#[tokio::test]
async fn comandos_desconhecidos_mostram_o_uso_e_falham() {
    let m = mundo().await;
    let cred = m.dir.path().join("credenciais.json");
    for args in [
        vec![],
        vec!["apagar"],
        vec!["regra"],
        vec!["regra", "ativar"],
    ] {
        let s = kioskctl(&cred, &args);
        assert!(
            !s.status.success() && texto(&s.stderr).contains("uso:"),
            "{args:?}"
        );
    }
    let d = kioskd(&m.db_url, None, &["nada"]);
    assert!(!d.status.success() && texto(&d.stderr).contains("uso:"));
    let sem_cred = kioskctl(&cred, &["regra", "ativar", &Uuid::new_v4().to_string()]);
    assert!(!sem_cred.status.success() && texto(&sem_cred.stderr).contains("ler"));
}

#[test]
fn credenciais_ida_e_volta_ocultam_a_chave_no_debug_e_recusam_arquivo_corrompido() {
    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join("a").join("b").join("c.json");
    let (mut c, chave) = Credenciais::nova("100.64.0.1:8443");
    c.device_id = Some(Uuid::new_v4());
    c.salvar(&caminho).unwrap();
    let lida = Credenciais::carregar(&caminho).unwrap();
    assert_eq!(lida.chave().unwrap().to_bytes(), chave.to_bytes());
    assert_eq!(
        (lida.servidor.as_str(), lida.device_id),
        ("100.64.0.1:8443", c.device_id)
    );
    assert!(
        !format!("{lida:?}").contains(&lida.chave_privada),
        "a chave não pode vazar no Debug"
    );

    std::fs::write(&caminho, "{ não é json").unwrap();
    assert!(Credenciais::carregar(&caminho).is_err());
    let mut ruim = lida.clone();
    ruim.chave_privada = "curta".into();
    assert!(ruim.chave().is_err());
}

#[tokio::test]
async fn limite_uso_e_log_pelo_cli() {
    let m = mundo().await;
    let porta = porta_livre();
    let _srv = subir_servico(&m.db_url, porta);
    let servidor = format!("127.0.0.1:{porta}");
    let cred = m.dir.path().join("credenciais.json");
    let codigo = texto(&kioskd(&m.db_url, None, &["codigo-pareamento"]).stdout)
        .trim()
        .to_owned();
    assert!(
        kioskctl(
            &cred,
            &["parear", "--servidor", &servidor, "--codigo", &codigo]
        )
        .status
        .success()
    );

    let perfil = m.perfil.to_string();
    let criar = kioskctl(
        &cred,
        &[
            "regra",
            "criar",
            "--perfil",
            &perfil,
            "--app",
            &m.app.to_string(),
            "--limite",
            "3600",
        ],
    );
    let saida = texto(&criar.stdout);
    let id = saida.trim().rsplit(' ').next().unwrap().to_owned();

    let l = kioskctl(&cred, &["regra", "limite", &id, "--segundos", "5400"]);
    assert!(l.status.success(), "{}", texto(&l.stderr));
    assert!(
        texto(&l.stdout).contains("1h 30min 00s"),
        "{}",
        texto(&l.stdout)
    );
    let db = KioskDbManager::new(&m.db_url).await.unwrap();
    let r = db
        .get_active_rule(Uuid::parse_str(&id).unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(r.daily_limit_seconds, Some(5400));

    // uso de um dia conhecido
    let ini = chrono::TimeZone::with_ymd_and_hms(&chrono::Utc, 2026, 3, 9, 12, 0, 0).unwrap();
    let ctx = EventContext::local();
    db.registrar_uso(
        &ctx,
        m.perfil,
        m.app,
        ini,
        ini + chrono::Duration::seconds(300),
        &chrono::Utc,
    )
    .await
    .unwrap();
    db.registrar_sessao(
        &ctx,
        m.perfil,
        ini,
        ini + chrono::Duration::seconds(400),
        &chrono::Utc,
    )
    .await
    .unwrap();
    let u = kioskctl(&cred, &["uso", "--perfil", &perfil, "--data", "2026-03-09"]);
    assert!(u.status.success(), "{}", texto(&u.stderr));
    let uso = texto(&u.stdout);
    assert!(
        uso.contains("sessão ativa: 6min 40s") && uso.contains("jogo.exe: 5min 00s"),
        "{uso}"
    );

    let log = kioskctl(&cred, &["log", "--perfil", &perfil, "--dias", "1"]);
    assert!(log.status.success(), "{}", texto(&log.stderr));
    let saida = texto(&log.stdout);
    assert!(
        saida.contains("ADD_RULE") && saida.contains("MODIFY_LIMIT"),
        "{saida}"
    );

    // perfil inexistente: a consulta é recusada sem vazar nada
    let ruim = kioskctl(&cred, &["uso", "--perfil", &Uuid::new_v4().to_string()]);
    assert!(!ruim.status.success() && texto(&ruim.stderr).contains("recusou"));
}
