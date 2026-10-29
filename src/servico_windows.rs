//! Serviço do Windows e autoinstalação (ADR-008, etapa 1; E-007 F2; resolve o GC-16).
//!
//! - `instalar`: copia `kioskd.exe` e `kioskctl.exe` para `%ProgramFiles%\kiosk`, protege
//!   `%ProgramData%\kiosk` (dono e acesso só de SYSTEM e Administradores), registra o serviço
//!   `kiosk` no SCM (conta LocalSystem, início automático, reinício em falha), grava
//!   `KIOSK_LISTEN` no ambiente do serviço (valor `Environment` da chave do serviço no registro)
//!   e o inicia. Reexecutar atualiza a instalação (para, copia, reconfigura e inicia).
//! - `desinstalar`: para e remove o serviço e os executáveis; os dados só com `apagar_dados`.
//! - `estado`: o que o SCM e o registro dizem do serviço.
//! - `despachar`: o modo serviço propriamente dito (`kioskd service`, chamado pelo SCM; a 0.0.4
//!   registrou `kioskd servico`, que segue valendo porque os nomes em português equivalem).
//!
//! Só compila no Windows. O CI (`windows-latest`, administrador) instala, confere, derruba o
//! processo para ver o reinício, prova que uma conta padrão não para o serviço e desinstala.

use std::ffi::c_void;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tokio::sync::Notify;
use windows_sys::Win32::Foundation::{
    ERROR_CALL_NOT_IMPLEMENTED, ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_SPECIFIC_ERROR,
    LocalFree, NO_ERROR, TRUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT,
    SetNamedSecurityInfoW,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, UNPROTECTED_DACL_SECURITY_INFORMATION,
};
use windows_sys::Win32::Storage::FileSystem::{DELETE, MOVEFILE_DELAY_UNTIL_REBOOT, MoveFileExW};
use windows_sys::Win32::System::Console::{STD_ERROR_HANDLE, SetStdHandle};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_MULTI_SZ, RRF_NOEXPAND, RRF_RT_REG_EXPAND_SZ,
    RRF_RT_REG_MULTI_SZ, RRF_RT_REG_SZ, RegCloseKey, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
};
use windows_sys::Win32::System::Services::*;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    HWND_BROADCAST, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_SETTINGCHANGE,
};
use windows_sys::core::PWSTR;

use crate::i18n::{Msg, idioma};
use crate::sistema_windows::{para_utf16, pwstr, ultimo_erro};

/// Nome do serviço no SCM (`sc query kiosk`).
pub const NOME_SERVICO: &str = "kiosk";

/// Pasta de dados: dono e acesso total só para SYSTEM (SY) e Administradores (BA), herdado
/// pelo que for criado dentro; `P` corta a herança de `%ProgramData%`, que dá leitura a Usuários.
const SDDL_DADOS: &str = "O:BAD:PAI(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
/// Itens que já estavam na pasta: dono Administradores e só o que herdam da pasta.
const SDDL_ITEM_DADOS: &str = "O:BAD:AI";
/// Serviço: SYSTEM e Administradores controlam; usuários interativos (IU) e serviços (SU) só
/// consultam. Sem `RP` (iniciar), `WP` (parar), `DT` (pausar) nem `SD` (apagar) para eles.
pub const SDDL_SERVICO: &str = "D:(A;;CCLCSWRPWPDTLOCRRC;;;SY)(A;;CCDCLCSWRPWPDTLOCRSDRCWDWO;;;BA)(A;;CCLCSWLOCRRC;;;IU)(A;;CCLCSWLOCRRC;;;SU)";

/// Reinício automático: 5 s depois de cada falha; a contagem zera após 1 dia sem falhas.
const ESPERA_REINICIO_MS: u32 = 5_000;
const ZERAR_FALHAS_S: u32 = 24 * 3600;
/// Quanto esperar o SCM mudar de estado (iniciar, parar).
const ESPERA_ESTADO: Duration = Duration::from_secs(30);
/// Log do serviço (stderr redirecionado); gira para `.1` ao passar disto, na partida.
const LOG_MAXIMO: u64 = 10 * 1024 * 1024;

const EXECUTAVEIS: [&str; 2] = ["kioskd.exe", "kioskctl.exe"];

/// O processo do serviço informa "parado" e só então sai: por um instante o `.exe` segue travado.
fn com_tentativas<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut restantes = 20;
    loop {
        match f() {
            Err(_) if restantes > 0 => {
                restantes -= 1;
                std::thread::sleep(Duration::from_millis(250));
            }
            r => return r,
        }
    }
}

fn pasta_de(var: &str, padrao: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(padrao))
}

/// `%ProgramFiles%\kiosk`: onde ficam os executáveis instalados.
pub fn pasta_programa() -> PathBuf {
    pasta_de("ProgramFiles", r"C:\Program Files").join("kiosk")
}

