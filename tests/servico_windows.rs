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
    let estado = ok(&["status"]);
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
