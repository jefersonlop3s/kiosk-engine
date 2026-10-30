//! Protocolo serviço ↔ UI (ADR-004): mensagens, decodificação defensiva e monitor de batimento.
//! Independe do transporte (named pipe no Windows). A UI é tratada como entrada **não confiável**:
//! tamanho máximo, esquema fechado, versão exata e limite de taxa. A UI não envia comandos que
//! alterem regras ou dados; só confirma avisos e mostra que está viva.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::agente::Aviso;

pub const VERSAO_PROTOCOLO: u32 = 1;
/// Maior mensagem aceita (uma linha JSON, sem o `\n`).
pub const MAX_MENSAGEM_BYTES: usize = 4096;
pub const INTERVALO_BATIMENTO_SEGUNDOS: i64 = 5;
/// Batimentos seguidos sem chegar antes de a UI ser considerada parada (ADR-004: 3).
pub const BATIMENTOS_PERDIDOS_PARA_ALERTA: i64 = 3;
/// Mensagens por segundo que o serviço aceita da UI.
pub const MAX_MENSAGENS_POR_SEGUNDO: u32 = 10;

/// Do serviço para a UI. O texto em pt-BR é montado pela UI a partir destes campos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum ParaUi {
    /// Tempo restante para mostrar na bandeja; `None` = sem limite ativo.
    Estado {
        restante_segundos: Option<u64>,
        motivo: Option<String>,
    },
    Aviso {
        restante_segundos: u64,
        motivo: String,
        app: Option<String>,
    },
}

/// Da UI para o serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case", deny_unknown_fields)]
pub enum DaUi {
    Batimento {},
    /// A UI exibiu o aviso do marco indicado (300 ou 60 segundos).
    AvisoExibido {
        marco: u64,
    },
}

/// Só para serializar: `serde` não combina `flatten` com `deny_unknown_fields`, então a
/// decodificação (o lado hostil) confere versão e esquema fechado à mão.
#[derive(Serialize)]
struct Envelope<'a, T> {
    v: u32,
    #[serde(flatten)]
    msg: &'a T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroIpc {
    MuitoGrande(usize),
    NaoEUtf8,
    Json(String),
    VersaoIncompativel(u32),
    MarcoInvalido(u64),
}

impl std::fmt::Display for ErroIpc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MuitoGrande(n) => write!(f, "mensagem de {n} bytes excede {MAX_MENSAGEM_BYTES}"),
            Self::NaoEUtf8 => f.write_str("mensagem não é UTF-8"),
            Self::Json(e) => write!(f, "mensagem malformada: {e}"),
            Self::VersaoIncompativel(v) => write!(f, "versão {v} do protocolo não suportada"),
            Self::MarcoInvalido(m) => write!(f, "marco de aviso {m} inválido"),
        }
    }
}

impl std::error::Error for ErroIpc {}

/// Confere tamanho, UTF-8, objeto JSON e versão exata, e devolve o objeto sem o campo `v`. É o
/// que os dois lados fazem antes de olhar o tipo da mensagem.
fn desembrulhar(linha: &[u8]) -> Result<serde_json::Value, ErroIpc> {
    if linha.len() > MAX_MENSAGEM_BYTES {
        return Err(ErroIpc::MuitoGrande(linha.len()));
    }
    let texto = std::str::from_utf8(linha).map_err(|_| ErroIpc::NaoEUtf8)?;
    let mut valor: serde_json::Value =
        serde_json::from_str(texto.trim_end()).map_err(|e| ErroIpc::Json(e.to_string()))?;
    let objeto = valor
        .as_object_mut()
        .ok_or_else(|| ErroIpc::Json("esperado um objeto".into()))?;
    let versao = objeto
        .remove("v")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| ErroIpc::Json("campo `v` ausente ou inválido".into()))?;
    if versao != u64::from(VERSAO_PROTOCOLO) {
        return Err(ErroIpc::VersaoIncompativel(
            u32::try_from(versao).unwrap_or(u32::MAX),
        ));
    }
    Ok(valor)
}

/// Decodifica uma linha recebida da UI.
pub fn decodificar_da_ui(linha: &[u8]) -> Result<DaUi, ErroIpc> {
    let valor = desembrulhar(linha)?;
    let msg: DaUi = serde_json::from_value(valor).map_err(|e| ErroIpc::Json(e.to_string()))?;
    if let DaUi::AvisoExibido { marco } = msg
        // GC-08: os limiares são por regra, então qualquer marco positivo é
        // aceito; zero não faz sentido.
        && marco == 0
    {
        return Err(ErroIpc::MarcoInvalido(marco));
    }
    Ok(msg)
}

