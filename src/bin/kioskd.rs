//! Serviço do kiosk. Uso:
//!
//! - `kioskd servir [--sem-agente]`: sobe a API de comando (exige `KIOSK_LISTEN`, sem valor
//!   padrão — ADR-005) e, no Windows, o agente de enforcement (tick de 2 s) e a limpeza diária
//!   do log (retenção de 90 dias). Fora do Windows só a API sobe.
//! - `kioskd codigo-pareamento`: gera o código de pareamento (válido por 5 min) para digitar no
//!   dispositivo dos pais. No Windows, só administrador elevado; a recusa vai para o log forense.
//! - `kioskd dispositivos`: lista os dispositivos pareados.
//! - `kioskd revogar <id>`: revoga um dispositivo.
//! - `kioskd perfil criar --conta USUARIO --nome NOME [--limite-total SEGUNDOS]` (Windows; em
//!   outros sistemas, `--sid S-1-...` no lugar de `--conta`), `perfil listar`,
//!   `perfil limite <id> --segundos N|--sem-limite`, `perfil janela <id> --dias seg,ter,... --janela HH:MM-HH:MM`.
//! - `kioskd app criar --processo NOME.exe [--isento | --ignorar]` e `app listar`.
//! - `kioskd regra criar --perfil ID --app ID [--limite S] [--janela HH:MM-HH:MM] [--ate RFC3339] [--ativar]`,
//!   `regra ativar|revogar <id>`, `regra limite <id> --segundos N` e `regra listar [--perfil ID]`:
//!   administração local das regras, com a mesma validação e auditoria da API.
//! - `kioskd instalar --escutar ip:porta[,...]` (Windows, administrador): copia os executáveis
//!   para `%ProgramFiles%\kiosk`, protege `%ProgramData%\kiosk` e registra e inicia o serviço
//!   `kiosk` (LocalSystem, início automático, reinício em falha; ADR-008). `kioskd desinstalar
//!   [--apagar-dados]` desfaz; `kioskd estado` mostra o serviço; `kioskd servico` é o modo
//!   serviço, chamado pelo Windows.
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
  kioskd codigo-pareamento   (Windows: PowerShell como administrador)
  kioskd dispositivos
  kioskd revogar <id-do-dispositivo>
  kioskd perfil criar --conta USUARIO --nome NOME [--limite-total SEGUNDOS]   (ou --sid S-1-... no lugar de --conta)
  kioskd perfil listar
  kioskd perfil limite <id-do-perfil> --segundos N | --sem-limite
  kioskd perfil janela <id-do-perfil> --dias seg,ter,qua,qui,sex --janela 08:00-20:00
  kioskd app criar --processo NOME.exe [--isento | --ignorar]
  kioskd app listar
  kioskd regra criar --perfil ID --app ID [--limite SEGUNDOS] [--janela HH:MM-HH:MM] [--ate RFC3339] [--ativar]
  kioskd regra ativar <id-da-regra>
  kioskd regra revogar <id-da-regra>
  kioskd regra limite <id-da-regra> --segundos N
  kioskd regra listar [--perfil ID]
  kioskd instalar --escutar ip:porta[,ip:porta...]   (Windows, PowerShell como administrador)
  kioskd desinstalar [--apagar-dados]
  kioskd estado
  kioskd servico   (chamado pelo Windows ao iniciar o serviço; não rode à mão)