/// `%ProgramData%\kiosk`: banco e log do serviço (o banco padrão do `kioskd` mora aqui).
pub fn pasta_dados() -> PathBuf {
    pasta_de("ProgramData", r"C:\ProgramData").join("kiosk")
}

pub fn arquivo_log() -> PathBuf {
    pasta_dados().join("kioskd.log")
}

/// Handle do SCM ou de um serviço, fechado ao sair do escopo.
struct Sc(SC_HANDLE);

impl Drop for Sc {
    fn drop(&mut self) {
        // SAFETY: handle aberto por nós e fechado uma única vez.
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn abrir_scm(acesso: u32) -> Result<Sc, String> {
    // SAFETY: máquina e base locais (nulos).
    let h = unsafe { OpenSCManagerW(null(), null(), acesso) };
    if h.is_null() {
        let e = std::io::Error::last_os_error();
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            return Err(Msg::SemAcessoAoScm.to_string());
        }
        return Err(format!("OpenSCManagerW: {e}"));
    }
    Ok(Sc(h))
}

/// `None` se o serviço não existe.
fn abrir_servico(scm: &Sc, acesso: u32) -> Result<Option<Sc>, String> {
    let nome = para_utf16(NOME_SERVICO);
    // SAFETY: handle do SCM válido; nome terminado em zero.
    let h = unsafe { OpenServiceW(scm.0, nome.as_ptr(), acesso) };
    if h.is_null() {
        let e = std::io::Error::last_os_error();
        if e.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST as i32) {
            return Ok(None);
        }
        return Err(format!("OpenServiceW({NOME_SERVICO}): {e}"));
    }
    Ok(Some(Sc(h)))
}

fn status(svc: &Sc) -> Result<SERVICE_STATUS_PROCESS, String> {
    // SAFETY: estrutura de dados simples; zerada é válida.
    let mut st: SERVICE_STATUS_PROCESS = unsafe { std::mem::zeroed() };
    let mut necessario = 0u32;
    // SAFETY: buffer do tamanho exato da estrutura pedida (SC_STATUS_PROCESS_INFO).
    let ok = unsafe {
        QueryServiceStatusEx(
            svc.0,
            SC_STATUS_PROCESS_INFO,
            (&mut st as *mut SERVICE_STATUS_PROCESS).cast(),
            size_of::<SERVICE_STATUS_PROCESS>() as u32,
            &mut necessario,
        )
    };
    if ok == 0 {
        return Err(ultimo_erro("QueryServiceStatusEx"));
    }
    Ok(st)
}

