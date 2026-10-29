//! E-005 F2 (ADR-006): assinatura, anti-replay, pareamento e autorização contra SQLite real.

use chrono::{DateTime, Duration, TimeZone, Utc};
use ed25519_dalek::SigningKey;
use kiosk_engine::assinatura::*;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use kiosk_engine::pareamento::*;
use rand::rngs::OsRng;
use tempfile::TempDir;
use uuid::Uuid;

fn agora() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap()
}

fn ctx() -> EventContext {
    EventContext {
        client_ip: "100.64.0.9".into(),
    }
}

async fn banco() -> (TempDir, KioskDbManager) {
    let dir = tempfile::tempdir().unwrap();
    let db = KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
        .await
        .unwrap();
    (dir, db)
}

struct Cliente {
    chave: SigningKey,
    device_id: Uuid,
}

async fn parear(db: &KioskDbManager, nome: &str) -> Cliente {
    let chave = SigningKey::generate(&mut OsRng);
    let codigo = db.criar_codigo_pareamento(&ctx(), agora()).await.unwrap();
    let device_id = db
        .parear_dispositivo(
            &ctx(),
            &codigo,
            chave.verifying_key().as_bytes(),
            nome,
            agora(),
        )
        .await
        .unwrap();
    Cliente { chave, device_id }
}

struct Pedido {
    metodo: String,
    caminho: String,
    corpo: Vec<u8>,
    device_id: String,
    timestamp: String,
    nonce: String,
    assinatura: String,
}

impl Pedido {
    fn novo(c: &Cliente, ts: i64) -> Self {
        let nonce = novo_nonce();
        let corpo = br#"{"app":"jogo.exe","limite":3600}"#.to_vec();
        let assinatura = assinar(
            &c.chave,
            "POST",
            "/api/windows/add_rule",
            ts,
            &nonce,
            &corpo,
        );
        Self {
            metodo: "POST".into(),
            caminho: "/api/windows/add_rule".into(),
            corpo,
            device_id: c.device_id.to_string(),
            timestamp: ts.to_string(),
            nonce,
            assinatura,
        }
    }

    fn req(&self) -> RequisicaoAssinada<'_> {
        RequisicaoAssinada {
            metodo: &self.metodo,
            caminho: &self.caminho,
            corpo: &self.corpo,
            device_id: &self.device_id,
            timestamp: &self.timestamp,
            nonce: &self.nonce,
            assinatura: &self.assinatura,
        }
    }
}

async fn eventos(db: &KioskDbManager, tipo: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT rejection_reason_detail FROM security_log WHERE event_type = ? ORDER BY rowid",
    )
    .bind(tipo)
    .fetch_all(db.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn requisicao_assinada_por_dispositivo_pareado_e_aceita() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular da mãe").await;
    let p = Pedido::novo(&c, agora().timestamp());
    let d = db.autenticar(&ctx(), &p.req(), agora()).await.unwrap();
    assert_eq!(
        (d.device_id, d.display_name.as_str(), d.papel),
        (c.device_id, "Celular da mãe", Papel::Admin)
    );
    assert!(eventos(&db, "AUTH_FAIL").await.is_empty());
    db.autorizar(&ctx(), &d, ActionType::AddRule).await.unwrap();
}

#[tokio::test]
async fn replay_do_mesmo_nonce_e_recusado() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    let p = Pedido::novo(&c, agora().timestamp());
    db.autenticar(&ctx(), &p.req(), agora()).await.unwrap();
    let r = db
        .autenticar(&ctx(), &p.req(), agora() + Duration::seconds(5))
        .await;
    assert_eq!(r.unwrap_err(), Rejeicao::NaoAutorizado);
    assert!(
        eventos(&db, "AUTH_FAIL")
            .await
            .iter()
            .any(|d| d.contains("replay"))
    );
}

