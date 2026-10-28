//! Adaptador Win32 de [`Sistema`] (E-004; ADR-002, ADR-003). O serviço roda como SYSTEM.
//!
//! - processos: `CreateToolhelp32Snapshot` + SID do dono via `OpenProcessToken`/`TokenUser`;
//! - sessões ativas: `WTSEnumerateSessionsW` + `WTSSessionInfoEx` (logada **e** desbloqueada),
//!   SID pela conta da sessão (`LookupAccountNameW`);
//! - encerrar: `TerminateProcess`;
//! - bloquear sessão: `WTSDisconnectSession` (volta à tela de entrada; `LockWorkStation` só vale
//!   para a sessão de quem chama, e o serviço está na sessão 0);
//! - logoff: `WTSLogoffSession`;
//! - avisar: `WTSSendMessageW`, caixa de mensagem na sessão da criança. **Provisório** até a UI de
//!   bandeja da ADR-004 existir (E-004 F3).
//!
//! Só compila e roda no Windows. O CI (`windows-latest`) exercita listar processos com SID e
//! encerrar um processo; bloquear, logoff e avisar exigem Windows interativo.

use std::ffi::c_void;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, FALSE, HANDLE, INVALID_HANDLE_VALUE, LocalFree};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    GetTokenInformation, LookupAccountNameW, PSID, SID_NAME_USE, SidTypeUser, TOKEN_QUERY,
    TOKEN_USER, TokenUser,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW, WTS_SESSIONSTATE_UNLOCK, WTSActive,
    WTSDisconnectSession, WTSDomainName, WTSEnumerateSessionsW, WTSFreeMemory, WTSINFOEXW,
    WTSLogoffSession, WTSQuerySessionInformationW, WTSSendMessageW, WTSSessionInfoEx, WTSUserName,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    TerminateProcess,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK};

use crate::agente::{Aviso, ProcessoVisto, Sistema};

/// Quanto tempo a caixa de aviso fica aberta antes de fechar sozinha.
const AVISO_TIMEOUT_SEGUNDOS: u32 = 60;

fn ultimo_erro(o_que: &str) -> String {
    format!("{o_que}: {}", std::io::Error::last_os_error())
}

/// Fecha o handle ao sair do escopo.
struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: handle aberto por nós e fechado uma única vez.
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn de_utf16(buf: &[u16]) -> String {
    let fim = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..fim])
}

fn para_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Texto de uma `PWSTR` terminada em zero.
///
/// SAFETY: `p` deve apontar para uma string UTF-16 válida terminada em zero.
unsafe fn pwstr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    // SAFETY: a string termina em zero, garantido por quem chama.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: os `n` elementos foram lidos acima.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, n) })
}

/// SID em texto (`S-1-5-21-...`).
///
/// SAFETY: `sid` deve ser um SID válido.
unsafe fn sid_em_texto(sid: PSID) -> Result<String, String> {
    let mut texto: *mut u16 = null_mut();
    // SAFETY: `sid` válido; `texto` recebe memória alocada pelo sistema, liberada abaixo.
    if unsafe { ConvertSidToStringSidW(sid, &mut texto) } == FALSE {
        return Err(ultimo_erro("ConvertSidToStringSidW"));
    }
    // SAFETY: `texto` é uma string terminada em zero devolvida pela API.
    let s = unsafe { pwstr(texto) };
    // SAFETY: memória alocada por ConvertSidToStringSidW com LocalAlloc.
    unsafe { LocalFree(texto as *mut c_void) };
    Ok(s)
}

/// SID do dono do processo `pid`, ou `None` se o processo não puder ser aberto (protegido ou já
/// encerrado).
fn sid_do_processo(pid: u32) -> Option<String> {
    // SAFETY: chamadas Win32 com argumentos válidos; handles fechados por `Handle`.
    unsafe {
        let processo = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid));
        if processo.0.is_null() {
            return None;
        }
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(processo.0, TOKEN_QUERY, &mut token) == FALSE {
            return None;
        }
        let token = Handle(token);
        let mut tamanho = 0u32;
        GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut tamanho);
        if tamanho == 0 {
            return None;
        }
        // buffer alinhado para TOKEN_USER
        let mut buf = vec![0u64; (tamanho as usize).div_ceil(8)];
        if GetTokenInformation(
            token.0,
            TokenUser,
            buf.as_mut_ptr() as *mut c_void,
            tamanho,
            &mut tamanho,
        ) == FALSE
        {
            return None;
        }
        let usuario = &*(buf.as_ptr() as *const TOKEN_USER);
        sid_em_texto(usuario.User.Sid).ok()
    }
}

