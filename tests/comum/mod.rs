//! Conta local padrão (grupo Usuários) para os testes que provam o que uma conta sem privilégio
//! consegue ou não fazer. Só Windows; cada teste que a usa apaga a conta ao sair.
#![allow(dead_code)]

use std::process::{Command, Output};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    ImpersonateLoggedOnUser, LOGON32_LOGON_INTERACTIVE, LOGON32_PROVIDER_DEFAULT, LogonUserW,
    RevertToSelf,
};

pub fn texto(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

pub fn w(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Conta local padrão (grupo Usuários), apagada ao sair.
pub struct ContaPadrao {
    pub nome: String,
    pub senha: String,
}

impl ContaPadrao {
    /// A conta de nome fixo `kioskteste`.
    pub fn criar() -> Self {
        Self::com_nome("kioskteste")
    }

    /// Cria a conta local `nome` (até 20 caracteres), apagando uma anterior de mesmo nome.
    pub fn com_nome(nome: &str) -> Self {
        let nome = nome.to_string();
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

    pub fn token(&self) -> HANDLE {
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
pub fn como<T>(conta: &ContaPadrao, f: impl FnOnce() -> T) -> T {
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