fn esperar_estado(svc: &Sc, alvo: u32) -> Result<SERVICE_STATUS_PROCESS, String> {
    let fim = Instant::now() + ESPERA_ESTADO;
    loop {
        let st = status(svc)?;
        if st.dwCurrentState == alvo {
            return Ok(st);
        }
        if Instant::now() > fim {
            return Err(Msg::EstadoNaoChegou {
                alvo: nome_estado(alvo),
                segundos: ESPERA_ESTADO.as_secs(),
                atual: nome_estado(st.dwCurrentState),
                log: arquivo_log().display().to_string(),
            }
            .to_string());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

/// Para o serviço, se estiver rodando, e espera parar.
fn parar(svc: &Sc) -> Result<(), String> {
    if status(svc)?.dwCurrentState == SERVICE_STOPPED {
        return Ok(());
    }
    // SAFETY: estrutura de saída do tamanho certo.
    let mut st: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    // SAFETY: handle com SERVICE_STOP.
    if unsafe { ControlService(svc.0, SERVICE_CONTROL_STOP, &mut st) } == 0 {
        let e = std::io::Error::last_os_error();
        // já parando ou parado entre a consulta e o pedido: segue para a espera
        if e.raw_os_error() != Some(windows_sys::Win32::Foundation::ERROR_SERVICE_NOT_ACTIVE as i32)
        {
            return Err(format!("ControlService(parar): {e}"));
        }
    }
    esperar_estado(svc, SERVICE_STOPPED).map(|_| ())
}

pub fn nome_estado(estado: u32) -> String {
    match estado {
        SERVICE_STOPPED => Msg::EstParado,
        SERVICE_START_PENDING => Msg::EstIniciando,
        SERVICE_STOP_PENDING => Msg::EstParando,
        SERVICE_RUNNING => Msg::EstExecucao,
        _ => Msg::EstOutro,
    }
    .to_string()
}

/// PID do processo do serviço em execução (`None` se não está instalado ou está parado). Qualquer
/// conta consulta (a DACL do serviço deixa), então serve ao cliente do pipe `kiosk-audit` para
/// conferir que o servidor do pipe é o serviço e não um processo que ocupou o nome.
pub fn pid_do_servico() -> Result<Option<u32>, String> {
    let scm = abrir_scm(SC_MANAGER_CONNECT)?;
    let Some(svc) = abrir_servico(&scm, SERVICE_QUERY_STATUS)? else {
        return Ok(None);
    };
    let st = status(&svc)?;
    Ok((st.dwCurrentState == SERVICE_RUNNING && st.dwProcessId != 0).then_some(st.dwProcessId))
}

/// Descritor de segurança a partir de SDDL, liberado ao sair do escopo.
pub(crate) struct Descritor(pub(crate) PSECURITY_DESCRIPTOR);

impl Descritor {
    pub(crate) fn de_sddl(sddl: &str) -> Result<Self, String> {
        let texto = para_utf16(sddl);
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        // SAFETY: SDDL terminada em zero; o descritor é liberado no Drop com LocalFree.
        let ok = unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                texto.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                null_mut(),
            )
        };
        if ok == 0 {
            return Err(ultimo_erro(&format!("invalid SDDL {sddl:?}")));
        }
        Ok(Self(sd))
    }

    fn dacl(&self) -> Result<*mut ACL, String> {
        let (mut presente, mut padrao) = (0, 0);
        let mut dacl: *mut ACL = null_mut();
        // SAFETY: descritor válido; os ponteiros apontam para dentro dele.
        if unsafe { GetSecurityDescriptorDacl(self.0, &mut presente, &mut dacl, &mut padrao) } == 0
        {
            return Err(ultimo_erro("GetSecurityDescriptorDacl"));
        }
        Ok(dacl)
    }

    fn dono(&self) -> Result<PSID, String> {
        let mut padrao = 0;
        let mut dono: PSID = null_mut();
        // SAFETY: idem.
        if unsafe { GetSecurityDescriptorOwner(self.0, &mut dono, &mut padrao) } == 0 {
            return Err(ultimo_erro("GetSecurityDescriptorOwner"));
        }
        Ok(dono)
    }
}

impl Drop for Descritor {
    fn drop(&mut self) {
        // SAFETY: alocado por ConvertStringSecurityDescriptorToSecurityDescriptorW.
        unsafe { LocalFree(self.0) };
    }
}

fn aplicar_sddl(
    caminho: &Path,
    sddl: &str,
    dacl: OBJECT_SECURITY_INFORMATION,
) -> Result<(), String> {
    let sd = Descritor::de_sddl(sddl)?;
    let nome = para_utf16(&caminho.to_string_lossy());
    // SAFETY: nome terminado em zero; dono e DACL vivem enquanto `sd` existir.
    let erro = unsafe {
        SetNamedSecurityInfoW(
            nome.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | dacl,
            sd.dono()?,
            null_mut(),
            sd.dacl()?,
            null(),
        )
    };
    if erro != NO_ERROR {
        return Err(Msg::NaoProtegeu {
            pasta: caminho.display().to_string(),
            erro: std::io::Error::from_raw_os_error(erro as i32).to_string(),
        }
        .to_string());
    }
    Ok(())
}

/// Cria a pasta de dados e a deixa só para SYSTEM e Administradores, inclusive o que já havia
/// dentro (a pasta pode ter sido criada antes por qualquer conta, já que `%ProgramData%` deixa).
pub fn proteger_pasta_dados() -> Result<PathBuf, String> {
    let pasta = pasta_dados();
    std::fs::create_dir_all(&pasta).map_err(|e| {
        Msg::NaoCriouPasta {
            pasta: pasta.display().to_string(),
            erro: e.to_string(),
        }
        .to_string()
    })?;
    aplicar_sddl(&pasta, SDDL_DADOS, PROTECTED_DACL_SECURITY_INFORMATION)?;
    let mut pendentes = vec![pasta.clone()];
    while let Some(dir) = pendentes.pop() {
        let itens = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for item in itens {
            let item = item.map_err(|e| format!("{}: {e}", dir.display()))?;
            let caminho = item.path();
            aplicar_sddl(
                &caminho,
                SDDL_ITEM_DADOS,
                UNPROTECTED_DACL_SECURITY_INFORMATION,
            )?;
            if item.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                pendentes.push(caminho);
            }
        }
    }
    Ok(pasta)
}

