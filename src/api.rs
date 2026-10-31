//! API de comando (E-005; ADR-005 e ADR-006): `POST /api/windows/{ação}` atravessa as Camadas 0 a 5
//! da arquitetura. Toda resposta de falha é genérica; o motivo exato vai só para o log forense.
//!
//! - Camada 0: recepção (corpo limitado a `MAX_CORPO_BYTES`).
//! - Camada 1: assinatura, janela de tempo e nonce (`autenticar`).
//! - Camada 2: papel do dispositivo (`autorizar`).
//! - Camada 3: parâmetros (`comandos::interpretar` e existência das referências).
//! - Camada 4: transição atômica com o evento no log (`create_active_rule`, `set_rule_state`).
//! - Camada 5: `202 Accepted` genérico.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, DefaultBodyLimit, Path, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use chrono::Utc;
use tokio::net::TcpListener;
use uuid::Uuid;

use crate::assinatura::{
    CABECALHO_ASSINATURA, CABECALHO_DISPOSITIVO, CABECALHO_NONCE, CABECALHO_TIMESTAMP,
};
use crate::comandos::{Comando, acao_do_nome, interpretar};
use crate::database_schema::*;
use crate::db_manager::{EventContext, KioskDbManager};
use crate::i18n::Msg;
use crate::pareamento::Rejeicao;

/// Corpo máximo de uma requisição (Camada 0).
pub const MAX_CORPO_BYTES: usize = 16 * 1024;

const CORPO_ACEITA: &str = r#"{"status":"accepted"}"#;
const CORPO_REJEITADA: &str = r#"{"status":"rejected"}"#;
const CORPO_ERRO: &str = r#"{"status":"error"}"#;

#[derive(Debug)]
pub enum ErroServidor {
    SemEnderecos,
    EnderecoRecusado(SocketAddr, MotivoEndereco),
    Io(std::io::Error),
}

/// Por que um endereço de escuta foi recusado (ADR-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MotivoEndereco {
    Curinga,
    ForaDaRede,
}

impl MotivoEndereco {
    pub fn msg(self) -> Msg {
        match self {
            Self::Curinga => Msg::EnderecoCuringa,
            Self::ForaDaRede => Msg::EnderecoForaDaRede,
        }
    }
}

impl std::fmt::Display for ErroServidor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SemEnderecos => write!(f, "{}", Msg::SemEnderecosConfigurados),
            Self::EnderecoRecusado(a, m) => write!(
                f,
                "{}",
                Msg::EnderecoRecusado {
                    endereco: a.to_string(),
                    motivo: m.msg().to_string(),
                }
            ),
            Self::Io(e) => write!(
                f,
                "{}",
                Msg::FalhaDeEs {
                    erro: e.to_string()
                }
            ),
        }
    }
}

impl std::error::Error for ErroServidor {}

/// Política de rede da ADR-005: só loopback e a rede privada Tailscale (CGNAT `100.64.0.0/10` e
/// `fd7a:115c:a1e0::/48`). Curinga e LAN são recusados; a LAN exige HTTPS, ainda não implementado.
/// Para ativar o modo LAN (GC-15), defina `KIOSK_LAN=1` — permite endereços de rede privada
/// (10.x.x.x, 192.168.x.x etc.) mas **sem TLS** até a implementação futura.
pub fn endereco_permitido(ip: IpAddr) -> Result<(), MotivoEndereco> {
    if ip.is_unspecified() {
        return Err(MotivoEndereco::Curinga);
    }
    if ip.is_loopback() {
        return Ok(());
    }
    let tailscale = match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 100 && (64..=127).contains(&o[1])
        }
        IpAddr::V6(v6) => {
            let s = v6.segments();
            s[0] == 0xfd7a && s[1] == 0x115c && s[2] == 0xa1e0
        }
    };
    if tailscale {
        return Ok(());
    }
    // GC-15: modo LAN explícito por ambiente, sem TLS (pendente).
    // Redes privadas RFC 1918: 10/8, 172.16/12, 192.168/16.
    let lan = match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 10 || (o[0] == 172 && (16..=31).contains(&o[1])) || (o[0] == 192 && o[1] == 168)
        }
        IpAddr::V6(_) => false, // redes locais IPv6 raras em casa; ignoradas
    };
    if lan && std::env::var("KIOSK_LAN").as_deref() == Ok("1") {
        return Ok(());
    }
    Err(MotivoEndereco::ForaDaRede)
}

pub fn validar_enderecos(enderecos: &[SocketAddr]) -> Result<(), ErroServidor> {
    if enderecos.is_empty() {
        return Err(ErroServidor::SemEnderecos);
    }
    for a in enderecos {
        endereco_permitido(a.ip()).map_err(|m| ErroServidor::EnderecoRecusado(*a, m))?;
    }
    Ok(())
}

