pub mod agente;
pub mod api;
pub mod assinatura;
pub mod auditoria;
#[cfg(windows)]
pub mod auditoria_windows;
pub mod bandeja;
#[cfg(windows)]
pub mod bandeja_windows;
pub mod cli;
pub mod cliente;
pub mod comandos;
pub mod database_schema;
pub mod db_manager;
pub mod detalhes;
pub mod i18n;
pub mod ipc;
pub mod motor;
pub mod pareamento;
pub mod security_logger_impl;
#[cfg(windows)]
pub mod servico_windows;
#[cfg(windows)]
pub mod sistema_windows;
pub mod textos;
#[cfg(windows)]
pub mod ui_cliente_windows;
#[cfg(windows)]
pub mod ui_windows;
pub mod uso;
pub mod versao;
