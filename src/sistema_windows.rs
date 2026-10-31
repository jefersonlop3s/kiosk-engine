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

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_TOKEN, FALSE, HANDLE, INVALID_HANDLE_VALUE, LocalFree, TRUE,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{
    CreateWellKnownSid, EqualSid, GetTokenInformation, LookupAccountNameW, PSID, RevertToSelf,
    SECURITY_MAX_SID_SIZE, SID_NAME_USE, SidTypeUser, TOKEN_GROUPS, TOKEN_QUERY, TOKEN_USER,
    TokenGroups, TokenUser, WinBuiltinAdministratorsSid,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Pipes::{
    GetNamedPipeClientProcessId, GetNamedPipeServerProcessId, ImpersonateNamedPipeClient,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_USERS, RRF_RT_REG_MULTI_SZ, RRF_RT_REG_SZ, RegGetValueW,
};
use windows_sys::Win32::System::RemoteDesktop::{
    WTS_CURRENT_SERVER_HANDLE, WTS_SESSION_INFOW, WTS_SESSIONSTATE_UNLOCK, WTSActive,
    WTSDisconnectSession, WTSDomainName, WTSEnumerateSessionsW, WTSFreeMemory, WTSINFOEXW,
    WTSLogoffSession, WTSQuerySessionInformationW, WTSSendMessageW, WTSSessionInfoEx, WTSUserName,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, OpenProcess, OpenProcessToken, OpenThreadToken,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    QueryFullProcessImageNameW, TerminateProcess,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK};

use crate::agente::{Aviso, ProcessoVisto, Sistema};
use crate::motor::Motivo;

/// Quanto tempo a caixa de aviso fica aberta antes de fechar sozinha.
const AVISO_TIMEOUT_SEGUNDOS: u32 = 60;

pub(crate) fn ultimo_erro(o_que: &str) -> String {
    format!("{o_que}: {}", std::io::Error::last_os_error())
}

/// Fecha o handle ao sair do escopo.
pub(crate) struct Handle(pub(crate) HANDLE);

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

pub(crate) fn para_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Texto de uma `PWSTR` terminada em zero.
///
/// SAFETY: `p` deve apontar para uma string UTF-16 válida terminada em zero.
pub(crate) unsafe fn pwstr(p: *const u16) -> String {
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
/// SID do dono de um token.
fn sid_do_token(token: HANDLE) -> Option<String> {
    // SAFETY: token válido com TOKEN_QUERY; buffer alinhado para TOKEN_USER.
    unsafe {
        let mut tamanho = 0u32;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut tamanho);
        if tamanho == 0 {
            return None;
        }
        let mut buf = vec![0u64; (tamanho as usize).div_ceil(8)];
        if GetTokenInformation(
            token,
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
        sid_do_token(token.0)
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

/// Primeiro texto de um valor do registro (`REG_SZ`, ou o 1º item de um `REG_MULTI_SZ`).
fn texto_do_registro(raiz: HKEY, subchave: &str, valor: &str) -> Option<String> {
    let (sub, val) = (para_utf16(subchave), para_utf16(valor));
    let flags = RRF_RT_REG_SZ | RRF_RT_REG_MULTI_SZ;
    let mut bytes = 0u32;
    // SAFETY: primeira chamada só mede o tamanho.
    let erro = unsafe {
        RegGetValueW(
            raiz,
            sub.as_ptr(),
            val.as_ptr(),
            flags,
            null_mut(),
            null_mut(),
            &mut bytes,
        )
    };
    if erro != 0 || bytes == 0 {
        return None;
    }
    let mut buf = vec![0u16; (bytes as usize).div_ceil(2)];
    // SAFETY: buffer com `bytes` bytes graváveis.
    let erro = unsafe {
        RegGetValueW(
            raiz,
            sub.as_ptr(),
            val.as_ptr(),
            flags,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut bytes,
        )
    };
    if erro != 0 {
        return None;
    }
    let texto = de_utf16(&buf);
    (!texto.is_empty()).then_some(texto)
}

/// Idioma de exibição da conta com este SID, lido do perfil dela, que fica carregado enquanto ela
/// está logada (`HKEY_USERS\<SID>`): `PreferredUILanguages`, ou o `LocaleName` da conta. `None` se
/// a conta não está logada ou o idioma não é um dos suportados.
pub fn idioma_da_conta(sid: &str) -> Option<crate::i18n::Idioma> {
    use crate::i18n::Idioma;
    [
        (r"Control Panel\Desktop", "PreferredUILanguages"),
        (r"Control Panel\International", "LocaleName"),
    ]
    .iter()
    .filter_map(|(sub, valor)| texto_do_registro(HKEY_USERS, &format!(r"{sid}\{sub}"), valor))
    .find_map(|t| Idioma::de_tag(&t))
}

/// Título e mensagem do aviso à criança, no idioma da conta dela (ADR-010), lido do perfil
/// carregado; sem ele (conta fora da sessão, ou idioma que o kiosk não traduz), no do serviço.
pub fn texto_do_aviso(aviso: &crate::agente::Aviso) -> (String, String) {
    let idioma = idioma_da_conta(&aviso.sid).unwrap_or_else(crate::i18n::idioma);
    crate::textos::aviso(aviso, idioma)
}

/// `true` se o token tem o grupo Administradores em qualquer estado: ativo (elevado) ou só para
/// negação (conta de administrador sem elevação, com o UAC). Conta comum não tem o grupo.
fn token_e_de_conta_admin(token: HANDLE) -> Result<bool, String> {
    let mut admins = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut tamanho = SECURITY_MAX_SID_SIZE;
    // SAFETY: buffer do tamanho máximo de um SID.
    let ok = unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            null_mut(),
            admins.as_mut_ptr().cast(),
            &mut tamanho,
        )
    };
    if ok == 0 {
        return Err(ultimo_erro("CreateWellKnownSid(Administradores)"));
    }
    let mut tamanho = 0u32;
    // SAFETY: primeira chamada só mede o buffer.
    unsafe { GetTokenInformation(token, TokenGroups, null_mut(), 0, &mut tamanho) };
    if tamanho == 0 {
        return Err(ultimo_erro("GetTokenInformation(TokenGroups)"));
    }
    let mut buf = vec![0u64; (tamanho as usize).div_ceil(8)];
    // SAFETY: buffer alinhado com `tamanho` bytes.
    if unsafe {
        GetTokenInformation(
            token,
            TokenGroups,
            buf.as_mut_ptr().cast(),
            tamanho,
            &mut tamanho,
        )
    } == FALSE
    {
        return Err(ultimo_erro("GetTokenInformation(TokenGroups)"));
    }
    // SAFETY: preenchido acima; `GroupCount` entradas seguem o cabeçalho dentro de `buf`.
    let grupos = unsafe {
        let g = &*(buf.as_ptr() as *const TOKEN_GROUPS);
        std::slice::from_raw_parts(g.Groups.as_ptr(), g.GroupCount as usize)
    };
    // SAFETY: SIDs válidos (do token e o criado acima).
    Ok(grupos
        .iter()
        .any(|g| unsafe { EqualSid(g.Sid, admins.as_mut_ptr().cast()) } != FALSE))
}

/// Quem está do outro lado de um named pipe, lido do **token da conexão**, nunca da mensagem
/// (ADR-011). Só vale depois de o servidor ter lido do pipe. Personifica o cliente apenas para
/// abrir o token e volta à própria identidade na hora.
pub fn identidade_do_cliente_do_pipe(
    pipe: &impl std::os::windows::io::AsRawHandle,
) -> Result<crate::auditoria::Chamador, String> {
    let pipe: HANDLE = pipe.as_raw_handle();
    /// Volta à identidade do serviço ao sair do escopo, aconteça o que acontecer.
    struct Reverter;
    impl Drop for Reverter {
        fn drop(&mut self) {
            // SAFETY: desfaz a personificação da thread atual.
            unsafe { RevertToSelf() };
        }
    }
    // SAFETY: `pipe` é o handle de um pipe do servidor, vivo durante a chamada.
    if unsafe { ImpersonateNamedPipeClient(pipe) } == 0 {
        return Err(ultimo_erro("ImpersonateNamedPipeClient"));
    }
    let _reverter = Reverter;
    let mut token: HANDLE = null_mut();
    // SAFETY: `openasself` (TRUE) abre o token com a identidade do serviço, porque a do cliente
    // não pode abri-lo; o token é fechado por `Handle`.
    if unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, TRUE, &mut token) } == FALSE {
        return Err(ultimo_erro("OpenThreadToken"));
    }
    let token = Handle(token);
    let sid = sid_do_token(token.0).ok_or("SID do cliente ilegível")?;
    let conta_admin = token_e_de_conta_admin(token.0)?;
    Ok(crate::auditoria::Chamador { sid, conta_admin })
}