/// Sessão interativa: id, SID do usuário e se está logada e desbloqueada.
struct Sessao {
    id: u32,
    sid: String,
    ativa: bool,
}

fn consultar_texto(sessao: u32, classe: i32) -> Option<String> {
    let mut buf: *mut u16 = null_mut();
    let mut bytes = 0u32;
    // SAFETY: a API aloca `buf`, liberado com WTSFreeMemory.
    unsafe {
        if WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            sessao,
            classe,
            &mut buf,
            &mut bytes,
        ) == FALSE
        {
            return None;
        }
        let s = pwstr(buf);
        WTSFreeMemory(buf as *mut c_void);
        Some(s)
    }
}

fn desbloqueada(sessao: u32) -> bool {
    let mut buf: *mut u16 = null_mut();
    let mut bytes = 0u32;
    // SAFETY: a API aloca `buf` com um WTSINFOEXW; lido só se o tamanho bater e liberado depois.
    unsafe {
        if WTSQuerySessionInformationW(
            WTS_CURRENT_SERVER_HANDLE,
            sessao,
            WTSSessionInfoEx,
            &mut buf,
            &mut bytes,
        ) == FALSE
        {
            return false;
        }
        let mut ok = false;
        if bytes as usize >= std::mem::size_of::<WTSINFOEXW>() {
            let info = &*(buf as *const WTSINFOEXW);
            if info.Level == 1 {
                ok = info.Data.WTSInfoExLevel1.SessionFlags == WTS_SESSIONSTATE_UNLOCK as i32;
            }
        }
        WTSFreeMemory(buf as *mut c_void);
        ok
    }
}

fn sid_da_conta(dominio: &str, usuario: &str) -> Option<String> {
    let conta = if dominio.is_empty() {
        usuario.to_owned()
    } else {
        format!("{dominio}\\{usuario}")
    };
    procurar_conta(&conta).ok().map(|(sid, _)| sid)
}

/// SID e tipo de uma conta (`usuario` ou `DOMINIO\usuario`).
fn procurar_conta(conta: &str) -> Result<(String, SID_NAME_USE), String> {
    let nome = para_utf16(conta);
    let (mut tam_sid, mut tam_dom) = (0u32, 0u32);
    let mut uso: SID_NAME_USE = 0;
    // SAFETY: primeira chamada só mede os buffers; a segunda os preenche.
    unsafe {
        LookupAccountNameW(
            null(),
            nome.as_ptr(),
            null_mut(),
            &mut tam_sid,
            null_mut(),
            &mut tam_dom,
            &mut uso,
        );
        if tam_sid == 0 {
            return Err(ultimo_erro(&format!("conta {conta:?} não encontrada")));
        }
        let mut sid = vec![0u8; tam_sid as usize];
        let mut dom = vec![0u16; tam_dom as usize];
        if LookupAccountNameW(
            null(),
            nome.as_ptr(),
            sid.as_mut_ptr() as PSID,
            &mut tam_sid,
            dom.as_mut_ptr(),
            &mut tam_dom,
            &mut uso,
        ) == FALSE
        {
            return Err(ultimo_erro(&format!("LookupAccountNameW({conta:?})")));
        }
        Ok((sid_em_texto(sid.as_mut_ptr() as PSID)?, uso))
    }
}

/// SID de uma conta de **usuário** do Windows (recusa grupos, computadores etc.), para cadastrar
/// o perfil de uma criança pelo nome da conta (ADR-002).
pub fn sid_de_usuario(conta: &str) -> Result<String, String> {
    let (sid, tipo) = procurar_conta(conta)?;
    if tipo != SidTypeUser {
        return Err(format!("{conta:?} não é uma conta de usuário"));
    }
    Ok(sid)
}

fn sessoes() -> Result<Vec<Sessao>, String> {
    let mut lista: *mut WTS_SESSION_INFOW = null_mut();
    let mut n = 0u32;
    // SAFETY: a API aloca a lista, liberada com WTSFreeMemory ao final.
    unsafe {
        if WTSEnumerateSessionsW(WTS_CURRENT_SERVER_HANDLE, 0, 1, &mut lista, &mut n) == FALSE {
            return Err(ultimo_erro("WTSEnumerateSessionsW"));
        }
        let itens = std::slice::from_raw_parts(lista, n as usize);
        let mut saida = Vec::new();
        for s in itens {
            let usuario = consultar_texto(s.SessionId, WTSUserName).unwrap_or_default();
            if usuario.is_empty() {
                continue; // sessão 0, tela de entrada, RDP sem usuário
            }
            let dominio = consultar_texto(s.SessionId, WTSDomainName).unwrap_or_default();
            if let Some(sid) = sid_da_conta(&dominio, &usuario) {
                let ativa = s.State == WTSActive && desbloqueada(s.SessionId);
                saida.push(Sessao {
                    id: s.SessionId,
                    sid,
                    ativa,
                });
            }
        }
        WTSFreeMemory(lista as *mut c_void);
        Ok(saida)
    }
}

