//! CLI dos pais (E-005 F3). Uso:
//!
//! - `kioskctl parear --servidor HOST:PORTA --codigo CODIGO [--nome NOME]`
//! - `kioskctl regra criar --perfil ID --app ID [--limite SEGUNDOS] [--janela HH:MM-HH:MM] [--ate RFC3339]`
//! - `kioskctl regra ativar <id>` e `kioskctl regra revogar <id>`
//! - `kioskctl regra limite <id> --segundos N`
//! - `kioskctl uso --perfil ID [--data AAAA-MM-DD]`
//! - `kioskctl log --perfil ID [--dias N] [--limite N]`
//!
//! As credenciais (inclusive a chave privada) ficam em `KIOSK_CREDENCIAIS`, ou em
//! `~/.config/kiosk/credenciais.json` (`%APPDATA%\kiosk\credenciais.json` no Windows).

use std::path::PathBuf;
use std::process::ExitCode;

use kiosk_engine::cliente::{self, Credenciais};
use uuid::Uuid;

const USO: &str = "uso:\n  \
kioskctl parear --servidor HOST:PORTA --codigo CODIGO [--nome NOME]\n  \
kioskctl regra criar --perfil ID --app ID [--limite SEGUNDOS] [--janela HH:MM-HH:MM] [--ate RFC3339]\n  \
kioskctl regra ativar <id>\n  \
kioskctl regra revogar <id>\n  \
kioskctl regra limite <id> --segundos N\n  \
kioskctl uso --perfil ID [--data AAAA-MM-DD]\n  \
kioskctl log --perfil ID [--dias N] [--limite N]";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
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

/// Valor de `--nome` em `args`, se houver.
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
        "profile_id": exigir(args, "--perfil")?,
        "app_id": exigir(args, "--app")?,
    });
    if let Some(l) = opcao(args, "--limite") {
        let l: u64 = l.parse().map_err(|_| format!("--limite inválido: {l:?}"))?;
        obj["daily_limit_seconds"] = l.into();
    }
    if let Some(j) = opcao(args, "--janela") {
        let (a, b) = j
            .split_once('-')
            .ok_or_else(|| format!("--janela deve ser HH:MM-HH:MM, recebi {j:?}"))?;
        obj["window_start"] = a.into();
        obj["window_end"] = b.into();
    }
    if let Some(t) = opcao(args, "--ate") {
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
        ["parear", ..] => {
            let servidor = exigir(args, "--servidor")?;
            let codigo = exigir(args, "--codigo")?;
            let nome = opcao(args, "--nome").unwrap_or_else(|| "Dispositivo dos pais".into());
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
        ["regra", "criar", ..] => {
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let r = enviar(&cred, "add_rule", &json_da_regra(args)?).await?;
            let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap_or_default();
            println!(
                "regra criada (pendente): {}",
                v["id"].as_str().unwrap_or("?")
            );
            Ok(())
        }
        ["regra", acao @ ("ativar" | "revogar"), id] => {
            let id = Uuid::parse_str(id).map_err(|_| format!("identificador inválido: {id:?}"))?;
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let (nome_api, feito) = if *acao == "ativar" {
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
        ["regra", "limite", id, ..] => {
            let id = Uuid::parse_str(id).map_err(|_| format!("identificador inválido: {id:?}"))?;
            let segundos = exigir(args, "--segundos")?;
            let segundos: u64 = segundos
                .parse()
                .map_err(|_| format!("--segundos inválido: {segundos:?}"))?;
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let corpo = serde_json::json!({ "rule_id": id, "daily_limit_seconds": segundos });
            enviar(&cred, "modify_limit", &corpo.to_string()).await?;
            println!("regra {id}: limite diário agora é {}", duracao(segundos));
            Ok(())
        }
        ["uso", ..] => {
            let perfil = exigir(args, "--perfil")?;
            let data = opcao(args, "--data")
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
            let perfil = exigir(args, "--perfil")?;
            let dias: i64 = opcao(args, "--dias")
                .map(|d| d.parse().map_err(|_| format!("--dias inválido: {d:?}")))
                .transpose()?
                .unwrap_or(1);
            let agora = chrono::Utc::now();
            let mut corpo = serde_json::json!({
                "from": (agora - chrono::Duration::days(dias)).to_rfc3339(),
                "to": agora.to_rfc3339(),
                "profile_id": perfil,
            });
            if let Some(l) = opcao(args, "--limite") {
                let l: u32 = l.parse().map_err(|_| format!("--limite inválido: {l:?}"))?;
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
