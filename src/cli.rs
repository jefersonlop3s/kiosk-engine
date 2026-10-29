//! Nomes de comandos e opções da CLI (ADR-010). O inglês é a convenção, usada no texto de uso,
//! nos fluxogramas e na documentação; os nomes em português equivalem para sempre. Os
//! executáveis traduzem os argumentos para inglês antes de interpretá-los, então há um único
//! caminho de código para as duas línguas. A página
//! `docs/runbooks/comandos-em-portugues.md` lista estas tabelas (conferida em teste).

/// Comandos e subcomandos: (português, inglês).
pub const COMANDOS: &[(&str, &str)] = &[
    ("servir", "serve"),
    ("codigo-pareamento", "pairing-code"),
    ("dispositivos", "devices"),
    ("revogar", "revoke"),
    ("perfil", "profile"),
    ("criar", "create"),
    ("listar", "list"),
    ("limite", "limit"),
    ("janela", "window"),
    ("regra", "rule"),
    ("ativar", "activate"),
    ("instalar", "install"),
    ("desinstalar", "uninstall"),
    ("estado", "status"),
    ("servico", "service"),
    ("parear", "pair"),
    ("uso", "usage"),
];

/// Opções: (português, inglês).
pub const OPCOES: &[(&str, &str)] = &[
    ("--sem-agente", "--no-agent"),
    ("--conta", "--account"),
    ("--nome", "--name"),
    ("--limite-total", "--total-limit"),
    ("--segundos", "--seconds"),
    ("--sem-limite", "--no-limit"),
    ("--dias", "--days"),
    ("--janela", "--window"),
    ("--processo", "--process"),
    ("--isento", "--exempt"),
    ("--ignorar", "--ignore"),
    ("--perfil", "--profile"),
    ("--limite", "--limit"),
    ("--ate", "--until"),
    ("--ativar", "--activate"),
    ("--escutar", "--listen"),
    ("--apagar-dados", "--purge-data"),
    ("--servidor", "--server"),
    ("--codigo", "--code"),
    ("--data", "--date"),
];

/// Comandos que têm subcomando na segunda palavra.
const GRUPOS: [&str; 3] = ["profile", "app", "rule"];

fn traduzir(tabela: &[(&str, &str)], palavra: &str) -> String {
    tabela
        .iter()
        .find(|(pt, _)| *pt == palavra)
        .map_or(palavra, |(_, en)| en)
        .to_owned()
}

/// Argumentos com comandos e opções em inglês. Só são traduzidos o comando (1ª palavra), o
/// subcomando (2ª palavra, quando a 1ª é um grupo) e os nomes de opção; valores, como nomes,
/// IDs e horários, passam intactos.
pub fn em_ingles(args: &[String]) -> Vec<String> {
    let mut saida: Vec<String> = Vec::with_capacity(args.len());
    for (i, a) in args.iter().enumerate() {
        let traduzido = if a.starts_with("--") {
            traduzir(OPCOES, a)
        } else if i == 0 || (i == 1 && GRUPOS.contains(&saida[0].as_str())) {
            traduzir(COMANDOS, a)
        } else {
            a.clone()
        };
        saida.push(traduzido);
    }
    saida
}
