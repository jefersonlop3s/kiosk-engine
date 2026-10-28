//! E-005 F1/F3: testes HTTP reais contra o serviço (loopback), cliente TCP cru.

use std::net::SocketAddr;
use std::sync::Arc;

use chrono::{Duration, Utc};
use ed25519_dalek::SigningKey;
use kiosk_engine::api::{self, MAX_CORPO_BYTES};
use kiosk_engine::assinatura::*;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use rand::rngs::OsRng;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use uuid::Uuid;

const REJEITADA: &str = r#"{"status":"rejected"}"#;

struct Servico {
    _dir: TempDir,
    db: Arc<KioskDbManager>,
    addr: SocketAddr,
    chave: SigningKey,
    device_id: Uuid,
    perfil: Uuid,
    app: Uuid,
}

async fn servico() -> Servico {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(
        KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
            .await
            .unwrap(),
    );
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

    let chave = SigningKey::generate(&mut OsRng);
    let codigo = db.criar_codigo_pareamento(&ctx, Utc::now()).await.unwrap();
    let device_id = db
        .parear_dispositivo(
            &ctx,
            &codigo,
            chave.verifying_key().as_bytes(),
            "Celular",
            Utc::now(),
        )
        .await
        .unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(api::servir(listener, db.clone()));
    Servico {
        _dir: dir,
        db,
        addr,
        chave,
        device_id,
        perfil: perfil.profile_id,
        app: app.app_id,
    }
}

type Cabecalhos = Vec<(String, String)>;

impl Servico {
    fn assinado(&self, caminho: &str, corpo: &[u8]) -> Cabecalhos {
        let (ts, nonce) = (Utc::now().timestamp(), novo_nonce());
        vec![
            (CABECALHO_DISPOSITIVO.into(), self.device_id.to_string()),
            (CABECALHO_TIMESTAMP.into(), ts.to_string()),
            (CABECALHO_NONCE.into(), nonce.clone()),
            (
                CABECALHO_ASSINATURA.into(),
                assinar(&self.chave, "POST", caminho, ts, &nonce, corpo),
            ),
        ]
    }

    async fn comando(&self, acao: &str, corpo: &str) -> (u16, String) {
        let caminho = format!("/api/windows/{acao}");
        let h = self.assinado(&caminho, corpo.as_bytes());
        http(self.addr, "POST", &caminho, &h, corpo.as_bytes()).await
    }

    async fn contar(&self, tipo: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM security_log WHERE event_type = ?")
            .bind(tipo)
            .fetch_one(self.db.pool())
            .await
            .unwrap()
    }

    async fn regras(&self) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM active_rules")
            .fetch_one(self.db.pool())
            .await
            .unwrap()
    }

    fn nova_regra(&self, extra: &str) -> String {
        format!(
            r#"{{"profile_id":"{}","app_id":"{}"{extra}}}"#,
            self.perfil, self.app
        )
    }
}

async fn http(
    addr: SocketAddr,
    metodo: &str,
    caminho: &str,
    h: &Cabecalhos,
    corpo: &[u8],
) -> (u16, String) {
    let mut s = TcpStream::connect(addr).await.unwrap();
    let mut req = format!(
        "{metodo} {caminho} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\nConnection: close\r\n",
        corpo.len()
    );
    for (k, v) in h {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).await.unwrap();
    s.write_all(corpo).await.unwrap();
    let mut resp = Vec::new();
    let _ = s.read_to_end(&mut resp).await; // o servidor pode fechar cedo em corpo grande
    let texto = String::from_utf8_lossy(&resp).into_owned();
    let status = texto
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let corpo = texto.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    (status, corpo)
}

fn id_da_resposta(corpo: &str) -> Uuid {
    let v: serde_json::Value = serde_json::from_str(corpo).unwrap();
    Uuid::parse_str(v["id"].as_str().unwrap()).unwrap()
}

