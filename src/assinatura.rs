//! Assinatura de requisições (ADR-006): string canônica, assinatura Ed25519 e utilitários
//! compartilhados por quem assina (CLI/app dos pais) e por quem verifica (serviço).

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use rand::RngCore;
use sha2::{Digest, Sha256};

/// Versão do esquema, dentro da string assinada (troca de algoritmo exige novo pareamento).
pub const VERSAO: &str = "KIOSK-V1";
/// Desvio máximo aceito entre o `timestamp` da requisição e o relógio do serviço.
pub const JANELA_SEGUNDOS: i64 = 60;
/// Tamanho do nonce em bytes (antes da codificação).
pub const NONCE_BYTES: usize = 16;

pub const CABECALHO_DISPOSITIVO: &str = "X-Kiosk-Device";
pub const CABECALHO_TIMESTAMP: &str = "X-Kiosk-Timestamp";
pub const CABECALHO_NONCE: &str = "X-Kiosk-Nonce";
pub const CABECALHO_ASSINATURA: &str = "X-Kiosk-Signature";

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

pub(crate) fn de_b64(s: &str) -> Option<Vec<u8>> {
    URL_SAFE_NO_PAD.decode(s).ok()
}

/// SHA-256 em hexadecimal minúsculo.
pub fn sha256_hex(dados: &[u8]) -> String {
    format!("{:x}", Sha256::digest(dados))
}

/// String que é assinada: versão, método, caminho, `timestamp`, `nonce` e hash do corpo.
pub fn string_canonica(
    metodo: &str,
    caminho: &str,
    timestamp: i64,
    nonce: &str,
    corpo: &[u8],
) -> String {
    format!(
        "{VERSAO}\n{}\n{caminho}\n{timestamp}\n{nonce}\n{}",
        metodo.to_ascii_uppercase(),
        sha256_hex(corpo)
    )
}

/// Gera um nonce aleatório já codificado.
pub fn novo_nonce() -> String {
    let mut b = [0u8; NONCE_BYTES];
    rand::rngs::OsRng.fill_bytes(&mut b);
    b64(&b)
}

/// Assina uma requisição; devolve a assinatura codificada para o cabeçalho.
pub fn assinar(
    chave: &SigningKey,
    metodo: &str,
    caminho: &str,
    timestamp: i64,
    nonce: &str,
    corpo: &[u8],
) -> String {
    let msg = string_canonica(metodo, caminho, timestamp, nonce, corpo);
    b64(&chave.sign(msg.as_bytes()).to_bytes())
}

/// Confere a assinatura (modo estrito do Ed25519).
pub fn verificar(
    chave: &VerifyingKey,
    metodo: &str,
    caminho: &str,
    timestamp: i64,
    nonce: &str,
    corpo: &[u8],
    assinatura: &str,
) -> bool {
    let Some(bytes) = de_b64(assinatura) else {
        return false;
    };
    let Ok(sig) = Signature::from_slice(&bytes) else {
        return false;
    };
    let msg = string_canonica(metodo, caminho, timestamp, nonce, corpo);
    chave.verify_strict(msg.as_bytes(), &sig).is_ok()
}

/// O nonce tem de ser a codificação de exatamente `NONCE_BYTES` bytes.
pub fn nonce_valido(nonce: &str) -> bool {
    de_b64(nonce).is_some_and(|b| b.len() == NONCE_BYTES)
}

/// Alfabeto do código de pareamento: sem I, O, 0 e 1, que se confundem ao ler e digitar.
const ALFABETO: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
pub const TAMANHO_CODIGO: usize = 8;

/// Código de pareamento legível (8 caracteres, ~40 bits).
pub fn novo_codigo_pareamento() -> String {
    let mut rng = rand::rngs::OsRng;
    (0..TAMANHO_CODIGO)
        .map(|_| {
            // 32 símbolos dividem 256 sem viés
            let mut b = [0u8; 1];
            rng.fill_bytes(&mut b);
            ALFABETO[(b[0] as usize) % ALFABETO.len()] as char
        })
        .collect()
}

/// Normaliza o que a pessoa digitou (caixa e espaços/hífens) antes de comparar o hash.
pub fn normalizar_codigo(entrada: &str) -> String {
    entrada
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect()
}

pub fn codificar(bytes: &[u8]) -> String {
    b64(bytes)
}
