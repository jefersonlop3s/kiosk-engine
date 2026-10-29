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

fn evento_de(dias_atras: i64, perfil: Option<Uuid>, detalhe: &str) -> SecurityEvent {
    let mut e = SecurityEvent::new(
        "local",
        ActionType::System,
        EventType::WarningShown,
        TransactionStatus::Success,
        detalhe,
    )
    .with_profile(perfil);
    e.timestamp = Utc::now() - chrono::Duration::days(dias_atras);
    e
}

#[tokio::test]
async fn consulta_por_periodo_e_por_perfil_com_dados_de_varios_dias() {
    let (_dir, db) = novo_banco().await;
    let (ana, beto) = (Uuid::new_v4(), Uuid::new_v4());
    for (dias, perfil, nome) in [
        (10, ana, "ana-10"),
        (5, beto, "beto-5"),
        (3, ana, "ana-3"),
        (1, beto, "beto-1"),
    ] {
        db.record_event(&evento_de(dias, Some(perfil), nome))
            .await
            .unwrap();
    }
    db.record_event(&evento_de(4, None, "sem-perfil"))
        .await
        .unwrap();

    let agora = Utc::now();
    let detalhes = |v: Vec<SecurityEvent>| {
        v.into_iter()
            .map(|e| e.rejection_reason_detail)
            .collect::<Vec<_>>()
    };

    let tudo = db
        .security_events(agora - chrono::Duration::days(30), agora, None)
        .await
        .unwrap();
    assert_eq!(
        detalhes(tudo),
        ["ana-10", "beto-5", "sem-perfil", "ana-3", "beto-1"],
        "ordem cronológica"
    );

    let janela = db
        .security_events(
            agora - chrono::Duration::days(6),
            agora - chrono::Duration::days(2),
            None,
        )
        .await
        .unwrap();
    assert_eq!(detalhes(janela), ["beto-5", "sem-perfil", "ana-3"]);

    let da_ana = db
        .security_events(agora - chrono::Duration::days(30), agora, Some(ana))
        .await
        .unwrap();
    assert_eq!(detalhes(da_ana), ["ana-10", "ana-3"]);

    let nenhum = db
        .security_events(
            agora - chrono::Duration::days(30),
            agora,
            Some(Uuid::new_v4()),
        )
        .await
        .unwrap();
    assert!(nenhum.is_empty());
}

#[tokio::test]
async fn evento_de_operacao_leva_o_perfil_inclusive_na_falha() {
    let (_dir, db) = novo_banco().await;
    let p = perfil();
    db.create_profile(&ctx(), &p).await.unwrap();
    let fantasma = Uuid::new_v4(); // perfil que não existe: a regra falha e o log ainda grava
    let r = regra(fantasma, Uuid::new_v4());
    assert!(db.create_active_rule(&ctx(), &r).await.is_err());

    let agora = Utc::now();
    let h = agora - chrono::Duration::hours(1);
    let do_filho = db
        .security_events(h, agora, Some(p.profile_id))
        .await
        .unwrap();
    assert_eq!(do_filho.len(), 1);
    assert_eq!(do_filho[0].action_attempted, ActionType::AddProfile);
    let do_fantasma = db.security_events(h, agora, Some(fantasma)).await.unwrap();
    assert_eq!(
        (do_fantasma.len(), do_fantasma[0].transaction_status),
        (1, TransactionStatus::Failure)
    );
}

#[tokio::test]
async fn retencao_remove_so_o_que_passou_de_90_dias_e_registra_a_limpeza() {
    let (_dir, db) = novo_banco().await;
    for (dias, nome) in [
        (100, "velho-100"),
        (91, "velho-91"),
        (89, "novo-89"),
        (1, "novo-1"),
    ] {
        db.record_event(&evento_de(dias, None, nome)).await.unwrap();
    }
    assert_eq!(db.purge_expired_events(&ctx()).await.unwrap(), 2);

    let restantes: Vec<String> = eventos(&db).await.into_iter().map(|e| e.4).collect();
    assert_eq!(restantes.len(), 3);
    assert!(
        restantes.contains(&"novo-89".to_string()) && restantes.contains(&"novo-1".to_string())
    );
    assert!(
        restantes
            .iter()
            .any(|d| d.contains("RETENTION_PURGED") && d.contains("\"removed\":2")),
        "limpeza fica registrada"
    );

    // sem nada a remover, nada é registrado
    assert_eq!(db.purge_expired_events(&ctx()).await.unwrap(), 0);
    assert_eq!(eventos(&db).await.len(), 3);
}

#[tokio::test]
async fn banco_recusa_delete_de_evento_recente_mesmo_por_sql_direto() {
    let (_dir, db) = novo_banco().await;
    db.record_event(&evento_de(89, None, "novo-89"))
        .await
        .unwrap();
    db.record_event(&evento_de(91, None, "velho-91"))
        .await
        .unwrap();

    let recente = sqlx::query("DELETE FROM security_log WHERE rejection_reason_detail = 'novo-89'")
        .execute(db.pool())
        .await;
    assert!(recente.unwrap_err().to_string().contains("90 dias"));
    let velho = sqlx::query("DELETE FROM security_log WHERE rejection_reason_detail = 'velho-91'")
        .execute(db.pool())
        .await
        .unwrap();
    assert_eq!(velho.rows_affected(), 1);
    assert_eq!(eventos(&db).await.len(), 1);
}

#[tokio::test]
async fn update_continua_recusado_mesmo_em_evento_velho() {
    let (_dir, db) = novo_banco().await;
    db.record_event(&evento_de(200, None, "velho"))
        .await
        .unwrap();
    assert!(
        sqlx::query("UPDATE security_log SET rejection_reason_detail = 'x'")
            .execute(db.pool())
            .await
            .is_err()
    );
}