#[tokio::test]
async fn adulteracao_de_qualquer_parte_assinada_e_recusada() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    let ts = agora().timestamp();
    type Mexe = Box<dyn Fn(&mut Pedido)>;
    let casos: Vec<(&str, Mexe)> = vec![
        (
            "corpo",
            Box::new(|p| p.corpo = br#"{"app":"jogo.exe","limite":99999}"#.to_vec()),
        ),
        (
            "caminho",
            Box::new(|p| p.caminho = "/api/windows/revoke".into()),
        ),
        ("método", Box::new(|p| p.metodo = "PUT".into())),
        (
            "timestamp",
            Box::new(move |p| p.timestamp = (ts + 1).to_string()),
        ),
        ("nonce", Box::new(|p| p.nonce = novo_nonce())),
        ("assinatura", Box::new(|p| p.assinatura = "AAAA".into())),
        (
            "assinatura vazia",
            Box::new(|p| p.assinatura = String::new()),
        ),
    ];
    for (nome, mexe) in casos {
        let mut p = Pedido::novo(&c, ts);
        mexe(&mut p);
        let r = db.autenticar(&ctx(), &p.req(), agora()).await;
        assert_eq!(
            r.unwrap_err(),
            Rejeicao::NaoAutorizado,
            "adulteração de {nome} foi aceita"
        );
    }
    assert_eq!(eventos(&db, "AUTH_FAIL").await.len(), 7);
}

#[tokio::test]
async fn assinatura_de_outra_chave_e_recusada() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    let intruso = Cliente {
        chave: SigningKey::generate(&mut OsRng),
        device_id: c.device_id,
    };
    let p = Pedido::novo(&intruso, agora().timestamp());
    assert_eq!(
        db.autenticar(&ctx(), &p.req(), agora()).await.unwrap_err(),
        Rejeicao::NaoAutorizado
    );
}

#[tokio::test]
async fn janela_de_tempo_aceita_ate_60_segundos_e_recusa_alem() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    for (desvio, ok) in [
        (-60, true),
        (60, true),
        (-61, false),
        (61, false),
        (-3600, false),
    ] {
        let p = Pedido::novo(&c, agora().timestamp() + desvio);
        let r = db.autenticar(&ctx(), &p.req(), agora()).await;
        assert_eq!(r.is_ok(), ok, "desvio de {desvio}s");
    }
}

#[tokio::test]
async fn dispositivo_desconhecido_e_formatos_invalidos_sao_recusados() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    let desconhecido = Cliente {
        chave: SigningKey::generate(&mut OsRng),
        device_id: Uuid::new_v4(),
    };
    let p = Pedido::novo(&desconhecido, agora().timestamp());
    assert!(db.autenticar(&ctx(), &p.req(), agora()).await.is_err());

    for (campo, mexe) in [
        (
            "device",
            (|p: &mut Pedido| p.device_id = "não-é-uuid".into()) as fn(&mut Pedido),
        ),
        ("timestamp", |p| p.timestamp = "ontem".into()),
        ("nonce curto", |p| p.nonce = "abc".into()),
    ] {
        let mut p = Pedido::novo(&c, agora().timestamp());
        mexe(&mut p);
        assert!(
            db.autenticar(&ctx(), &p.req(), agora()).await.is_err(),
            "{campo}"
        );
    }
}

#[tokio::test]
async fn nonce_igual_em_dispositivos_diferentes_nao_colide() {
    let (_d, db) = banco().await;
    let (a, b) = (parear(&db, "A").await, parear(&db, "B").await);
    let ts = agora().timestamp();
    let nonce = novo_nonce();
    for c in [&a, &b] {
        let sig = assinar(&c.chave, "GET", "/api/windows/usage", ts, &nonce, b"");
        let (device_id, timestamp) = (c.device_id.to_string(), ts.to_string());
        let req = RequisicaoAssinada {
            metodo: "GET",
            caminho: "/api/windows/usage",
            corpo: b"",
            device_id: &device_id,
            timestamp: &timestamp,
            nonce: &nonce,
            assinatura: &sig,
        };
        db.autenticar(&ctx(), &req, agora()).await.unwrap();
    }
}

