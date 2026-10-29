//! Idiomas e catálogo de mensagens (ADR-010): en-US, pt-BR e es-ES.
//!
//! - O catálogo é um enum gerado por macro: cada mensagem **precisa** trazer os três idiomas, ou
//!   o build falha (e adicionar um idioma ao [`Idioma`] quebra o build até todas serem traduzidas).
//! - Não se traduzem nomes de comandos e opções, códigos da API nem o `event_type` e a `action` do
//!   log forense (ver [`crate::detalhes`] para o detalhe do log, que guarda um código).
//! - Idioma de quem lê, nesta ordem: `KIOSK_LANG`; o idioma do usuário no sistema (no Windows, o
//!   idioma de exibição da conta que roda o programa; nos outros, `LC_ALL`, `LC_MESSAGES`, `LANG`);
//!   en-US. Valor desconhecido (por exemplo `fr`) é ignorado e a busca segue, sem erro.

use std::fmt;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Idioma {
    EnUs,
    PtBr,
    EsEs,
}

impl Idioma {
    pub const TODOS: [Idioma; 3] = [Idioma::EnUs, Idioma::PtBr, Idioma::EsEs];

    pub fn tag(self) -> &'static str {
        match self {
            Idioma::EnUs => "en-US",
            Idioma::PtBr => "pt-BR",
            Idioma::EsEs => "es-ES",
        }
    }

    /// `pt-BR`, `pt_PT.UTF-8`, `es-MX`, `en-GB`, `pt`... -> o idioma suportado mais próximo.
    pub fn de_tag(tag: &str) -> Option<Idioma> {
        let t = tag.trim().to_lowercase().replace('_', "-");
        let t = t.split(['.', '@']).next().unwrap_or("");
        match t.split('-').next().unwrap_or("") {
            "en" => Some(Idioma::EnUs),
            "pt" => Some(Idioma::PtBr),
            "es" => Some(Idioma::EsEs),
            _ => None,
        }
    }

    /// Palavra que se digita para confirmar o que apaga dados (`--purge-data`).
    pub fn palavra_de_confirmacao(self) -> &'static str {
        match self {
            Idioma::EnUs => "DELETE",
            Idioma::PtBr => "APAGAR",
            Idioma::EsEs => "BORRAR",
        }
    }

    /// Dia da semana abreviado, `0` = domingo.
    pub fn dia_curto(self, dia: u8) -> &'static str {
        const EN: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];
        const PT: [&str; 7] = ["dom", "seg", "ter", "qua", "qui", "sex", "sab"];
        const ES: [&str; 7] = ["dom", "lun", "mar", "mie", "jue", "vie", "sab"];
        let i = usize::from(dia % 7);
        match self {
            Idioma::EnUs => EN[i],
            Idioma::PtBr => PT[i],
            Idioma::EsEs => ES[i],
        }
    }
}

/// Idioma definido por `KIOSK_LANG`, se válido.
fn da_variavel() -> Option<Idioma> {
    std::env::var("KIOSK_LANG")
        .ok()
        .and_then(|v| Idioma::de_tag(&v))
}

/// Idioma do usuário no sistema operacional.
fn do_sistema() -> Option<Idioma> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
        // SAFETY: buffer de 85 u16; devolve o tamanho gravado, com o zero final, ou 0.
        let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if n > 1 {
            let nome = String::from_utf16_lossy(&buf[..(n as usize - 1)]);
            if let Some(i) = Idioma::de_tag(&nome) {
                return Some(i);
            }
        }
        None
    }
    #[cfg(not(windows))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .filter_map(|v| std::env::var(v).ok())
            .filter(|v| !v.is_empty())
            .find_map(|v| Idioma::de_tag(&v))
    }
}

/// `KIOSK_LANG`, depois o idioma do sistema, depois en-US.
pub fn detectar() -> Idioma {
    da_variavel().or_else(do_sistema).unwrap_or(Idioma::EnUs)
}

static IDIOMA: OnceLock<Idioma> = OnceLock::new();

/// Idioma deste processo, detectado uma vez.
pub fn idioma() -> Idioma {
    *IDIOMA.get_or_init(detectar)
}

/// Fixa o idioma do processo (a primeira chamada vale). Para testes.
pub fn definir(i: Idioma) {
    let _ = IDIOMA.set(i);
}

macro_rules! catalogo {
    ($($variante:ident $({ $($campo:ident : $tipo:ty),* $(,)? })? => {
        en: $en:literal,
        pt: $pt:literal,
        es: $es:literal $(,)?
    }),* $(,)?) => {
        /// Uma mensagem do programa, com os dados que ela mostra.
        #[derive(Debug, Clone, PartialEq)]
        pub enum Msg {
            $($variante $({ $($campo: $tipo),* })?),*
        }

        impl Msg {
            /// O texto da mensagem no idioma pedido.
            #[allow(unused_variables)]
            pub fn em(&self, idioma: Idioma) -> String {
                match self {
                    $(Msg::$variante $({ $($campo),* })? => match idioma {
                        Idioma::EnUs => format!($en),
                        Idioma::PtBr => format!($pt),
                        Idioma::EsEs => format!($es),
                    },)*
                }
            }
        }
    };
}