fn mesmo_arquivo(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Copia os executáveis da pasta do `kioskd.exe` em uso para `%ProgramFiles%\kiosk`.
fn copiar_executaveis() -> Result<PathBuf, String> {
    let atual = std::env::current_exe().map_err(|e| format!("kioskd path: {e}"))?;
    let origem = atual
        .parent()
        .ok_or_else(|| Msg::KioskdSemPasta.to_string())?
        .to_path_buf();
    let destino = pasta_programa();
    std::fs::create_dir_all(&destino).map_err(|e| {
        Msg::NaoCriouPasta {
            pasta: destino.display().to_string(),
            erro: e.to_string(),
        }
        .to_string()
    })?;
    for exe in EXECUTAVEIS {
        let (de, para) = (origem.join(exe), destino.join(exe));
        if !de.exists() {
            return Err(Msg::FaltaExecutavel {
                arquivo: de.display().to_string(),
            }
            .to_string());
        }
        if mesmo_arquivo(&de, &para) {
            continue;
        }
        com_tentativas(|| std::fs::copy(&de, &para)).map_err(|e| {
            Msg::NaoCopiou {
                de: de.display().to_string(),
                para: para.display().to_string(),
                erro: e.to_string(),
            }
            .to_string()
        })?;
    }
    Ok(destino)
}

fn chave_do_servico() -> String {
    format!(r"SYSTEM\CurrentControlSet\Services\{NOME_SERVICO}")
}

/// Grava as variáveis do serviço (REG_MULTI_SZ `Environment`, lido pelo SCM ao iniciar).
fn gravar_ambiente(variaveis: &[(&str, &str)]) -> Result<(), String> {
    let mut dados: Vec<u16> = Vec::new();
    for (k, v) in variaveis {
        dados.extend(format!("{k}={v}").encode_utf16());
        dados.push(0);
    }
    dados.push(0);
    let sub = para_utf16(&chave_do_servico());
    let mut chave: HKEY = null_mut();
    // SAFETY: subchave terminada em zero; a chave é fechada logo abaixo.
    let erro = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut chave,
        )
    };
    if erro != NO_ERROR {
        return Err(format!(
            "service registry: {}",
            std::io::Error::from_raw_os_error(erro as i32)
        ));
    }
    let valor = para_utf16("Environment");
    // SAFETY: dados em UTF-16 terminados em zero duplo; tamanho em bytes.
    let erro = unsafe {
        RegSetValueExW(
            chave,
            valor.as_ptr(),
            0,
            REG_MULTI_SZ,
            dados.as_ptr().cast(),
            (dados.len() * 2) as u32,
        )
    };
    // SAFETY: chave aberta acima.
    unsafe { RegCloseKey(chave) };
    if erro != NO_ERROR {
        return Err(format!(
            "service environment: {}",
            std::io::Error::from_raw_os_error(erro as i32)
        ));
    }
    Ok(())
}

/// Variáveis gravadas no ambiente do serviço (vazio se não houver).
fn ler_ambiente() -> Vec<String> {
    let sub = para_utf16(&chave_do_servico());
    let valor = para_utf16("Environment");
    let mut buf = vec![0u16; 4096];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: buffer com `bytes` bytes graváveis.
    let erro = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            valor.as_ptr(),
            RRF_RT_REG_MULTI_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if erro != NO_ERROR {
        return Vec::new();
    }
    buf.truncate(bytes as usize / 2);
    buf.split(|&c| c == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// Chave do ambiente da máquina, onde fica o `Path` do sistema.
const CHAVE_AMBIENTE_MAQUINA: &str =
    r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment";

/// `Path` do sistema como está no registro (sem expandir `%...%`) e o tipo do valor.
fn ler_path_maquina() -> Result<(String, u32), String> {
    let sub = para_utf16(CHAVE_AMBIENTE_MAQUINA);
    let valor = para_utf16("Path");
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ | RRF_NOEXPAND;
    let (mut tipo, mut bytes) = (0u32, 0u32);
    // SAFETY: primeira chamada só mede o tamanho.
    let erro = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            valor.as_ptr(),
            flags,
            &mut tipo,
            null_mut(),
            &mut bytes,
        )
    };
    if erro != NO_ERROR {
        return Err(format!(
            "system PATH: {}",
            std::io::Error::from_raw_os_error(erro as i32)
        ));
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
    // SAFETY: buffer com `bytes` bytes graváveis.
    let erro = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            valor.as_ptr(),
            flags,
            &mut tipo,
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if erro != NO_ERROR {
        return Err(format!(
            "system PATH: {}",
            std::io::Error::from_raw_os_error(erro as i32)
        ));
    }
    let fim = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    Ok((String::from_utf16_lossy(&buf[..fim]), tipo))
}

fn gravar_path_maquina(texto: &str, tipo: u32) -> Result<(), String> {
    let sub = para_utf16(CHAVE_AMBIENTE_MAQUINA);
    let mut chave: HKEY = null_mut();
    // SAFETY: subchave terminada em zero; a chave é fechada logo abaixo.
    let erro = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            sub.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut chave,
        )
    };
    if erro != NO_ERROR {
        return Err(format!(
            "system PATH: {}",
            std::io::Error::from_raw_os_error(erro as i32)
        ));
    }
    let (valor, dados) = (para_utf16("Path"), para_utf16(texto));
    // SAFETY: texto UTF-16 terminado em zero; tamanho em bytes, com o zero.
    let erro = unsafe {
        RegSetValueExW(
            chave,
            valor.as_ptr(),
            0,
            tipo,
            dados.as_ptr().cast(),
            (dados.len() * 2) as u32,
        )
    };
    // SAFETY: chave aberta acima.
    unsafe { RegCloseKey(chave) };
    if erro != NO_ERROR {
        return Err(format!(
            "system PATH: {}",
            std::io::Error::from_raw_os_error(erro as i32)
        ));
    }
    // avisa o Explorer e quem escuta: janelas abertas daqui em diante já veem o PATH novo
    let ambiente = para_utf16("Environment");
    let mut resultado = 0usize;
    // SAFETY: mensagem de difusão com texto terminado em zero; espera limitada a 5 s.
    unsafe {
        SendMessageTimeoutW(
            HWND_BROADCAST,
            WM_SETTINGCHANGE,
            0,
            ambiente.as_ptr() as isize,
            SMTO_ABORTIFHUNG,
            5_000,
            &mut resultado,
        )
    };
    Ok(())
}

