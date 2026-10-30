//! CLI dos pais (E-005 F3). Comandos em inglês (convenção); os equivalentes em português valem
//! igualmente (ADR-010, `kiosk_engine::cli`). Uso:
//!
//! - `kioskctl pair --server HOST:PORTA --code CODIGO [--name NOME]`
//! - `kioskctl rule create --profile ID --app ID [--limit SEGUNDOS] [--window HH:MM-HH:MM] [--until RFC3339]`
//! - `kioskctl rule activate <id>` e `kioskctl rule revoke <id>`
//! - `kioskctl rule limit <id> --seconds N`
//! - `kioskctl profile list`, `kioskctl app list` e `kioskctl rule list [--profile ID]`
//! - `kioskctl usage --profile ID [--date AAAA-MM-DD]`
//! - `kioskctl log --profile ID [--days N] [--limit N]`
//!
//! As credenciais (inclusive a chave privada) ficam em `KIOSK_CREDENCIAIS`, ou em
//! `~/.config/kiosk/credenciais.json` (`%APPDATA%\kiosk\credenciais.json` no Windows).

use std::path::PathBuf;
use std::process::ExitCode;

use kiosk_engine::cliente::{self, Credenciais};
use kiosk_engine::i18n::{Msg, idioma};
use kiosk_engine::textos::duracao;
use uuid::Uuid;

fn uso() -> String {
    Msg::UsoKioskctl.to_string()
}

#[tokio::main]
async fn main() -> ExitCode {
    // português e inglês equivalem; daqui em diante, só inglês (ADR-010)
    let args = kiosk_engine::cli::em_ingles(&std::env::args().skip(1).collect::<Vec<_>>());
    match kiosk_engine::cli::validar(&args, kiosk_engine::cli::FORMAS_KIOSKCTL) {
        Ok(()) => {}
        Err(kiosk_engine::cli::ErroArgumentos::PedeAjuda) => {
            println!("{}", uso());
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("kioskctl: {}", e.msg());
            return ExitCode::FAILURE;
        }
    }
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
        let base = std::env::var("APPDATA").map_err(|_| {
            Msg::VarNaoDefinida {
                nome: "APPDATA".into(),
            }
            .to_string()
        })?;
        return Ok(PathBuf::from(base).join("kiosk").join("credenciais.json"));
    }
    let home = std::env::var("HOME").map_err(|_| {
        Msg::VarNaoDefinida {
            nome: "HOME".into(),
        }
        .to_string()
    })?;
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
    opcao(args, nome).ok_or_else(|| {
        format!(
            "{}\n{}",
            Msg::FaltaOpcao {
                nome: nome.to_owned()
            },
            uso()
        )
    })
}

fn json_da_regra(args: &[String]) -> Result<String, String> {
    let mut obj = serde_json::json!({
        "profile_id": exigir(args, "--profile")?,
        "app_id": exigir(args, "--app")?,
    });
    if let Some(l) = opcao(args, "--limit") {
        let l: u64 = l.parse().map_err(|_| {
            Msg::OpcaoInvalida {
                nome: "--limit".into(),
                valor: l.clone(),
            }
            .to_string()
        })?;
        obj["daily_limit_seconds"] = l.into();
    }
    if let Some(j) = opcao(args, "--window") {
        let (a, b) = j.split_once('-').ok_or_else(|| {
            Msg::JanelaFormato {
                recebido: j.clone(),
            }
            .to_string()
        })?;
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
        Err(Msg::ServicoRecusou { status: r.status }.to_string())
    }
}

/// Consulta de listagem: a resposta é JSON, e o que não for lido vira erro, nunca lista vazia.
async fn listar(
    cred: &Credenciais,
    acao: &str,
    corpo: serde_json::Value,
    chave: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let r = enviar(cred, acao, &corpo.to_string()).await?;
    let v: serde_json::Value =
        serde_json::from_str(&r.corpo).map_err(|_| Msg::RespostaDeListaIlegivel.to_string())?;
    v[chave]
        .as_array()
        .cloned()
        .ok_or_else(|| Msg::RespostaDeListaIlegivel.to_string())
}

fn texto<'a>(v: &'a serde_json::Value, campo: &str) -> &'a str {
    v[campo].as_str().unwrap_or("?")
}

fn janela_do_json(v: &serde_json::Value) -> Option<String> {
    Some(format!(
        "{}-{}",
        v["window_start"].as_str()?,
        v["window_end"].as_str()?
    ))
}

