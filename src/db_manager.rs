use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::database_schema::*;

/// Pool de conexão para transações atômicas no SQLite local.
pub struct KioskDbManager {
    pool: SqlitePool,
}

impl KioskDbManager {
    /// Abre (e cria, se preciso) o banco em `conn_string` (ex.: `sqlite://kiosk.db`).
    pub async fn new(conn_string: &str) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::from_str(conn_string)?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        Ok(Self { pool })
    }

    pub async fn begin_transaction(&self) -> Result<Transaction<'_, Sqlite>, sqlx::Error> {
        self.pool.begin().await
    }

    /// Persistência da regra: implementada na F3 (migrações na F2).
    pub async fn create_active_rule(&self, _rule: &ActiveRule) -> Result<(), sqlx::Error> {
        unimplemented!("E-001 F3")
    }
}