/// Caminho do executável do processo que está do outro lado de um named pipe (`kiosk-ui`,
/// ADR-004): quem tem direito de abrir o pipe é qualquer conta, mas só o `kiosk-ui.exe` instalado,
/// que a conta da criança não consegue trocar, vale como a UI. Só vale com o pipe conectado.
pub fn imagem_do_cliente_do_pipe(
    pipe: &impl std::os::windows::io::AsRawHandle,
) -> Result<std::path::PathBuf, String> {
    let pipe: HANDLE = pipe.as_raw_handle();
    let mut pid = 0u32;
    // SAFETY: `pipe` é o handle de um pipe do servidor, vivo durante a chamada.
    if unsafe { GetNamedPipeClientProcessId(pipe, &mut pid) } == 0 {
        return Err(ultimo_erro("GetNamedPipeClientProcessId"));
    }
    // SAFETY: abre só para consulta; o handle é fechado por `Handle`.
    let processo = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid) };
    if processo.is_null() {
        return Err(ultimo_erro("OpenProcess(cliente do pipe)"));
    }
    let processo = Handle(processo);
    let mut buf = vec![0u16; 32_768];
    let mut tamanho = buf.len() as u32;
    // SAFETY: buffer de `tamanho` unidades; `tamanho` volta com o comprimento escrito.
    if unsafe {
        QueryFullProcessImageNameW(
            processo.0,
            PROCESS_NAME_WIN32,
            buf.as_mut_ptr(),
            &mut tamanho,
        )
    } == 0
    {
        return Err(ultimo_erro("QueryFullProcessImageNameW"));
    }
    Ok(std::path::PathBuf::from(String::from_utf16_lossy(
        &buf[..tamanho as usize],
    )))
}

