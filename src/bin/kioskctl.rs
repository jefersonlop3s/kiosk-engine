//! CLI dos pais (E-005 F3). Comandos em inglês (convenção); os equivalentes em português valem
//! igualmente (ADR-010, `kiosk_engine::cli`). Uso:
//!
//! - `kioskctl pair --server HOST:PORTA --code CODIGO [--name NOME]`
//! - `kioskctl rule create --profile ID --app ID [--limit SEGUNDOS] [--window HH:MM-HH:MM] [--until RFC3339]`
//! - `kioskctl rule activate <id>` e `kioskctl rule revoke <id>`
//! - `kioskctl rule limit <id> --seconds N`
//! - `kioskctl usage --profile ID [--date AAAA-MM-DD]`
//! - `kioskctl log --profile ID [--days N] [--limit N]`
//!
//! As credenciais (inclusive a chave privada) ficam em `KIOSK_CREDENCIAIS`, ou em
//! `~/.config/kiosk/credenciais.json` (`%APPDATA%\kiosk\credenciais.json` no Windows).

use std::path::PathBuf;
use std::process::ExitCode;

use kiosk_engine::cliente::{self, Credenciais};
use uuid::Uuid;

const USO: &str = "uso:\n  \
kioskctl pair --server HOST:PORTA --code CODIGO [--name NOME]\n  \
kioskctl rule create --profile ID --app ID [--limit SEGUNDOS] [--window HH:MM-HH:MM] [--until RFC3339]\n  \
kioskctl rule activate <id>\n  \
kioskctl rule revoke <id>\n  \
kioskctl rule limit <id> --seconds N\n  \
kioskctl usage --profile ID [--date AAAA-MM-DD]\n  \
kioskctl log --profile ID [--days N] [--limit N]\n\
os mesmos comandos em português também valem (ex.: kioskctl parear = kioskctl pair); lista em docs/runbooks/comandos-em-portugues.md";

