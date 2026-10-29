//! Serviço do kiosk. Comandos em inglês (convenção); os equivalentes em português valem
//! igualmente (ADR-010, `kiosk_engine::cli`). Uso:
//!
//! - `kioskd serve [--no-agent]`: sobe a API de comando (exige `KIOSK_LISTEN`, sem valor
//!   padrão — ADR-005) e, no Windows, o agente de enforcement (tick de 2 s) e a limpeza diária
//!   do log (retenção de 90 dias). Fora do Windows só a API sobe.
//! - `kioskd pairing-code`: gera o código de pareamento (válido por 5 min) para digitar no
//!   dispositivo dos pais. No Windows, só administrador elevado; a recusa vai para o log forense.
//! - `kioskd devices`: lista os dispositivos pareados; `kioskd revoke <id>` revoga um.
//! - `kioskd profile create --account USUARIO --name NOME [--total-limit SEGUNDOS]` (Windows; em
//!   outros sistemas, `--sid S-1-...` no lugar de `--account`), `profile list`,
//!   `profile limit <id> --seconds N|--no-limit`, `profile window <id> --days mon,tue,... --window HH:MM-HH:MM`.
//! - `kioskd app create --process NOME.exe [--exempt | --ignore]` e `app list`.
//! - `kioskd rule create --profile ID --app ID [--limit S] [--window HH:MM-HH:MM] [--until RFC3339] [--activate]`,
//!   `rule activate|revoke <id>`, `rule limit <id> --seconds N` e `rule list [--profile ID]`:
//!   administração local das regras, com a mesma validação e auditoria da API.
//! - `kioskd install --listen ip:porta[,...]` (Windows, administrador): copia os executáveis
//!   para `%ProgramFiles%\kiosk`, protege `%ProgramData%\kiosk` e registra e inicia o serviço
//!   `kiosk` (LocalSystem, início automático, reinício em falha; ADR-008). `kioskd uninstall
//!   [--purge-data]` desfaz; `kioskd status` mostra o serviço; `kioskd service` é o modo
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
use kiosk_engine::i18n::Msg;
use uuid::Uuid;

fn uso() -> String {
    Msg::UsoKioskd.to_string()
}

fn main() -> ExitCode {
    // português e inglês equivalem; daqui em diante, só inglês (ADR-010)
    let args = kiosk_engine::cli::em_ingles(&std::env::args().skip(1).collect::<Vec<_>>());
    match kiosk_engine::cli::validar(&args, kiosk_engine::cli::FORMAS_KIOSKD) {
        Ok(()) => {}
        Err(kiosk_engine::cli::ErroArgumentos::PedeAjuda) => {
            println!("{}", uso());
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("kioskd: {}", e.msg());
            return ExitCode::FAILURE;
        }
    }
    // o modo serviço entrega a thread principal ao SCM antes de existir qualquer runtime.
    // `servico` (português) é como a 0.0.4 registrou o serviço, e segue valendo
    let resultado = if args == ["service"] {
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
    eprintln!("kioskd: {}", Msg::BancoEmUso { url: url.clone() });
    // cria a pasta do arquivo, se faltar (o SQLite cria o arquivo, não a pasta)
    if let Some(caminho) = url.strip_prefix("sqlite://") {
        let arquivo = caminho.split('?').next().unwrap_or(caminho);
        if let Some(pasta) = std::path::Path::new(arquivo)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
        {
            std::fs::create_dir_all(pasta).map_err(|e| {
                Msg::PastaNaoCriada {
                    pasta: pasta.display().to_string(),
                    erro: e.to_string(),
                }
                .to_string()
            })?;
        }
    }
    KioskDbManager::new(&url).await.map_err(|e| {
        Msg::BancoNaoAbriu {
            url: url.clone(),
            erro: e.to_string(),
        }
        .to_string()
    })
}

fn enderecos() -> Result<Vec<SocketAddr>, String> {
    let bruto = std::env::var("KIOSK_LISTEN").map_err(|_| Msg::ListenNaoDefinida.to_string())?;
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
                    Msg::FaltaPorta {
                        ender: s.to_owned(),
                    }
                    .to_string()
                } else {
                    Msg::EnderecoInvalido {
                        ender: s.to_owned(),
                    }
                    .to_string()
                }
            })
        })
        .collect::<Result<Vec<SocketAddr>, String>>()?;
    if lista.is_empty() {
        return Err(Msg::NenhumEndereco.to_string());
    }
    api::validar_enderecos(&lista).map_err(|e| e.to_string())?;
    Ok(lista)
}