/// PID do processo que serve um pipe (para o cliente conferir que é o serviço).
pub fn pid_do_servidor_do_pipe(
    pipe: &impl std::os::windows::io::AsRawHandle,
) -> Result<u32, String> {
    let pipe: HANDLE = pipe.as_raw_handle();
    let mut pid = 0u32;
    // SAFETY: `pipe` é o handle de um pipe do cliente, vivo durante a chamada.
    if unsafe { GetNamedPipeServerProcessId(pipe, &mut pid) } == 0 {
        return Err(ultimo_erro("GetNamedPipeServerProcessId"));
    }
    Ok(pid)
}

/// Quem chama é administrador **elevado**: o grupo Administradores está ativo no token
/// efetivo (o da personificação, se houver). Com o UAC, uma conta de administrador fora do
/// "Executar como administrador" tem o grupo só para negação e dá `false`.
///
/// Lê os grupos do token em vez de usar `CheckTokenMembership(NULL, ...)`: sob personificação,
/// aquele abre o token com a identidade personificada e recebe "acesso negado".
pub fn administrador_elevado() -> Result<bool, String> {
    // atributos de SID_AND_ATTRIBUTES (winnt.h)
    const SE_GROUP_ENABLED: u32 = 0x4;
    const SE_GROUP_USE_FOR_DENY_ONLY: u32 = 0x10;

    let mut admins = [0u8; SECURITY_MAX_SID_SIZE as usize];
    let mut tamanho = SECURITY_MAX_SID_SIZE;
    // SAFETY: buffer do tamanho máximo de um SID.
    let ok = unsafe {
        CreateWellKnownSid(
            WinBuiltinAdministratorsSid,
            null_mut(),
            admins.as_mut_ptr().cast(),
            &mut tamanho,
        )
    };
    if ok == 0 {
        return Err(ultimo_erro("CreateWellKnownSid(Administradores)"));
    }

    let mut token: HANDLE = null_mut();
    // SAFETY: pseudo-handles da thread e do processo atuais; o token é fechado por `Handle`.
    // `openasself` usa a identidade do processo para abrir o token da personificação.
    let token = unsafe {
        if OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, TRUE, &mut token) == FALSE {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() != Some(ERROR_NO_TOKEN as i32) {
                return Err(format!("OpenThreadToken: {e}"));
            }
            // sem personificação: vale o token do processo
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == FALSE {
                return Err(ultimo_erro("OpenProcessToken"));
            }
        }
        Handle(token)
    };

    let mut tamanho = 0u32;
    // SAFETY: primeira chamada só mede o buffer.
    unsafe { GetTokenInformation(token.0, TokenGroups, null_mut(), 0, &mut tamanho) };
    if tamanho == 0 {
        return Err(ultimo_erro("GetTokenInformation(TokenGroups)"));
    }
    let mut buf = vec![0u64; (tamanho as usize).div_ceil(8)];
    // SAFETY: buffer alinhado com `tamanho` bytes.
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenGroups,
            buf.as_mut_ptr().cast(),
            tamanho,
            &mut tamanho,
        )
    } == FALSE
    {
        return Err(ultimo_erro("GetTokenInformation(TokenGroups)"));
    }
    // SAFETY: preenchido acima; `GroupCount` entradas seguem o cabeçalho dentro de `buf`.
    let grupos = unsafe {
        let g = &*(buf.as_ptr() as *const TOKEN_GROUPS);
        std::slice::from_raw_parts(g.Groups.as_ptr(), g.GroupCount as usize)
    };
    Ok(grupos.iter().any(|g| {
        // SAFETY: SIDs válidos (do token e o criado acima).
        let e_admins = unsafe { EqualSid(g.Sid, admins.as_mut_ptr().cast()) } != FALSE;
        e_admins
            && g.Attributes & SE_GROUP_ENABLED != 0
            && g.Attributes & SE_GROUP_USE_FOR_DENY_ONLY == 0
    }))
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

