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
use crate::pareamento::Rejeicao;

/// Corpo máximo de uma requisição (Camada 0).
pub const MAX_CORPO_BYTES: usize = 16 * 1024;

const CORPO_ACEITA: &str = r#"{"status":"accepted"}"#;
const CORPO_REJEITADA: &str = r#"{"status":"rejected"}"#;
const CORPO_ERRO: &str = r#"{"status":"error"}"#;

#[derive(Debug)]
pub enum ErroServidor {
    SemEnderecos,
    EnderecoRecusado(SocketAddr, &'static str),
    Io(std::io::Error),
}

impl std::fmt::Display for ErroServidor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SemEnderecos => f.write_str("nenhum endereço de escuta configurado (ADR-005)"),
            Self::EnderecoRecusado(a, m) => write!(f, "endereço {a} recusado: {m}"),
            Self::Io(e) => write!(f, "falha de E/S: {e}"),
        }
    }
}

impl std::error::Error for ErroServidor {}

/// Política de rede da ADR-005: só loopback e a rede privada Tailscale (CGNAT `100.64.0.0/10` e
/// `fd7a:115c:a1e0::/48`). Curinga e LAN são recusados; a LAN exige HTTPS, ainda não implementado.
pub fn endereco_permitido(ip: IpAddr) -> Result<(), &'static str> {
    if ip.is_unspecified() {
        return Err("endereço curinga (0.0.0.0 ou ::) não é permitido");
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
        Ok(())
    } else {
        Err("só loopback e rede Tailscale; LAN exige HTTPS, ainda não implementado (ADR-005)")
    }
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
        Err(motivo) => return recusar_validacao(&db, &ctx, acao_log, motivo).await,
    };
    let referencias = match &cmd {
        Comando::AddRule(r) => vec![
            ("perfil", db.existe_perfil(r.profile_id).await),
            ("app", db.existe_app(r.app_id).await),
        ],
        Comando::ActivateRule { rule_id } | Comando::RevokeRule { rule_id } => {
            vec![("regra", db.existe_regra(*rule_id).await)]
        }
    };
    for (nome_ref, existe) in referencias {
        match existe {
            Ok(true) => {}
            Ok(false) => {
                return recusar_validacao(&db, &ctx, cmd.acao(), format!("{nome_ref} inexistente"))
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
    };

    // Camada 5: resposta genérica
    match resultado {
        Ok(id) => aceita(id),
        Err(sqlx::Error::RowNotFound) => rejeitada(),
        Err(_) => erro_interno(),
    }
}
