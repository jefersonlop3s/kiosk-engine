//! Avisos entregues à bandeja e ainda não confirmados (E-004 F3, etapa 4, GC-17). A bandeja
//! confirma o que mostrou (`AvisoExibido`, ADR-004); se a confirmação não chega a tempo, o aviso
//! volta pela caixa de mensagem do Windows, para a criança nunca ficar sem saber. Puro e sem
//! relógio próprio, para ser testado fora do Windows.

use chrono::{DateTime, Utc};

use crate::agente::Aviso;

/// Quanto o serviço espera a confirmação da bandeja antes de usar a caixa de mensagem.
pub const CONFIRMACAO_SEGUNDOS: i64 = 6;

#[derive(Debug, Default, Clone)]
pub struct AvisosPendentes {
    itens: Vec<(Aviso, DateTime<Utc>)>,
}

impl AvisosPendentes {
    /// Anota um aviso entregue à bandeja em `agora`.
    pub fn registrar(&mut self, aviso: Aviso, agora: DateTime<Utc>) {
        self.itens.push((aviso, agora));
    }

    /// Quantos avisos aguardam confirmação.
    pub fn len(&self) -> usize {
        self.itens.len()
    }

    pub fn is_empty(&self) -> bool {
        self.itens.is_empty()
    }

    /// Tira da fila os confirmados (`confirmado(sid)`) e devolve os que passaram de
    /// [`CONFIRMACAO_SEGUNDOS`] sem confirmação, também tirados da fila.
    pub fn vencidos(
        &mut self,
        agora: DateTime<Utc>,
        confirmado: impl Fn(&str) -> bool,
    ) -> Vec<Aviso> {
        let mut vencidos = Vec::new();
        self.itens.retain(|(aviso, desde)| {
            if confirmado(&aviso.sid) {
                return false;
            }
            if (agora - *desde).num_seconds() >= CONFIRMACAO_SEGUNDOS {
                vencidos.push(aviso.clone());
                return false;
            }
            true
        });
        vencidos
    }
}
