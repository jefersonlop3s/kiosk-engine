//! E-004 F4: o agente que sobe depois de um período sem agente registra `AGENT_STOPPED`.

use chrono::{DateTime, Duration, TimeZone, Utc};
use kiosk_engine::agente::*;
use kiosk_engine::db_manager::KioskDbManager;
use tempfile::TempDir;

/// Sistema sem nada rodando: o que importa aqui é o batimento, não o enforcement.
struct Vazio;

impl Sistema for Vazio {
    fn processos(&mut self) -> Result<Vec<ProcessoVisto>, String> {
        Ok(vec![])
    }
    fn encerrar(&mut self, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn bloquear_sessao(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn sessoes_ativas(&mut self) -> Result<Vec<String>, String> {
        Ok(vec![])
    }
    fn encerrar_sessao(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn avisar(&mut self, _: &Aviso) -> Result<(), String> {
        Ok(())
    }
    fn contas_locais(&mut self) -> Result<Vec<ContaLocal>, String> {
        Ok(vec![])
    }
    fn atalhos_do_menu(&mut self) -> Result<Vec<ProgramaSugerido>, String> {
        Ok(vec![])
    }
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap()
}

fn seg(n: i64) -> DateTime<Utc> {
    t0() + Duration::seconds(n)
}

async fn banco() -> (TempDir, KioskDbManager) {
    let dir = tempfile::tempdir().unwrap();
    let db = KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
        .await
        .unwrap();
    (dir, db)
}

async fn paradas(db: &KioskDbManager) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT transaction_status, rejection_reason_detail FROM security_log \
         WHERE event_type = 'AGENT_STOPPED' ORDER BY timestamp, rowid",
    )
    .fetch_all(db.pool())
    .await
    .unwrap()
}

/// Um agente que vive `ate` segundos, com um tick a cada 2 s, e some sem avisar.
async fn viver(db: &KioskDbManager, de: i64, ate: i64) {
    let mut a = Agente::new(db, Vazio, Utc);
    let mut t = de;
    while t <= ate {
        a.tick(seg(t)).await;
        t += 2;
    }
}

#[tokio::test]
async fn primeira_execucao_do_banco_nao_registra_nada() {
    let (_d, db) = banco().await;
    viver(&db, 0, 20).await;
    assert!(paradas(&db).await.is_empty());
    assert!(db.ler_batimento().await.unwrap().is_some());
}

#[tokio::test]
async fn agente_que_sumiu_deixa_agent_stopped_de_falha_com_o_intervalo() {
    let (_d, db) = banco().await;
    viver(&db, 0, 60).await; // some sem dizer nada
    viver(&db, 3_600, 3_610).await; // sobe uma hora depois
    let p = paradas(&db).await;
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].0, "FAILURE");
    let d = kiosk_engine::detalhes::Detalhe::de_texto(&p[0].1).expect("detalhe em código");
    let kiosk_engine::detalhes::Detalhe::AgentWasDown { seconds, clean, .. } = d else {
        panic!("{d:?}");
    };
    assert!(!clean);
    // do último batimento (a cada 10 s, até o segundo 60) ao primeiro tick novo
    assert_eq!(seconds, 3_600 - 60);
}

#[tokio::test]
async fn parada_de_proposito_deixa_agent_stopped_de_sucesso() {
    let (_d, db) = banco().await;
    viver(&db, 0, 20).await;
    db.gravar_batimento(seg(22), true).await.unwrap(); // o serviço marca a parada normal
    viver(&db, 600, 610).await;
    let p = paradas(&db).await;
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!(p[0].0, "SUCCESS");
    assert!(p[0].1.contains("\"clean\":true"), "{}", p[0].1);
}

#[tokio::test]
async fn reinicio_rapido_nao_vira_evento() {
    let (_d, db) = banco().await;
    viver(&db, 0, 20).await;
    // o SCM reinicia o serviço 5 s depois de uma falha: abaixo de 3 batimentos
    viver(&db, 27, 40).await;
    assert!(paradas(&db).await.is_empty());
}

#[tokio::test]
async fn o_limiar_e_exato_tres_batimentos() {
    let (_d, db) = banco().await;
    viver(&db, 0, 0).await; // um batimento no segundo 0
    viver(&db, PARADA_MINIMA_SEGUNDOS, PARADA_MINIMA_SEGUNDOS).await;
    assert!(
        paradas(&db).await.is_empty(),
        "no limite ainda não é parada"
    );
    viver(
        &db,
        PARADA_MINIMA_SEGUNDOS * 2 + 1,
        PARADA_MINIMA_SEGUNDOS * 2 + 1,
    )
    .await;
    assert_eq!(paradas(&db).await.len(), 1, "passou do limite");
}

#[tokio::test]
async fn o_batimento_so_e_gravado_a_cada_dez_segundos() {
    let (_d, db) = banco().await;
    let mut a = Agente::new(&db, Vazio, Utc);
    a.tick(seg(0)).await;
    a.tick(seg(4)).await;
    a.tick(seg(8)).await;
    assert_eq!(db.ler_batimento().await.unwrap().unwrap().0, seg(0));
    a.tick(seg(10)).await;
    assert_eq!(db.ler_batimento().await.unwrap().unwrap().0, seg(10));
}

#[tokio::test]
async fn o_batimento_seguinte_desmarca_a_parada_normal() {
    let (_d, db) = banco().await;
    db.gravar_batimento(seg(0), true).await.unwrap();
    assert!(db.ler_batimento().await.unwrap().unwrap().1);
    viver(&db, 5, 6).await;
    assert!(!db.ler_batimento().await.unwrap().unwrap().1);
}

#[tokio::test]
async fn evento_e_dois_idiomas_do_detalhe_legivel() {
    let (_d, db) = banco().await;
    viver(&db, 0, 20).await;
    viver(&db, 4_000, 4_002).await;
    let p = paradas(&db).await;
    let pt = kiosk_engine::detalhes::legivel(&p[0].1, kiosk_engine::i18n::Idioma::PtBr);
    let en = kiosk_engine::detalhes::legivel(&p[0].1, kiosk_engine::i18n::Idioma::EnUs);
    assert!(pt.contains("sumiu sem parada normal"), "{pt}");
    assert!(en.contains("disappeared without a normal stop"), "{en}");
    assert!(pt.contains("1h"), "a duração sai legível: {pt}");
}

#[tokio::test]
async fn banco_sem_o_batimento_gravado_nao_trava_o_agente() {
    let (_d, db) = banco().await;
    sqlx::query("DROP TABLE agent_heartbeat")
        .execute(db.pool())
        .await
        .unwrap();
    let mut a = Agente::new(&db, Vazio, Utc);
    let rel = a.tick(seg(0)).await;
    assert!(!rel.erros.is_empty(), "o erro tem de aparecer no relatório");
    // e o agente segue: o próximo tick roda
    let _ = a.tick(seg(2)).await;
}