#[tokio::test]
async fn nonces_fora_da_janela_sao_limpos() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    db.autenticar(
        &ctx(),
        &Pedido::novo(&c, agora().timestamp()).req(),
        agora(),
    )
    .await
    .unwrap();
    let n = |db: &KioskDbManager| {
        let pool = db.pool().clone();
        async move {
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM seen_nonce")
                .fetch_one(&pool)
                .await
                .unwrap()
        }
    };
    assert_eq!(n(&db).await, 1);
    let depois = agora() + Duration::seconds(121);
    db.autenticar(&ctx(), &Pedido::novo(&c, depois.timestamp()).req(), depois)
        .await
        .unwrap();
    assert_eq!(n(&db).await, 1, "o nonce antigo saiu; só o novo permanece");
}

#[tokio::test]
async fn dispositivo_revogado_e_recusado_na_proxima_requisicao() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    db.autenticar(
        &ctx(),
        &Pedido::novo(&c, agora().timestamp()).req(),
        agora(),
    )
    .await
    .unwrap();
    db.revogar_dispositivo(&ctx(), c.device_id, agora())
        .await
        .unwrap();
    let r = db
        .autenticar(
            &ctx(),
            &Pedido::novo(&c, agora().timestamp()).req(),
            agora(),
        )
        .await;
    assert_eq!(r.unwrap_err(), Rejeicao::NaoAutorizado);
    assert!(
        eventos(&db, "AUTH_FAIL")
            .await
            .iter()
            .any(|d| d.contains("revoked"))
    );
    assert!(
        db.revogar_dispositivo(&ctx(), Uuid::new_v4(), agora())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn codigo_de_pareamento_e_de_uso_unico_expira_e_so_existe_como_hash() {
    let (_d, db) = banco().await;
    let chave = |_: ()| SigningKey::generate(&mut OsRng).verifying_key().to_bytes();

    let codigo = db.criar_codigo_pareamento(&ctx(), agora()).await.unwrap();
    assert_eq!(codigo.len(), TAMANHO_CODIGO);
    let guardado: Vec<String> = sqlx::query_scalar("SELECT code_hash FROM pairing_code")
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert_eq!(guardado.len(), 1);
    assert!(
        !guardado[0].contains(&codigo),
        "o código não pode estar em claro no banco"
    );

    // digitado com minúsculas e hífen também vale (normalização), uma única vez
    let digitado = format!("{}-{}", codigo[..4].to_lowercase(), &codigo[4..]);
    db.parear_dispositivo(&ctx(), &digitado, &chave(()), "Celular", agora())
        .await
        .unwrap();
    let de_novo = db
        .parear_dispositivo(&ctx(), &codigo, &chave(()), "Outro", agora())
        .await;
    assert_eq!(de_novo.unwrap_err(), Rejeicao::NaoAutorizado);

    // expirado
    let velho = db.criar_codigo_pareamento(&ctx(), agora()).await.unwrap();
    let tarde = agora() + Duration::seconds(VALIDADE_CODIGO_SEGUNDOS + 1);
    assert!(
        db.parear_dispositivo(&ctx(), &velho, &chave(()), "Celular", tarde)
            .await
            .is_err()
    );
    // dentro da validade ainda funciona
    let ok = agora() + Duration::seconds(VALIDADE_CODIGO_SEGUNDOS - 1);
    db.parear_dispositivo(&ctx(), &velho, &chave(()), "Celular 2", ok)
        .await
        .unwrap();
}

#[tokio::test]
async fn pareamento_recusa_codigo_errado_chave_invalida_e_chave_repetida() {
    let (_d, db) = banco().await;
    let chave = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
    let codigo = db.criar_codigo_pareamento(&ctx(), agora()).await.unwrap();

    assert!(
        db.parear_dispositivo(&ctx(), "AAAAAAAA", &chave, "Celular", agora())
            .await
            .is_err()
    );
    assert!(
        db.parear_dispositivo(&ctx(), &codigo, &chave[..16], "Celular", agora())
            .await
            .is_err()
    );
    assert!(
        db.parear_dispositivo(&ctx(), &codigo, &chave, "   ", agora())
            .await
            .is_err()
    );
    assert_eq!(eventos(&db, "VALIDATION_FAIL").await.len(), 2);
    // o código não foi gasto pelas tentativas inválidas
    db.parear_dispositivo(&ctx(), &codigo, &chave, "Celular", agora())
        .await
        .unwrap();

    // a mesma chave pública não pareia duas vezes, e o código dessa tentativa não é consumido
    let outro = db.criar_codigo_pareamento(&ctx(), agora()).await.unwrap();
    assert!(
        db.parear_dispositivo(&ctx(), &outro, &chave, "Clone", agora())
            .await
            .is_err()
    );
    let nova = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
    db.parear_dispositivo(&ctx(), &outro, &nova, "Outro", agora())
        .await
        .unwrap();
}

#[tokio::test]
async fn muitas_tentativas_de_codigo_errado_bloqueiam_o_pareamento() {
    let (_d, db) = banco().await;
    let chave = SigningKey::generate(&mut OsRng).verifying_key().to_bytes();
    let codigo = db.criar_codigo_pareamento(&ctx(), agora()).await.unwrap();
    for _ in 0..MAX_FALHAS_PAREAMENTO {
        assert!(
            db.parear_dispositivo(&ctx(), "ZZZZZZZZ", &chave, "Celular", agora())
                .await
                .is_err()
        );
    }
    // agora nem o código certo passa
    assert!(
        db.parear_dispositivo(&ctx(), &codigo, &chave, "Celular", agora())
            .await
            .is_err()
    );
    assert!(
        eventos(&db, "AUTH_FAIL")
            .await
            .iter()
            .any(|d| d.contains("pairing blocked"))
    );
    // passado o período, volta a funcionar
    let depois = agora() + Duration::seconds(VALIDADE_CODIGO_SEGUNDOS + 1);
    let novo = db.criar_codigo_pareamento(&ctx(), depois).await.unwrap();
    db.parear_dispositivo(&ctx(), &novo, &chave, "Celular", depois)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_recusa_externa_nao_vaza_o_motivo() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    let mut p = Pedido::novo(&c, agora().timestamp());
    p.assinatura = "AAAA".into();
    let erro = db.autenticar(&ctx(), &p.req(), agora()).await.unwrap_err();
    assert_eq!(erro.to_string(), "requisição recusada");
    let detalhes = eventos(&db, "AUTH_FAIL").await;
    assert!(
        detalhes[0].contains("invalid signature") && detalhes[0].contains(&c.device_id.to_string())
    );
}

#[tokio::test]
async fn pareamento_e_revogacao_geram_eventos_de_sucesso() {
    let (_d, db) = banco().await;
    let c = parear(&db, "Celular").await;
    db.revogar_dispositivo(&ctx(), c.device_id, agora())
        .await
        .unwrap();
    let acoes: Vec<String> = sqlx::query_scalar(
        "SELECT action_attempted FROM security_log WHERE event_type = 'SUCCESS' ORDER BY rowid",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    assert_eq!(
        acoes,
        ["CREATE_PAIRING_CODE", "PAIR_DEVICE", "REVOKE_DEVICE"]
    );
}

#[test]
fn string_canonica_e_codigo_sao_estaveis() {
    let s = string_canonica("post", "/x", 7, "N", b"abc");
    assert_eq!(
        s,
        "KIOSK-V1\nPOST\n/x\n7\nN\nba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let c = novo_codigo_pareamento();
    assert!(c.len() == TAMANHO_CODIGO && !c.contains(['0', '1', 'O', 'I']));
    assert_eq!(normalizar_codigo(" ab-cd 23 "), "ABCD23");
}

#[tokio::test]
async fn recusa_de_codigo_sem_administrador_grava_authz_fail_e_nao_cria_codigo() {
    let (_d, db) = banco().await;
    db.recusar_codigo_pareamento(
        &ctx(),
        "código de pareamento pedido sem administrador elevado",
    )
    .await
    .unwrap();
    let eventos: Vec<(String, String, String)> =
        sqlx::query_as("SELECT action_attempted, event_type, transaction_status FROM security_log")
            .fetch_all(db.pool())
            .await
            .unwrap();
    assert_eq!(
        eventos,
        vec![(
            "CREATE_PAIRING_CODE".to_string(),
            "AUTHZ_FAIL".to_string(),
            "FAILURE".to_string()
        )]
    );
    let codigos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pairing_code")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(codigos, 0);
}
