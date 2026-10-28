//! E-002 F1: o evento é gravado na mesma transação da mudança de estado.

use chrono::Utc;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use kiosk_engine::security_logger_impl::insert_event;
use tempfile::TempDir;
use uuid::Uuid;

async fn novo_banco() -> (TempDir, KioskDbManager) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("kiosk.db").display());
    (dir, KioskDbManager::new(&url).await.unwrap())
}

async fn eventos(db: &KioskDbManager) -> Vec<(String, String, String, String, String)> {
    sqlx::query_as(
        "SELECT action_attempted, event_type, transaction_status, client_ip, rejection_reason_detail \
         FROM security_log ORDER BY timestamp, rowid",
    )
    .fetch_all(db.pool())
    .await
    .unwrap()
}

fn ctx() -> EventContext {
    EventContext {
        client_ip: "100.64.0.7".into(),
    }
}

fn app() -> AppMetadata {
    AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: "game.exe".into(),
        status: ProcessStatus::Monitored,
    }
}

fn perfil() -> ChildProfile {
    ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: "S-1-5-21-1".into(),
        display_name: "Filho".into(),
        daily_total_limit_seconds: None,
    }
}

fn regra(profile_id: Uuid, app_id: Uuid) -> ActiveRule {
    ActiveRule {
        rule_id: Uuid::new_v4(),
        profile_id,
        app_id,
        creation_timestamp: Utc::now(),
        state: RuleState::Pending,
        daily_limit_seconds: None,
        time_window: None,
        effective_until_timestamp: None,
    }
}

#[tokio::test]
async fn cada_operacao_publica_de_escrita_grava_exatamente_um_evento() {
    let (_dir, db) = novo_banco().await;
    let (a, p) = (app(), perfil());
    db.create_app(&ctx(), &a).await.unwrap();
    db.create_profile(&ctx(), &p).await.unwrap();
    let janela = TimeWindow {
        start: chrono::NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        end: chrono::NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
    };
    db.set_profile_window(&ctx(), p.profile_id, 1, &janela)
        .await
        .unwrap();
    let r = regra(p.profile_id, a.app_id);
    db.create_active_rule(&ctx(), &r).await.unwrap();
    db.set_rule_state(&ctx(), r.rule_id, RuleState::Active)
        .await
        .unwrap();
    db.set_rule_state(&ctx(), r.rule_id, RuleState::Revoked)
        .await
        .unwrap();

    let acoes: Vec<String> = eventos(&db).await.into_iter().map(|e| e.0).collect();
    assert_eq!(
        acoes,
        [
            "ADD_APP",
            "ADD_PROFILE",
            "SET_PROFILE_WINDOW",
            "ADD_RULE",
            "ACTIVATE_RULE",
            "REVOKE"
        ]
    );
    for e in eventos(&db).await {
        assert_eq!(
            (e.1.as_str(), e.2.as_str(), e.3.as_str()),
            ("SUCCESS", "SUCCESS", "100.64.0.7")
        );
    }
}

#[tokio::test]
async fn falha_desfaz_a_mudanca_e_grava_um_evento_de_falha_com_a_razao() {
    let (_dir, db) = novo_banco().await;
    let p = perfil();
    db.create_profile(&ctx(), &p).await.unwrap();
    let r = regra(p.profile_id, Uuid::new_v4()); // app inexistente: viola a FK

    let erro = db.create_active_rule(&ctx(), &r).await.unwrap_err();
    assert!(db.get_active_rule(r.rule_id).await.unwrap().is_none());

    let falhas: Vec<_> = eventos(&db)
        .await
        .into_iter()
        .filter(|e| e.2 == "FAILURE")
        .collect();
    assert_eq!(falhas.len(), 1);
    assert_eq!(
        (falhas[0].0.as_str(), falhas[0].1.as_str()),
        ("ADD_RULE", "INTERNAL_ERROR")
    );
    assert_eq!(falhas[0].4, erro.to_string(), "razão exata fica só no log");
    // nenhum evento de SUCCESS para a regra que não existe
    assert!(
        eventos(&db)
            .await
            .iter()
            .all(|e| e.0 != "ADD_RULE" || e.2 == "FAILURE")
    );
}

