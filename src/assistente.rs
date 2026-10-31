use crate::agente::{Aviso, ContaLocal, ProcessoVisto, ProgramaSugerido, Sistema};
use crate::database_schema::{ActiveRule, AppMetadata, ChildProfile, ProcessStatus, RuleState};
use crate::db_manager::{EventContext, KioskDbManager};
use std::io::{self, BufRead, Write};
/// Pergunta um número inteiro com validação de faixa.
fn ask_u64(prompt: &str, min: u64, max: u64) -> Result<u64, String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    loop {
        println!("\n{}", prompt);
        print!("Valor ({}-{}): ", min, max);
        io::stdout().flush().map_err(|e| e.to_string())?;

        let mut input = String::new();
        reader.read_line(&mut input).map_err(|e| e.to_string())?;
        let input = input.trim();

        if let Ok(num) = input.parse::<u64>()
            && num >= min
            && num <= max
        {
            return Ok(num);
        }

        println!("Valor inválido. Digite um número entre {} e {}.", min, max);
    }
}
use std::sync::Arc;
use uuid::Uuid;

/// Sistema de simulação para testes e desenvolvimento em plataformas não-Windows.
pub struct MockSistema {
    pub db: Arc<KioskDbManager>,
    pub contas: Vec<ContaLocal>,
    pub programas: Vec<ProgramaSugerido>,
}

impl MockSistema {
    pub fn new(db: Arc<KioskDbManager>) -> Self {
        Self {
            db,
            contas: vec![
                ContaLocal {
                    sid: "S-1-5-21-111".to_string(),
                    nome: "Criança 1".to_string(),
                },
                ContaLocal {
                    sid: "S-1-5-21-222".to_string(),
                    nome: "Criança 2".to_string(),
                },
            ],
            programas: vec![
                ProgramaSugerido {
                    nome_amigavel: "Roblox".to_string(),
                    executavel: "RobloxPlayerBeta.exe".to_string(),
                },
                ProgramaSugerido {
                    nome_amigavel: "Minecraft".to_string(),
                    executavel: "Minecraft.exe".to_string(),
                },
                ProgramaSugerido {
                    nome_amigavel: "Chrome".to_string(),
                    executavel: "chrome.exe".to_string(),
                },
            ],
        }
    }
}

impl Sistema for MockSistema {
    fn processos(&mut self) -> Result<Vec<ProcessoVisto>, String> {
        Ok(vec![])
    }
    fn encerrar(&mut self, _pid: u32) -> Result<(), String> {
        Ok(())
    }
    fn bloquear_sessao(&mut self, _sid: &str) -> Result<(), String> {
        Ok(())
    }
    fn avisar(&mut self, _aviso: &Aviso) -> Result<(), String> {
        Ok(())
    }
    fn sessoes_ativas(&mut self) -> Result<Vec<String>, String> {
        Ok(vec!["S-1-5-21-111".to_string()])
    }
    fn encerrar_sessao(&mut self, _sid: &str) -> Result<(), String> {
        Ok(())
    }
    fn contas_locais(&mut self) -> Result<Vec<ContaLocal>, String> {
        Ok(self.contas.clone())
    }
    fn atalhos_do_menu(&mut self) -> Result<Vec<ProgramaSugerido>, String> {
        Ok(self.programas.clone())
    }
    fn tem_ui(&self) -> bool {
        false
    }
}