fn sessao_do_sid(sid: &str) -> Result<u32, String> {
    sessoes()?
        .into_iter()
        .find(|s| s.sid == sid)
        .map(|s| s.id)
        .ok_or_else(|| format!("nenhuma sessão interativa do SID {sid}"))
}

/// Adaptador real. Sem estado: cada chamada consulta o sistema.
#[derive(Debug, Default, Clone, Copy)]
pub struct SistemaWindows;

impl Sistema for SistemaWindows {
    fn processos(&mut self) -> Result<Vec<ProcessoVisto>, String> {
        // SAFETY: snapshot fechado por `Handle`; `dwSize` preenchido antes da primeira leitura.
        unsafe {
            let snap = Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0));
            if snap.0 == INVALID_HANDLE_VALUE {
                return Err(ultimo_erro("CreateToolhelp32Snapshot"));
            }
            let mut e: PROCESSENTRY32W = std::mem::zeroed();
            e.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            let mut saida = Vec::new();
            let mut ok = Process32FirstW(snap.0, &mut e);
            while ok != FALSE {
                let pid = e.th32ProcessID;
                if pid > 4 {
                    // sem SID (protegido, encerrado no meio) não é de criança: ignorado (ADR-002)
                    if let Some(sid) = sid_do_processo(pid) {
                        saida.push(ProcessoVisto {
                            pid,
                            process_name: de_utf16(&e.szExeFile),
                            sid,
                        });
                    }
                }
                ok = Process32NextW(snap.0, &mut e);
            }
            Ok(saida)
        }
    }

    fn encerrar(&mut self, pid: u32) -> Result<(), String> {
        // SAFETY: handle fechado por `Handle`.
        unsafe {
            let p = Handle(OpenProcess(PROCESS_TERMINATE, FALSE, pid));
            if p.0.is_null() {
                return Err(ultimo_erro(&format!("OpenProcess({pid})")));
            }
            if TerminateProcess(p.0, 1) == FALSE {
                return Err(ultimo_erro(&format!("TerminateProcess({pid})")));
            }
        }
        Ok(())
    }

    fn sessoes_ativas(&mut self) -> Result<Vec<String>, String> {
        Ok(sessoes()?
            .into_iter()
            .filter(|s| s.ativa)
            .map(|s| s.sid)
            .collect())
    }

    fn bloquear_sessao(&mut self, sid: &str) -> Result<(), String> {
        let id = sessao_do_sid(sid)?;
        // SAFETY: chamada simples com id de sessão existente.
        if unsafe { WTSDisconnectSession(WTS_CURRENT_SERVER_HANDLE, id, FALSE) } == FALSE {
            return Err(ultimo_erro("WTSDisconnectSession"));
        }
        Ok(())
    }

    fn encerrar_sessao(&mut self, sid: &str) -> Result<(), String> {
        let id = sessao_do_sid(sid)?;
        // SAFETY: chamada simples com id de sessão existente.
        if unsafe { WTSLogoffSession(WTS_CURRENT_SERVER_HANDLE, id, FALSE) } == FALSE {
            return Err(ultimo_erro("WTSLogoffSession"));
        }
        Ok(())
    }

    fn avisar(&mut self, aviso: &Aviso) -> Result<(), String> {
        let id = sessao_do_sid(&aviso.sid)?;
        let (titulo, mensagem) = crate::textos::aviso(aviso);
        let (t, m) = (para_utf16(&titulo), para_utf16(&mensagem));
        let mut resposta = 0;
        // SAFETY: buffers UTF-16 vivos durante a chamada; tamanhos em bytes sem o zero final;
        // `bWait = FALSE` não bloqueia o agente.
        let ok = unsafe {
            WTSSendMessageW(
                WTS_CURRENT_SERVER_HANDLE,
                id,
                t.as_ptr(),
                ((t.len() - 1) * 2) as u32,
                m.as_ptr(),
                ((m.len() - 1) * 2) as u32,
                MB_OK | MB_ICONWARNING,
                AVISO_TIMEOUT_SEGUNDOS,
                &mut resposta,
                FALSE,
            )
        };
        if ok == FALSE {
            return Err(ultimo_erro("WTSSendMessageW"));
        }
        Ok(())
    }
}
