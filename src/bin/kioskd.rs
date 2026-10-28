//! Serviço do kiosk. Uso:
//!
//! - `kioskd servir [--sem-agente]`: sobe a API de comando (exige `KIOSK_LISTEN`, sem valor
//!   padrão — ADR-005) e, no Windows, o agente de enforcement (tick de 2 s) e a limpeza diária
//!   do log (retenção de 90 dias). Fora do Windows só a API sobe.
//! - `kioskd codigo-pareamento`: gera o código de pareamento (válido por 5 min) para digitar no
//!   dispositivo dos pais. Rodar na conta de administrador do PC.
//! - `kioskd dispositivos`: lista os dispositivos pareados.
//! - `kioskd revogar <id>`: revoga um dispositivo.
//! - `kioskd perfil criar --conta USUARIO --nome NOME [--limite-total SEGUNDOS]` (Windows; em
//!   outros sistemas, `--sid S-1-...` no lugar de `--conta`), `perfil listar`,
//!   `perfil limite <id> --segundos N|--sem-limite`, `perfil janela <id> --dias seg,ter,... --janela HH:MM-HH:MM`.
//! - `kioskd app criar --processo NOME.exe [--isento | --ignorar]` e `app listar`.
//!
//! Variáveis: `KIOSK_DB` (padrão `sqlite://C:/ProgramData/kiosk/kiosk.db` no Windows e
//! `sqlite://kiosk.db` nos outros sistemas; o banco em uso é sempre informado) e `KIOSK_LISTEN` (`ip:porta[,ip:porta...]`,
//! somente loopback e Tailscale).

use std::net::SocketAddr;
use std::process::ExitCode;
use std::sync::Arc;

use chrono::Utc;
use kiosk_engine::api;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use uuid::Uuid;

const USO: &str = "uso:
  kioskd servir [--sem-agente]
  kioskd codigo-pareamento
  kioskd dispositivos
  kioskd revogar <id-do-dispositivo>
  kioskd perfil criar --conta USUARIO --nome NOME [--limite-total SEGUNDOS]   (ou --sid S-1-... no lugar de --conta)
  kioskd perfil listar
  kioskd perfil limite <id-do-perfil> --segundos N | --sem-limite
  kioskd perfil janela <id-do-perfil> --dias seg,ter,qua,qui,sex --janela 08:00-20:00
  kioskd app criar --processo NOME.exe [--isento | --ignorar]
  kioskd app listar