/// API, limpeza do log e (no Windows) o agente, até a API parar ou falhar, ou até `parar` (o
/// pedido de parada do Windows). Numa parada de propósito, o agente é encerrado e o banco guarda
/// que foi normal; o próximo `kioskd` a subir registra o intervalo sem enforcement (E-004 F4).
async fn servir(
    com_agente: bool,
    parar: impl std::future::Future<Output = ()>,
) -> Result<(), String> {
    let enderecos = enderecos()?;
    let db = Arc::new(abrir_banco().await?);
    iniciar_manutencao(db.clone());
    iniciar_auditoria(db.clone());
    let bandeja = Bandeja::default();
    iniciar_pipe_da_bandeja(db.clone(), bandeja.clone());
    let agente = if com_agente {
        iniciar_agente(db.clone(), bandeja)
    } else {
        None
    };
    let lista: Vec<String> = enderecos.iter().map(ToString::to_string).collect();
    eprintln!(
        "kioskd: {}",
        Msg::Escutando {
            lista: lista.join(", ")
        }
    );
    let resultado = tokio::select! {
        r = api::servir_em(db.clone(), &enderecos) => r.map_err(|e| e.to_string()),
        () = parar => Ok(()),
    };
    if let Some(agente) = agente {
        // sem o agente, ninguém grava um batimento depois da marca de parada normal
        agente.abort();
        let _ = agente.await;
        if let Err(e) = db.gravar_batimento(Utc::now(), true).await {
            eprintln!("kioskd: {e}");
        }
    }
    resultado
}

/// `kioskd service`: o processo que o SCM inicia (ADR-008).
#[cfg(windows)]
fn servico() -> Result<(), String> {
    kiosk_engine::servico_windows::despachar(|parar| {
        let rt = tokio::runtime::Runtime::new().map_err(|e| format!("runtime: {e}"))?;
        let resultado = rt.block_on(async { servir(true, parar.notified()).await });
        rt.shutdown_timeout(std::time::Duration::from_secs(5));
        resultado
    })
}

#[cfg(not(windows))]
fn servico() -> Result<(), String> {
    Err(Msg::SoNoWindows.to_string())
}

/// Todo comando que mexe nos dados exige administrador elevado no PC do serviço (regra do dono,
/// E-008). Sem elevação, recusa **antes de tocar em qualquer coisa** e avisa o serviço, que grava
/// a tentativa no log forense (ADR-011): quem recusa não abre o banco, porque a conta sem
/// privilégio não consegue. Sem o serviço, diz que a tentativa não foi registrada.
#[cfg(windows)]
fn exigir_administrador(comando: &str) -> Result<(), String> {
    if kiosk_engine::sistema_windows::administrador_elevado()? {
        return Ok(());
    }
    let base = Msg::SoAdministrador {
        comando: comando.to_owned(),
    }
    .to_string();
    Err(
        match kiosk_engine::auditoria_windows::avisar_tentativa(comando) {
            Ok(()) => Msg::RecusaRegistrada { base }.to_string(),
            Err(erro) => Msg::RecusaSemRegistro { base, erro }.to_string(),
        },
    )
}

/// Fora do Windows não há serviço (ADR-002); a exigência de `sudo` vem com o E-009 e o E-010.
#[cfg(not(windows))]
fn exigir_administrador(_comando: &str) -> Result<(), String> {
    Ok(())
}

/// Escuta o pipe de auditoria (ADR-011) enquanto o serviço roda. Falha alta: se não der para
/// escutar, o serviço segue e o diz, e os comandos recusados dizem que não registraram.
#[cfg(windows)]
fn iniciar_auditoria(db: Arc<KioskDbManager>) {
    tokio::spawn(async move {
        if let Err(erro) = kiosk_engine::auditoria_windows::servir(db).await {
            eprintln!("kioskd: {}", Msg::AuditoriaIndisponivel { erro });
        }
    });
}