async fn executar(args: &[String]) -> Result<(), String> {
    let partes: Vec<&str> = args.iter().map(String::as_str).collect();
    match partes.as_slice() {
        ["version"] => {
            println!("{}", kiosk_engine::versao::atual().completa("kioskctl"));
            Ok(())
        }
        ["pair", ..] => {
            let servidor = exigir(args, "--server")?;
            let codigo = exigir(args, "--code")?;
            let nome = opcao(args, "--name").unwrap_or_else(|| Msg::NomeDoDispositivo.to_string());
            let caminho = caminho_credenciais()?;
            if caminho.exists() {
                return Err(Msg::CredenciaisJaExistem {
                    caminho: caminho.display().to_string(),
                }
                .to_string());
            }
            let (mut cred, chave) = Credenciais::nova(&servidor);
            let id = cliente::parear(&servidor, &codigo, &chave, &nome).await?;
            cred.device_id = Some(id);
            cred.salvar(&caminho)?;
            println!("{}", Msg::DispositivoPareado { id: id.to_string() });
            eprintln!(
                "{}",
                Msg::CredenciaisGravadas {
                    caminho: caminho.display().to_string()
                }
            );
            Ok(())
        }
        ["rule", "create", ..] => {
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let r = enviar(&cred, "add_rule", &json_da_regra(args)?).await?;
            let v: serde_json::Value = serde_json::from_str(&r.corpo).unwrap_or_default();
            println!(
                "{}",
                Msg::RegraCriadaPeloCliente {
                    id: v["id"].as_str().unwrap_or("?").to_owned()
                }
            );
            Ok(())
        }
        ["rule", acao @ ("activate" | "revoke"), id] => {
            let id = Uuid::parse_str(id).map_err(|_| {
                Msg::IdInvalido {
                    valor: (*id).to_owned(),
                }
                .to_string()
            })?;
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let ativar = *acao == "activate";
            let nome_api = if ativar {
                "activate_rule"
            } else {
                "revoke_rule"
            };
            enviar(
                &cred,
                nome_api,
                &serde_json::json!({ "rule_id": id }).to_string(),
            )
            .await?;
            let id = id.to_string();
            println!(
                "{}",
                if ativar {
                    Msg::RegraAtivadaCliente { id }
                } else {
                    Msg::RegraRevogadaCliente { id }
                }
            );
            Ok(())
        }
        ["rule", "limit", id, ..] => {
            let id = Uuid::parse_str(id).map_err(|_| {
                Msg::IdInvalido {
                    valor: (*id).to_owned(),
                }
                .to_string()
            })?;
            let segundos = exigir(args, "--seconds")?;
            let segundos: u64 = segundos.parse().map_err(|_| {
                Msg::OpcaoInvalida {
                    nome: "--seconds".into(),
                    valor: segundos.clone(),
                }
                .to_string()
            })?;
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let corpo = serde_json::json!({ "rule_id": id, "daily_limit_seconds": segundos });
            enviar(&cred, "modify_limit", &corpo.to_string()).await?;
            println!(
                "{}",
                Msg::LimiteDaRegraCliente {
                    id: id.to_string(),
                    limite: duracao(segundos)
                }
            );
            Ok(())
        }
        ["profile", "list"] => {
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let perfis = listar(&cred, "profiles", serde_json::json!({}), "profiles").await?;
            if perfis.is_empty() {
                eprintln!("kioskctl: {}", Msg::NenhumPerfil);
            }
            let idioma = idioma();
            for p in perfis {
                let limite = p["daily_total_limit_seconds"]
                    .as_u64()
                    .map_or(Msg::SemLimite.to_string(), duracao);
                let janelas: Vec<String> = p["windows"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|j| {
                        let dia = u8::try_from(j["weekday"].as_u64()?).ok()?;
                        Some(format!("{} {}", idioma.dia_curto(dia), janela_do_json(j)?))
                    })
                    .collect();
                let janelas = if janelas.is_empty() {
                    Msg::SemJanela.to_string()
                } else {
                    janelas.join(", ")
                };
                println!(
                    "{}\t{}\t{}\t{janelas}",
                    texto(&p, "profile_id"),
                    texto(&p, "display_name"),
                    Msg::RotuloLimiteDiario { limite }
                );
            }
            Ok(())
        }
        ["app", "list"] => {
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let apps = listar(&cred, "apps", serde_json::json!({}), "apps").await?;
            if apps.is_empty() {
                eprintln!("kioskctl: {}", Msg::NenhumApp);
            }
            for a in apps {
                println!(
                    "{}\t{}\t{}",
                    texto(&a, "app_id"),
                    texto(&a, "process_name"),
                    texto(&a, "status")
                );
            }
            Ok(())
        }
        ["rule", "list", ..] => {
            let mut corpo = serde_json::json!({});
            if let Some(p) = opcao(args, "--profile") {
                corpo["profile_id"] = p.into();
            }
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let regras = listar(&cred, "rules", corpo, "rules").await?;
            if regras.is_empty() {
                eprintln!("kioskctl: {}", Msg::NenhumaRegra);
            }
            // o nome do app vem da listagem de apps, como no `kioskd rule list`
            let apps: std::collections::HashMap<String, String> =
                listar(&cred, "apps", serde_json::json!({}), "apps")
                    .await?
                    .iter()
                    .map(|a| {
                        (
                            texto(a, "app_id").to_owned(),
                            texto(a, "process_name").to_owned(),
                        )
                    })
                    .collect();
            for r in regras {
                let app = apps.get(texto(&r, "app_id")).map_or("?", String::as_str);
                let limite = r["daily_limit_seconds"]
                    .as_u64()
                    .map_or(Msg::SemLimite.to_string(), duracao);
                let janela = janela_do_json(&r).unwrap_or_else(|| Msg::SemJanela.to_string());
                println!(
                    "{}\t{}\t{app}\t{}\t{}\t{janela}",
                    texto(&r, "rule_id"),
                    texto(&r, "profile_id"),
                    texto(&r, "state"),
                    Msg::RotuloLimite { limite }
                );
            }
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
                .map_err(|_| Msg::RespostaDeUsoIlegivel.to_string())?;
            println!("{}", Msg::UsoEm { data: data.clone() });
            println!(
                "{}",
                Msg::SessaoAtiva {
                    duracao: duracao(v["session_seconds"].as_u64().unwrap_or(0))
                }
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
            // sem --profile: todos os eventos do PC, inclusive os que não são de um perfil
            // (AGENT_STOPPED, por exemplo), que de outro modo os pais não veriam
            let perfil = opcao(args, "--profile");
            let dias: i64 = opcao(args, "--days")
                .map(|d| {
                    d.parse().map_err(|_| {
                        Msg::OpcaoInvalida {
                            nome: "--days".into(),
                            valor: d.clone(),
                        }
                        .to_string()
                    })
                })
                .transpose()?
                .unwrap_or(1);
            let agora = chrono::Utc::now();
            let mut corpo = serde_json::json!({
                "from": (agora - chrono::Duration::days(dias)).to_rfc3339(),
                "to": agora.to_rfc3339(),
            });
            if let Some(p) = &perfil {
                corpo["profile_id"] = serde_json::json!(p);
            }
            if let Some(l) = opcao(args, "--limit") {
                let l: u32 = l.parse().map_err(|_| {
                    Msg::OpcaoInvalida {
                        nome: "--limit".into(),
                        valor: l.clone(),
                    }
                    .to_string()
                })?;
                corpo["limit"] = l.into();
            }
            let cred = Credenciais::carregar(&caminho_credenciais()?)?;
            let r = enviar(&cred, "log", &corpo.to_string()).await?;
            let v: serde_json::Value = serde_json::from_str(&r.corpo)
                .map_err(|_| Msg::RespostaDeLogIlegivel.to_string())?;
            let eventos = v["events"].as_array().cloned().unwrap_or_default();
            if eventos.is_empty() {
                eprintln!("kioskctl: {}", Msg::NenhumEvento);
            }
            for e in eventos {
                // lendo o PC todo, diz de quem é cada evento: as 8 primeiras letras do perfil, ou
                // `PC` se o evento não é de um perfil
                let de_quem = if perfil.is_none() {
                    let p = e["profile_id"]
                        .as_str()
                        .map_or("PC", |p| &p[..p.len().min(8)]);
                    format!("[{p}]  ")
                } else {
                    String::new()
                };
                println!(
                    "{}  {}  {}  {}{}",
                    e["timestamp"].as_str().unwrap_or("?"),
                    e["event_type"].as_str().unwrap_or("?"),
                    e["action"].as_str().unwrap_or("?"),
                    de_quem,
                    kiosk_engine::detalhes::legivel(e["detail"].as_str().unwrap_or(""), idioma())
                );
            }
            Ok(())
        }
        _ => Err(uso()),
    }
}