variáveis: KIOSK_DB (padrão no Windows: sqlite://C:/ProgramData/kiosk/kiosk.db), KIOSK_LISTEN (ip:porta[,ip:porta...], obrigatória em `servir`)";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // o modo serviço entrega a thread principal ao SCM antes de existir qualquer runtime
    let resultado = if args == ["servico"] {
        servico()
    } else {
        tokio::runtime::Runtime::new()
            .map_err(|e| format!("runtime: {e}"))
            .and_then(|rt| rt.block_on(executar(&args)))
    };
    match resultado {
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
    lista_de_enderecos(&bruto)
}

/// `ip:porta[,ip:porta...]`, já recusando o que a ADR-005 não permite.
fn lista_de_enderecos(bruto: &str) -> Result<Vec<SocketAddr>, String> {
    let lista = bruto
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
        .collect::<Result<Vec<SocketAddr>, String>>()?;
    if lista.is_empty() {
        return Err("nenhum endereço de escuta informado (use ip:porta)".into());
    }
    api::validar_enderecos(&lista).map_err(|e| e.to_string())?;
    Ok(lista)
}

/// API, limpeza do log e (no Windows) o agente, até a API parar ou falhar.
async fn servir(com_agente: bool) -> Result<(), String> {
    let enderecos = enderecos()?;
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

/// `kioskd servico`: o processo que o SCM inicia (ADR-008).
#[cfg(windows)]
fn servico() -> Result<(), String> {
    kiosk_engine::servico_windows::despachar(|parar| {
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
        let resultado = rt.block_on(async {
            tokio::select! {
                r = servir(true) => r,
                () = parar.notified() => Ok(()),
            }
        });
        rt.shutdown_timeout(std::time::Duration::from_secs(5));
        resultado
    })
}

#[cfg(not(windows))]
fn servico() -> Result<(), String> {
    Err(SO_WINDOWS.into())
}

#[cfg(not(windows))]
const SO_WINDOWS: &str = "este comando só existe no Windows (serviço do Windows, ADR-008)";

/// Só administrador elevado gera código de pareamento no PC do serviço (regra do dono, E-008):
/// sem isto, a barreira era só a ACL da pasta do banco, que não existe antes do `instalar` nem
/// com `KIOSK_DB` em outro lugar. A recusa vai para o log forense quando o banco abre.
#[cfg(windows)]
async fn exigir_administrador() -> Result<(), String> {
    if kiosk_engine::sistema_windows::administrador_elevado()? {
        return Ok(());
    }
    let motivo = "código de pareamento pedido sem administrador elevado";
    let registro = match abrir_banco().await {
        Ok(db) => db
            .recusar_codigo_pareamento(&EventContext::local(), motivo)
            .await
            .map_err(|e| e.to_string()),
        Err(e) => Err(e),
    };
    let base = "só um administrador gera o código de pareamento: abra o PowerShell com \"Executar como administrador\"";
    Err(match registro {
        Ok(()) => base.to_string(),
        Err(e) => format!("{base} (a recusa não foi registrada no log forense: {e})"),
    })
}

/// Fora do Windows não há serviço (ADR-002); a exigência de `sudo` vem com o E-009 e o E-010.
#[cfg(not(windows))]
async fn exigir_administrador() -> Result<(), String> {
    Ok(())
}

/// Pede que digitem APAGAR antes de apagar os dados (ADR-008).
fn confirmar_apagar_dados() -> Result<(), String> {
    eprint!(
        "kioskd: isto apaga o banco (perfis, regras, uso e log forense). Digite APAGAR para confirmar: "
    );
    let mut linha = String::new();
    std::io::stdin()
        .read_line(&mut linha)
        .map_err(|e| format!("leitura da confirmação: {e}"))?;
    if linha.trim() != "APAGAR" {
        return Err("não confirmado: nada foi alterado".into());
    }
    Ok(())
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

#[cfg(windows)]
fn instalar(escutar: &str) -> Result<(), String> {
    use kiosk_engine::servico_windows as sv;
    let pid = sv::instalar(escutar)?;
    eprintln!("kioskd: serviço em execução (pid {pid}), escutando em {escutar}");
    eprintln!(
        "kioskd: log do serviço em {}; os comandos de cadastro agora pedem PowerShell como administrador",
        sv::arquivo_log().display()
    );
    Ok(())
}

#[cfg(windows)]
fn desinstalar(apagar_dados: bool) -> Result<(), String> {
    kiosk_engine::servico_windows::desinstalar(apagar_dados)
}

#[cfg(windows)]
fn estado() -> Result<(), String> {
    use kiosk_engine::servico_windows as sv;
    let Some(e) = sv::estado()? else {
        println!("serviço: {} não instalado", sv::NOME_SERVICO);
        return Ok(());
    };
    println!("serviço: {}", sv::NOME_SERVICO);
    if e.pid != 0 {
        println!("estado: {} (pid {})", sv::nome_estado(e.estado), e.pid);
    } else {
        println!("estado: {}", sv::nome_estado(e.estado));
    }
    let inicio = if e.inicio_automatico {
        "automático"
    } else {
        "manual ou desativado"
    };
    println!("início: {inicio}");
    println!("conta: {}", e.conta);
    println!("executável: {}", e.executavel);
    for v in &e.ambiente {
        println!("ambiente: {v}");
    }
    let no_path = if e.no_path {
        "sim"
    } else {
        "não (rode `kioskd instalar` de novo)"
    };
    println!("PATH do sistema: {no_path}");
    println!("dados: {}", sv::pasta_dados().display());
    println!("log: {}", sv::arquivo_log().display());
    Ok(())
}

#[cfg(not(windows))]
fn instalar(_escutar: &str) -> Result<(), String> {
    Err(SO_WINDOWS.into())
}

#[cfg(not(windows))]
fn desinstalar(_apagar_dados: bool) -> Result<(), String> {
    Err(SO_WINDOWS.into())
}

#[cfg(not(windows))]
fn estado() -> Result<(), String> {
    Err(SO_WINDOWS.into())
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
            servir(com_agente).await
        }
        ["instalar", "--escutar", escutar] => {
            let lista = lista_de_enderecos(escutar)?;
            let normalizado: Vec<String> = lista.iter().map(ToString::to_string).collect();
            instalar(&normalizado.join(","))
        }
        ["desinstalar", resto @ ..] => {
            let apagar = match resto {
                [] => false,
                ["--apagar-dados"] => true,
                _ => return Err(USO.to_string()),
            };
            if apagar {
                confirmar_apagar_dados()?;
            }
            desinstalar(apagar)
        }
        ["estado"] => estado(),
        ["codigo-pareamento"] => {
            exigir_administrador().await?;
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
        ["regra", "criar", ..] => {
            use kiosk_engine::comandos::{Comando, interpretar};
            use kiosk_engine::database_schema::{ActiveRule, RuleState};
            // mesmo corpo e mesma validação da API (Camada 3)
            let mut corpo = serde_json::json!({
                "profile_id": exigir(args, "--perfil")?,
                "app_id": exigir(args, "--app")?,
            });
            if let Some(l) = opcao(args, "--limite") {
                corpo["daily_limit_seconds"] = segundos(&l, "--limite")?.into();
            }
            if let Some(j) = opcao(args, "--janela") {
                let (a, b) = j
                    .split_once('-')
                    .ok_or_else(|| format!("--janela deve ser HH:MM-HH:MM, recebi {j:?}"))?;
                corpo["window_start"] = a.into();
                corpo["window_end"] = b.into();
            }
            if let Some(t) = opcao(args, "--ate") {
                corpo["effective_until"] = t.into();
            }
            let agora = Utc::now();
            let Comando::AddRule(nova) =
                interpretar("add_rule", corpo.to_string().as_bytes(), agora)?
            else {
                unreachable!("add_rule sempre produz AddRule")
            };
            let db = abrir_banco().await?;
            if !db
                .existe_perfil(nova.profile_id)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err("perfil inexistente (veja `kioskd perfil listar`)".into());
            }
            if !db
                .existe_app(nova.app_id)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err("app inexistente (veja `kioskd app listar`)".into());
            }
            let regra = ActiveRule {
                rule_id: Uuid::new_v4(),
                profile_id: nova.profile_id,
                app_id: nova.app_id,
                creation_timestamp: agora,
                state: RuleState::Pending,
                daily_limit_seconds: nova.daily_limit_seconds,
                time_window: nova.time_window,
                effective_until_timestamp: nova.effective_until,
            };
            let ctx = EventContext::local();
            db.create_active_rule(&ctx, &regra)
                .await
                .map_err(|e| e.to_string())?;
            let ativar = args.iter().any(|a| a == "--ativar");
            if ativar {
                db.set_rule_state(&ctx, regra.rule_id, RuleState::Active)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            println!("{}", regra.rule_id);
            eprintln!(
                "kioskd: regra criada ({})",
                if ativar {
                    "ativa"
                } else {
                    "pendente: use `regra ativar`"
                }
            );
            Ok(())
        }
        ["regra", acao @ ("ativar" | "revogar"), regra] => {
            use kiosk_engine::database_schema::RuleState;
            let regra = id(regra)?;
            let estado = if *acao == "ativar" {
                RuleState::Active
            } else {
                RuleState::Revoked
            };
            let db = abrir_banco().await?;
            db.set_rule_state(&EventContext::local(), regra, estado)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => "regra inexistente".to_string(),
                    outro => outro.to_string(),
                })?;
            eprintln!(
                "kioskd: regra {}",
                if *acao == "ativar" {
                    "ativada"
                } else {
                    "revogada"
                }
            );
            Ok(())
        }
        ["regra", "limite", regra, ..] => {
            let regra = id(regra)?;
            let s = segundos(&exigir(args, "--segundos")?, "--segundos")?;
            if s > kiosk_engine::comandos::MAX_LIMITE_SEGUNDOS {
                return Err(format!(
                    "--segundos acima de {}",
                    kiosk_engine::comandos::MAX_LIMITE_SEGUNDOS
                ));
            }
            let db = abrir_banco().await?;
            db.set_rule_limit(&EventContext::local(), regra, s)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => "regra inexistente ou revogada".to_string(),
                    outro => outro.to_string(),
                })?;
            eprintln!("kioskd: limite diário da regra agora é {}", duracao(s));
            Ok(())
        }
        ["regra", "listar", ..] => {
            let perfil = opcao(args, "--perfil").map(|p| id(&p)).transpose()?;
            let db = abrir_banco().await?;
            let apps: std::collections::HashMap<Uuid, String> = db
                .listar_apps()
                .await
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|a| (a.app_id, a.process_name))
                .collect();
            let regras = db.listar_regras(perfil).await.map_err(|e| e.to_string())?;
            if regras.is_empty() {
                eprintln!("kioskd: nenhuma regra cadastrada");
            }
            for r in regras {
                let app = apps.get(&r.app_id).map_or("?", String::as_str);
                let limite = r.daily_limit_seconds.map_or("sem limite".into(), duracao);
                let janela = r.time_window.map_or("sem janela".into(), |j| {
                    format!("{}-{}", j.start.format("%H:%M"), j.end.format("%H:%M"))
                });
                println!(
                    "{}\t{}\t{app}\t{}\tlimite: {limite}\t{janela}",
                    r.rule_id,
                    r.profile_id,
                    r.state.as_str()
                );
            }
            Ok(())
        }
        _ => Err(USO.to_string()),
    }
}