#[tokio::test]
async fn ciclo_completo_criar_ativar_revogar_pela_api() {
    let s = servico().await;
    let (st, corpo) = s
        .comando(
            "add_rule",
            &s.nova_regra(
                r#","daily_limit_seconds":3600,"window_start":"08:00","window_end":"20:00""#,
            ),
        )
        .await;
    assert_eq!(st, 202, "{corpo}");
    let id = id_da_resposta(&corpo);
    let r = s.db.get_active_rule(id).await.unwrap().unwrap();
    assert_eq!(
        (r.state, r.daily_limit_seconds),
        (RuleState::Pending, Some(3600))
    );
    assert!(r.time_window.is_some());

    let corpo_id = format!(r#"{{"rule_id":"{id}"}}"#);
    let (st, corpo) = s.comando("activate_rule", &corpo_id).await;
    assert_eq!((st, corpo.as_str()), (202, r#"{"status":"accepted"}"#));
    assert_eq!(
        s.db.get_active_rule(id).await.unwrap().unwrap().state,
        RuleState::Active
    );

    let (st, _) = s.comando("revoke_rule", &corpo_id).await;
    assert_eq!(st, 202);
    assert_eq!(
        s.db.get_active_rule(id).await.unwrap().unwrap().state,
        RuleState::Revoked
    );

    let acoes: Vec<(String, String)> = sqlx::query_as(
        "SELECT action_attempted, client_ip FROM security_log \
         WHERE event_type = 'SUCCESS' AND action_attempted IN ('ADD_RULE','ACTIVATE_RULE','REVOKE') ORDER BY rowid",
    )
    .fetch_all(s.db.pool())
    .await
    .unwrap();
    let nomes: Vec<&str> = acoes.iter().map(|a| a.0.as_str()).collect();
    assert_eq!(nomes, ["ADD_RULE", "ACTIVATE_RULE", "REVOKE"]);
    assert!(
        acoes.iter().all(|a| a.1 == "127.0.0.1"),
        "o IP vem da conexão"
    );
}

#[tokio::test]
async fn requisicao_sem_assinatura_e_recusada_com_resposta_generica() {
    let s = servico().await;
    let corpo = s.nova_regra("");
    let (st, resp) = http(
        s.addr,
        "POST",
        "/api/windows/add_rule",
        &vec![],
        corpo.as_bytes(),
    )
    .await;
    assert_eq!((st, resp.as_str()), (403, REJEITADA));
    assert_eq!((s.contar("AUTH_FAIL").await, s.regras().await), (1, 0));
}

#[tokio::test]
async fn replay_da_mesma_requisicao_e_recusado() {
    let s = servico().await;
    let corpo = s.nova_regra("");
    let h = s.assinado("/api/windows/add_rule", corpo.as_bytes());
    assert_eq!(
        http(
            s.addr,
            "POST",
            "/api/windows/add_rule",
            &h,
            corpo.as_bytes()
        )
        .await
        .0,
        202
    );
    let (st, resp) = http(
        s.addr,
        "POST",
        "/api/windows/add_rule",
        &h,
        corpo.as_bytes(),
    )
    .await;
    assert_eq!((st, resp.as_str()), (403, REJEITADA));
    assert_eq!(s.regras().await, 1, "o replay não criou outra regra");
}

#[tokio::test]
async fn corpo_adulterado_em_transito_e_recusado() {
    let s = servico().await;
    let assinado_sobre = s.nova_regra(r#","daily_limit_seconds":600"#);
    let enviado = s.nova_regra(r#","daily_limit_seconds":86400"#);
    let h = s.assinado("/api/windows/add_rule", assinado_sobre.as_bytes());
    let (st, _) = http(
        s.addr,
        "POST",
        "/api/windows/add_rule",
        &h,
        enviado.as_bytes(),
    )
    .await;
    assert_eq!(st, 403);
    assert_eq!(s.regras().await, 0);
}

#[tokio::test]
async fn caminho_diferente_do_assinado_e_recusado() {
    let s = servico().await;
    let corpo = format!(r#"{{"rule_id":"{}"}}"#, Uuid::new_v4());
    let h = s.assinado("/api/windows/activate_rule", corpo.as_bytes());
    let (st, _) = http(
        s.addr,
        "POST",
        "/api/windows/revoke_rule",
        &h,
        corpo.as_bytes(),
    )
    .await;
    assert_eq!(st, 403);
    assert_eq!(
        s.contar("AUTH_FAIL").await,
        1,
        "falhou na assinatura, não na validação"
    );
}

#[tokio::test]
async fn parametros_invalidos_de_dispositivo_autenticado_sao_recusados_e_registrados() {
    let s = servico().await;
    let fantasma = Uuid::new_v4();
    let casos: Vec<(&str, String)> = vec![
        ("add_rule", s.nova_regra(r#","daily_limit_seconds":86401"#)),
        ("add_rule", s.nova_regra(r#","window_start":"08:00""#)),
        (
            "add_rule",
            s.nova_regra(r#","window_start":"20:00","window_end":"08:00""#),
        ),
        (
            "add_rule",
            s.nova_regra(r#","window_start":"8:00","window_end":"20:00""#),
        ),
        (
            "add_rule",
            s.nova_regra(r#","window_start":"25:00","window_end":"26:00""#),
        ),
        (
            "add_rule",
            s.nova_regra(r#","effective_until":"2001-01-01T00:00:00Z""#),
        ),
        ("add_rule", s.nova_regra(r#","effective_until":"amanhã""#)),
        ("add_rule", s.nova_regra(r#","campo_novo":1"#)),
        ("add_rule", r#"{"profile_id":"x","app_id":"y"}"#.into()),
        (
            "add_rule",
            format!(r#"{{"profile_id":"{fantasma}","app_id":"{}"}}"#, s.app),
        ),
        (
            "add_rule",
            format!(r#"{{"profile_id":"{}","app_id":"{fantasma}"}}"#, s.perfil),
        ),
        ("add_rule", "isto não é json".into()),
        ("add_rule", String::new()),
        ("revoke_rule", format!(r#"{{"rule_id":"{fantasma}"}}"#)),
        ("activate_rule", r#"{"rule_id":"não-uuid"}"#.into()),
        (
            "revoke_rule",
            format!(r#"{{"rule_id":"{fantasma}","extra":true}}"#),
        ),
        ("apagar_tudo", "{}".into()),
    ];
    let total = casos.len() as i64;
    for (acao, corpo) in casos {
        let (st, resp) = s.comando(acao, &corpo).await;
        assert_eq!((st, resp.as_str()), (403, REJEITADA), "{acao} {corpo}");
    }
    assert_eq!(s.contar("VALIDATION_FAIL").await, total);
    assert_eq!(s.regras().await, 0);
    // o motivo exato fica só no log
    let motivos: Vec<String> = sqlx::query_scalar(
        "SELECT rejection_reason_detail FROM security_log WHERE event_type = 'VALIDATION_FAIL'",
    )
    .fetch_all(s.db.pool())
    .await
    .unwrap();
    assert!(motivos.iter().any(|m| m.contains("86400")));
    assert!(motivos.iter().any(|m| m.contains("perfil inexistente")));
    assert!(motivos.iter().any(|m| m.contains("ação desconhecida")));
}

#[tokio::test]
async fn dispositivo_revogado_perde_o_acesso_na_proxima_requisicao() {
    let s = servico().await;
    assert_eq!(s.comando("add_rule", &s.nova_regra("")).await.0, 202);
    s.db.revogar_dispositivo(&EventContext::local(), s.device_id, Utc::now())
        .await
        .unwrap();
    let (st, resp) = s.comando("add_rule", &s.nova_regra("")).await;
    assert_eq!((st, resp.as_str()), (403, REJEITADA));
    assert_eq!(s.regras().await, 1);
}

#[tokio::test]
async fn corpo_acima_do_limite_e_recusado_e_o_servico_continua() {
    let s = servico().await;
    let grande = vec![b'a'; MAX_CORPO_BYTES + 1];
    let h = s.assinado("/api/windows/add_rule", &grande);
    let (st, _) = http(s.addr, "POST", "/api/windows/add_rule", &h, &grande).await;
    assert_eq!(st, 413);
    assert_eq!(s.comando("add_rule", &s.nova_regra("")).await.0, 202);
}

#[tokio::test]
async fn so_post_e_so_a_rota_da_api_existem() {
    let s = servico().await;
    let (st, _) = http(s.addr, "GET", "/api/windows/add_rule", &vec![], b"").await;
    assert_eq!(st, 405);
    let (st, _) = http(s.addr, "POST", "/admin", &vec![], b"{}").await;
    assert_eq!(st, 404);
    assert_eq!(s.regras().await, 0);
}

#[tokio::test]
async fn nenhuma_resposta_externa_vaza_detalhe_interno() {
    let s = servico().await;
    let mut h = s.assinado("/api/windows/add_rule", b"{}");
    h[3].1 = "AAAA".into(); // assinatura inválida
    let (_, a) = http(s.addr, "POST", "/api/windows/add_rule", &h, b"{}").await;
    let (_, b) = s.comando("add_rule", "{}").await; // autenticada, mas inválida
    let (_, c) = s.comando("apagar_tudo", "{}").await;
    assert!(
        a == REJEITADA && b == REJEITADA && c == REJEITADA,
        "{a} | {b} | {c}"
    );
    for proibido in [
        &s.device_id.to_string(),
        "assinatura",
        "perfil",
        "nonce",
        "janela",
    ] {
        assert!(!a.contains(proibido) && !b.contains(proibido) && !c.contains(proibido));
    }
}

#[tokio::test]
async fn timestamp_velho_e_recusado_pela_api() {
    let s = servico().await;
    let corpo = s.nova_regra("");
    let ts = (Utc::now() - Duration::seconds(120)).timestamp();
    let nonce = novo_nonce();
    let assinatura = assinar(
        &s.chave,
        "POST",
        "/api/windows/add_rule",
        ts,
        &nonce,
        corpo.as_bytes(),
    );
    let h: Cabecalhos = vec![
        (CABECALHO_DISPOSITIVO.into(), s.device_id.to_string()),
        (CABECALHO_TIMESTAMP.into(), ts.to_string()),
        (CABECALHO_NONCE.into(), nonce),
        (CABECALHO_ASSINATURA.into(), assinatura),
    ];
    let (st, _) = http(
        s.addr,
        "POST",
        "/api/windows/add_rule",
        &h,
        corpo.as_bytes(),
    )
    .await;
    assert_eq!(st, 403);
    assert_eq!(s.regras().await, 0);
}

#[test]
fn a_politica_de_enderecos_aceita_so_loopback_e_tailscale() {
    let ok = [
        "127.0.0.1:8443",
        "[::1]:8443",
        "100.64.0.1:8443",
        "100.127.255.255:8443",
        "[fd7a:115c:a1e0::1]:8443",
    ];
    let ruim = [
        "0.0.0.0:8443",
        "[::]:8443",
        "192.168.0.10:8443",
        "10.0.0.5:8443",
        "172.16.3.4:8443",
        "8.8.8.8:8443",
        "100.63.255.255:8443",
        "100.128.0.1:8443",
        "[fe80::1]:8443",
    ];
    for a in ok {
        assert!(api::validar_enderecos(&[a.parse().unwrap()]).is_ok(), "{a}");
    }
    for a in ruim {
        assert!(
            api::validar_enderecos(&[a.parse().unwrap()]).is_err(),
            "{a}"
        );
    }
    assert!(matches!(
        api::validar_enderecos(&[]),
        Err(api::ErroServidor::SemEnderecos)
    ));
    // um endereço ruim na lista recusa tudo
    assert!(
        api::validar_enderecos(&["127.0.0.1:1".parse().unwrap(), "0.0.0.0:2".parse().unwrap()])
            .is_err()
    );
}

#[tokio::test]
async fn vincular_recusa_curinga_e_lan_sem_abrir_nenhuma_porta() {
    assert!(
        api::vincular(&["0.0.0.0:0".parse().unwrap()])
            .await
            .is_err()
    );
    assert!(
        api::vincular(&[
            "127.0.0.1:0".parse().unwrap(),
            "192.168.0.10:0".parse().unwrap()
        ])
        .await
        .is_err()
    );
    let abertos = api::vincular(&["127.0.0.1:0".parse().unwrap()])
        .await
        .unwrap();
    assert_eq!(abertos.len(), 1);
}

// --- GC-14: modify_limit e consultas de uso e de log

async fn regra_criada(s: &Servico, extra: &str) -> Uuid {
    let (st, corpo) = s.comando("add_rule", &s.nova_regra(extra)).await;
    assert_eq!(st, 202, "{corpo}");
    id_da_resposta(&corpo)
}

#[tokio::test]
async fn modify_limit_altera_o_limite_e_grava_o_evento_com_o_perfil() {
    let s = servico().await;
    let id = regra_criada(&s, r#","daily_limit_seconds":3600"#).await;
    let (st, corpo) = s
        .comando(
            "modify_limit",
            &format!(r#"{{"rule_id":"{id}","daily_limit_seconds":1800}}"#),
        )
        .await;
    assert_eq!((st, corpo.as_str()), (202, r#"{"status":"accepted"}"#));
    assert_eq!(
        s.db.get_active_rule(id)
            .await
            .unwrap()
            .unwrap()
            .daily_limit_seconds,
        Some(1800)
    );
    let (acao, perfil): (String, Option<String>) = sqlx::query_as(
        "SELECT action_attempted, profile_id FROM security_log WHERE action_attempted = 'MODIFY_LIMIT' AND event_type = 'SUCCESS'",
    )
    .fetch_one(s.db.pool())
    .await
    .unwrap();
    assert_eq!(
        (acao.as_str(), perfil),
        ("MODIFY_LIMIT", Some(s.perfil.to_string()))
    );
}

#[tokio::test]
async fn modify_limit_recusa_regra_revogada_regra_inexistente_e_limite_invalido() {
    let s = servico().await;
    let id = regra_criada(&s, r#","daily_limit_seconds":3600"#).await;
    let corpo_id = format!(r#"{{"rule_id":"{id}"}}"#);
    assert_eq!(s.comando("revoke_rule", &corpo_id).await.0, 202);

    let casos = [
        format!(r#"{{"rule_id":"{id}","daily_limit_seconds":600}}"#), // revogada
        format!(
            r#"{{"rule_id":"{}","daily_limit_seconds":600}}"#,
            Uuid::new_v4()
        ), // inexistente
        format!(r#"{{"rule_id":"{id}","daily_limit_seconds":86401}}"#), // acima do máximo
        format!(r#"{{"rule_id":"{id}"}}"#),                           // sem limite
        format!(r#"{{"rule_id":"{id}","daily_limit_seconds":-5}}"#),  // negativo
        format!(r#"{{"rule_id":"{id}","daily_limit_seconds":600,"x":1}}"#), // campo extra
    ];
    for c in casos {
        let (st, resp) = s.comando("modify_limit", &c).await;
        assert_eq!((st, resp.as_str()), (403, REJEITADA), "{c}");
    }
    assert_eq!(
        s.db.get_active_rule(id)
            .await
            .unwrap()
            .unwrap()
            .daily_limit_seconds,
        Some(3600)
    );
}

#[tokio::test]
async fn usage_devolve_o_consumo_do_dia_e_exige_autenticacao() {
    let s = servico().await;
    let ctx = EventContext::local();
    let dia = chrono::NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
    let ini = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 3, 9, 12, 0, 0).unwrap();
    let fim = ini + Duration::seconds(300);
    s.db.registrar_uso(&ctx, s.perfil, s.app, ini, fim, &Utc)
        .await
        .unwrap();
    s.db.registrar_sessao(&ctx, s.perfil, ini, fim + Duration::seconds(100), &Utc)
        .await
        .unwrap();

    let corpo = format!(r#"{{"profile_id":"{}","date":"{dia}"}}"#, s.perfil);
    let (st, resp) = s.comando("usage", &corpo).await;
    assert_eq!(st, 200, "{resp}");
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["session_seconds"], 400);
    assert_eq!(v["apps"][0]["process_name"], "jogo.exe");
    assert_eq!(v["apps"][0]["seconds"], 300);

    let vazio = format!(r#"{{"profile_id":"{}","date":"2026-03-10"}}"#, s.perfil);
    let (st, resp) = s.comando("usage", &vazio).await;
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(
        (
            st,
            v["session_seconds"].as_u64(),
            v["apps"].as_array().map(Vec::len)
        ),
        (200, Some(0), Some(0))
    );

    // sem assinatura: nenhum dado
    let (st, resp) = http(
        s.addr,
        "POST",
        "/api/windows/usage",
        &vec![],
        corpo.as_bytes(),
    )
    .await;
    assert_eq!((st, resp.as_str()), (403, REJEITADA));
    // parâmetros inválidos
    for ruim in [
        r#"{"profile_id":"x","date":"2026-03-09"}"#.to_string(),
        format!(r#"{{"profile_id":"{}","date":"09/03/2026"}}"#, s.perfil),
        format!(
            r#"{{"profile_id":"{}","date":"2026-03-09"}}"#,
            Uuid::new_v4()
        ),
    ] {
        assert_eq!(
            s.comando("usage", &ruim).await,
            (403, REJEITADA.to_string()),
            "{ruim}"
        );
    }
}

#[tokio::test]
async fn log_traz_eventos_filtra_por_perfil_e_omite_o_detalhe_das_falhas_de_api() {
    let s = servico().await;
    regra_criada(&s, "").await; // ADD_RULE / SUCCESS com o perfil
    // falha de autenticação: o detalhe cita o dispositivo e não pode sair pela API
    let mut h = s.assinado("/api/windows/add_rule", b"{}");
    h[3].1 = "AAAA".into();
    http(s.addr, "POST", "/api/windows/add_rule", &h, b"{}").await;
    s.comando("add_rule", "{}").await; // VALIDATION_FAIL
    // evento de enforcement: o detalhe sai
    s.db.record_event(
        &SecurityEvent::new(
            "local",
            ActionType::System,
            EventType::AppTerminated,
            TransactionStatus::Success,
            "app jogo.exe encerrado: LIMITE_DIARIO_APP",
        )
        .with_profile(Some(s.perfil)),
    )
    .await
    .unwrap();

    let janela = |extra: &str| {
        format!(
            r#"{{"from":"{}","to":"{}"{extra}}}"#,
            (Utc::now() - Duration::hours(1)).to_rfc3339(),
            (Utc::now() + Duration::minutes(1)).to_rfc3339()
        )
    };
    let (st, resp) = s.comando("log", &janela("")).await;
    assert_eq!(st, 200, "{resp}");
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let eventos = v["events"].as_array().unwrap();
    let tipos: Vec<&str> = eventos
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    for esperado in ["SUCCESS", "AUTH_FAIL", "VALIDATION_FAIL", "APP_TERMINATED"] {
        assert!(tipos.contains(&esperado), "falta {esperado} em {tipos:?}");
    }
    for e in eventos {
        let tipo = e["event_type"].as_str().unwrap();
        if [
            "AUTH_FAIL",
            "AUTHZ_FAIL",
            "VALIDATION_FAIL",
            "INTERNAL_ERROR",
        ]
        .contains(&tipo)
        {
            assert!(e["detail"].is_null(), "o detalhe de {tipo} vazou: {e}");
        }
        if tipo == "APP_TERMINATED" {
            assert_eq!(e["detail"], "app jogo.exe encerrado: LIMITE_DIARIO_APP");
        }
    }
    assert!(
        !resp.contains(&s.device_id.to_string()),
        "o id do dispositivo vazou pelo detalhe"
    );

    // por perfil: só os eventos que carregam o perfil
    let (_, resp) = s
        .comando("log", &janela(&format!(r#","profile_id":"{}""#, s.perfil)))
        .await;
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    let tipos: Vec<&str> = v["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event_type"].as_str().unwrap())
        .collect();
    assert!(
        tipos.contains(&"APP_TERMINATED") && !tipos.contains(&"AUTH_FAIL"),
        "{tipos:?}"
    );

    // limite
    let (_, resp) = s.comando("log", &janela(r#","limit":1"#)).await;
    let v: serde_json::Value = serde_json::from_str(&resp).unwrap();
    assert_eq!(v["events"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn log_recusa_periodo_e_limite_invalidos_e_exige_assinatura() {
    let s = servico().await;
    let agora = Utc::now();
    let corpo = |de: chrono::DateTime<Utc>, ate: chrono::DateTime<Utc>, extra: &str| {
        format!(
            r#"{{"from":"{}","to":"{}"{extra}}}"#,
            de.to_rfc3339(),
            ate.to_rfc3339()
        )
    };
    let ruins = [
        corpo(agora, agora - Duration::hours(1), ""), // invertido
        corpo(agora - Duration::days(32), agora, ""), // período longo demais
        corpo(agora - Duration::hours(1), agora, r#","limit":0"#), // limite zero
        corpo(agora - Duration::hours(1), agora, r#","limit":501"#), // limite acima do máximo
        corpo(agora - Duration::hours(1), agora, r#","profile_id":"x""#), // perfil ilegível
        corpo(
            agora - Duration::hours(1),
            agora,
            &format!(r#","profile_id":"{}""#, Uuid::new_v4()),
        ), // inexistente
        r#"{"from":"ontem","to":"hoje"}"#.to_string(),
    ];
    for r in ruins {
        assert_eq!(
            s.comando("log", &r).await,
            (403, REJEITADA.to_string()),
            "{r}"
        );
    }
    let ok = corpo(agora - Duration::days(31), agora, "");
    assert_eq!(s.comando("log", &ok).await.0, 200, "31 dias é o limite");
    let (st, resp) = http(s.addr, "POST", "/api/windows/log", &vec![], ok.as_bytes()).await;
    assert_eq!((st, resp.as_str()), (403, REJEITADA));
}