#[tokio::test]
async fn rollback_da_mudanca_tambem_descarta_o_evento() {
    let (_dir, db) = novo_banco().await;
    {
        let mut tx = db.begin_transaction().await.unwrap();
        let e = SecurityEvent::new(
            "local",
            ActionType::System,
            EventType::Success,
            TransactionStatus::Success,
            "",
        );
        insert_event(&mut tx, &e).await.unwrap();
    } // drop sem commit
    assert!(eventos(&db).await.is_empty());
}

#[tokio::test]
async fn estado_inexistente_falha_alto_e_fica_registrado() {
    let (_dir, db) = novo_banco().await;
    assert!(
        db.set_rule_state(&ctx(), Uuid::new_v4(), RuleState::Revoked)
            .await
            .is_err()
    );
    let ev = eventos(&db).await;
    assert_eq!(ev.len(), 1);
    assert_eq!((ev[0].0.as_str(), ev[0].2.as_str()), ("REVOKE", "FAILURE"));
}

#[tokio::test]
async fn todos_os_tipos_de_evento_sao_aceitos_pelo_banco() {
    let (_dir, db) = novo_banco().await;
    let tipos = [
        EventType::Success,
        EventType::AuthFail,
        EventType::AuthzFail,
        EventType::ValidationFail,
        EventType::InternalError,
        EventType::StaleRule,
        EventType::WarningShown,
        EventType::AppTerminated,
        EventType::SessionLocked,
        EventType::AgentStopped,
        EventType::TamperSuspected,
    ];
    for t in tipos {
        let st = if t == EventType::Success {
            TransactionStatus::Success
        } else {
            TransactionStatus::Failure
        };
        db.record_event(&SecurityEvent::new("local", ActionType::System, t, st, "x"))
            .await
            .unwrap();
    }
    assert_eq!(eventos(&db).await.len(), tipos.len());
}

#[tokio::test]
async fn tipo_de_evento_desconhecido_e_recusado_pelo_banco() {
    let (_dir, db) = novo_banco().await;
    let r = sqlx::query(
        "INSERT INTO security_log (log_id, timestamp, client_ip, action_attempted, event_type, transaction_status, \
         rejection_reason_detail) VALUES ('1', 't', 'ip', 'SYSTEM', 'INVENTADO', 'SUCCESS', '')",
    )
    .execute(db.pool())
    .await;
    assert!(r.is_err());
}

async fn um_evento(db: &KioskDbManager) -> SecurityEvent {
    let e = SecurityEvent::new(
        "local",
        ActionType::System,
        EventType::Success,
        TransactionStatus::Success,
        "original",
    );
    db.record_event(&e).await.unwrap();
    e
}

#[tokio::test]
async fn update_no_log_e_recusado_e_o_registro_fica_intacto() {
    let (_dir, db) = novo_banco().await;
    let e = um_evento(&db).await;
    let r = sqlx::query(
        "UPDATE security_log SET rejection_reason_detail = 'adulterado' WHERE log_id = ?",
    )
    .bind(e.event_id.to_string())
    .execute(db.pool())
    .await;
    let msg = r.unwrap_err().to_string();
    assert!(msg.contains("append-only"), "mensagem inesperada: {msg}");
    assert_eq!(eventos(&db).await[0].4, "original");
}

#[tokio::test]
async fn delete_no_log_e_recusado_e_o_registro_permanece() {
    let (_dir, db) = novo_banco().await;
    um_evento(&db).await;
    assert!(
        sqlx::query("DELETE FROM security_log")
            .execute(db.pool())
            .await
            .is_err()
    );
    assert_eq!(eventos(&db).await.len(), 1);
}

#[tokio::test]
async fn insert_continua_funcionando_com_os_triggers_ativos() {
    let (_dir, db) = novo_banco().await;
    um_evento(&db).await;
    um_evento(&db).await;
    assert_eq!(eventos(&db).await.len(), 2);
}