catalogo! {
    // ---- texto de uso ----
    UsoKioskd => {
        en: "usage:\n  kioskd serve [--no-agent]\n  kioskd pairing-code   (Windows: PowerShell as administrator)\n  kioskd devices\n  kioskd revoke <device-id>\n  kioskd profile create --account ACCOUNT --name NAME [--total-limit SECONDS]   (or --sid S-1-... instead of --account)\n  kioskd profile list\n  kioskd profile limit <profile-id> --seconds N | --no-limit\n  kioskd profile window <profile-id> --days mon,tue,wed,thu,fri --window 08:00-20:00\n  kioskd app create --process NAME.exe [--exempt | --ignore]\n  kioskd app list\n  kioskd rule create --profile ID --app ID [--limit SECONDS] [--window HH:MM-HH:MM] [--until RFC3339] [--activate]\n  kioskd rule activate <rule-id>\n  kioskd rule revoke <rule-id>\n  kioskd rule limit <rule-id> --seconds N\n  kioskd rule list [--profile ID]\n  kioskd install --listen ip:port[,ip:port...]   (Windows, PowerShell as administrator)\n  kioskd uninstall [--purge-data]\n  kioskd status\n  kioskd version\n  kioskd service   (called by Windows when it starts the service; do not run by hand)\nthe same commands in Portuguese also work (e.g. kioskd estado = kioskd status); list in docs/runbooks/comandos-em-portugues.md\nvariables: KIOSK_DB (default on Windows: sqlite://C:/ProgramData/kiosk/kiosk.db), KIOSK_LISTEN (ip:port[,ip:port...], required for `serve`), KIOSK_LANG (en-US, pt-BR or es-ES)",
        pt: "uso:\n  kioskd serve [--no-agent]\n  kioskd pairing-code   (Windows: PowerShell como administrador)\n  kioskd devices\n  kioskd revoke <id-do-dispositivo>\n  kioskd profile create --account USUARIO --name NOME [--total-limit SEGUNDOS]   (ou --sid S-1-... no lugar de --account)\n  kioskd profile list\n  kioskd profile limit <id-do-perfil> --seconds N | --no-limit\n  kioskd profile window <id-do-perfil> --days mon,tue,wed,thu,fri --window 08:00-20:00\n  kioskd app create --process NOME.exe [--exempt | --ignore]\n  kioskd app list\n  kioskd rule create --profile ID --app ID [--limit SEGUNDOS] [--window HH:MM-HH:MM] [--until RFC3339] [--activate]\n  kioskd rule activate <id-da-regra>\n  kioskd rule revoke <id-da-regra>\n  kioskd rule limit <id-da-regra> --seconds N\n  kioskd rule list [--profile ID]\n  kioskd install --listen ip:porta[,ip:porta...]   (Windows, PowerShell como administrador)\n  kioskd uninstall [--purge-data]\n  kioskd status\n  kioskd version\n  kioskd service   (chamado pelo Windows ao iniciar o serviço; não rode à mão)\nos mesmos comandos em português também valem (ex.: kioskd estado = kioskd status); lista em docs/runbooks/comandos-em-portugues.md\nvariáveis: KIOSK_DB (padrão no Windows: sqlite://C:/ProgramData/kiosk/kiosk.db), KIOSK_LISTEN (ip:porta[,ip:porta...], obrigatória em `serve`), KIOSK_LANG (en-US, pt-BR ou es-ES)",
        es: "uso:\n  kioskd serve [--no-agent]\n  kioskd pairing-code   (Windows: PowerShell como administrador)\n  kioskd devices\n  kioskd revoke <id-del-dispositivo>\n  kioskd profile create --account CUENTA --name NOMBRE [--total-limit SEGUNDOS]   (o --sid S-1-... en lugar de --account)\n  kioskd profile list\n  kioskd profile limit <id-del-perfil> --seconds N | --no-limit\n  kioskd profile window <id-del-perfil> --days mon,tue,wed,thu,fri --window 08:00-20:00\n  kioskd app create --process NOMBRE.exe [--exempt | --ignore]\n  kioskd app list\n  kioskd rule create --profile ID --app ID [--limit SEGUNDOS] [--window HH:MM-HH:MM] [--until RFC3339] [--activate]\n  kioskd rule activate <id-de-la-regla>\n  kioskd rule revoke <id-de-la-regla>\n  kioskd rule limit <id-de-la-regla> --seconds N\n  kioskd rule list [--profile ID]\n  kioskd install --listen ip:puerto[,ip:puerto...]   (Windows, PowerShell como administrador)\n  kioskd uninstall [--purge-data]\n  kioskd status\n  kioskd version\n  kioskd service   (lo invoca Windows al iniciar el servicio; no lo ejecute a mano)\nlos mismos comandos en portugués también valen (p. ej. kioskd estado = kioskd status); lista en docs/runbooks/comandos-em-portugues.md\nvariables: KIOSK_DB (por defecto en Windows: sqlite://C:/ProgramData/kiosk/kiosk.db), KIOSK_LISTEN (ip:puerto[,ip:puerto...], obligatoria en `serve`), KIOSK_LANG (en-US, pt-BR o es-ES)",
    },
    UsoKioskctl => {
        en: "usage:\n  kioskctl pair --server HOST:PORT --code CODE [--name NAME]\n  kioskctl rule create --profile ID --app ID [--limit SECONDS] [--window HH:MM-HH:MM] [--until RFC3339]\n  kioskctl rule activate <id>\n  kioskctl rule revoke <id>\n  kioskctl rule limit <id> --seconds N\n  kioskctl usage --profile ID [--date YYYY-MM-DD]\n  kioskctl log --profile ID [--days N] [--limit N]\n  kioskctl version\nthe same commands in Portuguese also work (e.g. kioskctl parear = kioskctl pair); list in docs/runbooks/comandos-em-portugues.md\nvariable: KIOSK_LANG (en-US, pt-BR or es-ES)",
        pt: "uso:\n  kioskctl pair --server HOST:PORTA --code CODIGO [--name NOME]\n  kioskctl rule create --profile ID --app ID [--limit SEGUNDOS] [--window HH:MM-HH:MM] [--until RFC3339]\n  kioskctl rule activate <id>\n  kioskctl rule revoke <id>\n  kioskctl rule limit <id> --seconds N\n  kioskctl usage --profile ID [--date AAAA-MM-DD]\n  kioskctl log --profile ID [--days N] [--limit N]\n  kioskctl version\nos mesmos comandos em português também valem (ex.: kioskctl parear = kioskctl pair); lista em docs/runbooks/comandos-em-portugues.md\nvariável: KIOSK_LANG (en-US, pt-BR ou es-ES)",
        es: "uso:\n  kioskctl pair --server HOST:PUERTO --code CODIGO [--name NOMBRE]\n  kioskctl rule create --profile ID --app ID [--limit SEGUNDOS] [--window HH:MM-HH:MM] [--until RFC3339]\n  kioskctl rule activate <id>\n  kioskctl rule revoke <id>\n  kioskctl rule limit <id> --seconds N\n  kioskctl usage --profile ID [--date AAAA-MM-DD]\n  kioskctl log --profile ID [--days N] [--limit N]\n  kioskctl version\nlos mismos comandos en portugués también valen (p. ej. kioskctl parear = kioskctl pair); lista en docs/runbooks/comandos-em-portugues.md\nvariable: KIOSK_LANG (en-US, pt-BR o es-ES)",
    },

    // ---- argumentos e validações (kioskd e kioskctl) ----
    IdInvalido { valor: String } => {
        en: "invalid identifier: {valor:?}",
        pt: "identificador inválido: {valor:?}",
        es: "identificador no válido: {valor:?}",
    },
    OpcaoInvalida { nome: String, valor: String } => {
        en: "invalid {nome}: {valor:?}",
        pt: "{nome} inválido: {valor:?}",
        es: "{nome} no válido: {valor:?}",
    },
    FaltaOpcao { nome: String } => {
        en: "missing {nome}",
        pt: "falta {nome}",
        es: "falta {nome}",
    },
    JanelaFormato { recebido: String } => {
        en: "--window must be HH:MM-HH:MM, got {recebido:?}",
        pt: "--window deve ser HH:MM-HH:MM, recebi {recebido:?}",
        es: "--window debe ser HH:MM-HH:MM, se recibió {recebido:?}",
    },
    HoraInvalida { valor: String } => {
        en: "invalid time: {valor:?} (use HH:MM)",
        pt: "hora inválida: {valor:?} (use HH:MM)",
        es: "hora no válida: {valor:?} (use HH:MM)",
    },
    DiaInvalido { valor: String } => {
        en: "invalid day: {valor:?} (use sun, mon, tue, wed, thu, fri, sat)",
        pt: "dia inválido: {valor:?} (use dom, seg, ter, qua, qui, sex, sab)",
        es: "día no válido: {valor:?} (use dom, lun, mar, mie, jue, vie, sab)",
    },
    SidInvalido { valor: String } => {
        en: "invalid SID: {valor:?}",
        pt: "SID inválido: {valor:?}",
        es: "SID no válido: {valor:?}",
    },
    ContaSoNoWindows { conta: String } => {
        en: "--account {conta:?} is only resolved on Windows; pass --sid",
        pt: "--account {conta:?} só é resolvida no Windows; informe --sid",
        es: "--account {conta:?} solo se resuelve en Windows; indique --sid",
    },
    SoNoWindows => {
        en: "this command only exists on Windows (Windows service, ADR-008)",
        pt: "este comando só existe no Windows (serviço do Windows, ADR-008)",
        es: "este comando solo existe en Windows (servicio de Windows, ADR-008)",
    },
    InicioAntesDoFim => {
        en: "the window start must be before its end",
        pt: "o início da janela deve ser anterior ao fim",
        es: "el inicio de la ventana debe ser anterior al fin",
    },
    ExemptOuIgnore => {
        en: "use --exempt or --ignore, not both",
        pt: "use --exempt ou --ignore, não os dois",
        es: "use --exempt o --ignore, no ambos",
    },
    LimiteAcimaDe { max: u64 } => {
        en: "--limit above {max}",
        pt: "--limit acima de {max}",
        es: "--limit por encima de {max}",
    },
    UntilNaoRfc => {
        en: "--until must be an RFC 3339 date and time",
        pt: "--until deve ser uma data e hora RFC 3339",
        es: "--until debe ser una fecha y hora RFC 3339",
    },
    UntilNoPassado => {
        en: "--until must be in the future",
        pt: "--until deve estar no futuro",
        es: "--until debe estar en el futuro",
    },
    SegundosAcima { max: u64 } => {
        en: "--seconds above {max}",
        pt: "--seconds acima de {max}",
        es: "--seconds por encima de {max}",
    },
    PerfilInexistenteVeja => {
        en: "profile does not exist (see `kioskd profile list`)",
        pt: "perfil inexistente (veja `kioskd profile list`)",
        es: "el perfil no existe (vea `kioskd profile list`)",
    },
    AppInexistenteVeja => {
        en: "app does not exist (see `kioskd app list`)",
        pt: "app inexistente (veja `kioskd app list`)",
        es: "la app no existe (vea `kioskd app list`)",
    },
    PerfilInexistente => {
        en: "profile does not exist",
        pt: "perfil inexistente",
        es: "el perfil no existe",
    },
    RegraInexistente => {
        en: "rule does not exist",
        pt: "regra inexistente",
        es: "la regla no existe",
    },
    RegraInexistenteOuRevogada => {
        en: "rule does not exist or was revoked",
        pt: "regra inexistente ou revogada",
        es: "la regla no existe o fue revocada",
    },
    DispositivoInexistente => {
        en: "device does not exist or was already revoked",
        pt: "dispositivo inexistente ou já revogado",
        es: "el dispositivo no existe o ya fue revocado",
    },
    PerfilJaExiste { sid: String } => {
        en: "a profile already exists for SID {sid}",
        pt: "já existe um perfil para o SID {sid}",
        es: "ya existe un perfil para el SID {sid}",
    },
    AppJaCadastrado { processo: String } => {
        en: "{processo} is already registered",
        pt: "{processo} já está cadastrado",
        es: "{processo} ya está registrado",
    },
    JanelaNaoGravada { erro: String } => {
        en: "window not saved (does the profile exist?): {erro}",
        pt: "janela não gravada (o perfil existe?): {erro}",
        es: "ventana no guardada (¿existe el perfil?): {erro}",
    },

    // ---- valores e rótulos das listagens ----
    OpcaoDesconhecida { opcao: String, comando: String } => {
        en: "unknown option {opcao} for `{comando}`",
        pt: "opção desconhecida {opcao} em `{comando}`",
        es: "opción desconocida {opcao} en `{comando}`",
    },
    OpcaoDesconhecidaSugestao { opcao: String, comando: String, sugestao: String } => {
        en: "unknown option {opcao} for `{comando}` (did you mean {sugestao}?)",
        pt: "opção desconhecida {opcao} em `{comando}` (você quis dizer {sugestao}?)",
        es: "opción desconocida {opcao} en `{comando}` (¿quiso decir {sugestao}?)",
    },
    OpcaoSemValor { opcao: String, comando: String } => {
        en: "{opcao} needs a value in `{comando}`",
        pt: "{opcao} precisa de um valor em `{comando}`",
        es: "{opcao} necesita un valor en `{comando}`",
    },
    ArgumentoSobrando { valor: String, comando: String } => {
        en: "unexpected argument {valor:?} for `{comando}`",
        pt: "argumento inesperado {valor:?} em `{comando}`",
        es: "argumento inesperado {valor:?} en `{comando}`",
    },
    LimiteJaUltrapassado { app: String, usado: String, limite: String } => {
        en: "warning: {app} has already used {usado} today, which is at or above this rule's limit of {limite}: it will be closed as soon as it opens (raise the limit with `rule limit`, or revoke the rule)",
        pt: "atenção: {app} já usou {usado} hoje, o que é igual ou maior que o limite desta regra, {limite}: ele será fechado assim que abrir (aumente o limite com `rule limit` ou revogue a regra)",
        es: "atención: {app} ya usó {usado} hoy, lo que iguala o supera el límite de esta regla, {limite}: se cerrará en cuanto se abra (suba el límite con `rule limit` o revoque la regla)",
    },
    LimiteTotalJaUltrapassado { usado: String, limite: String } => {
        en: "warning: the account has already spent {usado} in an active session today, at or above the total limit of {limite}: the session will be locked right away (raise it with `profile limit`, or use --no-limit)",
        pt: "atenção: a conta já ficou {usado} em sessão ativa hoje, o que é igual ou maior que o limite total, {limite}: a sessão será bloqueada em seguida (aumente com `profile limit` ou use --no-limit)",
        es: "atención: la cuenta ya pasó {usado} en sesión activa hoy, lo que iguala o supera el límite total, {limite}: la sesión se bloqueará enseguida (súbalo con `profile limit` o use --no-limit)",
    },
    SemLimite => {
        en: "no limit",
        pt: "sem limite",
        es: "sin límite",
    },
    SemJanela => {
        en: "no window",
        pt: "sem janela",
        es: "sin ventana",
    },
    NenhumDispositivo => {
        en: "no paired devices",
        pt: "nenhum dispositivo pareado",
        es: "ningún dispositivo emparejado",
    },
    NenhumPerfil => {
        en: "no profiles registered",
        pt: "nenhum perfil cadastrado",
        es: "ningún perfil registrado",
    },
    NenhumApp => {
        en: "no apps registered",
        pt: "nenhum app cadastrado",
        es: "ninguna app registrada",
    },
    NenhumaRegra => {
        en: "no rules registered",
        pt: "nenhuma regra cadastrada",
        es: "ninguna regla registrada",
    },
    DispositivoAtivo => {
        en: "active",
        pt: "ativo",
        es: "activo",
    },
    DispositivoRevogadoEm { quando: String } => {
        en: "revoked on {quando}",
        pt: "revogado em {quando}",
        es: "revocado el {quando}",
    },
    CriadoEm { quando: String } => {
        en: "created on {quando}",
        pt: "criado em {quando}",
        es: "creado el {quando}",
    },
    RotuloLimiteDiario { limite: String } => {
        en: "daily limit: {limite}",
        pt: "limite diário: {limite}",
        es: "límite diario: {limite}",
    },
    RotuloLimite { limite: String } => {
        en: "limit: {limite}",
        pt: "limite: {limite}",
        es: "límite: {limite}",
    },

    // ---- resultados dos comandos do kioskd ----
    BancoEmUso { url: String } => {
        en: "database {url}",
        pt: "banco {url}",
        es: "base de datos {url}",
    },
    PastaNaoCriada { pasta: String, erro: String } => {
        en: "could not create folder {pasta}: {erro}",
        pt: "não foi possível criar a pasta {pasta}: {erro}",
        es: "no se pudo crear la carpeta {pasta}: {erro}",
    },
    BancoNaoAbriu { url: String, erro: String } => {
        en: "could not open the database ({url}): {erro}",
        pt: "não foi possível abrir o banco ({url}): {erro}",
        es: "no se pudo abrir la base de datos ({url}): {erro}",
    },
    ListenNaoDefinida => {
        en: "KIOSK_LISTEN is not set: provide the listen addresses (ADR-005)",
        pt: "KIOSK_LISTEN não definida: informe os endereços de escuta (ADR-005)",
        es: "KIOSK_LISTEN no está definida: indique las direcciones de escucha (ADR-005)",
    },
    FaltaPorta { ender: String } => {
        en: "missing port in {ender:?}: use ip:port, for example {ender}:8443",
        pt: "falta a porta em {ender:?}: use ip:porta, por exemplo {ender}:8443",
        es: "falta el puerto en {ender:?}: use ip:puerto, por ejemplo {ender}:8443",
    },
    EnderecoInvalido { ender: String } => {
        en: "invalid listen address: {ender:?} (use ip:port)",
        pt: "endereço de escuta inválido: {ender:?} (use ip:porta)",
        es: "dirección de escucha no válida: {ender:?} (use ip:puerto)",
    },
    NenhumEndereco => {
        en: "no listen address given (use ip:port)",
        pt: "nenhum endereço de escuta informado (use ip:porta)",
        es: "no se indicó ninguna dirección de escucha (use ip:puerto)",
    },
    SemEnderecosConfigurados => {
        en: "no listen address configured (ADR-005)",
        pt: "nenhum endereço de escuta configurado (ADR-005)",
        es: "ninguna dirección de escucha configurada (ADR-005)",
    },
    EnderecoRecusado { endereco: String, motivo: String } => {
        en: "address {endereco} refused: {motivo}",
        pt: "endereço {endereco} recusado: {motivo}",
        es: "dirección {endereco} rechazada: {motivo}",
    },
    EnderecoCuringa => {
        en: "wildcard address (0.0.0.0 or ::) is not allowed",
        pt: "endereço curinga (0.0.0.0 ou ::) não é permitido",
        es: "no se permite la dirección comodín (0.0.0.0 o ::)",
    },
    EnderecoForaDaRede => {
        en: "loopback and the Tailscale network only; LAN needs HTTPS, not implemented yet (ADR-005)",
        pt: "só loopback e rede Tailscale; LAN exige HTTPS, ainda não implementado (ADR-005)",
        es: "solo loopback y la red Tailscale; la LAN exige HTTPS, aún no implementado (ADR-005)",
    },
    FalhaDeEs { erro: String } => {
        en: "I/O failure: {erro}",
        pt: "falha de E/S: {erro}",
        es: "fallo de E/S: {erro}",
    },
    Escutando { lista: String } => {
        en: "listening on {lista}",
        pt: "escutando em {lista}",
        es: "escuchando en {lista}",
    },
    CodigoDePareamento => {
        en: "code valid for 5 minutes, single use:",
        pt: "código válido por 5 minutos, de uso único:",
        es: "código válido durante 5 minutos, de un solo uso:",
    },
    SoAdministrador { comando: String } => {
        en: "`{comando}` needs an administrator: open PowerShell with \"Run as administrator\"",
        pt: "`{comando}` exige administrador: abra o PowerShell com \"Executar como administrador\"",
        es: "`{comando}` exige un administrador: abra PowerShell con \"Ejecutar como administrador\"",
    },
    RecusaRegistrada { base: String } => {
        en: "{base} (the attempt was recorded in the forensic log)",
        pt: "{base} (a tentativa foi registrada no log forense)",
        es: "{base} (el intento quedó registrado en el registro forense)",
    },
    RecusaSemRegistro { base: String, erro: String } => {
        en: "{base} (the attempt was NOT recorded in the forensic log: {erro})",
        pt: "{base} (a tentativa NÃO foi registrada no log forense: {erro})",
        es: "{base} (el intento NO quedó registrado en el registro forense: {erro})",
    },
    ServicoNaoResponde => {
        en: "the kiosk service is not running or did not answer",
        pt: "o serviço kiosk não está rodando ou não respondeu",
        es: "el servicio kiosk no está en ejecución o no respondió",
    },
    PipeNaoEDoServico => {
        en: "the audit pipe is not served by the kiosk service",
        pt: "o pipe de auditoria não é servido pelo serviço kiosk",
        es: "el pipe de auditoría no lo sirve el servicio kiosk",
    },
    ServicoNaoGravou => {
        en: "the service did not confirm that it recorded the attempt",
        pt: "o serviço não confirmou que gravou a tentativa",
        es: "el servicio no confirmó que registró el intento",
    },
    AuditoriaIndisponivel { erro: String } => {
        en: "attempt audit channel unavailable: {erro}",
        pt: "canal de auditoria de tentativas indisponível: {erro}",
        es: "canal de auditoría de intentos no disponible: {erro}",
    },
    DispositivoRevogado { id: String } => {
        en: "device {id} revoked",
        pt: "dispositivo {id} revogado",
        es: "dispositivo {id} revocado",
    },
    PerfilCriado { nome: String, sid: String } => {
        en: "profile {nome:?} created for account {sid}",
        pt: "perfil {nome:?} criado para a conta {sid}",
        es: "perfil {nome:?} creado para la cuenta {sid}",
    },
    PerfilLimiteAgora { limite: String } => {
        en: "the profile's daily limit is now {limite}",
        pt: "limite diário do perfil agora é {limite}",
        es: "el límite diario del perfil ahora es {limite}",
    },
    JanelaGravada { dias: usize } => {
        en: "window saved for {dias} day(s)",
        pt: "janela gravada para {dias} dia(s)",
        es: "ventana guardada para {dias} día(s)",
    },
    AppCadastrado { processo: String, status: String } => {
        en: "app {processo} registered ({status})",
        pt: "app {processo} cadastrado ({status})",
        es: "app {processo} registrada ({status})",
    },
    RegraCriadaAtiva => {
        en: "rule created (active)",
        pt: "regra criada (ativa)",
        es: "regla creada (activa)",
    },
    RegraCriadaPendente => {
        en: "rule created (pending: use `rule activate`)",
        pt: "regra criada (pendente: use `rule activate`)",
        es: "regla creada (pendiente: use `rule activate`)",
    },
    RegraAtivadaLocal => {
        en: "rule activated",
        pt: "regra ativada",
        es: "regla activada",
    },
    RegraRevogadaLocal => {
        en: "rule revoked",
        pt: "regra revogada",
        es: "regla revocada",
    },
    LimiteDaRegraAgora { limite: String } => {
        en: "the rule's daily limit is now {limite}",
        pt: "limite diário da regra agora é {limite}",
        es: "el límite diario de la regla ahora es {limite}",
    },
    ConfirmarApagar { palavra: String } => {
        en: "this deletes the database (profiles, rules, usage and forensic log). Type {palavra} to confirm: ",
        pt: "isto apaga o banco (perfis, regras, uso e log forense). Digite {palavra} para confirmar: ",
        es: "esto borra la base de datos (perfiles, reglas, uso y registro forense). Escriba {palavra} para confirmar: ",
    },
    NaoConfirmado => {
        en: "not confirmed: nothing was changed",
        pt: "não confirmado: nada foi alterado",
        es: "no confirmado: no se cambió nada",
    },
    ErroLeitura { erro: String } => {
        en: "could not read the confirmation: {erro}",
        pt: "falha ao ler a confirmação: {erro}",
        es: "error al leer la confirmación: {erro}",
    },
    Retencao { n: u64 } => {
        en: "retention: {n} old events removed",
        pt: "retenção: {n} eventos antigos removidos",
        es: "retención: {n} eventos antiguos eliminados",
    },
    RetencaoFalhou { erro: String } => {
        en: "retention failed: {erro}",
        pt: "retenção falhou: {erro}",
        es: "la retención falló: {erro}",
    },
    BandejaIndisponivel { erro: String } => {
        en: "tray channel unavailable (the service keeps running without it): {erro}",
        pt: "canal da bandeja indisponível (o serviço segue sem ele): {erro}",
        es: "canal de la bandeja no disponible (el servicio sigue sin él): {erro}",
    },
    AgenteAtivo { segundos: u64 } => {
        en: "enforcement agent active (tick every {segundos}s)",
        pt: "agente de enforcement ativo (tick de {segundos}s)",
        es: "agente de aplicación activo (ciclo de {segundos}s)",
    },
    AgenteSoWindows => {
        en: "the enforcement agent only exists on Windows; serving the API only",
        pt: "agente de enforcement só existe no Windows; servindo apenas a API",
        es: "el agente de aplicación solo existe en Windows; solo se sirve la API",
    },
    AgenteErro { erro: String } => {
        en: "agent: {erro}",
        pt: "agente: {erro}",
        es: "agente: {erro}",
    },

    // ---- serviço do Windows: kioskd install, uninstall, status e service ----
    InstaladoEmExecucao { pid: u32, escutar: String } => {
        en: "service running (pid {pid}), listening on {escutar}",
        pt: "serviço em execução (pid {pid}), escutando em {escutar}",
        es: "servicio en ejecución (pid {pid}), escuchando en {escutar}",
    },
    LogDoServicoInstalado { caminho: String } => {
        en: "service log at {caminho}; registration commands now need PowerShell as administrator",
        pt: "log do serviço em {caminho}; os comandos de cadastro agora pedem PowerShell como administrador",
        es: "registro del servicio en {caminho}; los comandos de alta ahora requieren PowerShell como administrador",
    },
    LblServico { v: String } => {
        en: "service: {v}",
        pt: "serviço: {v}",
        es: "servicio: {v}",
    },
    LblEstado { v: String } => {
        en: "state: {v}",
        pt: "estado: {v}",
        es: "estado: {v}",
    },
    LblInicio { v: String } => {
        en: "startup: {v}",
        pt: "início: {v}",
        es: "inicio: {v}",
    },
    LblConta { v: String } => {
        en: "account: {v}",
        pt: "conta: {v}",
        es: "cuenta: {v}",
    },
    LblVersao { v: String } => {
        en: "version: {v}",
        pt: "versão: {v}",
        es: "versión: {v}",
    },
    LblVersaoInstalada { v: String } => {
        en: "installed version: {v}",
        pt: "versão instalada: {v}",
        es: "versión instalada: {v}",
    },
    VersaoIndisponivel => {
        en: "unknown (could not run the installed executable)",
        pt: "desconhecida (não foi possível rodar o executável instalado)",
        es: "desconocida (no se pudo ejecutar el ejecutable instalado)",
    },
    VersaoInstalada { v: String } => {
        en: "installed version {v}",
        pt: "versão instalada {v}",
        es: "versión instalada {v}",
    },
    LblCommit { v: String } => {
        en: "commit: {v}",
        pt: "commit: {v}",
        es: "commit: {v}",
    },
    LblDataDaBuild { v: String } => {
        en: "build date: {v}",
        pt: "data da compilação: {v}",
        es: "fecha de compilación: {v}",
    },
    LblAlvo { v: String } => {
        en: "target: {v}",
        pt: "alvo: {v}",
        es: "destino: {v}",
    },
    LblPerfilDaBuild { v: String } => {
        en: "build profile: {v}",
        pt: "perfil da compilação: {v}",
        es: "perfil de compilación: {v}",
    },
    LblExecutavel { v: String } => {
        en: "executable: {v}",
        pt: "executável: {v}",
        es: "ejecutable: {v}",
    },
    LblAmbiente { v: String } => {
        en: "environment: {v}",
        pt: "ambiente: {v}",
        es: "entorno: {v}",
    },
    LblPath { v: String } => {
        en: "system PATH: {v}",
        pt: "PATH do sistema: {v}",
        es: "PATH del sistema: {v}",
    },
    LblDados { v: String } => {
        en: "data: {v}",
        pt: "dados: {v}",
        es: "datos: {v}",
    },
    LblLog { v: String } => {
        en: "log: {v}",
        pt: "log: {v}",
        es: "registro: {v}",
    },
    NaoInstalado => {
        en: "not installed",
        pt: "não instalado",
        es: "no instalado",
    },
    InicioAutomatico => {
        en: "automatic",
        pt: "automático",
        es: "automático",
    },
    InicioManual => {
        en: "manual or disabled",
        pt: "manual ou desativado",
        es: "manual o desactivado",
    },
    PathSim => {
        en: "yes",
        pt: "sim",
        es: "sí",
    },
    PathNao => {
        en: "no (run `kioskd install` again)",
        pt: "não (rode `kioskd install` de novo)",
        es: "no (ejecute `kioskd install` de nuevo)",
    },
    EstParado => {
        en: "stopped",
        pt: "parado",
        es: "detenido",
    },
    EstIniciando => {
        en: "starting",
        pt: "iniciando",
        es: "iniciando",
    },
    EstParando => {
        en: "stopping",
        pt: "parando",
        es: "deteniéndose",
    },
    EstExecucao => {
        en: "running",
        pt: "em execução",
        es: "en ejecución",
    },
    EstOutro => {
        en: "other",
        pt: "outro",
        es: "otro",
    },
    ServicoNomeExibicao => {
        en: "Kiosk (parental control)",
        pt: "Kiosk (controle parental)",
        es: "Kiosk (control parental)",
    },
    ServicoDescricao => {
        en: "Enforces the usage limits set by the parents and serves the command API (loopback and Tailscale).",
        pt: "Aplica os limites de uso definidos pelos pais e atende a API de comando (loopback e Tailscale).",
        es: "Aplica los límites de uso definidos por los padres y atiende la API de comandos (loopback y Tailscale).",
    },

    // ---- instalação e remoção (progresso e erros) ----
    SemAcessoAoScm => {
        en: "access denied by the service manager: run in a PowerShell opened with \"Run as administrator\"",
        pt: "acesso negado ao gerenciador de serviços: rode num PowerShell aberto com \"Executar como administrador\"",
        es: "acceso denegado al administrador de servicios: ejecute en un PowerShell abierto con \"Ejecutar como administrador\"",
    },
    EstadoNaoChegou { alvo: String, segundos: u64, atual: String, log: String } => {
        en: "the service did not reach \"{alvo}\" within {segundos}s (it is \"{atual}\"); see {log}",
        pt: "o serviço não chegou a \"{alvo}\" em {segundos}s (está \"{atual}\"); veja {log}",
        es: "el servicio no llegó a \"{alvo}\" en {segundos}s (está \"{atual}\"); vea {log}",
    },
    PastaProtegida { pasta: String } => {
        en: "{pasta} protected (SYSTEM and Administrators only)",
        pt: "{pasta} protegida (só SYSTEM e Administradores)",
        es: "{pasta} protegida (solo SYSTEM y Administradores)",
    },
    NaoProtegeu { pasta: String, erro: String } => {
        en: "could not protect {pasta}: {erro}",
        pt: "não foi possível proteger {pasta}: {erro}",
        es: "no se pudo proteger {pasta}: {erro}",
    },
    NaoCriouPasta { pasta: String, erro: String } => {
        en: "could not create {pasta}: {erro}",
        pt: "não foi possível criar {pasta}: {erro}",
        es: "no se pudo crear {pasta}: {erro}",
    },
    ServicoJaInstalado => {
        en: "service already installed; stopping it to update",
        pt: "serviço já instalado; parando para atualizar",
        es: "servicio ya instalado; deteniéndolo para actualizar",
    },
    ExecutaveisEm { pasta: String } => {
        en: "executables in {pasta}",
        pt: "executáveis em {pasta}",
        es: "ejecutables en {pasta}",
    },
    FaltaExecutavel { arquivo: String } => {
        en: "{arquivo} is missing next to kioskd: extract the whole .zip and try again",
        pt: "falta {arquivo} ao lado do kioskd: extraia o .zip inteiro e rode de novo",
        es: "falta {arquivo} junto a kioskd: extraiga el .zip completo y vuelva a ejecutar",
    },
    NaoCopiou { de: String, para: String, erro: String } => {
        en: "could not copy {de} to {para}: {erro}",
        pt: "não foi possível copiar {de} para {para}: {erro}",
        es: "no se pudo copiar {de} a {para}: {erro}",
    },
    NoPath { pasta: String } => {
        en: "{pasta} added to the system PATH: in new PowerShell windows just type `kioskd`",
        pt: "{pasta} incluída no PATH do sistema: em janelas novas do PowerShell basta digitar `kioskd`",
        es: "{pasta} añadida al PATH del sistema: en ventanas nuevas de PowerShell basta escribir `kioskd`",
    },
    ForaDoPath { pasta: String } => {
        en: "{pasta} removed from the system PATH",
        pt: "{pasta} retirada do PATH do sistema",
        es: "{pasta} eliminada del PATH del sistema",
    },
    ServicoRegistrado => {
        en: "service kiosk registered (LocalSystem, automatic start, restart on failure)",
        pt: "serviço kiosk registrado (LocalSystem, início automático, reinício em falha)",
        es: "servicio kiosk registrado (LocalSystem, inicio automático, reinicio ante fallos)",
    },
    ServicoParouLogo { codigo: u32, log: String } => {
        en: "the service started and then stopped (code {codigo}); see {log}",
        pt: "o serviço iniciou e parou (código {codigo}); veja {log}",
        es: "el servicio se inició y se detuvo (código {codigo}); vea {log}",
    },
    ServicoRemovido => {
        en: "service kiosk stopped and removed",
        pt: "serviço kiosk parado e removido",
        es: "servicio kiosk detenido y eliminado",
    },
    ServicoNaoEstavaInstalado => {
        en: "service kiosk was not installed",
        pt: "serviço kiosk não estava instalado",
        es: "el servicio kiosk no estaba instalado",
    },
    ExecutaveisRemovidos { pasta: String } => {
        en: "executables removed from {pasta}",
        pt: "executáveis removidos de {pasta}",
        es: "ejecutables eliminados de {pasta}",
    },
    ApagadaNoReinicio { pasta: String } => {
        en: "{pasta} will be deleted at the next restart (the running kioskd is in it)",
        pt: "{pasta} será apagada no próximo reinício (o kioskd em uso está lá)",
        es: "{pasta} se eliminará en el próximo reinicio (el kioskd en uso está dentro)",
    },
    PastaNaoVazia { pasta: String, erro: String } => {
        en: "{pasta} was not empty: {erro}; delete what is left by hand",
        pt: "{pasta} não ficou vazia: {erro}; apague à mão o que sobrou",
        es: "{pasta} no quedó vacía: {erro}; borre a mano lo que sobró",
    },
    NaoRemoveu { caminho: String, erro: String } => {
        en: "could not remove {caminho}: {erro}",
        pt: "não foi possível remover {caminho}: {erro}",
        es: "no se pudo eliminar {caminho}: {erro}",
    },
    DadosApagados { pasta: String } => {
        en: "data deleted ({pasta})",
        pt: "dados apagados ({pasta})",
        es: "datos borrados ({pasta})",
    },
    DadosMantidos { pasta: String } => {
        en: "data kept in {pasta} (use --purge-data to delete)",
        pt: "dados mantidos em {pasta} (use --purge-data para apagar)",
        es: "datos conservados en {pasta} (use --purge-data para borrarlos)",
    },
    ServicoParadoPeloWindows => {
        en: "service stopped at Windows' request",
        pt: "serviço parado a pedido do Windows",
        es: "servicio detenido a petición de Windows",
    },
    ServicoParouComErro { erro: String } => {
        en: "service stopped with an error: {erro}",
        pt: "serviço parou com erro: {erro}",
        es: "el servicio se detuvo con un error: {erro}",
    },
    ServicoParouPorPanico => {
        en: "service stopped because of a panic",
        pt: "serviço parou por pânico",
        es: "el servicio se detuvo por un pánico",
    },
    ServicoIniciado { versao: String, pid: u32 } => {
        en: "service started (version {versao}, pid {pid})",
        pt: "serviço iniciado (versão {versao}, pid {pid})",
        es: "servicio iniciado (versión {versao}, pid {pid})",
    },
    ServicoForaDoScm { erro: String } => {
        en: "{erro}: `kioskd service` is called by Windows; to install use `kioskd install`",
        pt: "{erro}: `kioskd service` é chamado pelo Windows; para instalar use `kioskd install`",
        es: "{erro}: `kioskd service` lo invoca Windows; para instalar use `kioskd install`",
    },
    KioskdSemPasta => {
        en: "kioskd is not in a folder",
        pt: "o kioskd não está numa pasta",
        es: "kioskd no está en una carpeta",
    },

    // ---- kioskctl e cliente ----
    VarNaoDefinida { nome: String } => {
        en: "{nome} is not set",
        pt: "{nome} não definida",
        es: "{nome} no está definida",
    },
    ServicoRecusou { status: u16 } => {
        en: "the service refused the command (HTTP {status}). The reason is in the PC's log.",
        pt: "o serviço recusou o comando (HTTP {status}). O motivo fica no log do PC.",
        es: "el servicio rechazó el comando (HTTP {status}). El motivo queda en el registro del PC.",
    },
    CredenciaisJaExistem { caminho: String } => {
        en: "{caminho} already exists: delete the file to pair again",
        pt: "já existe {caminho}: apague o arquivo para parear de novo",
        es: "ya existe {caminho}: borre el archivo para emparejar de nuevo",
    },
    NomeDoDispositivo => {
        en: "Parents' device",
        pt: "Dispositivo dos pais",
        es: "Dispositivo de los padres",
    },
    DispositivoPareado { id: String } => {
        en: "device paired: {id}",
        pt: "dispositivo pareado: {id}",
        es: "dispositivo emparejado: {id}",
    },
    CredenciaisGravadas { caminho: String } => {
        en: "credentials saved to {caminho}",
        pt: "credenciais gravadas em {caminho}",
        es: "credenciales guardadas en {caminho}",
    },
    RegraCriadaPeloCliente { id: String } => {
        en: "rule created (pending): {id}",
        pt: "regra criada (pendente): {id}",
        es: "regla creada (pendiente): {id}",
    },
    RegraAtivadaCliente { id: String } => {
        en: "rule {id} activated",
        pt: "regra {id} ativada",
        es: "regla {id} activada",
    },
    RegraRevogadaCliente { id: String } => {
        en: "rule {id} revoked",
        pt: "regra {id} revogada",
        es: "regla {id} revocada",
    },
    LimiteDaRegraCliente { id: String, limite: String } => {
        en: "rule {id}: daily limit is now {limite}",
        pt: "regra {id}: limite diário agora é {limite}",
        es: "regla {id}: el límite diario ahora es {limite}",
    },
    UsoEm { data: String } => {
        en: "usage on {data}",
        pt: "uso em {data}",
        es: "uso el {data}",
    },
    SessaoAtiva { duracao: String } => {
        en: "  active session: {duracao}",
        pt: "  sessão ativa: {duracao}",
        es: "  sesión activa: {duracao}",
    },
    RespostaDeUsoIlegivel => {
        en: "unreadable usage response",
        pt: "resposta de uso ilegível",
        es: "respuesta de uso ilegible",
    },
    RespostaDeLogIlegivel => {
        en: "unreadable log response",
        pt: "resposta de log ilegível",
        es: "respuesta de registro ilegible",
    },
    NenhumEvento => {
        en: "no events in the period",
        pt: "nenhum evento no período",
        es: "ningún evento en el período",
    },
    ChavePrivadaIlegivel => {
        en: "unreadable private key in the credentials file",
        pt: "chave privada ilegível no arquivo de credenciais",
        es: "clave privada ilegible en el archivo de credenciales",
    },
    ChavePrivadaTamanho => {
        en: "private key has an invalid size",
        pt: "chave privada com tamanho inválido",
        es: "la clave privada tiene un tamaño no válido",
    },
    NaoLeu { caminho: String, erro: String } => {
        en: "could not read {caminho}: {erro}",
        pt: "não foi possível ler {caminho}: {erro}",
        es: "no se pudo leer {caminho}: {erro}",
    },
    CredenciaisInvalidas { caminho: String, erro: String } => {
        en: "credentials in {caminho} are invalid: {erro}",
        pt: "credenciais em {caminho} inválidas: {erro}",
        es: "las credenciales de {caminho} no son válidas: {erro}",
    },
    NaoGravou { caminho: String, erro: String } => {
        en: "could not write {caminho}: {erro}",
        pt: "não foi possível gravar {caminho}: {erro}",
        es: "no se pudo escribir {caminho}: {erro}",
    },
    PermissaoDe { caminho: String, erro: String } => {
        en: "permissions of {caminho}: {erro}",
        pt: "permissão de {caminho}: {erro}",
        es: "permisos de {caminho}: {erro}",
    },
    SemResposta { servidor: String, segundos: u64 } => {
        en: "no response from {servidor} within {segundos}s",
        pt: "sem resposta de {servidor} em {segundos}s",
        es: "sin respuesta de {servidor} en {segundos}s",
    },
    NaoConectou { servidor: String, erro: String } => {
        en: "could not connect to {servidor}: {erro}",
        pt: "não foi possível conectar a {servidor}: {erro}",
        es: "no se pudo conectar con {servidor}: {erro}",
    },
    FalhaAoLerResposta { erro: String } => {
        en: "failed to read the response: {erro}",
        pt: "falha ao ler a resposta: {erro}",
        es: "error al leer la respuesta: {erro}",
    },
    RespostaHttpIlegivel => {
        en: "unreadable HTTP response",
        pt: "resposta HTTP ilegível",
        es: "respuesta HTTP ilegible",
    },
    NaoPareado => {
        en: "this device has not been paired yet (use the `pair` command)",
        pt: "este dispositivo ainda não foi pareado (use o comando `pair`)",
        es: "este dispositivo aún no ha sido emparejado (use el comando `pair`)",
    },
    PareamentoRecusado { status: u16 } => {
        en: "pairing refused by the service (HTTP {status}). The code has 8 characters, is valid for 5 minutes and can be used only once; generate another on the PC with `kioskd pairing-code`, using the same KIOSK_DB as `kioskd serve`",
        pt: "pareamento recusado pelo serviço (HTTP {status}). O código tem 8 caracteres, vale por 5 minutos e só pode ser usado uma vez; gere outro no PC com `kioskd pairing-code`, usando o mesmo KIOSK_DB do `kioskd serve`",
        es: "emparejamiento rechazado por el servicio (HTTP {status}). El código tiene 8 caracteres, vale 5 minutos y solo puede usarse una vez; genere otro en el PC con `kioskd pairing-code`, usando el mismo KIOSK_DB que `kioskd serve`",
    },
    RespostaDePareamentoIlegivel => {
        en: "unreadable pairing response",
        pt: "resposta de pareamento ilegível",
        es: "respuesta de emparejamiento ilegible",
    },
    RespostaSemIdDoDispositivo => {
        en: "pairing response without the device identifier",
        pt: "resposta de pareamento sem identificador do dispositivo",
        es: "respuesta de emparejamiento sin el identificador del dispositivo",
    },

    // ---- motivos de restrição, como o adulto os lê no log ----
    MotivoAppBloqueado => {
        en: "app blocked",
        pt: "app bloqueado",
        es: "app bloqueada",
    },
    MotivoLimiteDoApp => {
        en: "app daily limit",
        pt: "limite diário do app",
        es: "límite diario de la app",
    },
    MotivoLimiteTotal => {
        en: "total daily limit",
        pt: "limite diário total",
        es: "límite diario total",
    },
    MotivoForaDaJanelaDoApp => {
        en: "outside the app's time window",
        pt: "fora do horário do app",
        es: "fuera del horario de la app",
    },
    MotivoForaDaJanelaGeral => {
        en: "outside the allowed hours",
        pt: "fora do horário permitido",
        es: "fuera del horario permitido",
    },

    // ---- detalhe do log forense, lido pelo adulto (ver `detalhes`) ----
    LogAvisoExibido { marco: u64, restante: u64, motivo: String, app: String } => {
        en: "{marco}s warning shown: {restante}s left ({motivo}){app}",
        pt: "aviso de {marco}s exibido: restam {restante}s ({motivo}){app}",
        es: "aviso de {marco}s mostrado: quedan {restante}s ({motivo}){app}",
    },
    LogAvisoFalhou { motivo: String, erro: String } => {
        en: "warning not shown ({motivo}): {erro}",
        pt: "aviso não exibido ({motivo}): {erro}",
        es: "aviso no mostrado ({motivo}): {erro}",
    },
    LogAppEncerrado { processo: String, pid: u32, motivo: String } => {
        en: "app {processo} (pid {pid}) terminated: {motivo}",
        pt: "app {processo} (pid {pid}) encerrado: {motivo}",
        es: "app {processo} (pid {pid}) cerrada: {motivo}",
    },
    LogFalhaAoEncerrar { processo: String, pid: u32, tentativa: u32, erro: String } => {
        en: "failed to terminate {processo} (pid {pid}), attempt {tentativa}: {erro}",
        pt: "falha ao encerrar {processo} (pid {pid}), tentativa {tentativa}: {erro}",
        es: "error al cerrar {processo} (pid {pid}), intento {tentativa}: {erro}",
    },
    LogNaoEncerra { processo: String, pid: u32, tentativas: u32 } => {
        en: "process {processo} (pid {pid}) does not terminate after {tentativas} attempts",
        pt: "processo {processo} (pid {pid}) não encerra após {tentativas} tentativas",
        es: "el proceso {processo} (pid {pid}) no se cierra tras {tentativas} intentos",
    },
    LogSessaoBloqueada { motivo: String } => {
        en: "session locked: {motivo}",
        pt: "sessão bloqueada: {motivo}",
        es: "sesión bloqueada: {motivo}",
    },
    LogSessaoEncerrada { bloqueios: u32, motivo: String } => {
        en: "session logged off after {bloqueios} locks were undone: {motivo}",
        pt: "sessão encerrada (logoff) após {bloqueios} bloqueios desfeitos: {motivo}",
        es: "sesión cerrada (logoff) tras deshacerse {bloqueios} bloqueos: {motivo}",
    },
    LogSessaoNaoBloqueada { motivo: String, erro: String } => {
        en: "session not locked ({motivo}): {erro}",
        pt: "sessão não bloqueada ({motivo}): {erro}",
        es: "sesión no bloqueada ({motivo}): {erro}",
    },
    LogSessaoNaoEncerrada { motivo: String, erro: String } => {
        en: "session not logged off ({motivo}): {erro}",
        pt: "sessão não encerrada ({motivo}): {erro}",
        es: "sesión no cerrada ({motivo}): {erro}",
    },
    LogIntervaloSuspeito { inicio: String, fim: String, duracao: i64, max: i64 } => {
        en: "suspicious usage interval: {inicio} -> {fim} ({duracao}s; accepted 0..={max}s)",
        pt: "intervalo de uso suspeito: {inicio} -> {fim} ({duracao}s; aceito 0..={max}s)",
        es: "intervalo de uso sospechoso: {inicio} -> {fim} ({duracao}s; aceptado 0..={max}s)",
    },
    LogAgenteParado { inicio: String, fim: String, duracao: String } => {
        en: "enforcement was off for {duracao} ({inicio} -> {fim}): the service was stopped normally",
        pt: "o controle ficou desligado por {duracao} ({inicio} -> {fim}): o serviço foi parado normalmente",
        es: "el control estuvo apagado durante {duracao} ({inicio} -> {fim}): el servicio se detuvo normalmente",
    },
    LogAgenteSumiu { inicio: String, fim: String, duracao: String } => {
        en: "enforcement was off for {duracao} ({inicio} -> {fim}): the agent disappeared without a normal stop (crash, killed or power loss)",
        pt: "o controle ficou desligado por {duracao} ({inicio} -> {fim}): o agente sumiu sem parada normal (falha, encerrado à força ou falta de energia)",
        es: "el control estuvo apagado durante {duracao} ({inicio} -> {fim}): el agente desapareció sin una parada normal (fallo, cierre forzado o corte de energía)",
    },
    LogBatimentoNaoGravado { erro: String } => {
        en: "agent heartbeat not recorded: {erro}",
        pt: "batimento do agente não gravado: {erro}",
        es: "latido del agente no registrado: {erro}",
    },
    LogUiParada { duracao: String } => {
        en: "the tray stopped responding ({duracao} without a heartbeat): it may have been closed",
        pt: "a bandeja parou de dar sinal ({duracao} sem batimento): pode ter sido fechada",
        es: "la bandeja dejó de responder ({duracao} sin latido): puede haberse cerrado",
    },
    LogUiNaoIniciada { erro: String } => {
        en: "could not start the tray in the child's session: {erro}",
        pt: "não foi possível iniciar a bandeja na sessão da criança: {erro}",
        es: "no se pudo iniciar la bandeja en la sesión del niño: {erro}",
    },
    LogUiRecusada { motivo: String } => {
        en: "tray connection refused: {motivo}",
        pt: "conexão da bandeja recusada: {motivo}",
        es: "conexión de la bandeja rechazada: {motivo}",
    },
    UiMotivoOutroPrograma => {
        en: "the program is not the installed kiosk-ui",
        pt: "o programa não é o kiosk-ui instalado",
        es: "el programa no es el kiosk-ui instalado",
    },
    UiMotivoMensagemInvalida => {
        en: "invalid message",
        pt: "mensagem inválida",
        es: "mensaje no válido",
    },
    UiMotivoExcessoDeMensagens => {
        en: "too many messages",
        pt: "mensagens demais",
        es: "demasiados mensajes",
    },
    LogRetencao { removidos: u64 } => {
        en: "retention: {removidos} events older than 90 days removed",
        pt: "retenção: {removidos} eventos com mais de 90 dias removidos",
        es: "retención: {removidos} eventos de más de 90 días eliminados",
    },
    LogTentativaSemPrivilegio { comando: String, quem: String, extra: String } => {
        en: "`{comando}` needs an elevated administrator; attempted by {quem}{extra}",
        pt: "`{comando}` exige administrador elevado; tentativa de {quem}{extra}",
        es: "`{comando}` exige un administrador elevado; intento de {quem}{extra}",
    },
    QuemContaComum => {
        en: "a standard account",
        pt: "uma conta comum",
        es: "una cuenta estándar",
    },
    QuemAdminSemElevar => {
        en: "an administrator account that was not elevated",
        pt: "uma conta de administrador sem elevação",
        es: "una cuenta de administrador sin elevar",
    },
    ExtraSuprimidas { n: u64 } => {
        en: " (+{n} more attempts in the last minute were not recorded)",
        pt: " (+{n} tentativas no último minuto não foram registradas)",
        es: " (+{n} intentos en el último minuto no se registraron)",
    },
    LogCodigoSemAdmin => {
        en: "pairing code requested without an elevated administrator",
        pt: "código de pareamento pedido sem administrador elevado",
        es: "código de emparejamiento solicitado sin administrador elevado",
    },
    LogFalhaListarProcessos { erro: String } => {
        en: "failed to list processes: {erro}",
        pt: "falha ao listar processos: {erro}",
        es: "error al listar los procesos: {erro}",
    },
    LogFalhaListarSessoes { erro: String } => {
        en: "failed to list sessions: {erro}",
        pt: "falha ao listar sessões: {erro}",
        es: "error al listar las sesiones: {erro}",
    },
    LogUsoNaoContabilizado { erro: String } => {
        en: "usage not recorded: {erro}",
        pt: "uso não contabilizado: {erro}",
        es: "uso no contabilizado: {erro}",
    },
    LogSessaoNaoContabilizada { erro: String } => {
        en: "session time not recorded: {erro}",
        pt: "tempo de sessão não contabilizado: {erro}",
        es: "tiempo de sesión no contabilizado: {erro}",
    },
    LogDecisaoDaSessaoFalhou { erro: String } => {
        en: "session decision failed: {erro}",
        pt: "decisão de sessão falhou: {erro}",
        es: "la decisión de sesión falló: {erro}",
    },
    LogDecisaoDoAppFalhou { erro: String } => {
        en: "app decision failed: {erro}",
        pt: "decisão de app falhou: {erro}",
        es: "la decisión de app falló: {erro}",
    },
    LogPerfilNaoConsultado { erro: String } => {
        en: "profile not queried: {erro}",
        pt: "perfil não consultado: {erro}",
        es: "perfil no consultado: {erro}",
    },
    LogAppNaoConsultado { erro: String } => {
        en: "app not queried: {erro}",
        pt: "app não consultado: {erro}",
        es: "app no consultada: {erro}",
    },
    LogEventoNaoGravado { tipo: String, erro: String } => {
        en: "event {tipo} not recorded: {erro}",
        pt: "evento {tipo} não gravado: {erro}",
        es: "evento {tipo} no registrado: {erro}",
    },
}

impl fmt::Display for Msg {
    /// No idioma deste processo.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.em(idioma()))
    }
}
