//! Gravação do log forense. A inserção recebe a conexão da transação do chamador:
//! o evento e a mudança de estado que ele descreve confirmam ou desfazem juntos.

use sqlx::SqliteConnection;

pub use crate::database_schema::SecurityEvent;
use crate::database_schema::log_ts;

pub async fn insert_event(
    conn: &mut SqliteConnection,
    e: &SecurityEvent,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO security_log (log_id, timestamp, client_ip, action_attempted, event_type, \
         transaction_status, rejection_reason_detail, profile_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.event_id.to_string())
    .bind(log_ts(e.timestamp))
    .bind(&e.client_ip)
    .bind(e.action_attempted.as_str())
    .bind(e.event_type.as_str())
    .bind(e.transaction_status.as_str())
    .bind(&e.rejection_reason_detail)
    .bind(e.profile_id.map(|p| p.to_string()))
    .execute(conn)
    .await?;
    Ok(())
}