/// Valida a lista de endereços e só então abre as portas.
pub async fn vincular(enderecos: &[SocketAddr]) -> Result<Vec<TcpListener>, ErroServidor> {
    validar_enderecos(enderecos)?;
    let mut abertos = Vec::new();
    for a in enderecos {
        abertos.push(TcpListener::bind(a).await.map_err(ErroServidor::Io)?);
    }
    Ok(abertos)
}

pub fn roteador(db: Arc<KioskDbManager>) -> Router {
    Router::new()
        .route("/api/windows/{acao}", post(comando))
        .route("/api/pair", post(parear))
        .layer(DefaultBodyLimit::max(MAX_CORPO_BYTES))
        .with_state(db)
}

/// Atende em um `listener` já aberto até o servidor terminar.
pub async fn servir(listener: TcpListener, db: Arc<KioskDbManager>) -> std::io::Result<()> {
    axum::serve(
        listener,
        roteador(db).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
}

/// Valida, abre e atende em todos os endereços até um deles falhar.
pub async fn servir_em(
    db: Arc<KioskDbManager>,
    enderecos: &[SocketAddr],
) -> Result<(), ErroServidor> {
    let mut tarefas = tokio::task::JoinSet::new();
    for l in vincular(enderecos).await? {
        tarefas.spawn(servir(l, db.clone()));
    }
    match tarefas.join_next().await {
        Some(Ok(r)) => r.map_err(ErroServidor::Io),
        Some(Err(e)) => Err(ErroServidor::Io(std::io::Error::other(e))),
        None => Err(ErroServidor::SemEnderecos),
    }
}

fn resposta(status: StatusCode, corpo: String) -> Response {
    (status, [(header::CONTENT_TYPE, "application/json")], corpo).into_response()
}

fn rejeitada() -> Response {
    resposta(StatusCode::FORBIDDEN, CORPO_REJEITADA.into())
}

fn erro_interno() -> Response {
    resposta(StatusCode::INTERNAL_SERVER_ERROR, CORPO_ERRO.into())
}

fn de_rejeicao(r: Rejeicao) -> Response {
    match r {
        Rejeicao::NaoAutorizado => rejeitada(),
        Rejeicao::Interno => erro_interno(),
    }
}

fn aceita(id: Option<Uuid>) -> Response {
    let corpo = match id {
        Some(id) => format!(r#"{{"status":"accepted","id":"{id}"}}"#),
        None => CORPO_ACEITA.into(),
    };
    resposta(StatusCode::ACCEPTED, corpo)
}

async fn recusar_validacao(
    db: &KioskDbManager,
    ctx: &EventContext,
    acao: ActionType,
    motivo: String,
) -> Response {
    let e = SecurityEvent::new(
        &ctx.client_ip,
        acao,
        EventType::ValidationFail,
        TransactionStatus::Failure,
        &motivo,
    );
    match db.record_event(&e).await {
        Ok(()) => rejeitada(),
        Err(_) => erro_interno(),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// Corpo do pareamento. A API fala só inglês (ADR-010).
struct PareamentoBruto {
    code: String,
    public_key: String,
    name: String,
}

/// `POST /api/pair`: o código de uso único, mostrado na conta de administrador, é a credencial.
/// Não passa pelas Camadas 1 e 2 (o dispositivo ainda não existe); o limite de tentativas
/// vive em `parear_dispositivo`.
async fn parear(
    State(db): State<Arc<KioskDbManager>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    corpo: Bytes,
) -> Response {
    let ctx = EventContext {
        client_ip: peer.ip().to_string(),
    };
    let bruto: PareamentoBruto = match serde_json::from_slice(&corpo) {
        Ok(b) => b,
        Err(e) => {
            let motivo = format!("invalid pairing body: {e}");
            return recusar_validacao(&db, &ctx, ActionType::PairDevice, motivo).await;
        }
    };
    let Some(chave) = crate::pareamento::decodificar_chave_publica(&bruto.public_key) else {
        let motivo = "pairing public key is not base64".to_string();
        return recusar_validacao(&db, &ctx, ActionType::PairDevice, motivo).await;
    };
    match db
        .parear_dispositivo(&ctx, &bruto.code, &chave, &bruto.name, Utc::now())
        .await
    {
        Ok(id) => resposta(
            StatusCode::ACCEPTED,
            format!(r#"{{"status":"accepted","device_id":"{id}"}}"#),
        ),
        Err(r) => de_rejeicao(r),
    }
}

async fn comando(
    State(db): State<Arc<KioskDbManager>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    Path(nome): Path<String>,
    uri: Uri,
    headers: HeaderMap,
    corpo: Bytes,
) -> Response {
    let ctx = EventContext {
        client_ip: peer.ip().to_string(),
    };
    let agora = Utc::now();
    let h = |nome: &str| {
        headers
            .get(nome)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    };

    // Camada 1: autenticação
    let req = crate::pareamento::RequisicaoAssinada {
        metodo: "POST",
        caminho: uri.path(),
        corpo: &corpo,
        device_id: h(CABECALHO_DISPOSITIVO),
        timestamp: h(CABECALHO_TIMESTAMP),
        nonce: h(CABECALHO_NONCE),
        assinatura: h(CABECALHO_ASSINATURA),
    };
    let dispositivo = match db.autenticar(&ctx, &req, agora).await {
        Ok(d) => d,
        Err(r) => return de_rejeicao(r),
    };

    // Camada 2: autorização
    if let Some(acao) = acao_do_nome(&nome)
        && let Err(r) = db.autorizar(&ctx, &dispositivo, acao).await
    {
        return de_rejeicao(r);
    }

    // Camada 3: parâmetros
    let acao_log = acao_do_nome(&nome).unwrap_or(ActionType::System);
    let cmd = match interpretar(&nome, &corpo, agora) {
        Ok(c) => c,
        Err(motivo) => return recusar_validacao(&db, &ctx, acao_log, motivo.to_string()).await,
    };
    let referencias = match &cmd {
        Comando::AddRule(r) => vec![
            ("profile", db.existe_perfil(r.profile_id).await),
            ("app", db.existe_app(r.app_id).await),
        ],
        Comando::ActivateRule { rule_id }
        | Comando::RevokeRule { rule_id }
        | Comando::ModifyLimit { rule_id, .. } => {
            vec![("rule", db.existe_regra(*rule_id).await)]
        }
        Comando::Usage { profile_id, .. } => vec![("profile", db.existe_perfil(*profile_id).await)],
        Comando::Log(c) => match c.profile_id {
            Some(p) => vec![("profile", db.existe_perfil(p).await)],
            None => vec![],
        },
        Comando::Rules {
            profile_id: Some(p),
        } => vec![("profile", db.existe_perfil(*p).await)],
        Comando::Profiles | Comando::Apps | Comando::Rules { profile_id: None } => vec![],
    };
    for (nome_ref, existe) in referencias {
        match existe {
            Ok(true) => {}
            Ok(false) => {
                return recusar_validacao(
                    &db,
                    &ctx,
                    cmd.acao(),
                    format!("{nome_ref} does not exist"),
                )
                .await;
            }
            Err(_) => return erro_interno(),
        }
    }

    // Camada 4: execução atômica (a operação grava o evento de sucesso na mesma transação)
    let resultado = match cmd {
        Comando::AddRule(r) => {
            let regra = ActiveRule {
                rule_id: Uuid::new_v4(),
                profile_id: r.profile_id,
                app_id: r.app_id,
                creation_timestamp: agora,
                state: RuleState::Pending,
                daily_limit_seconds: r.daily_limit_seconds,
                time_window: r.time_window,
                effective_until_timestamp: r.effective_until,
                warning_seconds: None,
            };
            db.create_active_rule(&ctx, &regra)
                .await
                .map(|()| Some(regra.rule_id))
        }
        Comando::ActivateRule { rule_id } => db
            .set_rule_state(&ctx, rule_id, RuleState::Active)
            .await
            .map(|()| None),
        Comando::RevokeRule { rule_id } => db
            .set_rule_state(&ctx, rule_id, RuleState::Revoked)
            .await
            .map(|()| None),
        Comando::ModifyLimit {
            rule_id,
            daily_limit_seconds,
        } => db
            .set_rule_limit(&ctx, rule_id, daily_limit_seconds)
            .await
            .map(|()| None),
        // consultas: somente leitura, respondem com dados e não geram evento
        Comando::Usage { profile_id, dia } => return consulta_uso(&db, profile_id, dia).await,
        Comando::Log(c) => return consulta_log(&db, &c).await,
        Comando::Profiles => return consulta_perfis(&db).await,
        Comando::Apps => return consulta_apps(&db).await,
        Comando::Rules { profile_id } => return consulta_regras(&db, profile_id).await,
    };

    // Camada 5: resposta genérica
    match resultado {
        Ok(id) => aceita(id),
        Err(sqlx::Error::RowNotFound) => rejeitada(),
        Err(_) => erro_interno(),
    }
}

async fn consulta_uso(db: &KioskDbManager, profile_id: Uuid, dia: chrono::NaiveDate) -> Response {
    match db.uso_do_dia(profile_id, dia).await {
        Ok((sessao, apps)) => {
            let apps: Vec<_> = apps
                .into_iter()
                .map(|(id, nome, s)| {
                    serde_json::json!({ "app_id": id, "process_name": nome, "seconds": s })
                })
                .collect();
            let corpo = serde_json::json!({
                "profile_id": profile_id,
                "date": dia.to_string(),
                "session_seconds": sessao,
                "apps": apps,
            });
            resposta(StatusCode::OK, corpo.to_string())
        }
        Err(_) => erro_interno(),
    }
}

/// O detalhe exato só sai para eventos de enforcement e de operação; o das falhas das camadas
/// da API (`AUTH_FAIL`, `AUTHZ_FAIL`, `VALIDATION_FAIL`, `INTERNAL_ERROR`) fica só no banco.
/// Exceção (ADR-011): a tentativa de comando administrativo sem privilégio, que sai sem o SID.
fn detalhe_para_os_pais(tipo: EventType, detalhe: String) -> Option<String> {
    let publico = !matches!(
        tipo,
        EventType::AuthFail
            | EventType::AuthzFail
            | EventType::ValidationFail
            | EventType::InternalError
    );
    if publico {
        return Some(detalhe);
    }
    if tipo == EventType::AuthzFail {
        return crate::detalhes::para_os_pais(&detalhe);
    }
    None
}

async fn consulta_log(db: &KioskDbManager, c: &crate::comandos::ConsultaLog) -> Response {
    match db
        .security_events_limitado(c.de, c.ate, c.profile_id, i64::from(c.limite))
        .await
    {
        Ok(eventos) => {
            let eventos: Vec<_> = eventos
                .into_iter()
                .map(|e| {
                    serde_json::json!({
                        "timestamp": log_ts(e.timestamp),
                        "action": e.action_attempted.as_str(),
                        "event_type": e.event_type.as_str(),
                        "status": e.transaction_status.as_str(),
                        "client_ip": e.client_ip,
                        "profile_id": e.profile_id,
                        "detail": detalhe_para_os_pais(e.event_type, e.rejection_reason_detail),
                    })
                })
                .collect();
            resposta(
                StatusCode::OK,
                serde_json::json!({ "events": eventos }).to_string(),
            )
        }
        Err(_) => erro_interno(),
    }
}

/// `HH:MM` das janelas devolvidas pela API.
fn hhmm(t: chrono::NaiveTime) -> String {
    t.format("%H:%M").to_string()
}

/// Perfis (E-008 F1). Sem o SID da conta do Windows: o cliente não precisa dele e ele não sai do PC.
async fn consulta_perfis(db: &KioskDbManager) -> Response {
    let perfis = match db.listar_perfis().await {
        Ok(p) => p,
        Err(_) => return erro_interno(),
    };
    let mut saida = Vec::with_capacity(perfis.len());
    for p in perfis {
        let janelas = match db.janelas_do_perfil(p.profile_id).await {
            Ok(j) => j,
            Err(_) => return erro_interno(),
        };
        let janelas: Vec<_> = janelas
            .into_iter()
            .map(|(dia, j)| {
                serde_json::json!({
                    "weekday": dia,
                    "window_start": hhmm(j.start),
                    "window_end": hhmm(j.end),
                })
            })
            .collect();
        saida.push(serde_json::json!({
            "profile_id": p.profile_id,
            "display_name": p.display_name,
            "daily_total_limit_seconds": p.daily_total_limit_seconds,
            "windows": janelas,
        }));
    }
    resposta(
        StatusCode::OK,
        serde_json::json!({ "profiles": saida }).to_string(),
    )
}

async fn consulta_apps(db: &KioskDbManager) -> Response {
    match db.listar_apps().await {
        Ok(apps) => {
            let apps: Vec<_> = apps
                .into_iter()
                .map(|a| {
                    serde_json::json!({
                        "app_id": a.app_id,
                        "process_name": a.process_name,
                        "status": a.status.as_str(),
                    })
                })
                .collect();
            resposta(
                StatusCode::OK,
                serde_json::json!({ "apps": apps }).to_string(),
            )
        }
        Err(_) => erro_interno(),
    }
}

async fn consulta_regras(db: &KioskDbManager, profile_id: Option<Uuid>) -> Response {
    match db.listar_regras(profile_id).await {
        Ok(regras) => {
            let regras: Vec<_> = regras
                .into_iter()
                .map(|r| {
                    serde_json::json!({
                        "rule_id": r.rule_id,
                        "profile_id": r.profile_id,
                        "app_id": r.app_id,
                        "state": r.state.as_str(),
                        "created_at": log_ts(r.creation_timestamp),
                        "daily_limit_seconds": r.daily_limit_seconds,
                        "window_start": r.time_window.as_ref().map(|j| hhmm(j.start)),
                        "window_end": r.time_window.as_ref().map(|j| hhmm(j.end)),
                        "effective_until": r.effective_until_timestamp.map(log_ts),
                        "warning_seconds": r.warning_seconds,
                    })
                })
                .collect();
            resposta(
                StatusCode::OK,
                serde_json::json!({ "rules": regras }).to_string(),
            )
        }
        Err(_) => erro_interno(),
    }
}