#[cfg(not(windows))]
fn iniciar_auditoria(_db: Arc<KioskDbManager>) {}

/// Pede que digitem a palavra de confirmação (do idioma) antes de apagar os dados (ADR-008).
fn confirmar_apagar_dados() -> Result<(), String> {
    let palavra = kiosk_engine::i18n::idioma().palavra_de_confirmacao();
    eprint!(
        "kioskd: {}",
        Msg::ConfirmarApagar {
            palavra: palavra.to_owned()
        }
    );
    let mut linha = String::new();
    std::io::stdin().read_line(&mut linha).map_err(|e| {
        Msg::ErroLeitura {
            erro: e.to_string(),
        }
        .to_string()
    })?;
    if linha.trim() != palavra {
        return Err(Msg::NaoConfirmado.to_string());
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
                Ok(n) => eprintln!("kioskd: {}", Msg::Retencao { n }),
                Err(e) => eprintln!(
                    "kioskd: {}",
                    Msg::RetencaoFalhou {
                        erro: e.to_string()
                    }
                ),
            }
        }
    });
}

/// Intervalo do agente (ADR-003: polling de 2 s).
#[cfg(windows)]
const TICK_AGENTE: std::time::Duration = std::time::Duration::from_secs(2);

/// As UIs (bandejas) conectadas: só no Windows há bandeja.
#[cfg(windows)]
type Bandeja = kiosk_engine::ui_windows::GerenteUi;
#[cfg(not(windows))]
#[derive(Default, Clone)]
#[allow(dead_code)] // fora do Windows não há bandeja: o tipo só mantém a mesma forma de chamada
struct Bandeja(());

/// Escuta o pipe das bandejas (ADR-004). Se não der (por exemplo, outro `kioskd serve` já o
/// escuta), o serviço segue sem bandeja e o diz; o agente a relança e registra a ausência.
#[cfg(windows)]
fn iniciar_pipe_da_bandeja(db: Arc<KioskDbManager>, bandeja: Bandeja) {
    use kiosk_engine::servico_windows::pasta_programa;
    use kiosk_engine::ui_windows::{Configuracao, servir};
    tokio::spawn(async move {
        let config = Configuracao::instalada(&pasta_programa());
        if let Err(erro) = servir(config, bandeja, db).await {
            eprintln!("kioskd: {}", Msg::BandejaIndisponivel { erro });
        }
    });
}

#[cfg(not(windows))]
fn iniciar_pipe_da_bandeja(_db: Arc<KioskDbManager>, _bandeja: Bandeja) {}

#[cfg(windows)]
fn iniciar_agente(
    db: Arc<KioskDbManager>,
    bandeja: Bandeja,
) -> Option<tokio::task::JoinHandle<()>> {
    eprintln!(
        "kioskd: {}",
        Msg::AgenteAtivo {
            segundos: TICK_AGENTE.as_secs()
        }
    );
    Some(tokio::spawn(async move {
        kiosk_engine::agente::rodar_em_laco(
            &db,
            kiosk_engine::sistema_windows::SistemaWindows::com_ui(
                bandeja,
                kiosk_engine::servico_windows::pasta_programa().join("kiosk-ui.exe"),
            ),
            chrono::Local,
            TICK_AGENTE,
            |rel| {
                for e in &rel.erros {
                    eprintln!("kioskd: {}", Msg::AgenteErro { erro: e.clone() });
                }
                true
            },
        )
        .await;
    }))
}

#[cfg(not(windows))]
fn iniciar_agente(
    _db: Arc<KioskDbManager>,
    _bandeja: Bandeja,
) -> Option<tokio::task::JoinHandle<()>> {
    eprintln!("kioskd: {}", Msg::AgenteSoWindows);
    None
}

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

fn id(texto: &str) -> Result<Uuid, String> {
    Uuid::parse_str(texto).map_err(|_| {
        Msg::IdInvalido {
            valor: texto.to_owned(),
        }
        .to_string()
    })
}

fn segundos(texto: &str, nome: &str) -> Result<u64, String> {
    texto.parse().map_err(|_| {
        Msg::OpcaoInvalida {
            nome: nome.to_owned(),
            valor: texto.to_owned(),
        }
        .to_string()
    })
}

