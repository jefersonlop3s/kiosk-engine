//! Dispositivos pareados, autenticação de requisições e autorização (Camadas 1 e 2 da
//! arquitetura; ADR-006). A resposta externa é sempre genérica ([`Rejeicao`]); a razão exata vai
//! só para o log forense (`AUTH_FAIL`, `AUTHZ_FAIL`, `VALIDATION_FAIL`).

use std::fmt;

use chrono::{DateTime, Duration, Utc};
use ed25519_dalek::VerifyingKey;
use uuid::Uuid;

use crate::assinatura::{
    JANELA_SEGUNDOS, de_b64, nonce_valido, normalizar_codigo, novo_codigo_pareamento, sha256_hex,
    verificar,
};
use crate::database_schema::*;
use crate::db_manager::{EventContext, KioskDbManager};

/// Validade do código de pareamento.
pub const VALIDADE_CODIGO_SEGUNDOS: i64 = 300;
/// Falhas de pareamento toleradas dentro de `VALIDADE_CODIGO_SEGUNDOS` antes de recusar tudo.
pub const MAX_FALHAS_PAREAMENTO: i64 = 10;
/// Por quanto tempo um nonce é lembrado (a janela vale para os dois lados do relógio).
const RETENCAO_NONCE_SEGUNDOS: i64 = 2 * JANELA_SEGUNDOS;

/// Resposta externa a uma requisição recusada: sem detalhe, de propósito.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejeicao {
    NaoAutorizado,
    Interno,
}

impl fmt::Display for Rejeicao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NaoAutorizado => f.write_str("requisição recusada"),
            Self::Interno => f.write_str("falha interna"),
        }
    }
}

impl std::error::Error for Rejeicao {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Papel {
    Admin,
}

impl Papel {
    fn parse(s: &str) -> Option<Self> {
        (s == "ADMIN").then_some(Self::Admin)
    }

