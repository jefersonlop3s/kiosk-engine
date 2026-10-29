//! Inicia o `kiosk-ui.exe` na sessão de uma conta (ADR-004): o serviço (SYSTEM) pega o token de
//! quem está logado na sessão e cria o processo com ele, na área de trabalho dela. A criança não
//! tem como impedir: o serviço é quem inicia, e a reinicia se ela morrer.

use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE};
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::RemoteDesktop::WTSQueryUserToken;
use windows_sys::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessAsUserW, PROCESS_INFORMATION, STARTUPINFOW,
};

use crate::sistema_windows::{Handle, ultimo_erro};

fn largo(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Cria `exe` como a conta logada na sessão `sessao`, na área de trabalho interativa dela, e
/// devolve o PID. O processo roda com o ambiente da própria conta (idioma, `%LOCALAPPDATA%`).
pub fn iniciar_na_sessao(sessao: u32, exe: &Path) -> Result<u32, String> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: exige SeTcbPrivilege (o serviço é SYSTEM); o token é fechado por `Handle`.
    if unsafe { WTSQueryUserToken(sessao, &mut token) } == FALSE {
        return Err(ultimo_erro("WTSQueryUserToken"));
    }
    let token = Handle(token);

    let mut ambiente: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: cria o bloco de ambiente da conta; é liberado abaixo.
    if unsafe { CreateEnvironmentBlock(&mut ambiente, token.0, FALSE) } == FALSE {
        return Err(ultimo_erro("CreateEnvironmentBlock"));
    }

    let aplicativo = largo(&exe.to_string_lossy());
    // a linha de comando é o caminho entre aspas: sem argumentos
    let mut linha = largo(&format!("\"{}\"", exe.display()));
    let mut area = largo(r"winsta0\default");
    // SAFETY: estruturas de campos numéricos e ponteiros; zeradas são valores válidos.
    let mut inicio: STARTUPINFOW = unsafe { std::mem::zeroed() };
    inicio.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    inicio.lpDesktop = area.as_mut_ptr();
    // SAFETY: idem.
    let mut processo: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: textos terminados em zero e vivos durante a chamada; `ambiente` veio de
    // CreateEnvironmentBlock e a flag diz que é UTF-16; `processo` recebe os handles.
    let criou = unsafe {
        CreateProcessAsUserW(
            token.0,
            aplicativo.as_ptr(),
            linha.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            FALSE,
            CREATE_UNICODE_ENVIRONMENT,
            ambiente,
            std::ptr::null(),
            &inicio,
            &mut processo,
        )
    };
    let erro = (criou == FALSE).then(|| ultimo_erro("CreateProcessAsUserW"));
    // SAFETY: libera o ambiente e os handles do filho (o serviço não os usa; o batimento basta).
    unsafe {
        DestroyEnvironmentBlock(ambiente);
        if criou != FALSE {
            CloseHandle(processo.hProcess);
            CloseHandle(processo.hThread);
        }
    }
    match erro {
        Some(e) => Err(e),
        None => Ok(processo.dwProcessId),
    }
}