variáveis: KIOSK_DB (padrão no Windows: sqlite://C:/ProgramData/kiosk/kiosk.db), KIOSK_LISTEN (ip:porta[,ip:porta...], obrigatória em `servir`)";

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

/// Banco padrão: no Windows, um lugar fixo da máquina, igual em qualquer janela e pasta.
#[cfg(windows)]
const BANCO_PADRAO: &str = "sqlite://C:/ProgramData/kiosk/kiosk.db";
#[cfg(not(windows))]
const BANCO_PADRAO: &str = "sqlite://kiosk.db";

async fn abrir_banco() -> Result<KioskDbManager, String> {
    let url = std::env::var("KIOSK_DB").unwrap_or_else(|_| BANCO_PADRAO.into());
    // sem isto, uma janela sem KIOSK_DB criava outro banco em silêncio e os cadastros "sumiam"
    eprintln!("kioskd: banco {url}");
    // cria a pasta do arquivo, se faltar (o SQLite cria o arquivo, não a pasta)
    if let Some(caminho) = url.strip_prefix("sqlite://") {
        let arquivo = caminho.split('?').next().unwrap_or(caminho);
        if let Some(pasta) = std::path::Path::new(arquivo)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            std::fs::create_dir_all(pasta)
                .map_err(|e| format!("não foi possível criar a pasta {}: {e}", pasta.display()))?;
        }
    }
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
            s.parse().map_err(|_| {
                if s.parse::<std::net::IpAddr>().is_ok() {
                    format!("falta a porta em {s:?}: use ip:porta, por exemplo {s}:8443")
                } else {
                    format!("endereço de escuta inválido: {s:?} (use ip:porta)")
                }
            })
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

fn opcao(args: &[String], nome: &str) -> Option<String> {
    args.iter()
        .position(|a| a == nome)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn exigir(args: &[String], nome: &str) -> Result<String, String> {
    opcao(args, nome).ok_or_else(|| format!("falta {nome}\n{USO}"))
}

fn id(texto: &str) -> Result<Uuid, String> {
    Uuid::parse_str(texto).map_err(|_| format!("identificador inválido: {texto:?}"))
}

fn segundos(texto: &str, nome: &str) -> Result<u64, String> {
    texto
        .parse()
        .map_err(|_| format!("{nome} inválido: {texto:?}"))
}

/// `dom,seg,...` ou `0,1,...` -> dias da semana (0 = domingo).
fn dias(texto: &str) -> Result<Vec<u8>, String> {
    const NOMES: [&str; 7] = ["dom", "seg", "ter", "qua", "qui", "sex", "sab"];
    texto
        .split(',')
        .map(|d| {
            let d = d.trim().to_lowercase().replace('á', "a");
            NOMES
                .iter()
                .position(|n| d.starts_with(n))
                .map(|i| i as u8)
                .or_else(|| d.parse::<u8>().ok().filter(|n| *n <= 6))
                .ok_or_else(|| {
                    format!("dia inválido: {d:?} (use dom, seg, ter, qua, qui, sex, sab)")
                })
        })
        .collect()
}

fn hora(texto: &str) -> Result<chrono::NaiveTime, String> {
    chrono::NaiveTime::parse_from_str(texto, "%H:%M")
        .map_err(|_| format!("hora inválida: {texto:?} (use HH:MM)"))
}

/// SID da conta: `--sid` explícito, ou `--conta` resolvida pelo Windows.
fn sid_do_argumento(args: &[String]) -> Result<String, String> {
    if let Some(sid) = opcao(args, "--sid") {
        if !sid.starts_with("S-1-") {
            return Err(format!("SID inválido: {sid:?}"));
        }
        return Ok(sid);
    }
    let conta = exigir(args, "--conta")?;
    #[cfg(windows)]
    {
        kiosk_engine::sistema_windows::sid_de_usuario(&conta)
    }
    #[cfg(not(windows))]
    {
        Err(format!(
            "--conta {conta:?} só é resolvida no Windows; informe --sid"
        ))
    }
}

fn duracao(s: u64) -> String {
    format!("{}h{:02}", s / 3600, s % 3600 / 60)
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
        ["perfil", "criar", ..] => {
            let sid = sid_do_argumento(args)?;
            let nome = exigir(args, "--nome")?;
            let limite = opcao(args, "--limite-total")
                .map(|l| segundos(&l, "--limite-total"))
                .transpose()?;
            let db = abrir_banco().await?;
            let perfil = kiosk_engine::database_schema::ChildProfile {
                profile_id: Uuid::new_v4(),
                windows_sid: sid.clone(),
                display_name: nome.clone(),
                daily_total_limit_seconds: limite,
            };
            db.create_profile(&EventContext::local(), &perfil)
                .await
                .map_err(|e| match e {
                    sqlx::Error::Database(d) if d.is_unique_violation() => {
                        format!("já existe um perfil para o SID {sid}")
                    }
                    outro => outro.to_string(),
                })?;
            println!("{}", perfil.profile_id);
            eprintln!("kioskd: perfil {nome:?} criado para a conta {sid}");
            Ok(())
        }
        ["perfil", "listar"] => {
            let db = abrir_banco().await?;
            let perfis = db.listar_perfis().await.map_err(|e| e.to_string())?;
            if perfis.is_empty() {
                eprintln!("kioskd: nenhum perfil cadastrado");
            }
            for p in perfis {
                let limite = p
                    .daily_total_limit_seconds
                    .map_or("sem limite".into(), duracao);
                let janelas = db
                    .janelas_do_perfil(p.profile_id)
                    .await
                    .map_err(|e| e.to_string())?;
                const NOMES: [&str; 7] = ["dom", "seg", "ter", "qua", "qui", "sex", "sab"];
                let janelas: Vec<String> = janelas
                    .iter()
                    .map(|(d, j)| {
                        format!(
                            "{} {}-{}",
                            NOMES[*d as usize],
                            j.start.format("%H:%M"),
                            j.end.format("%H:%M")
                        )
                    })
                    .collect();
                let janelas = if janelas.is_empty() {
                    "sem janela".into()
                } else {
                    janelas.join(", ")
                };
                println!(
                    "{}\t{}\t{}\tlimite diário: {limite}\t{janelas}",
                    p.profile_id, p.display_name, p.windows_sid
                );
            }
            Ok(())
        }
        ["perfil", "limite", perfil, ..] => {
            let perfil = id(perfil)?;
            let limite = if args.iter().any(|a| a == "--sem-limite") {
                None
            } else {
                Some(segundos(&exigir(args, "--segundos")?, "--segundos")?)
            };
            let db = abrir_banco().await?;
            db.set_profile_limit(&EventContext::local(), perfil, limite)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => "perfil inexistente".to_string(),
                    outro => outro.to_string(),
                })?;
            eprintln!(
                "kioskd: limite diário do perfil agora é {}",
                limite.map_or("sem limite".into(), duracao)
            );
            Ok(())
        }
        ["perfil", "janela", perfil, ..] => {
            let perfil = id(perfil)?;
            let dias = dias(&exigir(args, "--dias")?)?;
            let janela = exigir(args, "--janela")?;
            let (a, b) = janela
                .split_once('-')
                .ok_or_else(|| format!("--janela deve ser HH:MM-HH:MM, recebi {janela:?}"))?;
            let janela = kiosk_engine::database_schema::TimeWindow {
                start: hora(a)?,
                end: hora(b)?,
            };
            if janela.start >= janela.end {
                return Err("o início da janela deve ser anterior ao fim".into());
            }
            let db = abrir_banco().await?;
            for d in &dias {
                db.set_profile_window(&EventContext::local(), perfil, *d, &janela)
                    .await
                    .map_err(|e| format!("janela não gravada (o perfil existe?): {e}"))?;
            }
            eprintln!("kioskd: janela gravada para {} dia(s)", dias.len());
            Ok(())
        }
        ["app", "criar", ..] => {
            use kiosk_engine::database_schema::ProcessStatus;
            let processo = exigir(args, "--processo")?;
            let status = match (
                args.iter().any(|a| a == "--isento"),
                args.iter().any(|a| a == "--ignorar"),
            ) {
                (true, true) => return Err("use --isento ou --ignorar, não os dois".into()),
                (true, false) => ProcessStatus::Whitelisted,
                (false, true) => ProcessStatus::Ignored,
                (false, false) => ProcessStatus::Monitored,
            };
            let db = abrir_banco().await?;
            let app = kiosk_engine::database_schema::AppMetadata {
                app_id: Uuid::new_v4(),
                process_name: processo.clone(),
                status,
            };
            db.create_app(&EventContext::local(), &app)
                .await
                .map_err(|e| match e {
                    sqlx::Error::Database(d) if d.is_unique_violation() => {
                        format!("{processo} já está cadastrado")
                    }
                    outro => outro.to_string(),
                })?;
            println!("{}", app.app_id);
            eprintln!("kioskd: app {processo} cadastrado ({})", status.as_str());
            Ok(())
        }
        ["app", "listar"] => {
            let db = abrir_banco().await?;
            let apps = db.listar_apps().await.map_err(|e| e.to_string())?;
            if apps.is_empty() {
                eprintln!("kioskd: nenhum app cadastrado");
            }
            for a in apps {
                println!("{}\t{}\t{}", a.app_id, a.process_name, a.status.as_str());
            }
            Ok(())
        }
        _ => Err(USO.to_string()),
    }
}
