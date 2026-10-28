//! Serviço do kiosk. Uso:
//!
//! - `kioskd servir`: sobe a API de comando (exige `KIOSK_LISTEN`, sem valor padrão — ADR-005).
//! - `kioskd codigo-pareamento`: gera o código de pareamento (válido por 5 min) para digitar no
//!   dispositivo dos pais. Rodar na conta de administrador do PC.
//! - `kioskd dispositivos`: lista os dispositivos pareados.
//! - `kioskd revogar <id>`: revoga um dispositivo.
//!
//! Variáveis: `KIOSK_DB` (padrão `sqlite://kiosk.db`) e `KIOSK_LISTEN` (`ip:porta[,ip:porta...]`,
//! somente loopback e Tailscale).

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use chrono::Utc;
use kiosk_engine::api;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use uuid::Uuid;

const USO: &str = "uso: kioskd <servir | codigo-pareamento | dispositivos | revogar <id>>\n\
variáveis: KIOSK_DB (padrão sqlite://kiosk.db), KIOSK_LISTEN (ip:porta[,ip:porta...], obrigatória em `servir`)";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match executar(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("kioskd: {msg}");
            ExitCode::FAILURE
        }
    }
}

async fn abrir_banco() -> Result<KioskDbManager, String> {
    let url = std::env::var("KIOSK_DB").unwrap_or_else(|_| "sqlite://kiosk.db".into());
    KioskDbManager::new(&url)
        .await
        .map_err(|e| format!("não foi possível abrir o banco ({url}): {e}"))
}

fn enderecos() -> Result<Vec<SocketAddr>, String> {
    let bruto = std::env::var("KIOSK_LISTEN").map_err(|_| {
        "KIOSK_LISTEN não definida: informe os endereços de escuta (ADR-005)".to_string()
    })?;
    bruto
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.parse()
                .map_err(|_| format!("endereço de escuta inválido: {s:?}"))
        })
        .collect()
}

async fn executar(args: &[String]) -> Result<(), String> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["servir"] => {
            let enderecos = enderecos()?;
            api::validar_enderecos(&enderecos).map_err(|e| e.to_string())?;
            let db = Arc::new(abrir_banco().await?);
            let lista: Vec<String> = enderecos.iter().map(ToString::to_string).collect();
            eprintln!("kioskd: escutando em {}", lista.join(", "));
            api::servir_em(db, &enderecos)
                .await
                .map_err(|e| e.to_string())
        }
        ["codigo-pareamento"] => {
            let db = abrir_banco().await?;
            let codigo = db
                .criar_codigo_pareamento(&EventContext::local(), Utc::now())
                .await
                .map_err(|e| e.to_string())?;
            eprintln!("kioskd: código válido por 5 minutos, de uso único:");
            println!("{codigo}");
            Ok(())
        }
        ["dispositivos"] => {
            let db = abrir_banco().await?;
            let lista = db.listar_dispositivos().await.map_err(|e| e.to_string())?;
            if lista.is_empty() {
                eprintln!("kioskd: nenhum dispositivo pareado");
            }
            for (id, nome, criado, revogado) in lista {
                let estado = revogado.map_or("ativo".to_string(), |r| format!("revogado em {r}"));
                println!("{id}\t{nome}\tcriado em {criado}\t{estado}");
            }
            Ok(())
        }
        ["revogar", id] => {
            let id = Uuid::parse_str(id).map_err(|_| format!("identificador inválido: {id:?}"))?;
            let db = abrir_banco().await?;
            db.revogar_dispositivo(&EventContext::local(), id, Utc::now())
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => {
                        "dispositivo inexistente ou já revogado".to_string()
                    }
                    outro => outro.to_string(),
                })?;
            eprintln!("kioskd: dispositivo {id} revogado");
            Ok(())
        }
        _ => Err(USO.to_string()),
    }
}
