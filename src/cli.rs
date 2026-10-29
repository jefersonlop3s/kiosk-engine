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

/// Forma de um comando: as palavras (já em inglês), as opções que levam valor, as que não levam
/// e quantos argumentos posicionais (IDs) aceita.
pub struct Forma {
    pub comando: &'static [&'static str],
    pub com_valor: &'static [&'static str],
    pub sem_valor: &'static [&'static str],
    pub posicionais: usize,
}

const fn forma(
    comando: &'static [&'static str],
    com_valor: &'static [&'static str],
    sem_valor: &'static [&'static str],
    posicionais: usize,
) -> Forma {
    Forma {
        comando,
        com_valor,
        sem_valor,
        posicionais,
    }
}

/// Tudo o que o `kioskd` aceita. Conferido contra o texto de uso por teste.
pub const FORMAS_KIOSKD: &[Forma] = &[
    forma(&["serve"], &[], &["--no-agent"], 0),
    forma(&["pairing-code"], &[], &[], 0),
    forma(&["devices"], &[], &[], 0),
    forma(&["revoke"], &[], &[], 1),
    forma(
        &["profile", "create"],
        &["--account", "--sid", "--name", "--total-limit"],
        &[],
        0,
    ),
    forma(&["profile", "list"], &[], &[], 0),
    forma(&["profile", "limit"], &["--seconds"], &["--no-limit"], 1),
    forma(&["profile", "window"], &["--days", "--window"], &[], 1),
    forma(
        &["app", "create"],
        &["--process"],
        &["--exempt", "--ignore"],
        0,
    ),
    forma(&["app", "list"], &[], &[], 0),
    forma(
        &["rule", "create"],
        &["--profile", "--app", "--limit", "--window", "--until"],
        &["--activate"],
        0,
    ),
    forma(&["rule", "activate"], &[], &[], 1),
    forma(&["rule", "revoke"], &[], &[], 1),
    forma(&["rule", "limit"], &["--seconds"], &[], 1),
    forma(&["rule", "list"], &["--profile"], &[], 0),
    forma(&["install"], &["--listen"], &[], 0),
    forma(&["uninstall"], &[], &["--purge-data"], 0),
    forma(&["status"], &[], &[], 0),
    forma(&["service"], &[], &[], 0),
];

/// Tudo o que o `kioskctl` aceita.
pub const FORMAS_KIOSKCTL: &[Forma] = &[
    forma(&["pair"], &["--server", "--code", "--name"], &[], 0),
    forma(
        &["rule", "create"],
        &["--profile", "--app", "--limit", "--window", "--until"],
        &[],
        0,
    ),
    forma(&["rule", "activate"], &[], &[], 1),
    forma(&["rule", "revoke"], &[], &[], 1),
    forma(&["rule", "limit"], &["--seconds"], &[], 1),
    forma(&["usage"], &["--profile", "--date"], &[], 0),
    forma(&["log"], &["--profile", "--days", "--limit"], &[], 0),
];

/// Por que os argumentos foram recusados.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroArgumentos {
    /// `--help` ou `-h`: mostrar o uso, sem erro.
    PedeAjuda,
    OpcaoDesconhecida {
        opcao: String,
        comando: String,
        sugestao: Option<String>,
    },
    OpcaoSemValor {
        opcao: String,
        comando: String,
    },
    ArgumentoSobrando {
        valor: String,
        comando: String,
    },
}

impl ErroArgumentos {
    pub fn msg(&self) -> crate::i18n::Msg {
        use crate::i18n::Msg;
        match self.clone() {
            Self::PedeAjuda => unreachable!("pedir ajuda não é erro"),
            Self::OpcaoDesconhecida {
                opcao,
                comando,
                sugestao: Some(sugestao),
            } => Msg::OpcaoDesconhecidaSugestao {
                opcao,
                comando,
                sugestao,
            },
            Self::OpcaoDesconhecida {
                opcao,
                comando,
                sugestao: None,
            } => Msg::OpcaoDesconhecida { opcao, comando },
            Self::OpcaoSemValor { opcao, comando } => Msg::OpcaoSemValor { opcao, comando },
            Self::ArgumentoSobrando { valor, comando } => Msg::ArgumentoSobrando { valor, comando },
        }
    }
}

/// Distância de edição entre duas opções (para sugerir a que a pessoa quis digitar).
fn distancia(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut linha: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut anterior = linha[0];
        linha[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let atual = linha[j + 1];
            linha[j + 1] = (anterior + usize::from(ca != cb))
                .min(linha[j] + 1)
                .min(linha[j + 1] + 1);
            anterior = atual;
        }
    }
    linha[b.len()]
}

/// Recusa opção desconhecida, opção sem valor e argumento sobrando, em vez de ignorá-los em
/// silêncio (um `--activete` digitado errado criava uma regra pendente sem avisar). `args` já em
/// inglês ([`em_ingles`]). Se as palavras não casam com nenhum comando, não diz nada: quem
/// executa mostra o uso.
pub fn validar(args: &[String], formas: &[Forma]) -> Result<(), ErroArgumentos> {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        return Err(ErroArgumentos::PedeAjuda);
    }
    let Some(f) = formas
        .iter()
        .filter(|f| {
            f.comando.len() <= args.len() && f.comando.iter().zip(args).all(|(c, a)| c == a)
        })
        .max_by_key(|f| f.comando.len())
    else {
        return Ok(());
    };
    let nome = f.comando.join(" ");
    let mut posicionais = 0;
    let mut i = f.comando.len();
    while i < args.len() {
        let a = &args[i];
        if f.com_valor.contains(&a.as_str()) {
            if i + 1 >= args.len() {
                return Err(ErroArgumentos::OpcaoSemValor {
                    opcao: a.clone(),
                    comando: nome,
                });
            }
            i += 2;
            continue;
        }
        if f.sem_valor.contains(&a.as_str()) {
            i += 1;
            continue;
        }
        if a.starts_with("--") {
            let sugestao = f
                .com_valor
                .iter()
                .chain(f.sem_valor)
                .map(|o| (distancia(a, o), *o))
                .filter(|(d, _)| *d <= 2)
                .min_by_key(|(d, _)| *d)
                .map(|(_, o)| o.to_owned());
            return Err(ErroArgumentos::OpcaoDesconhecida {
                opcao: a.clone(),
                comando: nome,
                sugestao,
            });
        }
        posicionais += 1;
        if posicionais > f.posicionais {
            return Err(ErroArgumentos::ArgumentoSobrando {
                valor: a.clone(),
                comando: nome,
            });
        }
        i += 1;
    }
    Ok(())
}