/// Adaptador real. O estado do sistema é consultado a cada chamada; só a bandeja (se houver) é
/// guardada aqui: as UIs conectadas e o caminho do `kiosk-ui.exe` que o serviço inicia.
#[derive(Default, Clone)]
pub struct SistemaWindows {
    ui: Option<(crate::ui_windows::GerenteUi, std::path::PathBuf)>,
    /// Avisos entregues à bandeja que ela ainda não confirmou (reserva, GC-17).
    pendentes: crate::avisos_pendentes::AvisosPendentes,
}

impl SistemaWindows {
    /// Com bandeja: o agente passa a supervisionar a UI de cada criança, iniciando `exe_ui` na
    /// sessão dela.
    pub fn com_ui(gerente: crate::ui_windows::GerenteUi, exe_ui: std::path::PathBuf) -> Self {
        Self {
            ui: Some((gerente, exe_ui)),
            pendentes: Default::default(),
        }
    }
}

/// A reserva (e o caminho de antes da bandeja): caixa de mensagem na sessão da criança.
fn caixa_de_mensagem(aviso: &Aviso) -> Result<(), String> {
    let id = sessao_do_sid(&aviso.sid)?;
    let (titulo, mensagem) = texto_do_aviso(aviso);
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

impl Sistema for SistemaWindows {
    fn tem_ui(&self) -> bool {
        self.ui.is_some()
    }

    fn batimento_da_ui(&mut self, sid: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        self.ui.as_ref()?.0.ultimo_batimento(sid)
    }

    fn iniciar_ui(&mut self, sid: &str) -> Result<(), String> {
        let (_, exe) = self.ui.as_ref().ok_or("sem bandeja configurada")?;
        let sessao = sessao_do_sid(sid)?;
        crate::lancador_windows::iniciar_na_sessao(sessao, exe).map(|_| ())
    }

    fn estado(&mut self, sid: &str, restante_segundos: Option<u64>, motivo: Option<Motivo>) {
        if let Some((gerente, _)) = &self.ui {
            // melhor esforço: o aviso é que não pode se perder, e esse tem reserva
            gerente.enviar(
                sid,
                crate::ipc::ParaUi::Estado {
                    restante_segundos,
                    motivo: motivo.map(|m| m.as_str().to_owned()),
                },
            );
        }
    }

    fn avisos_sem_confirmacao(
        &mut self,
        agora: chrono::DateTime<chrono::Utc>,
    ) -> Vec<(Aviso, Result<(), String>)> {
        let Some((gerente, _)) = &self.ui else {
            return Vec::new();
        };
        let vencidos = self
            .pendentes
            .vencidos(agora, |sid| gerente.ultimo_aviso_exibido(sid).is_some());
        vencidos
            .into_iter()
            .map(|aviso| {
                let r = caixa_de_mensagem(&aviso);
                (aviso, r)
            })
            .collect()
    }

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
        // com bandeja conectada, o aviso vai por ela e fica pendente de confirmação; sem ela (ou
        // com a fila cheia), cai direto na caixa de mensagem
        if let Some((gerente, _)) = &self.ui
            && gerente.enviar(&aviso.sid, crate::ipc::ParaUi::from(aviso))
        {
            self.pendentes.registrar(aviso.clone(), chrono::Utc::now());
            return Ok(());
        }
        caixa_de_mensagem(aviso)
    }
}