/// A entrada do PATH é a pasta (sem diferenciar maiúsculas nem a barra final).
fn e_a_pasta(entrada: &str, pasta: &Path) -> bool {
    let normal = |t: &str| t.trim().trim_end_matches('\\').to_lowercase();
    normal(entrada) == normal(&pasta.to_string_lossy())
}

/// Quantas vezes `%ProgramFiles%\kiosk` aparece no PATH do sistema.
pub fn vezes_no_path() -> Result<usize, String> {
    let (path, _) = ler_path_maquina()?;
    let pasta = pasta_programa();
    Ok(path.split(';').filter(|e| e_a_pasta(e, &pasta)).count())
}

/// Põe a pasta no fim do PATH do sistema, se ainda não estiver; `true` se mudou.
/// Seguro porque só SYSTEM e Administradores escrevem em `%ProgramFiles%`.
fn incluir_no_path(pasta: &Path) -> Result<bool, String> {
    let (path, tipo) = ler_path_maquina()?;
    if path.split(';').any(|e| e_a_pasta(e, pasta)) {
        return Ok(false);
    }
    let separador = if path.is_empty() || path.ends_with(';') {
        ""
    } else {
        ";"
    };
    gravar_path_maquina(&format!("{path}{separador}{}", pasta.display()), tipo)?;
    Ok(true)
}

/// Tira a pasta do PATH do sistema, mantendo as outras entradas como estão; `true` se mudou.
fn remover_do_path(pasta: &Path) -> Result<bool, String> {
    let (path, tipo) = ler_path_maquina()?;
    let entradas: Vec<&str> = path.split(';').collect();
    let restantes: Vec<&str> = entradas
        .iter()
        .copied()
        .filter(|e| !e_a_pasta(e, pasta))
        .collect();
    if restantes.len() == entradas.len() {
        return Ok(false);
    }
    gravar_path_maquina(&restantes.join(";"), tipo)?;
    Ok(true)
}

fn configurar_extras(svc: &Sc) -> Result<(), String> {
    let mut texto = para_utf16(&Msg::ServicoDescricao.to_string());
    let descricao = SERVICE_DESCRIPTIONW {
        lpDescription: texto.as_mut_ptr(),
    };
    let mut acoes = [SC_ACTION {
        Type: SC_ACTION_RESTART,
        Delay: ESPERA_REINICIO_MS,
    }; 3];
    let falhas = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: ZERAR_FALHAS_S,
        lpRebootMsg: null_mut(),
        lpCommand: null_mut(),
        cActions: acoes.len() as u32,
        lpsaActions: acoes.as_mut_ptr(),
    };
    // reinicia também quando o processo sai com erro (não só quando cai)
    let bandeira = SERVICE_FAILURE_ACTIONS_FLAG {
        fFailureActionsOnNonCrashFailures: TRUE,
    };
    let passos: [(u32, *const c_void, &str); 3] = [
        (
            SERVICE_CONFIG_DESCRIPTION,
            (&descricao as *const SERVICE_DESCRIPTIONW).cast(),
            "service description",
        ),
        (
            SERVICE_CONFIG_FAILURE_ACTIONS,
            (&falhas as *const SERVICE_FAILURE_ACTIONSW).cast(),
            "restart on failure",
        ),
        (
            SERVICE_CONFIG_FAILURE_ACTIONS_FLAG,
            (&bandeira as *const SERVICE_FAILURE_ACTIONS_FLAG).cast(),
            "restart on non-crash exit",
        ),
    ];
    for (nivel, info, o_que) in passos {
        // SAFETY: `info` aponta para a estrutura do nível pedido, viva até aqui.
        if unsafe { ChangeServiceConfig2W(svc.0, nivel, info) } == 0 {
            return Err(ultimo_erro(&format!("configure {o_que}")));
        }
    }
    let sd = Descritor::de_sddl(SDDL_SERVICO)?;
    // SAFETY: descritor válido com DACL.
    if unsafe { SetServiceObjectSecurity(svc.0, DACL_SECURITY_INFORMATION, sd.0) } == 0 {
        return Err(ultimo_erro("service permissions"));
    }
    Ok(())
}