    /// Camada 2: o que cada papel pode pedir. Hoje só existe o admin, que pode tudo.
    fn permite(self, _acao: ActionType) -> bool {
        match self {
            Self::Admin => true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dispositivo {
    pub device_id: Uuid,
    pub display_name: String,
    pub papel: Papel,
}

/// Requisição como chega à Camada 1: cabeçalhos ainda em texto.
#[derive(Debug, Clone, Copy)]
pub struct RequisicaoAssinada<'a> {
    pub metodo: &'a str,
    pub caminho: &'a str,
    pub corpo: &'a [u8],
    pub device_id: &'a str,
    pub timestamp: &'a str,
    pub nonce: &'a str,
    pub assinatura: &'a str,
}

fn instante(t: DateTime<Utc>) -> String {
    log_ts(t)
}

impl KioskDbManager {
    /// Grava a recusa com o relógio da chamada (o mesmo da janela de anti-replay e do limite de
    /// tentativas de pareamento) e devolve a resposta genérica.
    async fn recusar(
        &self,
        ctx: &EventContext,
        acao: ActionType,
        tipo: EventType,
        detalhe: String,
        agora: DateTime<Utc>,
    ) -> Rejeicao {
        let mut e = SecurityEvent::new(
            &ctx.client_ip,
            acao,
            tipo,
            TransactionStatus::Failure,
            &detalhe,
        );
        e.timestamp = agora;
        match self.record_event(&e).await {
            Ok(()) => Rejeicao::NaoAutorizado,
            // não conseguir registrar a recusa também é falha: nega e sinaliza erro interno
            Err(_) => Rejeicao::Interno,
        }
    }

    async fn falha_interna(
        &self,
        ctx: &EventContext,
        acao: ActionType,
        err: sqlx::Error,
    ) -> Rejeicao {
        let e = SecurityEvent::new(
            &ctx.client_ip,
            acao,
            EventType::InternalError,
            TransactionStatus::Failure,
            &err.to_string(),
        );
        let _ = self.record_event(&e).await;
        Rejeicao::Interno
    }

    /// Camada 1: confere formato, janela de tempo, dispositivo, assinatura e nonce.
    /// O nonce só é gravado depois da assinatura válida, para que requisições forjadas não
    /// encham a tabela.
    pub async fn autenticar(
        &self,
        ctx: &EventContext,
        req: &RequisicaoAssinada<'_>,
        agora: DateTime<Utc>,
    ) -> Result<Dispositivo, Rejeicao> {
        const ACAO: ActionType = ActionType::Authenticate;
        macro_rules! recusa {
            ($($arg:tt)*) => {
                return Err(self.recusar(ctx, ACAO, EventType::AuthFail, format!($($arg)*), agora).await)
            };
        }

        let Ok(device_id) = Uuid::parse_str(req.device_id) else {
            recusa!("identificador de dispositivo ilegível");
        };
        let Ok(ts) = req.timestamp.parse::<i64>() else {
            recusa!("timestamp ilegível");
        };
        if !nonce_valido(req.nonce) {
            recusa!("nonce inválido");
        }
        let desvio = agora.timestamp() - ts;
        if desvio.abs() > JANELA_SEGUNDOS {
            recusa!("timestamp fora da janela ({desvio}s de desvio)");
        }

        let row: Option<(Vec<u8>, String, String, Option<String>)> = match sqlx::query_as(
            "SELECT public_key, display_name, role, revoked_at FROM paired_device WHERE device_id = ?",
        )
        .bind(device_id.to_string())
        .fetch_optional(self.pool())
        .await
        {
            Ok(r) => r,
            Err(e) => return Err(self.falha_interna(ctx, ACAO, e).await),
        };
        let Some((chave, nome, papel, revogado)) = row else {
            recusa!("dispositivo desconhecido {device_id}");
        };
        if revogado.is_some() {
            recusa!("dispositivo revogado {device_id}");
        }
        let (Some(papel), Ok(chave)) = (
            Papel::parse(&papel),
            <[u8; 32]>::try_from(chave.as_slice()).map(|b| VerifyingKey::from_bytes(&b)),
        ) else {
            recusa!("registro do dispositivo {device_id} corrompido");
        };
        let Ok(chave) = chave else {
            recusa!("chave pública do dispositivo {device_id} inválida");
        };

        if !verificar(
            &chave,
            req.metodo,
            req.caminho,
            ts,
            req.nonce,
            req.corpo,
            req.assinatura,
        ) {
            recusa!("assinatura inválida do dispositivo {device_id}");
        }

        // anti-replay: limpa o que saiu da janela e registra este nonce
        let corte = instante(agora - Duration::seconds(RETENCAO_NONCE_SEGUNDOS));
        if let Err(e) = sqlx::query("DELETE FROM seen_nonce WHERE seen_at < ?")
            .bind(corte)
            .execute(self.pool())
            .await
        {
            return Err(self.falha_interna(ctx, ACAO, e).await);
        }
        let inserido =
            sqlx::query("INSERT INTO seen_nonce (device_id, nonce, seen_at) VALUES (?, ?, ?)")
                .bind(device_id.to_string())
                .bind(req.nonce)
                .bind(instante(agora))
                .execute(self.pool())
                .await;
        match inserido {
            Ok(_) => {}
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                recusa!("nonce repetido (replay) do dispositivo {device_id}");
            }
            Err(e) => return Err(self.falha_interna(ctx, ACAO, e).await),
        }
        let papel_ok = papel;
        Ok(Dispositivo {
            device_id,
            display_name: nome,
            papel: papel_ok,
        })
    }

    /// Camada 2: o papel do dispositivo permite a ação?
    pub async fn autorizar(
        &self,
        ctx: &EventContext,
        dispositivo: &Dispositivo,
        acao: ActionType,
    ) -> Result<(), Rejeicao> {
        if dispositivo.papel.permite(acao) {
            return Ok(());
        }
        let detalhe = format!(
            "dispositivo {} sem permissão para {}",
            dispositivo.device_id,
            acao.as_str()
        );
        Err(self
            .recusar(ctx, acao, EventType::AuthzFail, detalhe, Utc::now())
            .await)
    }

    /// Gera um código de pareamento de uso único, válido por 5 minutos. O código em claro só é
    /// devolvido aqui (para mostrar na conta de administrador); o banco guarda o hash.
    pub async fn criar_codigo_pareamento(
        &self,
        ctx: &EventContext,
        agora: DateTime<Utc>,
    ) -> Result<String, sqlx::Error> {
        let codigo = novo_codigo_pareamento();
        let mut tx = self.begin_transaction().await?;
        let r = sqlx::query("INSERT INTO pairing_code (code_hash, expires_at) VALUES (?, ?)")
            .bind(sha256_hex(codigo.as_bytes()))
            .bind(instante(
                agora + Duration::seconds(VALIDADE_CODIGO_SEGUNDOS),
            ))
            .execute(&mut *tx)
            .await
            .map(|_| ());
        self.finish(tx, r, ctx, ActionType::CreatePairingCode, None)
            .await?;
        Ok(codigo)
    }

    /// Pareia um dispositivo: valida o código de uso único e grava a chave pública. Falhas
    /// respondem de forma genérica e gravam o motivo; muitas falhas seguidas bloqueiam o pareamento.
    pub async fn parear_dispositivo(
        &self,
        ctx: &EventContext,
        codigo: &str,
        chave_publica: &[u8],
        nome: &str,
        agora: DateTime<Utc>,
    ) -> Result<Uuid, Rejeicao> {
        const ACAO: ActionType = ActionType::PairDevice;
        let janela = instante(agora - Duration::seconds(VALIDADE_CODIGO_SEGUNDOS));
        let falhas: i64 = match sqlx::query_scalar(
            "SELECT count(*) FROM security_log \
             WHERE action_attempted = 'PAIR_DEVICE' AND event_type = 'AUTH_FAIL' AND timestamp >= ?",
        )
        .bind(janela)
        .fetch_one(self.pool())
        .await
        {
            Ok(n) => n,
            Err(e) => return Err(self.falha_interna(ctx, ACAO, e).await),
        };
        if falhas >= MAX_FALHAS_PAREAMENTO {
            let msg = format!(
                "pareamento bloqueado: {falhas} falhas nos últimos {VALIDADE_CODIGO_SEGUNDOS}s"
            );
            return Err(self
                .recusar(ctx, ACAO, EventType::AuthFail, msg, agora)
                .await);
        }

        let chave_ok = <[u8; 32]>::try_from(chave_publica)
            .ok()
            .is_some_and(|b| VerifyingKey::from_bytes(&b).is_ok());
        let nome = nome.trim();
        if !chave_ok || nome.is_empty() || nome.chars().count() > 64 {
            let msg = "pareamento com chave pública ou nome inválidos".to_string();
            return Err(self
                .recusar(ctx, ACAO, EventType::ValidationFail, msg, agora)
                .await);
        }

        let device_id = Uuid::new_v4();
        let mut tx = match self.begin_transaction().await {
            Ok(t) => t,
            Err(e) => return Err(self.falha_interna(ctx, ACAO, e).await),
        };
        let usado = sqlx::query(
            "UPDATE pairing_code SET used_at = ? WHERE code_hash = ? AND used_at IS NULL AND expires_at > ?",
        )
        .bind(instante(agora))
        .bind(sha256_hex(normalizar_codigo(codigo).as_bytes()))
        .bind(instante(agora))
        .execute(&mut *tx)
        .await;
        match usado {
            Ok(r) if r.rows_affected() == 1 => {}
            Ok(_) => {
                drop(tx);
                let msg = "código de pareamento inválido, usado ou expirado".to_string();
                return Err(self
                    .recusar(ctx, ACAO, EventType::AuthFail, msg, agora)
                    .await);
            }
            Err(e) => return Err(self.falha_interna(ctx, ACAO, e).await),
        }
        let gravado = sqlx::query(
            "INSERT INTO paired_device (device_id, public_key, display_name, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(device_id.to_string())
        .bind(chave_publica)
        .bind(nome)
        .bind(instante(agora))
        .execute(&mut *tx)
        .await
        .map(|_| ());
        match self.finish(tx, gravado, ctx, ACAO, None).await {
            Ok(()) => Ok(device_id),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                Err(Rejeicao::NaoAutorizado) // chave já pareada; o evento de falha já foi gravado por finish
            }
            Err(_) => Err(Rejeicao::Interno),
        }
    }

    /// Revoga um dispositivo; vale já na próxima requisição.
    pub async fn revogar_dispositivo(
        &self,
        ctx: &EventContext,
        device_id: Uuid,
        agora: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.begin_transaction().await?;
        let r = sqlx::query(
            "UPDATE paired_device SET revoked_at = ? WHERE device_id = ? AND revoked_at IS NULL",
        )
        .bind(instante(agora))
        .bind(device_id.to_string())
        .execute(&mut *tx)
        .await
        .and_then(|done| match done.rows_affected() {
            0 => Err(sqlx::Error::RowNotFound),
            _ => Ok(()),
        });
        self.finish(tx, r, ctx, ActionType::RevokeDevice, None)
            .await
    }
}

/// Decodifica uma chave pública enviada em texto (base64 sem preenchimento).
pub fn decodificar_chave_publica(texto: &str) -> Option<Vec<u8>> {
    de_b64(texto)
}
