//! E-003 F2: divisão do uso por dia local (virada de dia, horário de verão) e ledger em SQLite real.

use chrono::{Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use kiosk_engine::uso::dividir_por_dia_local;
use tempfile::TempDir;
use uuid::Uuid;

const NY: Tz = chrono_tz::America::New_York;
const SP: Tz = chrono_tz::America::Sao_Paulo;

fn dia(a: i32, m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(a, m, d).unwrap()
}

fn utc(a: i32, m: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(a, m, d, h, mi, 0).unwrap()
}

#[test]
fn intervalo_dentro_do_dia_fica_inteiro_nele() {
    let r = dividir_por_dia_local(&NY, utc(2026, 6, 10, 15, 0), utc(2026, 6, 10, 15, 5)).unwrap();
    assert_eq!(r, [(dia(2026, 6, 10), 300)]);
}

#[test]
fn intervalo_que_cruza_a_meia_noite_local_e_dividido() {
    // 23:50 -> 00:10 em Nova York (UTC-4 em junho) = 03:50 -> 04:10 UTC
    let r = dividir_por_dia_local(&NY, utc(2026, 6, 11, 3, 50), utc(2026, 6, 11, 4, 10)).unwrap();
    assert_eq!(r, [(dia(2026, 6, 10), 600), (dia(2026, 6, 11), 600)]);
}

#[test]
fn a_meia_noite_utc_nao_e_a_virada_do_dia() {
    // 00:30 UTC ainda é 20:30 do dia anterior em Nova York
    let r = dividir_por_dia_local(&NY, utc(2026, 6, 11, 0, 30), utc(2026, 6, 11, 0, 35)).unwrap();
    assert_eq!(r, [(dia(2026, 6, 10), 300)]);
}

#[test]
fn dia_de_inicio_do_horario_de_verao_tem_23_horas() {
    // 2026-03-08 em Nova York: 02:00 -> 03:00. Dia local = 05:00Z (UTC-5) até 04:00Z do dia 9 (UTC-4).
    let r = dividir_por_dia_local(&NY, utc(2026, 3, 8, 5, 0), utc(2026, 3, 9, 4, 0)).unwrap();
    assert_eq!(r, [(dia(2026, 3, 8), 23 * 3600)]);
}

#[test]
fn dia_de_fim_do_horario_de_verao_tem_25_horas() {
    // 2026-11-01 em Nova York: 02:00 -> 01:00. Dia local = 04:00Z (UTC-4) até 05:00Z do dia 2 (UTC-5).
    let r = dividir_por_dia_local(&NY, utc(2026, 11, 1, 4, 0), utc(2026, 11, 2, 5, 0)).unwrap();
    assert_eq!(r, [(dia(2026, 11, 1), 25 * 3600)]);
}

#[test]
fn meia_noite_inexistente_no_horario_de_verao_nao_trava_nem_perde_segundos() {
    // São Paulo em 2018-11-04: 00:00 não existiu (relógio foi de 23:59:59 a 01:00).
    // 02:00Z = 23:00 do dia 3 (UTC-3); 05:00Z = 03:00 do dia 4 (UTC-2).
    let r = dividir_por_dia_local(&SP, utc(2018, 11, 4, 2, 0), utc(2018, 11, 4, 5, 0)).unwrap();
    assert_eq!(r, [(dia(2018, 11, 3), 3600), (dia(2018, 11, 4), 7200)]);
    assert_eq!(r.iter().map(|p| p.1).sum::<u64>(), 3 * 3600);
}

#[test]
fn intervalo_negativo_e_recusado_e_vazio_nao_gera_partes() {
    assert!(dividir_por_dia_local(&NY, utc(2026, 6, 10, 15, 5), utc(2026, 6, 10, 15, 0)).is_err());
    assert!(
        dividir_por_dia_local(&NY, utc(2026, 6, 10, 15, 0), utc(2026, 6, 10, 15, 0))
            .unwrap()
            .is_empty()
    );
}

async fn banco_com_perfil_e_apps() -> (TempDir, KioskDbManager, Uuid, Uuid, Uuid) {
    let dir = tempfile::tempdir().unwrap();
    let db = KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
        .await
        .unwrap();
    let ctx = EventContext::local();
    let perfil = ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: "S-1-5-21-1".into(),
        display_name: "Filho".into(),
        daily_total_limit_seconds: None,
    };
    let app = |nome: &str| AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: nome.into(),
        status: ProcessStatus::Monitored,
    };
    let (a, b) = (app("jogo.exe"), app("video.exe"));
    db.create_profile(&ctx, &perfil).await.unwrap();
    db.create_app(&ctx, &a).await.unwrap();
    db.create_app(&ctx, &b).await.unwrap();
    (dir, db, perfil.profile_id, a.app_id, b.app_id)
}