/// Instala (ou atualiza) e inicia o serviço. `escutar` já validado (ADR-005).
/// Devolve o PID do serviço em execução.
pub fn instalar(escutar: &str) -> Result<u32, String> {
    let scm = abrir_scm(SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE)?;
    let dados = proteger_pasta_dados()?;
    eprintln!(
        "kioskd: {}",
        Msg::PastaProtegida {
            pasta: dados.display().to_string()
        }
    );

    let existente = abrir_servico(&scm, SERVICE_ALL_ACCESS)?;
    if let Some(svc) = &existente {
        eprintln!("kioskd: {}", Msg::ServicoJaInstalado);
        parar(svc)?;
    }
    let destino = copiar_executaveis()?;
    eprintln!(
        "kioskd: {}",
        Msg::ExecutaveisEm {
            pasta: destino.display().to_string()
        }
    );
    if incluir_no_path(&destino)? {
        eprintln!(
            "kioskd: {}",
            Msg::NoPath {
                pasta: destino.display().to_string()
            }
        );
    }

    let caminho = para_utf16(&format!(
        "\"{}\" service",
        destino.join("kioskd.exe").display()
    ));
    let exibicao = para_utf16(&Msg::ServicoNomeExibicao.to_string());
    let svc = match existente {
        Some(svc) => {
            // SAFETY: textos terminados em zero; nulos = sem mudança (conta continua LocalSystem).
            let ok = unsafe {
                ChangeServiceConfigW(
                    svc.0,
                    SERVICE_WIN32_OWN_PROCESS,
                    SERVICE_AUTO_START,
                    SERVICE_ERROR_NORMAL,
                    caminho.as_ptr(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    exibicao.as_ptr(),
                )
            };
            if ok == 0 {
                return Err(ultimo_erro("ChangeServiceConfigW"));
            }
            svc
        }
        None => {
            let nome = para_utf16(NOME_SERVICO);
            // SAFETY: textos terminados em zero; conta nula = LocalSystem.
            let h = unsafe {
                CreateServiceW(
                    scm.0,
                    nome.as_ptr(),
                    exibicao.as_ptr(),
                    SERVICE_ALL_ACCESS,
                    SERVICE_WIN32_OWN_PROCESS,
                    SERVICE_AUTO_START,
                    SERVICE_ERROR_NORMAL,
                    caminho.as_ptr(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                )
            };
            if h.is_null() {
                return Err(ultimo_erro("CreateServiceW"));
            }
            Sc(h)
        }
    };
    configurar_extras(&svc)?;
    // o serviço (SYSTEM) escreve o log no idioma de quem o instalou
    gravar_ambiente(&[("KIOSK_LISTEN", escutar), ("KIOSK_LANG", idioma().tag())])?;
    eprintln!("kioskd: {}", Msg::ServicoRegistrado);

    // SAFETY: sem argumentos.
    if unsafe { StartServiceW(svc.0, 0, null()) } == 0 {
        return Err(ultimo_erro("StartServiceW"));
    }
    esperar_estado(&svc, SERVICE_RUNNING)?;
    // o serviço informa "em execução" antes de abrir as portas; uma falha ali o derruba logo
    std::thread::sleep(Duration::from_secs(2));
    let st = status(&svc)?;
    if st.dwCurrentState != SERVICE_RUNNING {
        return Err(Msg::ServicoParouLogo {
            codigo: st.dwServiceSpecificExitCode,
            log: arquivo_log().display().to_string(),
        }
        .to_string());
    }
    Ok(st.dwProcessId)
}

/// Remove o arquivo; se for o próprio executável em uso, agenda para o próximo reinício.
fn remover_ou_agendar(caminho: &Path, atual: &Path) -> Result<bool, String> {
    if !caminho.exists() {
        return Ok(false);
    }
    if mesmo_arquivo(caminho, atual) {
        let nome = para_utf16(&caminho.to_string_lossy());
        // SAFETY: nome terminado em zero; destino nulo = apagar no reinício.
        if unsafe { MoveFileExW(nome.as_ptr(), null(), MOVEFILE_DELAY_UNTIL_REBOOT) } == 0 {
            return Err(ultimo_erro(&format!(
                "schedule removal of {}",
                caminho.display()
            )));
        }
        return Ok(true);
    }
    com_tentativas(|| std::fs::remove_file(caminho)).map_err(|e| {
        Msg::NaoRemoveu {
            caminho: caminho.display().to_string(),
            erro: e.to_string(),
        }
        .to_string()
    })?;
    Ok(false)
}

/// Para e remove o serviço e os executáveis; com `apagar_dados`, também a pasta de dados.
/// A confirmação de `apagar_dados` é de quem chama.
pub fn desinstalar(apagar_dados: bool) -> Result<(), String> {
    let scm = abrir_scm(SC_MANAGER_CONNECT)?;
    match abrir_servico(&scm, SERVICE_STOP | SERVICE_QUERY_STATUS | DELETE)? {
        Some(svc) => {
            parar(&svc)?;
            // SAFETY: handle com DELETE.
            if unsafe { DeleteService(svc.0) } == 0 {
                return Err(ultimo_erro("DeleteService"));
            }
            eprintln!("kioskd: {}", Msg::ServicoRemovido);
        }
        None => eprintln!("kioskd: {}", Msg::ServicoNaoEstavaInstalado),
    }

    let atual = std::env::current_exe().map_err(|e| format!("kioskd path: {e}"))?;
    let destino = pasta_programa();
    if remover_do_path(&destino)? {
        eprintln!(
            "kioskd: {}",
            Msg::ForaDoPath {
                pasta: destino.display().to_string()
            }
        );
    }
    let mut agendado = false;
    for exe in EXECUTAVEIS {
        agendado |= remover_ou_agendar(&destino.join(exe), &atual)?;
    }
    if destino.exists() {
        if agendado {
            let nome = para_utf16(&destino.to_string_lossy());
            // SAFETY: idem; a pasta é removida depois do arquivo agendado antes.
            unsafe { MoveFileExW(nome.as_ptr(), null(), MOVEFILE_DELAY_UNTIL_REBOOT) };
            eprintln!(
                "kioskd: {}",
                Msg::ApagadaNoReinicio {
                    pasta: destino.display().to_string()
                }
            );
        } else {
            std::fs::remove_dir(&destino).map_err(|e| {
                Msg::PastaNaoVazia {
                    pasta: destino.display().to_string(),
                    erro: e.to_string(),
                }
                .to_string()
            })?;
            eprintln!(
                "kioskd: {}",
                Msg::ExecutaveisRemovidos {
                    pasta: destino.display().to_string()
                }
            );
        }
    }

    let dados = pasta_dados();
    if apagar_dados {
        if dados.exists() {
            std::fs::remove_dir_all(&dados).map_err(|e| {
                Msg::NaoRemoveu {
                    caminho: dados.display().to_string(),
                    erro: e.to_string(),
                }
                .to_string()
            })?;
        }
        eprintln!(
            "kioskd: {}",
            Msg::DadosApagados {
                pasta: dados.display().to_string()
            }
        );
    } else if dados.exists() {
        eprintln!(
            "kioskd: {}",
            Msg::DadosMantidos {
                pasta: dados.display().to_string()
            }
        );
    }
    Ok(())
}

/// O que o Windows diz do serviço.
#[derive(Debug)]
pub struct Estado {
    pub estado: u32,
    pub pid: u32,
    pub inicio_automatico: bool,
    pub conta: String,
    pub executavel: String,
    pub ambiente: Vec<String>,
    /// `%ProgramFiles%\kiosk` está no PATH do sistema.
    pub no_path: bool,
}

/// `None` se o serviço não está instalado.
pub fn estado() -> Result<Option<Estado>, String> {
    let scm = abrir_scm(SC_MANAGER_CONNECT)?;
    let Some(svc) = abrir_servico(&scm, SERVICE_QUERY_STATUS | SERVICE_QUERY_CONFIG)? else {
        return Ok(None);
    };
    let st = status(&svc)?;
    let mut necessario = 0u32;
    // SAFETY: primeira chamada só mede o buffer (falha esperada com ERROR_INSUFFICIENT_BUFFER).
    unsafe { QueryServiceConfigW(svc.0, null_mut(), 0, &mut necessario) };
    let mut buf = vec![0u64; (necessario as usize).div_ceil(8).max(1)];
    let cfg = buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>();
    // SAFETY: buffer alinhado com `necessario` bytes.
    if unsafe { QueryServiceConfigW(svc.0, cfg, necessario, &mut necessario) } == 0 {
        return Err(ultimo_erro("QueryServiceConfigW"));
    }
    // SAFETY: preenchido pela chamada acima; as strings apontam para dentro de `buf`.
    let (inicio, conta, executavel) = unsafe {
        (
            (*cfg).dwStartType,
            pwstr((*cfg).lpServiceStartName),
            pwstr((*cfg).lpBinaryPathName),
        )
    };
    Ok(Some(Estado {
        estado: st.dwCurrentState,
        pid: st.dwProcessId,
        inicio_automatico: inicio == SERVICE_AUTO_START,
        conta,
        executavel,
        ambiente: ler_ambiente(),
        no_path: vezes_no_path()? > 0,
    }))
}

// --- modo serviço (`kioskd service`) ---

type Servir = fn(Arc<Notify>) -> Result<(), String>;

static SERVIR: OnceLock<Servir> = OnceLock::new();
static PARAR: OnceLock<Arc<Notify>> = OnceLock::new();
static STATUS: AtomicUsize = AtomicUsize::new(0);

fn informar(estado: u32, codigo: u32, espera_ms: u32) {
    let h = STATUS.load(Ordering::SeqCst) as SERVICE_STATUS_HANDLE;
    if h.is_null() {
        return;
    }
    let st = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: estado,
        dwControlsAccepted: if estado == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: if codigo == 0 {
            NO_ERROR
        } else {
            ERROR_SERVICE_SPECIFIC_ERROR
        },
        dwServiceSpecificExitCode: codigo,
        dwCheckPoint: 0,
        dwWaitHint: espera_ms,
    };
    // SAFETY: handle devolvido por RegisterServiceCtrlHandlerExW.
    unsafe { SetServiceStatus(h, &st) };
}

unsafe extern "system" fn controle(
    pedido: u32,
    _tipo: u32,
    _dados: *mut c_void,
    _ctx: *mut c_void,
) -> u32 {
    match pedido {
        SERVICE_CONTROL_STOP | SERVICE_CONTROL_SHUTDOWN => {
            informar(SERVICE_STOP_PENDING, 0, 10_000);
            if let Some(p) = PARAR.get() {
                p.notify_one();
            }
            NO_ERROR
        }
        SERVICE_CONTROL_INTERROGATE => NO_ERROR,
        _ => ERROR_CALL_NOT_IMPLEMENTED,
    }
}

/// Manda o stderr do processo para o log do serviço (um serviço não tem console).
fn redirecionar_log() -> Result<(), String> {
    use std::os::windows::io::IntoRawHandle;
    let log = arquivo_log();
    if let Some(pasta) = log.parent() {
        std::fs::create_dir_all(pasta).map_err(|e| format!("{}: {e}", pasta.display()))?;
    }
    if std::fs::metadata(&log).is_ok_and(|m| m.len() > LOG_MAXIMO) {
        let _ = std::fs::rename(&log, log.with_extension("log.1"));
    }
    let arquivo = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log)
        .map_err(|e| format!("{}: {e}", log.display()))?;
    // o handle fica com o processo até o fim (o stderr do Rust consulta GetStdHandle a cada escrita)
    // SAFETY: handle de arquivo válido, aberto para escrita.
    if unsafe { SetStdHandle(STD_ERROR_HANDLE, arquivo.into_raw_handle()) } == 0 {
        return Err(ultimo_erro("SetStdHandle"));
    }
    Ok(())
}

