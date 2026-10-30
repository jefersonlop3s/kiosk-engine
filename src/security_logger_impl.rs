//! Gravação do log forense. A inserção recebe a conexão da transação do chamador:
//! o evento e a mudança de estado que ele descreve confirmam ou desfazem juntos.

use sqlx::SqliteConnection;
use sha2::{Digest, Sha256};

pub use crate::database_schema::SecurityEvent;
use crate::database_schema::log_ts;

/// ADR-012: SHA-256(log_id_anterior || prev_hash_anterior) em hexadecimal.
/// Concatena os bytes do log_id e do prev_hash (ambos em UTF-8) e aplica
/// SHA-256. Para o primeiro evento (`prev_hash_anterior` = None), retorna None.
fn hash_chain(prev_log_id: Option<&str>, prev_hash: Option<&str>) -> Option<String> {
    match (prev_log_id, prev_hash) {
        (Some(id), Some(h)) => {
            let bytes: Vec<u8> = std::iter::chain(id.as_bytes(), h.as_bytes()).copied().collect();
            Some(format!("{:x}", Sha256::digest(&bytes)))
        }
        _ => None,
    }
}

/// SHA-256(prev_log_id || prev_hash) do último registro, para encadear.
async fn prev_hash_do_ultimo(conn: &mut SqliteConnection) -> Result<Option<String>, sqlx::Error> {
    let ultimo: Option<(String, Option<String>)> = sqlx::query_as(
        "SELECT log_id, prev_hash FROM security_log ORDER BY rowid DESC LIMIT 1",
    )
    .fetch_optional(conn)
    .await?;
    match ultimo {
        Some((id, h)) => Ok(hash_chain(Some(&id), h.as_deref())),
        None => Ok(None),
    }
}

pub async fn insert_event(
    conn: &mut SqliteConnection,
    e: &SecurityEvent,
) -> Result<(), sqlx::Error> {
    let prev = prev_hash_do_ultimo(conn).await?;
    sqlx::query(
        "INSERT INTO security_log (log_id, timestamp, client_ip, action_attempted, event_type, \
         transaction_status, rejection_reason_detail, profile_id, prev_hash) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.event_id.to_string())
    .bind(log_ts(e.timestamp))
    .bind(&e.client_ip)
    .bind(e.action_attempted.as_str())
    .bind(e.event_type.as_str())
    .bind(e.transaction_status.as_str())
    .bind(&e.rejection_reason_detail)
    .bind(e.profile_id.map(|p| p.to_string()))
    .bind(prev)
    .execute(conn)
    .await?;
    Ok(())
}