/// `sun,mon,...`, `dom,seg,...` ou `0,1,...` -> dias da semana (0 = domingo).
fn dias(texto: &str) -> Result<Vec<u8>, String> {
    use kiosk_engine::i18n::Idioma;
    texto
        .split(',')
        .map(|d| {
            let d: String = d
                .trim()
                .to_lowercase()
                .chars()
                .map(|c| match c {
                    'á' => 'a',
                    'é' => 'e',
                    'í' => 'i',
                    _ => c,
                })
                .collect();
            // os nomes de qualquer idioma valem, além dos números de 0 (domingo) a 6
            (0..7u8)
                .find(|i| Idioma::TODOS.iter().any(|l| d.starts_with(l.dia_curto(*i))))
                .or_else(|| d.parse::<u8>().ok().filter(|n| *n <= 6))
                .ok_or_else(|| Msg::DiaInvalido { valor: d.clone() }.to_string())
        })
        .collect()
}

fn hora(texto: &str) -> Result<chrono::NaiveTime, String> {
    chrono::NaiveTime::parse_from_str(texto, "%H:%M").map_err(|_| {
        Msg::HoraInvalida {
            valor: texto.to_owned(),
        }
        .to_string()
    })
}

/// SID da conta: `--sid` explícito, ou `--account` resolvida pelo Windows.
fn sid_do_argumento(args: &[String]) -> Result<String, String> {
    if let Some(sid) = opcao(args, "--sid") {
        if !sid.starts_with("S-1-") {
            return Err(Msg::SidInvalido { valor: sid }.to_string());
        }
        return Ok(sid);
    }
    let conta = exigir(args, "--account")?;
    #[cfg(windows)]
    {
        kiosk_engine::sistema_windows::sid_de_usuario(&conta)
    }
    #[cfg(not(windows))]
    {
        Err(Msg::ContaSoNoWindows { conta }.to_string())
    }
}

#[cfg(windows)]
fn instalar(escutar: &str) -> Result<(), String> {
    use kiosk_engine::servico_windows as sv;
    let pid = sv::instalar(escutar)?;
    eprintln!(
        "kioskd: {}",
        Msg::InstaladoEmExecucao {
            pid,
            escutar: escutar.to_owned()
        }
    );
    eprintln!(
        "kioskd: {}",
        Msg::VersaoInstalada {
            v: kiosk_engine::versao::atual().curta()
        }
    );
    eprintln!(
        "kioskd: {}",
        Msg::LogDoServicoInstalado {
            caminho: sv::arquivo_log().display().to_string()
        }
    );
    Ok(())
}

#[cfg(windows)]
fn desinstalar(apagar_dados: bool) -> Result<(), String> {
    kiosk_engine::servico_windows::desinstalar(apagar_dados)
}

/// Versão do executável que o serviço roda, lida de `<executável> version`: é a que importa
/// quando o `status` é rodado de uma pasta com outra versão. `executavel` é a linha de comando do
/// serviço (`"C:\\...\\kioskd.exe" service`). Prazo de 5 s; `None` se não deu para ler.
#[cfg(windows)]
fn versao_instalada(executavel: &str) -> Option<String> {
    let caminho = executavel.strip_prefix('"')?.split('"').next()?.to_owned();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let saida = std::process::Command::new(caminho)
            .arg("version")
            .stdin(std::process::Stdio::null())
            .output();
        let _ = tx.send(saida);
    });
    let saida = rx
        .recv_timeout(std::time::Duration::from_secs(5))
        .ok()?
        .ok()?;
    kiosk_engine::versao::da_primeira_linha(&String::from_utf8_lossy(&saida.stdout), "kioskd")
}