#[tokio::test]
async fn ledger_acumula_por_app_e_por_dia_com_total_do_perfil() {
    let (_d, db, p, jogo, video) = banco_com_perfil_e_apps().await;
    let ctx = EventContext::local();
    let t0 = utc(2026, 6, 10, 15, 0);
    db.registrar_uso(&ctx, p, jogo, t0, t0 + Duration::seconds(300), &NY)
        .await
        .unwrap();
    db.registrar_uso(
        &ctx,
        p,
        jogo,
        t0 + Duration::seconds(300),
        t0 + Duration::seconds(500),
        &NY,
    )
    .await
    .unwrap();
    db.registrar_uso(&ctx, p, video, t0, t0 + Duration::seconds(100), &NY)
        .await
        .unwrap();

    let c = db.consumo_do_dia(p, jogo, dia(2026, 6, 10)).await.unwrap();
    assert_eq!((c.app_segundos, c.total_segundos), (500, 600));
    let v = db.consumo_do_dia(p, video, dia(2026, 6, 10)).await.unwrap();
    assert_eq!((v.app_segundos, v.total_segundos), (100, 600));
}

#[tokio::test]
async fn consumo_reinicia_na_virada_do_dia_local() {
    let (_d, db, p, jogo, _v) = banco_com_perfil_e_apps().await;
    let ctx = EventContext::local();
    // 23:55 -> 00:05 em Nova York
    db.registrar_uso(
        &ctx,
        p,
        jogo,
        utc(2026, 6, 11, 3, 55),
        utc(2026, 6, 11, 4, 5),
        &NY,
    )
    .await
    .unwrap();
    assert_eq!(
        db.consumo_do_dia(p, jogo, dia(2026, 6, 10))
            .await
            .unwrap()
            .app_segundos,
        300
    );
    assert_eq!(
        db.consumo_do_dia(p, jogo, dia(2026, 6, 11))
            .await
            .unwrap()
            .app_segundos,
        300
    );
    assert_eq!(
        db.consumo_do_dia(p, jogo, dia(2026, 6, 12)).await.unwrap(),
        Default::default()
    );
}

async fn eventos_de_tamper(db: &KioskDbManager) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM security_log WHERE event_type = 'TAMPER_SUSPECTED'")
        .fetch_one(db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn salto_de_relogio_gera_tamper_e_nao_contabiliza() {
    let (_d, db, p, jogo, _v) = banco_com_perfil_e_apps().await;
    let ctx = EventContext::local();
    let t0 = utc(2026, 6, 10, 15, 0);

    // salto para a frente (uma hora num único tick) e para trás
    assert!(
        db.registrar_uso(&ctx, p, jogo, t0, t0 + Duration::hours(1), &NY)
            .await
            .is_err()
    );
    assert!(
        db.registrar_uso(&ctx, p, jogo, t0, t0 - Duration::seconds(30), &NY)
            .await
            .is_err()
    );

    assert_eq!(eventos_de_tamper(&db).await, 2);
    assert_eq!(
        db.consumo_do_dia(p, jogo, dia(2026, 6, 10)).await.unwrap(),
        Default::default()
    );
}

#[tokio::test]
async fn uso_de_app_ou_perfil_inexistente_falha_alto_e_nao_grava() {
    let (_d, db, p, jogo, _v) = banco_com_perfil_e_apps().await;
    let ctx = EventContext::local();
    let t0 = utc(2026, 6, 10, 15, 0);
    let fim = t0 + Duration::seconds(60);
    assert!(
        db.registrar_uso(&ctx, p, Uuid::new_v4(), t0, fim, &NY)
            .await
            .is_err()
    );
    assert!(
        db.registrar_uso(&ctx, Uuid::new_v4(), jogo, t0, fim, &NY)
            .await
            .is_err()
    );
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM usage_ledger")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}