unsafe extern "system" fn servico_main(_argc: u32, _argv: *mut PWSTR) {
    let parar = PARAR.get_or_init(|| Arc::new(Notify::new())).clone();
    let nome = para_utf16(NOME_SERVICO);
    // SAFETY: nome terminado em zero; `controle` vive o processo inteiro.
    let h = unsafe { RegisterServiceCtrlHandlerExW(nome.as_ptr(), Some(controle), null()) };
    if h.is_null() {
        return;
    }
    STATUS.store(h as usize, Ordering::SeqCst);
    informar(SERVICE_START_PENDING, 0, 5_000);
    if redirecionar_log().is_err() {
        // sem log não há onde registrar falhas: recusa subir (fail-loud); o código fica no SCM
        informar(SERVICE_STOPPED, 3, 0);
        return;
    }
    eprintln!(
        "kioskd: {}",
        Msg::ServicoIniciado {
            versao: env!("CARGO_PKG_VERSION").to_owned(),
            pid: std::process::id()
        }
    );
    informar(SERVICE_RUNNING, 0, 0);
    let Some(servir) = SERVIR.get() else {
        informar(SERVICE_STOPPED, 2, 0);
        return;
    };
    let codigo = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| servir(parar))) {
        Ok(Ok(())) => {
            eprintln!("kioskd: {}", Msg::ServicoParadoPeloWindows);
            0
        }
        Ok(Err(e)) => {
            eprintln!("kioskd: {}", Msg::ServicoParouComErro { erro: e });
            1
        }
        Err(_) => {
            eprintln!("kioskd: {}", Msg::ServicoParouPorPanico);
            2
        }
    };
    informar(SERVICE_STOPPED, codigo, 0);
}

/// Entrega o processo ao SCM; `servir` roda até o `Notify` ser acionado pelo pedido de parada.
/// Fora do SCM (alguém rodou `kioskd service` à mão), falha com uma explicação.
pub fn despachar(servir: Servir) -> Result<(), String> {
    let _ = SERVIR.set(servir);
    let mut nome = para_utf16(NOME_SERVICO);
    let tabela = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: nome.as_mut_ptr(),
            lpServiceProc: Some(servico_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: null_mut(),
            lpServiceProc: None,
        },
    ];
    // SAFETY: tabela terminada pela entrada nula; bloqueia até o serviço parar.
    if unsafe { StartServiceCtrlDispatcherW(tabela.as_ptr()) } == 0 {
        return Err(Msg::ServicoForaDoScm {
            erro: std::io::Error::last_os_error().to_string(),
        }
        .to_string());
    }
    Ok(())
}