#[cfg(windows)]
fn estado() -> Result<(), String> {
    use kiosk_engine::servico_windows as sv;
    let Some(e) = sv::estado()? else {
        println!(
            "{}",
            Msg::LblServico {
                v: format!("{} {}", sv::NOME_SERVICO, Msg::NaoInstalado)
            }
        );
        println!(
            "{}",
            Msg::LblVersao {
                v: kiosk_engine::versao::atual().curta()
            }
        );
        return Ok(());
    };
    println!(
        "{}",
        Msg::LblServico {
            v: sv::NOME_SERVICO.to_owned()
        }
    );
    let estado = sv::nome_estado(e.estado);
    let estado = if e.pid != 0 {
        format!("{estado} (pid {})", e.pid)
    } else {
        estado
    };
    println!("{}", Msg::LblEstado { v: estado });
    let inicio = if e.inicio_automatico {
        Msg::InicioAutomatico
    } else {
        Msg::InicioManual
    };
    println!(
        "{}",
        Msg::LblInicio {
            v: inicio.to_string()
        }
    );
    println!("{}", Msg::LblConta { v: e.conta });
    println!(
        "{}",
        Msg::LblExecutavel {
            v: e.executavel.clone()
        }
    );
    println!(
        "{}",
        Msg::LblVersao {
            v: kiosk_engine::versao::atual().curta()
        }
    );
    println!(
        "{}",
        Msg::LblVersaoInstalada {
            v: versao_instalada(&e.executavel)
                .unwrap_or_else(|| Msg::VersaoIndisponivel.to_string())
        }
    );
    for v in e.ambiente {
        println!("{}", Msg::LblAmbiente { v });
    }
    let no_path = if e.no_path {
        Msg::PathSim
    } else {
        Msg::PathNao
    };
    println!(
        "{}",
        Msg::LblPath {
            v: no_path.to_string()
        }
    );
    println!(
        "{}",
        Msg::LblDados {
            v: sv::pasta_dados().display().to_string()
        }
    );
    println!(
        "{}",
        Msg::LblLog {
            v: sv::arquivo_log().display().to_string()
        }
    );
    Ok(())
}

#[cfg(not(windows))]
fn instalar(_escutar: &str) -> Result<(), String> {
    Err(Msg::SoNoWindows.to_string())
}

#[cfg(not(windows))]
fn desinstalar(_apagar_dados: bool) -> Result<(), String> {
    Err(Msg::SoNoWindows.to_string())
}

#[cfg(not(windows))]
fn estado() -> Result<(), String> {
    Err(Msg::SoNoWindows.to_string())
}

/// Erro de validação do comando, traduzido quando o `kioskd` o mostra a um adulto.
fn erro_de_comando(e: kiosk_engine::comandos::ErroComando) -> String {
    use kiosk_engine::comandos::ErroComando as E;
    match e {
        E::LimitAbove(max) => Msg::LimiteAcimaDe { max }.to_string(),
        E::WindowOrder => Msg::InicioAntesDoFim.to_string(),
        E::UntilNotRfc => Msg::UntilNaoRfc.to_string(),
        E::UntilPast => Msg::UntilNoPassado.to_string(),
        outro => outro.to_string(),
    }
}

use kiosk_engine::textos::duracao;

/// Avisa quando uma regra ativa já nasce com o limite menor ou igual ao uso de hoje: o app seria
/// fechado assim que abrisse, e parece que a regra "não pode ser revogada". É só um aviso, depois
/// de a regra já ter sido gravada; um erro ao calcular não desfaz nada.
async fn avisar_limite_ja_ultrapassado(db: &KioskDbManager, regra: Uuid) {
    use kiosk_engine::database_schema::RuleState;
    let Ok(Some(r)) = db.get_active_rule(regra).await else {
        return;
    };
    let Some(limite) = r.daily_limit_seconds.filter(|l| *l > 0) else {
        return; // sem limite, ou limite 0 (bloqueio de propósito)
    };
    if r.state != RuleState::Active {
        return;
    }
    let hoje = chrono::Local::now().date_naive();
    let Ok(c) = db.consumo_do_dia(r.profile_id, r.app_id, hoje).await else {
        return;
    };
    if c.app_segundos < limite {
        return;
    }
    let app = db
        .listar_apps()
        .await
        .ok()
        .and_then(|a| a.into_iter().find(|a| a.app_id == r.app_id))
        .map_or_else(|| "?".to_owned(), |a| a.process_name);
    eprintln!(
        "kioskd: {}",
        Msg::LimiteJaUltrapassado {
            app,
            usado: duracao(c.app_segundos),
            limite: duracao(limite),
        }
    );
}

