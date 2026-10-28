//! Serviço do kiosk. Uso:
//!
//! - `kioskd servir [--sem-agente]`: sobe a API de comando (exige `KIOSK_LISTEN`, sem valor
//!   padrão — ADR-005) e, no Windows, o agente de enforcement (tick de 2 s) e a limpeza diária
//!   do log (retenção de 90 dias). Fora do Windows só a API sobe.
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

const USO: &str = "uso: kioskd <servir [--sem-agente] | codigo-pareamento | dispositivos | revogar <id>>\n\
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

/// Limpeza do log forense (retenção de 90 dias), na partida e a cada 24 h.
fn iniciar_manutencao(db: Arc<KioskDbManager>) {
    tokio::spawn(async move {
        let mut relogio = tokio::time::interval(std::time::Duration::from_secs(24 * 3600));
        loop {
            relogio.tick().await;
            match db.purge_expired_events(&EventContext::local()).await {
                Ok(0) => {}
                Ok(n) => eprintln!("kioskd: retenção: {n} eventos antigos removidos"),
                Err(e) => eprintln!("kioskd: retenção falhou: {e}"),
            }
        }
    });
}

/// Intervalo do agente (ADR-003: polling de 2 s).
#[cfg(windows)]
const TICK_AGENTE: std::time::Duration = std::time::Duration::from_secs(2);

#[cfg(windows)]
fn iniciar_agente(db: Arc<KioskDbManager>) {
    eprintln!(
        "kioskd: agente de enforcement ativo (tick de {}s)",
        TICK_AGENTE.as_secs()
    );
    tokio::spawn(async move {
        kiosk_engine::agente::rodar_em_laco(
            &db,
            kiosk_engine::sistema_windows::SistemaWindows,
            chrono::Local,
            TICK_AGENTE,
            |rel| {
                for e in &rel.erros {
                    eprintln!("kioskd: agente: {e}");
                }
                true
            },
        )
        .await;
    });
}

#[cfg(not(windows))]
fn iniciar_agente(_db: Arc<KioskDbManager>) {
    eprintln!("kioskd: agente de enforcement só existe no Windows; servindo apenas a API");
}

async fn executar(args: &[String]) -> Result<(), String> {
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["servir", resto @ ..] => {
            let com_agente = match resto {
                [] => true,
                ["--sem-agente"] => false,
                _ => return Err(USO.to_string()),
            };
            let enderecos = enderecos()?;
            api::validar_enderecos(&enderecos).map_err(|e| e.to_string())?;
            let db = Arc::new(abrir_banco().await?);
            iniciar_manutencao(db.clone());
            if com_agente {
                iniciar_agente(db.clone());
            }
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
