//! Cliente da API de comando (E-005 F3): usado pelo CLI dos pais e pelos testes.
//! HTTP/1.1 simples sobre TCP, sem TLS: a API só escuta em loopback e na rede Tailscale
//! (ADR-005), cujo transporte já é cifrado pela VPN.

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use uuid::Uuid;

use crate::assinatura::{
    CABECALHO_ASSINATURA, CABECALHO_DISPOSITIVO, CABECALHO_NONCE, CABECALHO_TIMESTAMP, assinar,
    codificar, novo_nonce,
};

/// Tempo máximo de uma chamada (conexão, envio e resposta).
pub const TIMEOUT_SEGUNDOS: u64 = 10;
/// Maior resposta lida do servidor.
const MAX_RESPOSTA_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RespostaApi {
    pub status: u16,
    pub corpo: String,
}

impl RespostaApi {
    /// `202` (comando aceito) ou `200` (consulta respondida).
    pub fn aceita(&self) -> bool {
        matches!(self.status, 200 | 202)
    }
}

/// Identidade do dispositivo dos pais: a chave privada nunca sai deste arquivo.
#[derive(Serialize, Deserialize, Clone)]
pub struct Credenciais {
    /// `host:porta` do serviço (loopback ou endereço Tailscale).
    pub servidor: String,
    /// Presente depois do pareamento.
    pub device_id: Option<Uuid>,
    /// Chave privada Ed25519 (32 bytes, base64 sem preenchimento).
    pub chave_privada: String,
}

impl std::fmt::Debug for Credenciais {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credenciais")
            .field("servidor", &self.servidor)
            .field("device_id", &self.device_id)
            .field("chave_privada", &"<oculta>")
            .finish()
    }
}

impl Credenciais {
    pub fn nova(servidor: &str) -> (Self, SigningKey) {
        let chave = SigningKey::generate(&mut OsRng);
        let c = Self {
            servidor: servidor.to_owned(),
            device_id: None,
            chave_privada: URL_SAFE_NO_PAD.encode(chave.to_bytes()),
        };
        (c, chave)
    }

    pub fn chave(&self) -> Result<SigningKey, String> {
        let bytes = URL_SAFE_NO_PAD
            .decode(&self.chave_privada)
            .map_err(|_| "chave privada ilegível no arquivo de credenciais".to_string())?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "chave privada com tamanho inválido".to_string())?;
        Ok(SigningKey::from_bytes(&bytes))
    }

    pub fn carregar(caminho: &Path) -> Result<Self, String> {
        let texto = std::fs::read_to_string(caminho)
            .map_err(|e| format!("não foi possível ler {}: {e}", caminho.display()))?;
        serde_json::from_str(&texto)
            .map_err(|e| format!("credenciais em {} inválidas: {e}", caminho.display()))
    }

    /// Grava o arquivo; no Unix com permissão `0600` desde a criação (sem janela legível).
    pub fn salvar(&self, caminho: &Path) -> Result<(), String> {
        use std::io::Write;
        let texto = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        if let Some(pai) = caminho.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(pai)
                .map_err(|e| format!("não foi possível criar {}: {e}", pai.display()))?;
        }
        let mut opcoes = std::fs::OpenOptions::new();
        opcoes.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opcoes.mode(0o600);
        }
        let mut f = opcoes
            .open(caminho)
            .map_err(|e| format!("não foi possível gravar {}: {e}", caminho.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|e| format!("permissão de {}: {e}", caminho.display()))?;
        }
        f.write_all(texto.as_bytes())
            .map_err(|e| format!("não foi possível gravar {}: {e}", caminho.display()))
    }
}

/// `POST` cru; devolve status e corpo.
pub async fn post(
    servidor: &str,
    caminho: &str,
    cabecalhos: &[(String, String)],
    corpo: &[u8],
) -> Result<RespostaApi, String> {
    let limite = Duration::from_secs(TIMEOUT_SEGUNDOS);
    tokio::time::timeout(limite, post_sem_prazo(servidor, caminho, cabecalhos, corpo))
        .await
        .map_err(|_| format!("sem resposta de {servidor} em {TIMEOUT_SEGUNDOS}s"))?
}

async fn post_sem_prazo(
    servidor: &str,
    caminho: &str,
    cabecalhos: &[(String, String)],
    corpo: &[u8],
) -> Result<RespostaApi, String> {
    let mut s = TcpStream::connect(servidor)
        .await
        .map_err(|e| format!("não foi possível conectar a {servidor}: {e}"))?;
    let mut req = format!(
        "POST {caminho} HTTP/1.1\r\nHost: {servidor}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n",
        corpo.len()
    );
    for (k, v) in cabecalhos {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    s.write_all(corpo).await.map_err(|e| e.to_string())?;
    let mut resp = Vec::new();
    s.take(MAX_RESPOSTA_BYTES as u64)
        .read_to_end(&mut resp)
        .await
        .map_err(|e| format!("falha ao ler a resposta: {e}"))?;
    let texto = String::from_utf8_lossy(&resp).into_owned();
    let status = texto
        .split(' ')
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| "resposta HTTP ilegível".to_string())?;
    let corpo = texto.split("\r\n\r\n").nth(1).unwrap_or("").to_owned();
    Ok(RespostaApi { status, corpo })
}

/// Envia um comando assinado (`acao` = `add_rule`, `activate_rule`, `revoke_rule`).
pub async fn comando(
    cred: &Credenciais,
    acao: &str,
    corpo_json: &str,
) -> Result<RespostaApi, String> {
    let device_id = cred
        .device_id
        .ok_or("este dispositivo ainda não foi pareado (use o comando `parear`)")?;
    let chave = cred.chave()?;
    let caminho = format!("/api/windows/{acao}");
    let (ts, nonce) = (chrono::Utc::now().timestamp(), novo_nonce());
    let assinatura = assinar(&chave, "POST", &caminho, ts, &nonce, corpo_json.as_bytes());
    let cabecalhos = [
        (CABECALHO_DISPOSITIVO.to_owned(), device_id.to_string()),
        (CABECALHO_TIMESTAMP.to_owned(), ts.to_string()),
        (CABECALHO_NONCE.to_owned(), nonce),
        (CABECALHO_ASSINATURA.to_owned(), assinatura),
    ];
    post(&cred.servidor, &caminho, &cabecalhos, corpo_json.as_bytes()).await
}

/// Pareia este dispositivo com o código mostrado na conta de administrador do PC.
pub async fn parear(
    servidor: &str,
    codigo: &str,
    chave: &SigningKey,
    nome: &str,
) -> Result<Uuid, String> {
    let corpo = serde_json::json!({
        "codigo": codigo,
        "chave_publica": codificar(chave.verifying_key().as_bytes()),
        "nome": nome,
    })
    .to_string();
    let r = post(servidor, "/api/pair", &[], corpo.as_bytes()).await?;
    if !r.aceita() {
        return Err(format!(
            "pareamento recusado pelo serviço (HTTP {}). O código tem 8 caracteres, vale por 5 minutos \
             e só pode ser usado uma vez; gere outro no PC com `kioskd codigo-pareamento`, usando o \
             mesmo KIOSK_DB do `kioskd servir`",
            r.status
        ));
    }
    let v: serde_json::Value = serde_json::from_str(&r.corpo)
        .map_err(|_| "resposta de pareamento ilegível".to_string())?;
    v["device_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(|| "resposta de pareamento sem identificador do dispositivo".to_string())
}