/// O mesmo para o limite total do perfil (a sessão seria bloqueada em seguida).
async fn avisar_total_ja_ultrapassado(db: &KioskDbManager, perfil: Uuid, limite: Option<u64>) {
    let Some(limite) = limite.filter(|l| *l > 0) else {
        return;
    };
    let hoje = chrono::Local::now().date_naive();
    let Ok((sessao, _)) = db.uso_do_dia(perfil, hoje).await else {
        return;
    };
    if sessao >= limite {
        eprintln!(
            "kioskd: {}",
            Msg::LimiteTotalJaUltrapassado {
                usado: duracao(sessao),
                limite: duracao(limite),
            }
        );
    }
}

async fn executar(args: &[String]) -> Result<(), String> {
    // um único ponto confere a elevação de todo comando administrativo (lista fechada, ADR-011)
    if let Some(nome) = kiosk_engine::cli::nome_do_comando(args, kiosk_engine::cli::FORMAS_KIOSKD)
        && kiosk_engine::auditoria::COMANDOS.contains(&nome.as_str())
    {
        exigir_administrador(&nome)?;
    }
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["serve", resto @ ..] => {
            let com_agente = match resto {
                [] => true,
                ["--no-agent"] => false,
                _ => return Err(uso()),
            };
            servir(com_agente, std::future::pending()).await
        }
        ["install", "--listen", escutar] => {
            let lista = lista_de_enderecos(escutar)?;
            let normalizado: Vec<String> = lista.iter().map(ToString::to_string).collect();
            instalar(&normalizado.join(","))
        }
        ["uninstall", resto @ ..] => {
            let apagar = match resto {
                [] => false,
                ["--purge-data"] => true,
                _ => return Err(uso()),
            };
            if apagar {
                confirmar_apagar_dados()?;
            }
            desinstalar(apagar)
        }
        ["status"] => estado(),
        ["version"] => {
            println!("{}", kiosk_engine::versao::atual().completa("kioskd"));
            Ok(())
        }
        ["pairing-code"] => {
            let db = abrir_banco().await?;
            let codigo = db
                .criar_codigo_pareamento(&EventContext::local(), Utc::now())
                .await
                .map_err(|e| e.to_string())?;
            eprintln!("kioskd: {}", Msg::CodigoDePareamento);
            println!("{codigo}");
            Ok(())
        }
        ["devices"] => {
            let db = abrir_banco().await?;
            let lista = db.listar_dispositivos().await.map_err(|e| e.to_string())?;
            if lista.is_empty() {
                eprintln!("kioskd: {}", Msg::NenhumDispositivo);
            }
            for (id, nome, criado, revogado) in lista {
                let estado = revogado.map_or(Msg::DispositivoAtivo.to_string(), |r| {
                    Msg::DispositivoRevogadoEm { quando: r }.to_string()
                });
                let criado = Msg::CriadoEm { quando: criado };
                println!("{id}\t{nome}\t{criado}\t{estado}");
            }
            Ok(())
        }
        ["revoke", id] => {
            let id = self::id(id)?;
            let db = abrir_banco().await?;
            db.revogar_dispositivo(&EventContext::local(), id, Utc::now())
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => Msg::DispositivoInexistente.to_string(),
                    outro => outro.to_string(),
                })?;
            eprintln!(
                "kioskd: {}",
                Msg::DispositivoRevogado { id: id.to_string() }
            );
            Ok(())
        }
        ["profile", "create", ..] => {
            let sid = sid_do_argumento(args)?;
            let nome = exigir(args, "--name")?;
            let limite = opcao(args, "--total-limit")
                .map(|l| segundos(&l, "--total-limit"))
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
                        Msg::PerfilJaExiste { sid: sid.clone() }.to_string()
                    }
                    outro => outro.to_string(),
                })?;
            println!("{}", perfil.profile_id);
            eprintln!("kioskd: {}", Msg::PerfilCriado { nome, sid });
            Ok(())
        }
        ["profile", "list"] => {
            let db = abrir_banco().await?;
            let perfis = db.listar_perfis().await.map_err(|e| e.to_string())?;
            if perfis.is_empty() {
                eprintln!("kioskd: {}", Msg::NenhumPerfil);
            }
            for p in perfis {
                let limite = p
                    .daily_total_limit_seconds
                    .map_or(Msg::SemLimite.to_string(), duracao);
                let janelas = db
                    .janelas_do_perfil(p.profile_id)
                    .await
                    .map_err(|e| e.to_string())?;
                let idioma = kiosk_engine::i18n::idioma();
                let janelas: Vec<String> = janelas
                    .iter()
                    .map(|(d, j)| {
                        format!(
                            "{} {}-{}",
                            idioma.dia_curto(*d),
                            j.start.format("%H:%M"),
                            j.end.format("%H:%M")
                        )
                    })
                    .collect();
                let janelas = if janelas.is_empty() {
                    Msg::SemJanela.to_string()
                } else {
                    janelas.join(", ")
                };
                let rotulo = Msg::RotuloLimiteDiario { limite };
                println!(
                    "{}\t{}\t{}\t{rotulo}\t{janelas}",
                    p.profile_id, p.display_name, p.windows_sid
                );
            }
            Ok(())
        }
        ["profile", "limit", perfil, ..] => {
            let perfil = id(perfil)?;
            let limite = if args.iter().any(|a| a == "--no-limit") {
                None
            } else {
                Some(segundos(&exigir(args, "--seconds")?, "--seconds")?)
            };
            let db = abrir_banco().await?;
            db.set_profile_limit(&EventContext::local(), perfil, limite)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => Msg::PerfilInexistente.to_string(),
                    outro => outro.to_string(),
                })?;
            eprintln!(
                "kioskd: {}",
                Msg::PerfilLimiteAgora {
                    limite: limite.map_or(Msg::SemLimite.to_string(), duracao)
                }
            );
            avisar_total_ja_ultrapassado(&db, perfil, limite).await;
            Ok(())
        }
        ["profile", "window", perfil, ..] => {
            let perfil = id(perfil)?;
            let dias = dias(&exigir(args, "--days")?)?;
            let janela = exigir(args, "--window")?;
            let (a, b) = janela.split_once('-').ok_or_else(|| {
                Msg::JanelaFormato {
                    recebido: janela.clone(),
                }
                .to_string()
            })?;
            let janela = kiosk_engine::database_schema::TimeWindow {
                start: hora(a)?,
                end: hora(b)?,
            };
            if janela.start >= janela.end {
                return Err(Msg::InicioAntesDoFim.to_string());
            }
            let db = abrir_banco().await?;
            for d in &dias {
                db.set_profile_window(&EventContext::local(), perfil, *d, &janela)
                    .await
                    .map_err(|e| {
                        Msg::JanelaNaoGravada {
                            erro: e.to_string(),
                        }
                        .to_string()
                    })?;
            }
            eprintln!("kioskd: {}", Msg::JanelaGravada { dias: dias.len() });
            Ok(())
        }
        ["app", "create", ..] => {
            use kiosk_engine::database_schema::ProcessStatus;
            let processo = exigir(args, "--process")?;
            let status = match (
                args.iter().any(|a| a == "--exempt"),
                args.iter().any(|a| a == "--ignore"),
            ) {
                (true, true) => return Err(Msg::ExemptOuIgnore.to_string()),
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
                    sqlx::Error::Database(d) if d.is_unique_violation() => Msg::AppJaCadastrado {
                        processo: processo.clone(),
                    }
                    .to_string(),
                    outro => outro.to_string(),
                })?;
            println!("{}", app.app_id);
            eprintln!(
                "kioskd: {}",
                Msg::AppCadastrado {
                    processo,
                    status: status.as_str().to_owned()
                }
            );
            Ok(())
        }
        ["app", "list"] => {
            let db = abrir_banco().await?;
            let apps = db.listar_apps().await.map_err(|e| e.to_string())?;
            if apps.is_empty() {
                eprintln!("kioskd: {}", Msg::NenhumApp);
            }
            for a in apps {
                println!("{}\t{}\t{}", a.app_id, a.process_name, a.status.as_str());
            }
            Ok(())
        }
        ["rule", "create", ..] => {
            use kiosk_engine::comandos::{Comando, interpretar};
            use kiosk_engine::database_schema::{ActiveRule, RuleState};
            // mesmo corpo e mesma validação da API (Camada 3)
            let mut corpo = serde_json::json!({
                "profile_id": exigir(args, "--profile")?,
                "app_id": exigir(args, "--app")?,
            });
            if let Some(l) = opcao(args, "--limit") {
                corpo["daily_limit_seconds"] = segundos(&l, "--limit")?.into();
            }
            if let Some(j) = opcao(args, "--window") {
                let (a, b) = j.split_once('-').ok_or_else(|| {
                    Msg::JanelaFormato {
                        recebido: j.clone(),
                    }
                    .to_string()
                })?;
                corpo["window_start"] = a.into();
                corpo["window_end"] = b.into();
            }
            if let Some(t) = opcao(args, "--until") {
                corpo["effective_until"] = t.into();
            }
            let agora = Utc::now();
            let Comando::AddRule(nova) =
                interpretar("add_rule", corpo.to_string().as_bytes(), agora)
                    .map_err(erro_de_comando)?
            else {
                unreachable!("add_rule sempre produz AddRule")
            };
            let db = abrir_banco().await?;
            if !db
                .existe_perfil(nova.profile_id)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err(Msg::PerfilInexistenteVeja.to_string());
            }
            if !db
                .existe_app(nova.app_id)
                .await
                .map_err(|e| e.to_string())?
            {
                return Err(Msg::AppInexistenteVeja.to_string());
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
            let ativar = args.iter().any(|a| a == "--activate");
            if ativar {
                db.set_rule_state(&ctx, regra.rule_id, RuleState::Active)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            println!("{}", regra.rule_id);
            eprintln!(
                "kioskd: {}",
                if ativar {
                    Msg::RegraCriadaAtiva
                } else {
                    Msg::RegraCriadaPendente
                }
            );
            avisar_limite_ja_ultrapassado(&db, regra.rule_id).await;
            Ok(())
        }
        ["rule", acao @ ("activate" | "revoke"), regra] => {
            use kiosk_engine::database_schema::RuleState;
            let regra = id(regra)?;
            let estado = if *acao == "activate" {
                RuleState::Active
            } else {
                RuleState::Revoked
            };
            let db = abrir_banco().await?;
            db.set_rule_state(&EventContext::local(), regra, estado)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => Msg::RegraInexistente.to_string(),
                    outro => outro.to_string(),
                })?;
            eprintln!(
                "kioskd: {}",
                if *acao == "activate" {
                    Msg::RegraAtivadaLocal
                } else {
                    Msg::RegraRevogadaLocal
                }
            );
            if *acao == "activate" {
                avisar_limite_ja_ultrapassado(&db, regra).await;
            }
            Ok(())
        }
        ["rule", "limit", regra, ..] => {
            let regra = id(regra)?;
            let s = segundos(&exigir(args, "--seconds")?, "--seconds")?;
            if s > kiosk_engine::comandos::MAX_LIMITE_SEGUNDOS {
                return Err(Msg::SegundosAcima {
                    max: kiosk_engine::comandos::MAX_LIMITE_SEGUNDOS,
                }
                .to_string());
            }
            let db = abrir_banco().await?;
            db.set_rule_limit(&EventContext::local(), regra, s)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => Msg::RegraInexistenteOuRevogada.to_string(),
                    outro => outro.to_string(),
                })?;
            eprintln!("kioskd: {}", Msg::LimiteDaRegraAgora { limite: duracao(s) });
            avisar_limite_ja_ultrapassado(&db, regra).await;
            Ok(())
        }
        ["rule", "list", ..] => {
            let perfil = opcao(args, "--profile").map(|p| id(&p)).transpose()?;
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
                eprintln!("kioskd: {}", Msg::NenhumaRegra);
            }
            for r in regras {
                let app = apps.get(&r.app_id).map_or("?", String::as_str);
                let limite = r
                    .daily_limit_seconds
                    .map_or(Msg::SemLimite.to_string(), duracao);
                let rotulo = Msg::RotuloLimite { limite };
                let janela = r.time_window.map_or(Msg::SemJanela.to_string(), |j| {
                    format!("{}-{}", j.start.format("%H:%M"), j.end.format("%H:%M"))
                });
                println!(
                    "{}\t{}\t{app}\t{}\t{rotulo}\t{janela}",
                    r.rule_id,
                    r.profile_id,
                    r.state.as_str()
                );
            }
            Ok(())
        }
        _ => Err(uso()),
    }
}
