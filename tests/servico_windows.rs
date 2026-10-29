//! Serviço do Windows de ponta a ponta (ADR-008, etapa 1): instala, confere, derruba o processo
//! para ver o reinício, prova que uma conta padrão não para o serviço nem lê os dados, reinstala
//! e desinstala.
//!
//! Mexe na máquina de verdade (`%ProgramFiles%\kiosk`, `%ProgramData%\kiosk`, serviço `kiosk`,
//! uma conta local temporária), então só roda com `KIOSK_TESTE_SERVICO=1`, que o CI define no
//! runner Windows (descartável, com administrador). Num PC de uso, não defina.
#![cfg(windows)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, HANDLE, LocalFree};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows_sys::Win32::Security::{
    DACL_SECURITY_INFORMATION, ImpersonateLoggedOnUser, LOGON32_LOGON_INTERACTIVE,
    LOGON32_PROVIDER_DEFAULT, LogonUserW, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    RevertToSelf,
};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT, SERVICE_QUERY_STATUS,
    SERVICE_START, SERVICE_STOP,
};

const PORTA: u16 = 18443;

fn kioskd(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kioskd"))
        .args(args)
        .env("KIOSK_LANG", "pt-BR")
        .env_remove("KIOSK_DB")
        .env_remove("KIOSK_LISTEN")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn texto(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn ok(args: &[&str]) -> String {
    let o = kioskd(args);
    let t = texto(&o);
    assert!(o.status.success(), "kioskd {args:?} falhou:\n{t}");
    t
}

fn pasta(var: &str) -> PathBuf {
    PathBuf::from(std::env::var(var).unwrap()).join("kiosk")
}

/// PID do serviço em execução, segundo `kioskd estado`.
fn pid_em_execucao() -> Option<u32> {
    let t = ok(&["estado"]);
    let linha = t.lines().find(|l| l.starts_with("estado: em execução"))?;
    linha
        .split("pid ")
        .nth(1)?
        .trim_end_matches(')')
        .parse()
        .ok()
}

fn esperar<T>(limite: Duration, mut f: impl FnMut() -> Option<T>) -> Option<T> {
    let fim = Instant::now() + limite;
    while Instant::now() < fim {
        if let Some(v) = f() {
            return Some(v);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    None
}

/// `POST /api/pair` com corpo inválido: o serviço precisa recusar (403) e gravar no banco.
fn status_do_pareamento() -> Option<String> {
    let mut s = TcpStream::connect(("127.0.0.1", PORTA)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let req = "POST /api/pair HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}";
    s.write_all(req.as_bytes()).ok()?;
    let mut resp = String::new();
    s.read_to_string(&mut resp).ok()?;
    Some(resp)
}

/// Dono e DACL do caminho em SDDL, lidos do próprio Windows (sem PowerShell: no runner, o
/// `PSModulePath` do pwsh 7 quebra o `Get-Acl` do Windows PowerShell).
fn sddl_de(caminho: &std::path::Path) -> String {
    let nome = w(&caminho.to_string_lossy());
    let info = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: nome terminado em zero; `sd` é liberado com LocalFree.
    let erro = unsafe {
        GetNamedSecurityInfoW(
            nome.as_ptr(),
            SE_FILE_OBJECT,
            info,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sd,
        )
    };
    assert_eq!(erro, 0, "GetNamedSecurityInfoW: {erro}");
    let mut texto: *mut u16 = null_mut();
    // SAFETY: descritor válido; `texto` é liberado com LocalFree.
    let ok = unsafe {
        ConvertSecurityDescriptorToStringSecurityDescriptorW(
            sd,
            SDDL_REVISION_1,
            info,
            &mut texto,
            null_mut(),
        )
    };
    assert!(ok != 0, "{}", std::io::Error::last_os_error());
    let mut n = 0;
    // SAFETY: string terminada em zero devolvida pelo Windows.
    while unsafe { *texto.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: `n` u16 válidos; depois os dois blocos são liberados.
    let sddl = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(texto, n) });
    unsafe {
        LocalFree(texto.cast());
        LocalFree(sd);
    }
    sddl
}

/// Entradas não vazias do PATH do sistema, lidas do registro pelo `reg query` (fonte
/// independente do código que as grava).
fn path_do_sistema() -> Vec<String> {
    let o = Command::new("reg")
        .args([
            "query",
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            "/v",
            "Path",
        ])
        .output()
        .unwrap();
    let t = String::from_utf8_lossy(&o.stdout).into_owned();
    let linha = t
        .lines()
        .find(|l| l.trim_start().starts_with("Path"))
        .unwrap_or_else(|| panic!("reg query sem Path: {t}"));
    let valor = linha
        .split_once("REG_EXPAND_SZ")
        .or_else(|| linha.split_once("REG_SZ"))
        .unwrap()
        .1
        .trim();
    valor
        .split(';')
        .filter(|e| !e.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Conta local padrão (grupo Usuários), apagada ao sair.
struct ContaPadrao {
    nome: String,
    senha: String,
}

impl ContaPadrao {
    fn criar() -> Self {
        let nome = "kioskteste".to_string();
        // gerada a cada execução; a conta só existe durante o teste. Até 14 caracteres: acima
        // disso o `net user` pergunta S/N e travaria o teste.
        let senha = format!("Kt9-{}", &uuid::Uuid::new_v4().simple().to_string()[..10]);
        let _ = Command::new("net")
            .args(["user", &nome, "/delete"])
            .output();
        let o = Command::new("net")
            .args(["user", &nome, &senha, "/add"])
            .output()
            .unwrap();
        assert!(o.status.success(), "net user /add: {}", texto(&o));
        Self { nome, senha }
    }

    fn token(&self) -> HANDLE {
        let (nome, ponto, senha) = (w(&self.nome), w("."), w(&self.senha));
        let mut token: HANDLE = null_mut();
        // SAFETY: textos terminados em zero; o token é fechado por quem chama.
        let ok = unsafe {
            LogonUserW(
                nome.as_ptr(),
                ponto.as_ptr(),
                senha.as_ptr(),
                LOGON32_LOGON_INTERACTIVE,
                LOGON32_PROVIDER_DEFAULT,
                &mut token,
            )
        };
        assert!(ok != 0, "LogonUserW: {}", std::io::Error::last_os_error());
        token
    }
}

impl Drop for ContaPadrao {
    fn drop(&mut self) {
        let _ = Command::new("net")
            .args(["user", &self.nome, "/delete"])
            .output();
    }
}

/// Roda `f` na thread atual como a conta padrão.
fn como<T>(conta: &ContaPadrao, f: impl FnOnce() -> T) -> T {
    struct Volta(HANDLE);
    impl Drop for Volta {
        fn drop(&mut self) {
            // SAFETY: desfaz a personificação da thread e fecha o token.
            unsafe {
                RevertToSelf();
                CloseHandle(self.0);
            }
        }
    }
    let token = conta.token();
    let _volta = Volta(token);
    // SAFETY: token válido de LogonUserW.
    assert!(unsafe { ImpersonateLoggedOnUser(token) } != 0);
    f()
}

/// Abre o serviço com `acesso`; `Err(código)` se negado.
fn abrir_servico(acesso: u32) -> Result<(), i32> {
    // SAFETY: máquina local; handles fechados abaixo.
    unsafe {
        let scm = OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(-1));
        }
        let nome = w("kiosk");
        let svc = OpenServiceW(scm, nome.as_ptr(), acesso);
        let erro = std::io::Error::last_os_error().raw_os_error().unwrap_or(-1);
        CloseServiceHandle(scm);
        if svc.is_null() {
            return Err(erro);
        }
        CloseServiceHandle(svc);
        Ok(())
    }
}

#[test]
fn instala_reinicia_recusa_conta_padrao_e_desinstala() {
    if std::env::var("KIOSK_TESTE_SERVICO").as_deref() != Ok("1") {
        eprintln!("pulado: defina KIOSK_TESTE_SERVICO=1 (só em máquina descartável)");
        return;
    }
    let (programa, dados) = (pasta("ProgramFiles"), pasta("ProgramData"));
    let _ = kioskd(&["desinstalar"]);
    let path_antes = path_do_sistema();
    assert_eq!(kiosk_engine::servico_windows::vezes_no_path(), Ok(0));

    // controles negativos antes de tocar em nada: escuta fora da ADR-005 e sem porta
    for ruim in ["0.0.0.0:8443", "192.168.0.10:8443", "127.0.0.1"] {
        let o = kioskd(&["instalar", "--escutar", ruim]);
        assert!(!o.status.success(), "aceitou {ruim}");
    }
    assert!(ok(&["estado"]).contains("não instalado"));

    // instala e confere o que o Windows diz (em inglês, a convenção; o resto do teste mistura)
    let saida = ok(&["install", "--listen", &format!("127.0.0.1:{PORTA}")]);
    assert!(saida.contains("serviço em execução"), "{saida}");
    let versao = kiosk_engine::versao::atual().curta();
    assert!(
        saida.contains(&format!("versão instalada {versao}")),
        "{saida}"
    );
    let estado = ok(&["status"]);
    // este executável e o instalado (uma cópia dele) dizem a mesma versão
    assert!(estado.contains(&format!("versão: {versao}")), "{estado}");
    assert!(
        estado.contains(&format!("versão instalada: {versao}")),
        "{estado}"
    );
    assert!(estado.contains("início: automático"), "{estado}");
    assert!(estado.contains("conta: LocalSystem"), "{estado}");
    assert!(
        estado.contains(&format!("ambiente: KIOSK_LISTEN=127.0.0.1:{PORTA}")),
        "{estado}"
    );
    assert!(estado.contains(r"\kiosk\kioskd.exe"), "{estado}");
    // o serviço (SYSTEM) herda o idioma de quem o instalou e o nome de exibição sai nele
    assert!(estado.contains("ambiente: KIOSK_LANG=pt-BR"), "{estado}");
    let qc = Command::new("sc").args(["qc", "kiosk"]).output().unwrap();
    assert!(
        texto(&qc).contains("Kiosk (controle parental)"),
        "{}",
        texto(&qc)
    );
    // registrado com o nome em inglês; a 0.0.4 usava `servico`, que segue valendo como equivalente
    assert!(estado.contains(r#"kioskd.exe" service"#), "{estado}");
    // a pasta instalada entra no PATH do sistema uma vez, e as outras entradas ficam
    assert!(estado.contains("PATH do sistema: sim"), "{estado}");
    assert_eq!(kiosk_engine::servico_windows::vezes_no_path(), Ok(1));
    let path_instalado = path_do_sistema();
    assert!(
        path_antes.iter().all(|e| path_instalado.contains(e)),
        "o instalar perdeu entradas do PATH"
    );
    assert!(programa.join("kioskd.exe").exists() && programa.join("kioskctl.exe").exists());
    let sc = Command::new("sc")
        .args(["qfailure", "kiosk"])
        .output()
        .unwrap();
    assert!(texto(&sc).contains("RESTART"), "{}", texto(&sc));

    // a API responde como serviço e o banco (gravado como SYSTEM) recebe a recusa
    let resp = esperar(Duration::from_secs(15), status_do_pareamento).expect("API não respondeu");
    assert!(resp.starts_with("HTTP/1.1 403"), "{resp}");
    assert!(resp.contains(r#""status":"rejected""#), "{resp}");
    assert!(dados.join("kiosk.db").exists());
    assert!(
        std::fs::read_to_string(dados.join("kioskd.log"))
            .unwrap()
            .contains("escutando em 127.0.0.1"),
    );

    // pasta de dados: protegida e sem herança de %ProgramData%
    let sddl = sddl_de(&dados);
    assert!(sddl.starts_with("O:BA"), "dono: {sddl}");
    assert!(sddl.contains("D:P"), "herança não cortada: {sddl}");
    let donos: Vec<&str> = sddl
        .split('(')
        .skip(1)
        .map(|ace| ace.trim_end_matches(')').rsplit(';').next().unwrap())
        .collect();
    assert!(
        donos.iter().all(|s| *s == "SY" || *s == "BA"),
        "ACE para outra conta: {sddl}"
    );

    // conta padrão: consulta o serviço (controle positivo), mas não para, não inicia e não lê os dados
    let conta = ContaPadrao::criar();
    let (consulta, parada, inicio, leitura, programa_legivel, elevado) = como(&conta, || {
        (
            abrir_servico(SERVICE_QUERY_STATUS),
            abrir_servico(SERVICE_STOP),
            abrir_servico(SERVICE_START),
            std::fs::read_dir(&dados).map(|_| ()),
            std::fs::metadata(programa.join("kioskd.exe")).is_ok(),
            kiosk_engine::sistema_windows::administrador_elevado(),
        )
    });
    // a conta padrão não é administrador: `kioskd codigo-pareamento` a recusaria
    assert_eq!(elevado, Ok(false), "conta padrão vista como administrador");
    assert_eq!(
        consulta,
        Ok(()),
        "a conta padrão nem consulta: logon falhou?"
    );
    assert_eq!(
        parada,
        Err(ERROR_ACCESS_DENIED as i32),
        "conta padrão pode parar o serviço"
    );
    assert_eq!(inicio, Err(ERROR_ACCESS_DENIED as i32));
    assert_eq!(
        leitura.unwrap_err().kind(),
        std::io::ErrorKind::PermissionDenied,
        "conta padrão lê a pasta de dados"
    );
    assert!(
        programa_legivel,
        "controle positivo: a conta padrão enxerga Program Files"
    );
    // o administrador (quem roda o teste) pode
    assert_eq!(abrir_servico(SERVICE_STOP), Ok(()));

    tentativas_sem_privilegio_chegam_ao_log(&conta, &dados);
    drop(conta);

    // derrubar o processo: o Windows reinicia o serviço sozinho
    let pid = pid_em_execucao().expect("serviço não está em execução");
    let kill = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output()
        .unwrap();
    assert!(kill.status.success(), "taskkill: {}", texto(&kill));
    let novo = esperar(Duration::from_secs(40), || {
        pid_em_execucao().filter(|p| *p != pid)
    })
    .expect("o serviço não voltou depois de derrubado");
    assert_ne!(novo, pid);
    esperar(Duration::from_secs(15), status_do_pareamento).expect("API não voltou");

    // reinstalar atualiza sem duplicar (em português: as duas línguas valem, ADR-010)
    ok(&["instalar", "--escutar", &format!("127.0.0.1:{PORTA}")]);
    assert!(pid_em_execucao().is_some());
    assert_eq!(
        kiosk_engine::servico_windows::vezes_no_path(),
        Ok(1),
        "reinstalar duplicou a entrada do PATH"
    );

    // desinstalar sem --apagar-dados mantém os dados
    let saida = ok(&["uninstall"]);
    assert!(saida.contains("parado e removido"), "{saida}");
    assert!(
        esperar(Duration::from_secs(15), || ok(&["estado"])
            .contains("não instalado")
            .then_some(()))
        .is_some()
    );
    assert!(!programa.exists(), "sobrou {}", programa.display());
    assert_eq!(
        path_do_sistema(),
        path_antes,
        "o PATH do sistema não voltou ao que era"
    );
    assert!(dados.join("kiosk.db").exists(), "apagou os dados sem pedir");
    assert!(TcpStream::connect(("127.0.0.1", PORTA)).is_err());

    // --apagar-dados exige digitar APAGAR
    let mut filho = Command::new(env!("CARGO_BIN_EXE_kioskd"))
        .args(["desinstalar", "--apagar-dados"])
        .env("KIOSK_LANG", "pt-BR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    filho.stdin.take().unwrap().write_all(b"sim\n").unwrap();
    let o = filho.wait_with_output().unwrap();
    assert!(!o.status.success(), "apagou sem confirmação: {}", texto(&o));
    assert!(dados.exists());

    let mut filho = Command::new(env!("CARGO_BIN_EXE_kioskd"))
        .args(["uninstall", "--purge-data"])
        .env("KIOSK_LANG", "pt-BR")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    filho.stdin.take().unwrap().write_all(b"APAGAR\n").unwrap();
    let o = filho.wait_with_output().unwrap();
    assert!(o.status.success(), "{}", texto(&o));
    assert!(!dados.exists(), "dados não apagados");
}

#[test]
fn servico_rodado_a_mao_explica_e_falha() {
    let o = kioskd(&["servico"]);
    assert!(!o.status.success());
    assert!(texto(&o).contains("kioskd install"), "{}", texto(&o));
}

/// ADR-011: a conta padrão não abre o banco, mas o serviço (SYSTEM) grava a tentativa dela, e o
/// chamador vem do token da conexão, nunca da mensagem.
fn tentativas_sem_privilegio_chegam_ao_log(conta: &ContaPadrao, dados: &std::path::Path) {
    use kiosk_engine::auditoria::NOME_DO_PIPE;
    // a thread do aviso real não herda a personificação do teste: usa a variante da thread atual
    use kiosk_engine::auditoria_windows::avisar_tentativa_nesta_thread as avisar_tentativa;
    use kiosk_engine::detalhes::Detalhe;
    use kiosk_engine::sistema_windows::sid_de_usuario;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let db_url = format!(
        "sqlite://{}",
        dados
            .join("kiosk.db")
            .display()
            .to_string()
            .replace('\\', "/")
    );
    // eventos de enforcement e de falha que a auditoria grava: (tipo, ação, perfil, detalhe)
    let eventos = || -> Vec<(String, String, Option<String>, String)> {
        rt.block_on(async {
            let db = kiosk_engine::db_manager::KioskDbManager::new(&db_url).await.unwrap();
            sqlx::query_as(
                "SELECT event_type, action_attempted, profile_id, rejection_reason_detail \
                 FROM security_log WHERE event_type IN ('AUTHZ_FAIL', 'VALIDATION_FAIL') ORDER BY rowid",
            )
            .fetch_all(db.pool())
            .await
            .unwrap()
        })
    };
    let sid = sid_de_usuario(&conta.nome).expect("SID da conta de teste");
    let antes = eventos().len();
    // o que o Windows diz dos grupos da conta, para as mensagens de falha abaixo
    let grupos = como(conta, grupos_da_thread);

    // o serviço abre o pipe logo depois de subir
    assert!(
        esperar(Duration::from_secs(20), || std::path::Path::new(
            NOME_DO_PIPE
        )
        .exists()
        .then_some(()))
        .is_some(),
        "o serviço não abriu o pipe de auditoria"
    );

    // 0. uma sondagem (conecta e fecha sem enviar nada, como o `Test-Path` do roteiro do dono) não é
    //    erro do serviço: nada no log, e o pipe segue atendendo
    use std::os::windows::fs::OpenOptionsExt as _;
    drop(
        std::fs::OpenOptions::new()
            .access_mode(kiosk_engine::auditoria_windows::DIREITOS_DO_CLIENTE)
            .open(NOME_DO_PIPE)
            .expect("a sondagem devia conectar"),
    );
    std::thread::sleep(Duration::from_millis(1500));
    let log_do_servico =
        std::fs::read_to_string(pasta("ProgramData").join("kioskd.log")).expect("kioskd.log");
    assert!(
        !log_do_servico.contains("ImpersonateNamedPipeClient"),
        "a sondagem vazia virou erro no log: {log_do_servico}"
    );

    // 1. mensagem forjada: tenta se passar por outra conta; é recusada e o serviço responde ok:false
    let forjada = como(conta, || -> std::io::Result<String> {
        use std::os::windows::fs::OpenOptionsExt;
        let mut f = std::fs::OpenOptions::new()
            .access_mode(kiosk_engine::auditoria_windows::DIREITOS_DO_CLIENTE)
            .open(NOME_DO_PIPE)?;
        f.write_all(
            b"{\"v\":1,\"tipo\":\"tentativa_sem_privilegio\",\"comando\":\"pairing-code\",\"sid\":\"S-1-5-18\"}\n",
        )?;
        let mut r = String::new();
        let mut b = [0u8; 1];
        while f.read(&mut b)? == 1 && b[0] != b'\n' {
            r.push(b[0] as char);
        }
        Ok(r)
    })
    .expect("a conta padrão não conseguiu conectar ao pipe");
    assert!(
        !kiosk_engine::auditoria::resposta_ok(&forjada),
        "aceitou mensagem forjada: {forjada}"
    );

    // 2. aviso válido: o serviço grava, e o SID é o do TOKEN (a conta de teste), sem perfil
    let r = como(conta, || avisar_tentativa("pairing-code"));
    assert_eq!(r, Ok(()), "o serviço não confirmou a gravação");
    let e = eventos();
    let ultimo = e.last().unwrap();
    assert_eq!(
        (ultimo.0.as_str(), ultimo.1.as_str(), ultimo.2.as_deref()),
        ("AUTHZ_FAIL", "CREATE_PAIRING_CODE", None)
    );
    match Detalhe::de_texto(&ultimo.3).expect("detalhe em código") {
        Detalhe::AdminRequired {
            command,
            admin_account,
            caller_sid,
            ..
        } => {
            assert_eq!(command, "pairing-code");
            assert!(
                !admin_account,
                "conta padrão marcada como administrador; grupos da conta: {grupos}; SID esperado {sid}; \
                 detalhe gravado: {}",
                ultimo.3
            );
            assert_eq!(
                caller_sid.as_deref(),
                Some(sid.as_str()),
                "o SID tem de ser o do chamador"
            );
        }
        outro => panic!("{outro:?}"),
    }
    assert!(
        !e[antes..].iter().any(|x| x.3.contains("S-1-5-18")),
        "o SID forjado entrou no log"
    );
    assert!(
        e[antes..].iter().any(|x| x.0 == "VALIDATION_FAIL"),
        "a mensagem forjada não virou VALIDATION_FAIL"
    );

    // 3. com perfil cadastrado, o evento leva o perfil e deixa o SID de fora
    let perfil = kioskd(&["profile", "create", "--sid", &sid, "--name", "Teste"]);
    assert!(perfil.status.success(), "{}", texto(&perfil));
    let id_do_perfil = texto(&perfil)
        .split_whitespace()
        .find(|p| uuid::Uuid::parse_str(p).is_ok())
        .expect("id do perfil")
        .to_owned();
    let confirmou = como(conta, || avisar_tentativa("rule create"));
    assert_eq!(
        confirmou,
        Ok(()),
        "log do serviço: {}",
        std::fs::read_to_string(pasta("ProgramData").join("kioskd.log"))
            .map(|l| l.lines().rev().take(8).collect::<Vec<_>>().join(" | "))
            .unwrap_or_else(|e| e.to_string())
    );
    let e = eventos();
    let ultimo = e.last().unwrap();
    assert_eq!(
        (ultimo.1.as_str(), ultimo.2.as_deref()),
        ("ADD_RULE", Some(id_do_perfil.as_str()))
    );
    assert!(
        !ultimo.3.contains(&sid),
        "o SID não precisa estar no detalhe: {}",
        ultimo.3
    );

    // 4. inundação: já foram 3 avisos deste minuto (o forjado conta); só mais 2 viram evento
    let antes_da_inundacao = eventos().len();
    for _ in 0..10 {
        assert_eq!(
            como(conta, || avisar_tentativa("devices")),
            Ok(()),
            "descartado também confirma"
        );
    }
    assert_eq!(
        eventos().len() - antes_da_inundacao,
        2,
        "o limite de 5 avisos por minuto não valeu"
    );

    // 5. o `kioskd` inteiro, rodando como a conta padrão: recusa antes de abrir qualquer coisa e
    //    diz se registrou. `Start-Process -Credential` fica pendurado nos runners do GitHub (sem
    //    saída em 3 minutos), então este passo só roda a pedido: KIOSK_TESTE_PROCESSO_COMO_CONTA=1
    //    num PC real. No CI, o caminho do `kioskd` é coberto pela chamada do aviso nos passos 2-4.
    if std::env::var_os("KIOSK_TESTE_PROCESSO_COMO_CONTA").is_none() {
        return;
    }
    let antes_da_cli = eventos().len();
    let exe = pasta("ProgramFiles").join("kioskd.exe");
    let (codigo, _, saida_do_kioskd, ps) =
        rodar_como(conta, &exe.display().to_string(), &["rule", "list"]);
    assert_eq!(
        codigo,
        Some(1),
        "o kioskd da conta padrão devia recusar (exit 1): ps={ps} erro={saida_do_kioskd}"
    );
    assert!(saida_do_kioskd.contains("`rule list`"), "{saida_do_kioskd}");
    assert!(
        !saida_do_kioskd.contains("NOT recorded")
            && !saida_do_kioskd.contains("NÃO foi")
            && !saida_do_kioskd.contains("NO quedó"),
        "o serviço estava no ar, devia ter registrado: {saida_do_kioskd}"
    );
    assert!(
        !saida_do_kioskd.contains("database"),
        "abriu o banco antes de recusar: {saida_do_kioskd}"
    );
    let e = eventos();
    // a cota do minuto pode ter acabado (5 avisos): se o evento não entrou, a cota explica; senão confere
    if e.len() > antes_da_cli {
        let ultimo = e.last().unwrap();
        assert_eq!(ultimo.2.as_deref(), Some(id_do_perfil.as_str()));
        assert!(ultimo.3.contains("rule list"), "{}", ultimo.3);
    }
}

/// Grupos do token da thread atual (SID e atributos), para as mensagens de falha.
fn grupos_da_thread() -> String {
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{
        GetTokenInformation, TOKEN_GROUPS, TOKEN_QUERY, TokenGroups,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken,
    };
    let mut token: HANDLE = null_mut();
    // SAFETY: handles pseudo do processo; o token é fechado ao fim.
    let (de_thread, aberto) = unsafe {
        if OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) != 0 {
            ("thread", true)
        } else if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) != 0 {
            ("processo", true)
        } else {
            ("nenhum", false)
        }
    };
    if !aberto {
        return "token ilegível".into();
    }
    let mut n = 0u32;
    // SAFETY: primeira chamada só mede o buffer.
    unsafe { GetTokenInformation(token, TokenGroups, null_mut(), 0, &mut n) };
    let mut buf = vec![0u64; (n as usize).div_ceil(8)];
    // SAFETY: buffer alinhado com `n` bytes.
    let lido =
        unsafe { GetTokenInformation(token, TokenGroups, buf.as_mut_ptr().cast(), n, &mut n) };
    let mut saida = format!("[token da {de_thread}]");
    if lido != 0 {
        // SAFETY: preenchido acima.
        let g = unsafe { &*(buf.as_ptr() as *const TOKEN_GROUPS) };
        let grupos =
            unsafe { std::slice::from_raw_parts(g.Groups.as_ptr(), g.GroupCount as usize) };
        for e in grupos {
            let mut t: *mut u16 = null_mut();
            // SAFETY: SID válido; `t` é liberado com LocalFree.
            if unsafe { ConvertSidToStringSidW(e.Sid, &mut t) } != 0 {
                let mut len = 0;
                while unsafe { *t.add(len) } != 0 {
                    len += 1;
                }
                let sid = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(t, len) });
                unsafe { LocalFree(t.cast()) };
                saida.push_str(&format!(" {sid}=0x{:x}", e.Attributes));
            }
        }
    }
    // SAFETY: fecha o token aberto acima.
    unsafe { CloseHandle(token) };
    saida
}

/// Roda `exe` como a conta de teste (`Start-Process -Credential`), com a saída em arquivos que a
/// conta consegue escrever. Devolve (código de saída, stdout, stderr, texto do PowerShell).
fn rodar_como(
    conta: &ContaPadrao,
    exe: &str,
    args: &[&str],
) -> (Option<i32>, String, String, String) {
    let saida = PathBuf::from(r"C:\Users\Public\kiosk-teste-saida.txt");
    let erro = PathBuf::from(r"C:\Users\Public\kiosk-teste-erro.txt");
    let _ = (std::fs::remove_file(&saida), std::fs::remove_file(&erro));
    let lista = args
        .iter()
        .map(|a| format!("'{a}'"))
        .collect::<Vec<_>>()
        .join(",");
    let script = format!(
        "$s = ConvertTo-SecureString '{}' -AsPlainText -Force; \
         $c = New-Object System.Management.Automation.PSCredential('.\\{}', $s); \
         $p = Start-Process -FilePath '{exe}' -ArgumentList {lista} -Credential $c \
           -WorkingDirectory 'C:\\Windows\\System32' -LoadUserProfile -RedirectStandardOutput '{}' -RedirectStandardError '{}' -PassThru; \
         if (-not $p.WaitForExit(60000)) {{ $p.Kill(); Write-Output 'TEMPO ESGOTADO ao rodar como a conta'; exit 124 }}; \
         exit $p.ExitCode",
        conta.senha,
        conta.nome,
        saida.display(),
        erro.display()
    );
    let mut filho = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // prazo de 3 minutos para o PowerShell inteiro: nunca deixa o CI pendurado
    let limite = Instant::now() + Duration::from_secs(180);
    while filho.try_wait().unwrap().is_none() {
        if Instant::now() > limite {
            let _ = filho.kill();
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    let o = filho.wait_with_output().unwrap();
    let ler = |p: &PathBuf| std::fs::read_to_string(p).unwrap_or_default();
    let r = (o.status.code(), ler(&saida), ler(&erro), texto(&o));
    let _ = (std::fs::remove_file(&saida), std::fs::remove_file(&erro));
    r
}