/// Executa o assistente de configuração.
///
/// # Arguments
/// * `args` - Os argumentos passados para o comando `configure`.
///   Pode incluir `--respostas <arquivo.toml>` para modo não interativo.
pub async fn configurar(
    args: &[&str],
    db: Arc<KioskDbManager>,
    sistema: &mut dyn Sistema,
) -> Result<(), String> {
    if args.contains(&"--respostas") {
        println!("kioskd: Modo não interativo detectado (via --respostas).");
        // Em breve: implementar leitura de TOML
        return Ok(());
    }

    println!("kioskd: Iniciando assistente de configuração interativo...");

    // 1. Seleção de Contas
    let contas = sistema.contas_locais()?;
    if contas.is_empty() {
        return Err("Nenhuma conta local encontrada para configurar.".to_string());
    }

    let idx_conta = ask_one_choice("Selecione a conta da criança:", &contas, |c| {
        format!("{} ({})", c.nome, c.sid)
    })?;
    let conta_selecionada = &contas[idx_conta];
    println!("kioskd: Conta selecionada: {}", conta_selecionada.nome);

    // 2. Seleção de Programas
    let programas = sistema.atalhos_do_menu()?;
    if programas.is_empty() {
        return Err("Nenhum programa encontrado para configurar.".to_string());
    }

    let idx_prog = ask_one_choice("Selecione um programa para controlar:", &programas, |p| {
        format!("{} ({})", p.nome_amigavel, p.executavel)
    })?;
    let prog = &programas[idx_prog];
    println!("kioskd: Programa selecionado: {}", prog.nome_amigavel);

    // 3. Limite do app
    let limite_app_min = ask_u64(
        "Digite o limite diario para este app em minutos (0 = sem limite):",
        0,
        24 * 60,
    )?;
    let limite_app_seg = if limite_app_min > 0 {
        Some(limite_app_min * 60)
    } else {
        None
    };

    // 4. Limite total do perfil
    let limite_total_min = ask_u64(
        "Digite o limite total diario para esta crianca em minutos (0 = sem limite):",
        0,
        24 * 60,
    )?;
    let limite_total_seg = if limite_total_min > 0 {
        Some(limite_total_min * 60)
    } else {
        None
    };

    // 5. Resumo
    println!("\n--- Resumo da Configuracao ---");
    println!(
        "Perfil: {} ({})",
        conta_selecionada.nome, conta_selecionada.sid
    );
    println!("Programa: {}", prog.nome_amigavel);
    let f = |s: Option<u64>| {
        if let Some(v) = s {
            format!("{} min", v / 60)
        } else {
            "sem limite".to_string()
        }
    };
    println!("Limite do app: {}", f(limite_app_seg));
    println!("Limite total:  {}", f(limite_total_seg));
    println!("------------------------------");

    // 6. Confirmar
    let confirm = ask_one_choice(
        "Confirmar e gravar?",
        &["Sim".to_string(), "Cancelar".to_string()],
        |s| s.clone(),
    )?;
    if confirm == 1 {
        return Err("Operacao cancelada pelo usuario.".to_string());
    }

    // 7. Gravar no banco
    let ctx = EventContext::local();
    let perfil_id = Uuid::new_v4();
    let perfil = ChildProfile {
        profile_id: perfil_id,
        windows_sid: conta_selecionada.sid.clone(),
        display_name: conta_selecionada.nome.clone(),
        daily_total_limit_seconds: limite_total_seg,
    };
    db.create_profile(&ctx, &perfil)
        .await
        .map_err(|e| format!("erro ao criar perfil: {e}"))?;
    println!("kioskd: Perfil criado: {}", perfil_id);

    let app_id = Uuid::new_v4();
    let app = AppMetadata {
        app_id,
        process_name: prog.executavel.clone(),
        status: ProcessStatus::Monitored,
    };
    db.create_app(&ctx, &app)
        .await
        .map_err(|e| format!("erro ao criar app: {e}"))?;
    println!("kioskd: App criado: {}", app_id);

    let rule_id = Uuid::new_v4();
    let regra = ActiveRule {
        rule_id,
        profile_id: perfil_id,
        app_id,
        creation_timestamp: chrono::Utc::now(),
        state: RuleState::Active,
        daily_limit_seconds: limite_app_seg,
        time_window: None,
        effective_until_timestamp: None,
        warning_seconds: None,
    };
    db.create_active_rule(&ctx, &regra)
        .await
        .map_err(|e| format!("erro ao criar regra: {e}"))?;
    println!("kioskd: Regra ativa criada: {}", rule_id);

    // 8. Gerar codigo de pareamento
    let codigo = db
        .criar_codigo_pareamento(&ctx, chrono::Utc::now())
        .await
        .map_err(|e| format!("erro ao gerar codigo de pareamento: {e}"))?;
    println!("\nkioskd: Codigo de pareamento: {codigo}");
    println!("kioskd: Use este codigo no dispositivo dos pais para parear.");

    Ok(())
}

/// Função auxiliar para perguntas de múltipla escolha no terminal.
fn ask_one_choice<T, F>(prompt: &str, options: &[T], formatter: F) -> Result<usize, String>
where
    F: Fn(&T) -> String,
{
    let stdin = io::stdin();
    let mut reader = stdin.lock();

    loop {
        println!("\n{}", prompt);
        for (i, opt) in options.iter().enumerate() {
            println!("  {}. {}", i + 1, formatter(opt));
        }
        println!("  0. Cancelar");
        print!("\nEscolha uma opção: ");
        io::stdout().flush().map_err(|e| e.to_string())?;

        let mut input = String::new();
        reader.read_line(&mut input).map_err(|e| e.to_string())?;
        let input = input.trim();

        if input == "0" {
            return Err("Operação cancelada pelo usuário.".to_string());
        }

        if let Ok(num) = input.parse::<usize>()
            && num > 0
            && num <= options.len()
        {
            return Ok(num - 1);
        }

        println!("Opção inválida. Tente novamente.");
    }
}
