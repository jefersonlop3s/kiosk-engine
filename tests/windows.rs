//! Adaptador Win32 contra o Windows real (roda só no Windows; no CI, `windows-latest`).
//! Bloquear a sessão, logoff e avisos exigem sessão interativa e não são exercitados aqui.
#![cfg(windows)]

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use chrono::Utc;
use kiosk_engine::agente::{Acao, Agente, Sistema};
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use kiosk_engine::sistema_windows::SistemaWindows;
use uuid::Uuid;

/// SID do usuário atual segundo o próprio Windows (`whoami /user`).
fn sid_atual() -> String {
    let out = Command::new("whoami")
        .args(["/user", "/fo", "csv", "/nh"])
        .output()
        .unwrap();
    let texto = String::from_utf8_lossy(&out.stdout);
    texto
        .trim()
        .rsplit(',')
        .next()
        .unwrap()
        .trim_matches('"')
        .to_owned()
}

fn espera_sair(
    filho: &mut std::process::Child,
    limite: Duration,
) -> Option<std::process::ExitStatus> {
    let fim = Instant::now() + limite;
    while Instant::now() < fim {
        if let Some(s) = filho.try_wait().unwrap() {
            return Some(s);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn ping_longo() -> std::process::Child {
    Command::new("ping")
        .args(["-n", "120", "127.0.0.1"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn lista_processos_com_o_sid_do_dono_igual_ao_do_whoami() {
    let sid = sid_atual();
    assert!(sid.starts_with("S-1-5-"), "whoami devolveu {sid:?}");
    let meu_pid = std::process::id();
    let lista = SistemaWindows.processos().unwrap();
    let eu = lista
        .iter()
        .find(|p| p.pid == meu_pid)
        .expect("o processo do teste deve aparecer");
    assert_eq!(eu.sid, sid);
    assert!(eu.process_name.to_lowercase().ends_with(".exe"), "{eu:?}");
    assert!(lista.len() > 10, "poucos processos: {}", lista.len());
}

#[test]
fn encerra_um_processo_de_verdade() {
    let mut filho = ping_longo();
    SistemaWindows.encerrar(filho.id()).unwrap();
    let status = espera_sair(&mut filho, Duration::from_secs(10)).expect("o processo não terminou");
    assert!(!status.success(), "TerminateProcess usa código 1");
}

#[test]
fn encerrar_processo_inexistente_falha_alto() {
    let mut filho = Command::new("cmd")
        .args(["/c", "exit", "0"])
        .spawn()
        .unwrap();
    let pid = filho.id();
    filho.wait().unwrap();
    drop(filho); // libera o handle: o pid deixa de existir
    assert!(SistemaWindows.encerrar(pid).is_err());
}

#[test]
fn consulta_de_sessoes_responde_sem_erro() {
    // o runner do CI pode não ter sessão interativa; o que se exige é a chamada funcionar
    let sessoes = SistemaWindows.sessoes_ativas().unwrap();
    assert!(sessoes.iter().all(|s| s.starts_with("S-1-")), "{sessoes:?}");
}

#[tokio::test]
async fn o_agente_encerra_o_app_bloqueado_e_nao_toca_no_app_isento() {
    let dir = tempfile::tempdir().unwrap();
    let db = KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
        .await
        .unwrap();
    let ctx = EventContext::local();
    let perfil = ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: sid_atual(),
        display_name: "Conta do teste".into(),
        daily_total_limit_seconds: None,
    };
    db.create_profile(&ctx, &perfil).await.unwrap();
    let ping = AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: "PING.EXE".into(),
        status: ProcessStatus::Monitored,
    };
    let isento = AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: "powershell.exe".into(),
        status: ProcessStatus::Whitelisted,
    };
    for app in [&ping, &isento] {
        db.create_app(&ctx, app).await.unwrap();
        let regra = ActiveRule {
            rule_id: Uuid::new_v4(),
            profile_id: perfil.profile_id,
            app_id: app.app_id,
            creation_timestamp: Utc::now(),
            state: RuleState::Active,
            daily_limit_seconds: Some(0), // bloquearia os dois, se o isento não fosse isento
            time_window: None,
            effective_until_timestamp: None,
        };
        db.create_active_rule(&ctx, &regra).await.unwrap();
    }

    let mut bloqueado = ping_longo();
    let mut permitido = Command::new("powershell")
        .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 120"])
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_secs(2)); // o powershell demora a aparecer na lista

    let mut agente = Agente::new(&db, SistemaWindows, chrono::Local);
    let rel = agente.tick(Utc::now()).await;
    let encerrados: Vec<u32> = rel
        .acoes
        .iter()
        .filter_map(|a| {
            if let Acao::Encerrou { pid, .. } = a {
                Some(*pid)
            } else {
                None
            }
        })
        .collect();
    assert!(
        encerrados.contains(&bloqueado.id()),
        "acoes: {:?} erros: {:?}",
        rel.acoes,
        rel.erros
    );
    assert!(!encerrados.contains(&permitido.id()));

    assert!(
        espera_sair(&mut bloqueado, Duration::from_secs(10)).is_some(),
        "o ping não foi encerrado"
    );
    assert!(
        permitido.try_wait().unwrap().is_none(),
        "o app isento foi encerrado"
    );
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM security_log WHERE event_type = 'APP_TERMINATED'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert!(n >= 1);
    let _ = permitido.kill();
}

const KIOSKD: &str = env!("CARGO_BIN_EXE_kioskd");

#[test]
fn perfil_criado_pelo_nome_da_conta_recebe_o_sid_do_windows() {
    let dir = tempfile::tempdir().unwrap();
    let db_url = format!("sqlite://{}", dir.path().join("k.db").display());
    let conta = String::from_utf8_lossy(&Command::new("whoami").output().unwrap().stdout)
        .trim()
        .to_owned();
    let saida = Command::new(KIOSKD)
        .args([
            "perfil",
            "criar",
            "--conta",
            &conta,
            "--nome",
            "Conta do teste",
        ])
        .env("KIOSK_DB", &db_url)
        .output()
        .unwrap();
    assert!(
        saida.status.success(),
        "{}",
        String::from_utf8_lossy(&saida.stderr)
    );
    let lista = Command::new(KIOSKD)
        .args(["perfil", "listar"])
        .env("KIOSK_DB", &db_url)
        .output()
        .unwrap();
    let lista = String::from_utf8_lossy(&lista.stdout);
    assert!(
        lista.contains(&sid_atual()),
        "SID esperado {} em {lista}",
        sid_atual()
    );

    let inexistente = Command::new(KIOSKD)
        .args([
            "perfil",
            "criar",
            "--conta",
            "conta-que-nao-existe-kiosk",
            "--nome",
            "X",
        ])
        .env("KIOSK_DB", &db_url)
        .output()
        .unwrap();
    assert!(!inexistente.status.success());
    let grupo = Command::new(KIOSKD)
        .args([
            "perfil",
            "criar",
            "--conta",
            "BUILTIN\\Administrators",
            "--nome",
            "X",
        ])
        .env("KIOSK_DB", &db_url)
        .output()
        .unwrap();
    assert!(
        !grupo.status.success(),
        "um grupo não pode virar perfil de criança"
    );
}