/// Decodifica uma linha recebida do serviço, do lado da UI. O serviço é de confiança, mas a UI
/// confere do mesmo jeito: uma linha ruim não pode derrubá-la nem mostrar texto fora do esquema.
pub fn decodificar_para_ui(linha: &[u8]) -> Result<ParaUi, ErroIpc> {
    let valor = desembrulhar(linha)?;
    serde_json::from_value(valor).map_err(|e| ErroIpc::Json(e.to_string()))
}

/// Serializa uma mensagem da UI para o serviço (uma linha terminada em `\n`).
pub fn codificar_da_ui(msg: &DaUi) -> Vec<u8> {
    let mut v = serde_json::to_vec(&Envelope {
        v: VERSAO_PROTOCOLO,
        msg,
    })
    .expect("mensagens do protocolo sempre serializam");
    v.push(b'\n');
    v
}

/// Serializa uma mensagem para a UI (uma linha terminada em `\n`).
pub fn codificar_para_ui(msg: &ParaUi) -> Vec<u8> {
    let mut v = serde_json::to_vec(&Envelope {
        v: VERSAO_PROTOCOLO,
        msg,
    })
    .expect("mensagens do protocolo sempre serializam");
    v.push(b'\n');
    v
}

impl From<&Aviso> for ParaUi {
    fn from(a: &Aviso) -> Self {
        Self::Aviso {
            restante_segundos: a.restante_segundos,
            motivo: a.motivo.as_str().to_owned(),
            app: a.app.clone(),
        }
    }
}

/// Janela fixa de 1 s: recusa mensagens além de `MAX_MENSAGENS_POR_SEGUNDO`.
#[derive(Debug, Default)]
pub struct LimiteDeTaxa {
    janela: Option<(DateTime<Utc>, u32)>,
}

impl LimiteDeTaxa {
    /// `true` se a mensagem em `agora` pode ser processada.
    pub fn permite(&mut self, agora: DateTime<Utc>) -> bool {
        match &mut self.janela {
            Some((ini, n)) if agora >= *ini && agora < *ini + Duration::seconds(1) => {
                if *n >= MAX_MENSAGENS_POR_SEGUNDO {
                    return false;
                }
                *n += 1;
                true
            }
            _ => {
                self.janela = Some((agora, 1));
                true
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoUi {
    /// Ainda sem nenhum batimento desde o início do monitoramento.
    Aguardando,
    Viva,
    Parada,
}

/// Transição que o serviço deve tratar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MudancaUi {
    /// Batimentos pararam: registrar `TAMPER_SUSPECTED` e relançar a UI (ADR-004).
    Perdida {
        segundos_sem_batimento: i64,
    },
    Recuperada,
}

/// Detecta a UI parada ou morta pela falta de batimentos.
#[derive(Debug)]
pub struct MonitorBatimento {
    desde: DateTime<Utc>,
    ultimo: Option<DateTime<Utc>>,
    estado: EstadoUi,
}

impl MonitorBatimento {
    /// `desde`: quando o serviço começou a esperar a UI (inicialização da UI conta como tolerância).
    pub fn novo(desde: DateTime<Utc>) -> Self {
        Self {
            desde,
            ultimo: None,
            estado: EstadoUi::Aguardando,
        }
    }

    pub fn estado(&self) -> EstadoUi {
        self.estado
    }

    pub fn batimento(&mut self, agora: DateTime<Utc>) -> Option<MudancaUi> {
        self.ultimo = Some(agora);
        let anterior = std::mem::replace(&mut self.estado, EstadoUi::Viva);
        (anterior == EstadoUi::Parada).then_some(MudancaUi::Recuperada)
    }

    /// Chamar a cada tick. Devolve `Perdida` uma única vez por parada.
    pub fn verificar(&mut self, agora: DateTime<Utc>) -> Option<MudancaUi> {
        let referencia = self.ultimo.unwrap_or(self.desde);
        let sem = (agora - referencia).num_seconds();
        let limite = INTERVALO_BATIMENTO_SEGUNDOS * BATIMENTOS_PERDIDOS_PARA_ALERTA;
        if sem > limite && self.estado != EstadoUi::Parada {
            self.estado = EstadoUi::Parada;
            return Some(MudancaUi::Perdida {
                segundos_sem_batimento: sem,
            });
        }
        None
    }

    /// Reinicia a espera depois de relançar a UI.
    pub fn reiniciar(&mut self, agora: DateTime<Utc>) {
        *self = Self::novo(agora);
    }
}
