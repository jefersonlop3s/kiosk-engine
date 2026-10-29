//! O ícone da bandeja (`Shell_NotifyIconW`) numa janela escondida, com um laço de mensagens.
//!
//! O ícone só mostra: a dica com o tempo restante e o balão de aviso. **Não tem menu nem "sair"**:
//! quem decide se a UI vive é o serviço (ADR-001), e a criança que a fechar à força deixa o
//! batimento de fazer chegar e o serviço a relança e registra. Os comandos vêm de outra thread (a
//! da conexão com o serviço) por uma fila, e uma mensagem de janela acorda o laço.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::mpsc::UnboundedSender;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_TIP, NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, IDI_INFORMATION,
    LoadIconW, MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW,
    TranslateMessage, WM_APP, WM_DESTROY, WNDCLASSW,
};

use crate::ipc::DaUi;

/// O que a thread da conexão pede ao ícone.
pub enum Comando {
    /// Troca a dica (o tempo restante).
    Dica(String),
    /// Mostra um balão de aviso e, se o Windows o aceitou, confirma ao serviço.
    Balao {
        titulo: String,
        corpo: String,
        confirmacao: DaUi,
    },
}

const WM_COMANDO: u32 = WM_APP + 1;
const ID_DO_ICONE: u32 = 1;

type Fila = Arc<Mutex<VecDeque<Comando>>>;

/// Estado que a função da janela precisa e que o Windows não deixa passar por parâmetro.
struct Estado {
    fila: Fila,
    confirmar: UnboundedSender<DaUi>,
    dica: Mutex<String>,
}

static ESTADO: OnceLock<Estado> = OnceLock::new();
/// Número da mensagem `TaskbarCreated`: o Windows a manda quando a barra de tarefas (re)nasce, e
/// o ícone some junto; é preciso adicioná-lo de novo.
static TASKBAR_CRIADA: AtomicU32 = AtomicU32::new(0);

fn largo(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Copia `texto` para um campo fixo do Windows, cortando para caber e deixando o zero final.
fn copiar(destino: &mut [u16], texto: &str) {
    let mut n = 0;
    for u in texto.encode_utf16().take(destino.len() - 1) {
        destino[n] = u;
        n += 1;
    }
    destino[n] = 0;
}

fn dados(hwnd: HWND, bandeiras: u32) -> NOTIFYICONDATAW {
    // SAFETY: estrutura de campos numéricos e vetores; zerada é um valor válido.
    let mut d: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    d.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    d.hWnd = hwnd;
    d.uID = ID_DO_ICONE;
    d.uFlags = bandeiras;
    d
}

fn adicionar_icone(hwnd: HWND, dica: &str) -> bool {
    let mut d = dados(hwnd, NIF_ICON | NIF_TIP);
    // SAFETY: ícone do sistema, que não precisa ser liberado.
    d.hIcon = unsafe { LoadIconW(std::ptr::null_mut(), IDI_INFORMATION) };
    copiar(&mut d.szTip, dica);
    // SAFETY: `d` é uma NOTIFYICONDATAW preenchida acima.
    unsafe { Shell_NotifyIconW(NIM_ADD, &d) != 0 }
}

fn aplicar(hwnd: HWND, estado: &Estado, comando: Comando) {
    match comando {
        Comando::Dica(texto) => {
            let mut d = dados(hwnd, NIF_TIP);
            copiar(&mut d.szTip, &texto);
            // SAFETY: `d` é uma NOTIFYICONDATAW preenchida acima.
            unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) };
            *estado.dica.lock().unwrap_or_else(|e| e.into_inner()) = texto;
        }
        Comando::Balao {
            titulo,
            corpo,
            confirmacao,
        } => {
            let mut d = dados(hwnd, NIF_INFO);
            copiar(&mut d.szInfoTitle, &titulo);
            copiar(&mut d.szInfo, &corpo);
            d.dwInfoFlags = NIIF_WARNING;
            // SAFETY: `d` é uma NOTIFYICONDATAW preenchida acima.
            if unsafe { Shell_NotifyIconW(NIM_MODIFY, &d) } != 0 {
                let _ = estado.confirmar.send(confirmacao);
            }
        }
    }
}

