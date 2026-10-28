//! Gravação do log forense. A inserção recebe a conexão da transação do chamador:
//! o evento e a mudança de estado que ele descreve confirmam ou desfazem juntos.

use sqlx::SqliteConnection;

pub use crate::database_schema::SecurityEvent;

pub async fn insert_event(
    conn: &mut SqliteConnection,
    e: &SecurityEvent,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO security_log (log_id, timestamp, client_ip, action_attempted, event_type, \
         transaction_status, rejection_reason_detail) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(e.event_id.to_string())
    .bind(e.timestamp.to_rfc3339())
    .bind(&e.client_ip)
    .bind(e.action_attempted.as_str())
    .bind(e.event_type.as_str())
    .bind(e.transaction_status.as_str())
    .bind(&e.rejection_reason_detail)
    .execute(conn)
    .await?;
    Ok(())
}
