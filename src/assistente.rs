use std::io::{self, BufRead, Write};
use std::sync::Arc;
use crate::agente::{Sistema, ProcessoVisto, Aviso, ContaLocal, ProgramaSugerido};
use crate::db_manager::KioskDbManager;

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
                ContaLocal { sid: "S-1-5-21-111".to_string(), nome: "Criança 1".to_string() },
                ContaLocal { sid: "S-1-5-21-222".to_string(), nome: "Criança 2".to_string() },
            ],
            programas: vec![
                ProgramaSugerido { nome_amigavel: "Roblox".to_string(), executavel: "RobloxPlayerBeta.exe".to_string() },
                ProgramaSugerido { nome_amigavel: "Minecraft".to_string(), executavel: "Minecraft.exe".to_string() },
                ProgramaSugerido { nome_amigavel: "Chrome".to_string(), executavel: "chrome.exe".to_string() },
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
///            Pode incluir `--respostas <arquivo.toml>` para modo não interativo.
pub async fn configurar(
    args: &[&str],
    _db: Arc<KioskDbManager>,
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

    let idx_conta = ask_one_choice(
        "Selecione a conta da criança:",
        &contas,
        |c| format!("{} ({})", c.nome, c.sid),
    )?;
    let conta_selecionada = &contas[idx_conta];
    println!("kioskd: Conta selecionada: {}", conta_selecionada.nome);

    // 2. Seleção de Programas
    let programas = sistema.atalhos_do_menu()?;
    if programas.is_empty() {
        return Err("Nenhum programa encontrado para configurar.".to_string());
    }

    let idx_prog = ask_one_choice(
        "Selecione um programa para controlar:",
        &programas,
        |p| format!("{} ({})", p.nome_amigavel, p.executavel),
    )?;
    let prog_selecionado = &programas[idx_prog];
    println!("kioskd: Programa selecionado: {}", prog_selecionado.nome_amigavel);

    // 3. Resumo
    println!("\n--- Resumo da Configuração ---");
    println!("Perfil: {}", conta_selecionada.nome);
    println!("Programa: {}", prog_selecionado.nome_amigavel);
    println!("------------------------------");
    println!("kioskd: (A gravação real no banco e instalação do serviço serão implementadas na próxima fase)");

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

        if let Ok(num) = input.parse::<usize>() {
            if num > 0 && num <= options.len() {
                return Ok(num - 1);
            }
        }

        println!("Opção inválida. Tente novamente.");
    }
}