unsafe extern "system" fn janela(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if let Some(estado) = ESTADO.get() {
        if msg == WM_COMANDO {
            let pendentes: Vec<Comando> = estado
                .fila
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain(..)
                .collect();
            for c in pendentes {
                aplicar(hwnd, estado, c);
            }
            return 0;
        }
        if msg != 0 && msg == TASKBAR_CRIADA.load(Ordering::Relaxed) {
            let dica = estado
                .dica
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            adicionar_icone(hwnd, &dica);
            return 0;
        }
    }
    if msg == WM_DESTROY {
        let d = dados(hwnd, 0);
        // SAFETY: remove o ícone desta janela e encerra o laço.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &d);
            PostQuitMessage(0);
        }
        return 0;
    }
    // SAFETY: repassa a mensagem à função padrão do Windows.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Quem manda comandos ao ícone, de qualquer thread.
#[derive(Clone)]
pub struct Remetente {
    hwnd: isize,
    fila: Fila,
}

impl Remetente {
    pub fn enviar(&self, c: Comando) {
        self.fila
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push_back(c);
        // SAFETY: o `hwnd` é de uma janela deste processo; se ela já morreu, a chamada só falha.
        unsafe { PostMessageW(self.hwnd as HWND, WM_COMANDO, 0, 0) };
    }
}

/// A janela escondida e o ícone, prontos para o laço de mensagens.
pub struct Bandeja {
    hwnd: HWND,
    remetente: Remetente,
}

impl Bandeja {
    /// Cria a janela e o ícone com a dica inicial. `confirmar` recebe a confirmação de cada balão
    /// que o Windows aceitou. Só uma bandeja por processo.
    pub fn criar(dica: &str, confirmar: UnboundedSender<DaUi>) -> Result<Self, String> {
        let fila: Fila = Arc::default();
        ESTADO
            .set(Estado {
                fila: fila.clone(),
                confirmar,
                dica: Mutex::new(dica.to_owned()),
            })
            .map_err(|_| "a bandeja já foi criada neste processo".to_owned())?;
        let classe = largo("KioskBandeja");
        // SAFETY: registra uma classe de janela com função própria e cria uma janela escondida
        // (sem WS_VISIBLE); textos terminados em zero, vivos durante as chamadas.
        let hwnd = unsafe {
            let instancia = GetModuleHandleW(std::ptr::null());
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(janela);
            wc.hInstance = instancia;
            wc.lpszClassName = classe.as_ptr();
            if RegisterClassW(&wc) == 0 {
                return Err(format!(
                    "RegisterClassW: {}",
                    std::io::Error::last_os_error()
                ));
            }
            TASKBAR_CRIADA.store(
                RegisterWindowMessageW(largo("TaskbarCreated").as_ptr()),
                Ordering::Relaxed,
            );
            CreateWindowExW(
                0,
                classe.as_ptr(),
                largo("Kiosk").as_ptr(),
                0,
                0,
                0,
                0,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instancia,
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(format!(
                "CreateWindowExW: {}",
                std::io::Error::last_os_error()
            ));
        }
        // sem a barra de tarefas (ainda subindo), o ícone entra quando ela nascer (TaskbarCreated)
        adicionar_icone(hwnd, dica);
        Ok(Self {
            hwnd,
            remetente: Remetente {
                hwnd: hwnd as isize,
                fila,
            },
        })
    }

    pub fn remetente(&self) -> Remetente {
        self.remetente.clone()
    }

    /// O laço de mensagens; só volta se a janela for destruída.
    pub fn executar(self) {
        // SAFETY: laço padrão de mensagens da thread que criou a janela.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            DestroyWindow(self.hwnd);
        }
    }
}
