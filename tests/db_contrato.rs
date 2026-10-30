//! Testes contra SQLite real em arquivo temporário (sem mock de banco).

use chrono::{NaiveTime, Utc};
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use tempfile::TempDir;
use uuid::Uuid;

async fn novo_banco() -> (TempDir, KioskDbManager) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("kiosk.db").display());
    let db = KioskDbManager::new(&url).await.unwrap();
    (dir, db)
}

fn app(nome: &str) -> AppMetadata {
    AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: nome.into(),
        status: ProcessStatus::Monitored,
    }
}

fn perfil(sid: &str) -> ChildProfile {
    ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: sid.into(),
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
        daily_limit_seconds: Some(3600),
        time_window: Some(TimeWindow {
            start: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
            end: NaiveTime::from_hms_opt(20, 30, 0).unwrap(),
        }),
        effective_until_timestamp: None,
        warning_seconds: None,
    }
}

#[tokio::test]
async fn banco_novo_tem_migracoes_e_foreign_keys_ativas() {
    let (_dir, db) = novo_banco().await;
    let (fk,): (i64,) = sqlx::query_as("PRAGMA foreign_keys")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(fk, 1);
    for tabela in [
        "app_metadata",
        "child_profile",
        "profile_time_window",
        "active_rules",
        "usage_ledger",
        "security_log",
    ] {
        let (n,): (i64,) =
            sqlx::query_as("SELECT count(*) FROM sqlite_master WHERE type='table' AND name = ?")
                .bind(tabela)
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(n, 1, "tabela ausente: {tabela}");
    }
}

#[tokio::test]
async fn regra_criada_e_lida_de_volta_com_campos_nulos_preservados() {
    let (_dir, db) = novo_banco().await;
    let (a, p) = (app("game.exe"), perfil("S-1-5-21-1"));
    db.create_app(&EventContext::local(), &a).await.unwrap();
    db.create_profile(&EventContext::local(), &p).await.unwrap();

    let mut r = regra(p.profile_id, a.app_id);
    r.daily_limit_seconds = None;
    r.time_window = None;
    db.create_active_rule(&EventContext::local(), &r)
        .await
        .unwrap();

    let lida = db.get_active_rule(r.rule_id).await.unwrap().unwrap();
    assert_eq!(
        lida.daily_limit_seconds, None,
        "nulo deve significar sem limite, não 0"
    );
    assert_eq!(lida.time_window, None);
    assert_eq!(lida.state, RuleState::Pending);
    assert_eq!(
        lida.creation_timestamp.timestamp(),
        r.creation_timestamp.timestamp()
    );

    db.set_rule_state(&EventContext::local(), r.rule_id, RuleState::Active)
        .await
        .unwrap();
    assert_eq!(
        db.get_active_rule(r.rule_id).await.unwrap().unwrap().state,
        RuleState::Active
    );
}

#[tokio::test]
async fn janela_da_regra_faz_ida_e_volta() {
    let (_dir, db) = novo_banco().await;
    let (a, p) = (app("game.exe"), perfil("S-1-5-21-1"));
    db.create_app(&EventContext::local(), &a).await.unwrap();
    db.create_profile(&EventContext::local(), &p).await.unwrap();
    let r = regra(p.profile_id, a.app_id);
    db.create_active_rule(&EventContext::local(), &r)
        .await
        .unwrap();
    let lida = db.get_active_rule(r.rule_id).await.unwrap().unwrap();
    assert_eq!(lida.time_window, r.time_window);
    assert_eq!(lida.daily_limit_seconds, Some(3600));
}

#[tokio::test]
async fn transacao_sem_commit_nao_persiste() {
    let (_dir, db) = novo_banco().await;
    let a = app("game.exe");
    {
        let mut tx = db.begin_transaction().await.unwrap();
        sqlx::query("INSERT INTO app_metadata (app_id, process_name) VALUES (?, ?)")
            .bind(a.app_id.to_string())
            .bind(&a.process_name)
            .execute(&mut *tx)
            .await
            .unwrap();
    } // drop sem commit => rollback
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM app_metadata")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn fk_para_app_inexistente_e_recusada() {
    let (_dir, db) = novo_banco().await;
    let p = perfil("S-1-5-21-1");
    db.create_profile(&EventContext::local(), &p).await.unwrap();
    let r = regra(p.profile_id, Uuid::new_v4());
    assert!(
        db.create_active_rule(&EventContext::local(), &r)
            .await
            .is_err()
    );
    assert!(db.get_active_rule(r.rule_id).await.unwrap().is_none());
}

#[tokio::test]
async fn windows_sid_duplicado_e_recusado() {
    let (_dir, db) = novo_banco().await;
    db.create_profile(&EventContext::local(), &perfil("S-1-5-21-1"))
        .await
        .unwrap();
    assert!(
        db.create_profile(&EventContext::local(), &perfil("S-1-5-21-1"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn process_name_duplicado_e_recusado_sem_diferenciar_caixa() {
    let (_dir, db) = novo_banco().await;
    db.create_app(&EventContext::local(), &app("Game.exe"))
        .await
        .unwrap();
    assert!(
        db.create_app(&EventContext::local(), &app("game.EXE"))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn janela_invertida_e_estado_invalido_sao_recusados_pelo_banco() {
    let (_dir, db) = novo_banco().await;
    let p = perfil("S-1-5-21-1");
    db.create_profile(&EventContext::local(), &p).await.unwrap();
    let invertida = TimeWindow {
        start: NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
        end: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
    };
    assert!(
        db.set_profile_window(&EventContext::local(), p.profile_id, 1, &invertida)
            .await
            .is_err()
    );
    let valida = TimeWindow {
        start: invertida.end,
        end: invertida.start,
    };
    assert!(
        db.set_profile_window(&EventContext::local(), p.profile_id, 7, &valida)
            .await
            .is_err(),
        "weekday fora de 0..=6"
    );
    db.set_profile_window(&EventContext::local(), p.profile_id, 1, &valida)
        .await
        .unwrap();

    let a = app("game.exe");
    db.create_app(&EventContext::local(), &a).await.unwrap();
    let ins = sqlx::query(
        "INSERT INTO active_rules (rule_id, profile_id, app_id, creation_timestamp, current_state) VALUES ('r', ?, ?, 't', 'XYZ')",
    )
    .bind(p.profile_id.to_string())
    .bind(a.app_id.to_string())
    .execute(db.pool())
    .await;
    assert!(ins.is_err());
}

#[tokio::test]
async fn set_rule_state_de_regra_inexistente_falha_alto() {
    let (_dir, db) = novo_banco().await;
    assert!(
        db.set_rule_state(&EventContext::local(), Uuid::new_v4(), RuleState::Revoked)
            .await
            .is_err()
    );
}