#[tokio::main]
async fn main() -> ExitCode {
    // português e inglês equivalem; daqui em diante, só inglês (ADR-010)
    let args = kiosk_engine::cli::em_ingles(&std::env::args().skip(1).collect::<Vec<_>>());
    match executar(&args).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(msg) => {
            eprintln!("kioskctl: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn caminho_credenciais() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("KIOSK_CREDENCIAIS") {
        return Ok(PathBuf::from(p));
    }
    if cfg!(windows) {
        let base = std::env::var("APPDATA").map_err(|_| "APPDATA não definida".to_string())?;
        return Ok(PathBuf::from(base).join("kiosk").join("credenciais.json"));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME não definida".to_string())?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("kiosk")
        .join("credenciais.json"))
}

/// Valor de `--name` em `args`, se houver.
fn opcao(args: &[String], nome: &str) -> Option<String> {
    args.iter()
        .position(|a| a == nome)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn exigir(args: &[String], nome: &str) -> Result<String, String> {
    opcao(args, nome).ok_or_else(|| format!("falta {nome}\n{USO}"))
}

fn json_da_regra(args: &[String]) -> Result<String, String> {
    let mut obj = serde_json::json!({
        "profile_id": exigir(args, "--profile")?,
        "app_id": exigir(args, "--app")?,
    });
    if let Some(l) = opcao(args, "--limit") {
        let l: u64 = l.parse().map_err(|_| format!("--limit inválido: {l:?}"))?;
        obj["daily_limit_seconds"] = l.into();
    }
    if let Some(j) = opcao(args, "--window") {
        let (a, b) = j
            .split_once('-')
            .ok_or_else(|| format!("--window deve ser HH:MM-HH:MM, recebi {j:?}"))?;
        obj["window_start"] = a.into();
        obj["window_end"] = b.into();
    }
    if let Some(t) = opcao(args, "--until") {
        obj["effective_until"] = t.into();
    }
    Ok(obj.to_string())
}

async fn enviar(
    cred: &Credenciais,
    acao: &str,
    corpo: &str,
) -> Result<cliente::RespostaApi, String> {
    let r = cliente::comando(cred, acao, corpo).await?;
    if r.aceita() {
        Ok(r)
    } else {
        // a recusa é genérica de propósito; o motivo exato está no log do PC
        Err(format!(
            "o serviço recusou o comando (HTTP {}). O motivo fica no log do PC.",
            r.status
        ))
    }
}

/// `3725` -> `1h 02min 05s`.
fn duracao(segundos: u64) -> String {
    let (h, m, s) = (segundos / 3600, segundos % 3600 / 60, segundos % 60);
    match (h, m) {
        (0, 0) => format!("{s}s"),
        (0, _) => format!("{m}min {s:02}s"),
        _ => format!("{h}h {m:02}min {s:02}s"),
    }
}

async fn executar(args: &[String]) -> Result<(), String> {
    let partes: Vec<&str> = args.iter().map(String::as_str).collect();
    match partes.as_slice() {
        ["pair", ..] => {
            let servidor = exigir(args, "--server")?;
            let codigo = exigir(args, "--code")?;
            let nome = opcao(args, "--name").unwrap_or_else(|| "Dispositivo dos pais".into());
            let caminho = caminho_credenciais()?;
            if caminho.exists() {
                return Err(format!(
                    "já existe {}: apague o arquivo para parear de novo",
                    caminho.display()
                ));
            }
            let (mut cred, chave) = Credenciais::nova(&servidor);
            let id = cliente::parear(&servidor, &codigo, &chave, &nome).await?;
            cred.device_id = Some(id);
            cred.salvar(&caminho)?;
            println!("dispositivo pareado: {id}");
            eprintln!("credenciais gravadas em {}", caminho.display());
            Ok(())
        }
        ["rule", "create", ..] => {
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let r = enviar(&cred, "add_rule", &json_da_regra(args)?).await?;
            let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap_or_default();
            println!(
                "regra criada (pendente): {}",
                v["id"].as_str().unwrap_or("?")
            );
            Ok(())
        }
        ["rule", acao @ ("activate" | "revoke"), id] => {
            let id = Uuid::parse_str(id).map_err(|_| format!("identificador inválido: {id:?}"))?;
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let (nome_api, feito) = if *acao == "activate" {
                ("activate_rule", "ativada")
            } else {
                ("revoke_rule", "revogada")
            };
            enviar(
                &cred,
                nome_api,
                &serde_json::json!({ "rule_id": id }).to_string(),
            )
            .await?;
            println!("regra {id} {feito}");
            Ok(())
        }
        ["rule", "limit", id, ..] => {
            let id = Uuid::parse_str(id).map_err(|_| format!("identificador inválido: {id:?}"))?;
            let segundos = exigir(args, "--seconds")?;
            let segundos: u64 = segundos
                .parse()
                .map_err(|_| format!("--seconds inválido: {segundos:?}"))?;
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let corpo = serde_json::json!({ "rule_id": id, "daily_limit_seconds": segundos });
            enviar(&cred, "modify_limit", &corpo.to_string()).await?;
            println!("regra {id}: limite diário agora é {}", duracao(segundos));
            Ok(())
        }
        ["usage", ..] => {
            let perfil = exigir(args, "--profile")?;
            let data = opcao(args, "--date")
                .unwrap_or_else(|| chrono::Local::now().date_naive().to_string());
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let corpo = serde_json::json!({ "profile_id": perfil, "date": data });
            let r = enviar(&cred, "usage", &corpo.to_string()).await?;
            let v: serde_json::Value = serde_json::from_str(&r.corpo)
                .map_err(|_| "resposta de uso ilegível".to_string())?;
            println!("uso em {data}");
            println!(
                "  sessão ativa: {}",
                duracao(v["session_seconds"].as_u64().unwrap_or(0))
            );
            for a in v["apps"].as_array().into_iter().flatten() {
                println!(
                    "  {}: {}",
                    a["process_name"].as_str().unwrap_or("?"),
                    duracao(a["seconds"].as_u64().unwrap_or(0))
                );
            }
            Ok(())
        }
        ["log", ..] => {
            let perfil = exigir(args, "--profile")?;
            let dias: i64 = opcao(args, "--days")
                .map(|d| d.parse().map_err(|_| format!("--days inválido: {d:?}")))
                .transpose()?
                .unwrap_or(1);
            let agora = chrono::Utc::now();
            let mut corpo = serde_json::json!({
                "from": (agora - chrono::Duration::days(dias)).to_rfc3339(),
                "to": agora.to_rfc3339(),
                "profile_id": perfil,
            });
            if let Some(l) = opcao(args, "--limit") {
                let l: u32 = l.parse().map_err(|_| format!("--limit inválido: {l:?}"))?;
                corpo["limit"] = l.into();
            }
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let r = enviar(&cred, "log", &corpo.to_string()).await?;
            let v: serde_json::Value = serde_json::from_str(&r.corpo)
                .map_err(|_| "resposta de log ilegível".to_string())?;
            let eventos = v["events"].as_array().cloned().unwrap_or_default();
            if eventos.is_empty() {
                eprintln!("kioskctl: nenhum evento no período");
            }
            for e in eventos {
                println!(
                    "{}  {}  {}  {}",
                    e["timestamp"].as_str().unwrap_or("?"),
                    e["event_type"].as_str().unwrap_or("?"),
                    e["action"].as_str().unwrap_or("?"),
                    e["detail"].as_str().unwrap_or("")
                );
            }
            Ok(())
        }
        _ => Err(USO.to_string()),
    }
}
