//! E-004 F3 e F4: o agente inicia a bandeja da criança, e a relança e registra quando ela some.

use chrono::{DateTime, Duration, TimeZone, Utc};
use kiosk_engine::agente::*;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use kiosk_engine::motor::Motivo;
use tempfile::TempDir;
use uuid::Uuid;

const CRIANCA: &str = "S-1-5-21-1000";
const ADULTO: &str = "S-1-5-21-1001";

/// Sistema com bandeja: o teste decide quando a UI "bate" e se iniciá-la falha.
#[derive(Default)]
struct ComUi {
    sem_ui: bool,
    sessoes: Vec<String>,
    batimento: Option<DateTime<Utc>>,
    iniciadas: Vec<String>,
    falha_ao_iniciar: bool,
    /// O que o agente empurrou para a dica da bandeja.
    estados: Vec<(String, Option<u64>)>,
    /// Avisos que a bandeja "não confirmou", com o resultado da reserva; entregues no próximo tick.
    reservas: Vec<(Aviso, Result<(), String>)>,
}

impl Sistema for ComUi {
    fn processos(&mut self) -> Result<Vec<ProcessoVisto>, String> {
        Ok(vec![])
    }
    fn encerrar(&mut self, _: u32) -> Result<(), String> {
        Ok(())
    }
    fn bloquear_sessao(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn sessoes_ativas(&mut self) -> Result<Vec<String>, String> {
        Ok(self.sessoes.clone())
    }
    fn encerrar_sessao(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn avisar(&mut self, _: &Aviso) -> Result<(), String> {
        Ok(())
    }
    fn tem_ui(&self) -> bool {
        !self.sem_ui
    }
    fn batimento_da_ui(&mut self, _: &str) -> Option<DateTime<Utc>> {
        self.batimento
    }
    fn iniciar_ui(&mut self, sid: &str) -> Result<(), String> {
        if self.falha_ao_iniciar {
            return Err("WTSQueryUserToken: sem sessão".into());
        }
        self.iniciadas.push(sid.to_owned());
        Ok(())
    }
    fn estado(&mut self, sid: &str, restante: Option<u64>, _: Option<Motivo>) {
        self.estados.push((sid.to_owned(), restante));
    }
    fn avisos_sem_confirmacao(&mut self, _: DateTime<Utc>) -> Vec<(Aviso, Result<(), String>)> {
        std::mem::take(&mut self.reservas)
    }
}

struct Mundo {
    _dir: TempDir,
    db: KioskDbManager,
    perfil: Uuid,
}

fn seg(n: i64) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap() + Duration::seconds(n)
}

async fn mundo() -> Mundo {
    mundo_com(None).await
}

async fn mundo_com(limite_total: Option<u64>) -> Mundo {
    let dir = tempfile::tempdir().unwrap();
    let db = KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
        .await
        .unwrap();
    let perfil = ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: CRIANCA.into(),
        display_name: "Filho".into(),
        daily_total_limit_seconds: limite_total,
    };
    db.create_profile(&EventContext::local(), &perfil)
        .await
        .unwrap();
    Mundo {
        _dir: dir,
        db,
        perfil: perfil.profile_id,
    }
}

impl Mundo {
    /// Eventos do tipo, com o perfil e o detalhe.
    async fn eventos(&self, tipo: &str) -> Vec<(Option<String>, String)> {
        sqlx::query_as(
            "SELECT profile_id, rejection_reason_detail FROM security_log \
             WHERE event_type = ? ORDER BY rowid",
        )
        .bind(tipo)
        .fetch_all(self.db.pool())
        .await
        .unwrap()
    }
}

fn agente(m: &Mundo, sistema: ComUi) -> Agente<'_, ComUi, Utc> {
    kiosk_engine::i18n::definir(kiosk_engine::i18n::Idioma::PtBr);
    Agente::new(&m.db, sistema, Utc)
}

/// Roda ticks de 2 s de `de` a `ate` (segundos), com a UI batendo a cada 5 s se `batendo`.
async fn rodar(a: &mut Agente<'_, ComUi, Utc>, de: i64, ate: i64, batendo: bool) {
    let mut t = de;
    while t <= ate {
        if batendo && t % 5 == 0 {
            a.sistema_mut().batimento = Some(seg(t));
        }
        a.tick(seg(t)).await;
        t += 1;
    }
}

#[tokio::test]
async fn a_sessao_da_crianca_ganha_a_bandeja_assim_que_aparece() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    a.tick(seg(0)).await;
    assert_eq!(a.sistema_mut().iniciadas, [CRIANCA]);
    assert!(m.eventos("TAMPER_SUSPECTED").await.is_empty());
}

#[tokio::test]
async fn bandeja_que_bate_nunca_e_relancada_nem_registrada() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    rodar(&mut a, 0, 600, true).await;
    assert_eq!(a.sistema_mut().iniciadas.len(), 1, "só o início");
    assert!(m.eventos("TAMPER_SUSPECTED").await.is_empty());
    assert!(m.eventos("INTERNAL_ERROR").await.is_empty());
}

#[tokio::test]
async fn bandeja_que_some_vira_tamper_ligado_ao_perfil_e_e_relancada() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    rodar(&mut a, 0, 60, true).await; // viva por um minuto
    assert_eq!(a.sistema_mut().iniciadas.len(), 1);
    // a criança fecha a bandeja à força: os batimentos param
    rodar(&mut a, 61, 90, false).await;
    let t = m.eventos("TAMPER_SUSPECTED").await;
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].0.as_deref(), Some(m.perfil.to_string().as_str()));
    assert!(t[0].1.contains("\"code\":\"UI_STOPPED\""), "{}", t[0].1);
    assert!(
        a.sistema_mut().iniciadas.len() >= 2,
        "a bandeja devia ter sido relançada"
    );
}

#[tokio::test]
async fn bandeja_relancada_que_volta_a_bater_nao_gera_novo_alerta() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    rodar(&mut a, 0, 30, true).await;
    rodar(&mut a, 31, 50, false).await; // sumiu
    assert_eq!(m.eventos("TAMPER_SUSPECTED").await.len(), 1);
    rodar(&mut a, 51, 400, true).await; // a nova bandeja bate
    assert_eq!(m.eventos("TAMPER_SUSPECTED").await.len(), 1);
}

#[tokio::test]
async fn crianca_que_fecha_a_bandeja_sempre_nao_enche_o_log() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    // nenhum batimento por 10 minutos: o agente relança de 15 em 15 s, mas registra pouco
    rodar(&mut a, 0, 600, false).await;
    let n = m.eventos("TAMPER_SUSPECTED").await.len();
    assert!(
        (2..=3).contains(&n),
        "no máximo um a cada {INTERVALO_ALERTA_UI_SEGUNDOS} s em 600 s: {n}"
    );
    assert!(a.sistema_mut().iniciadas.len() > 10, "segue relançando");
}

#[tokio::test]
async fn falha_ao_iniciar_a_bandeja_e_registrada_com_limite_e_nao_para_o_agente() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            falha_ao_iniciar: true,
            ..Default::default()
        },
    );
    rodar(&mut a, 0, 200, false).await;
    let e = m.eventos("INTERNAL_ERROR").await;
    assert_eq!(
        e.len(),
        1,
        "uma falha por janela de {INTERVALO_ALERTA_UI_SEGUNDOS} s: {e:?}"
    );
    assert_eq!(e[0].0.as_deref(), Some(m.perfil.to_string().as_str()));
    assert!(e[0].1.contains("UI_NOT_STARTED"), "{}", e[0].1);
    // depois de passar a janela, registra de novo
    rodar(&mut a, 201, 700, false).await;
    assert!(m.eventos("INTERNAL_ERROR").await.len() >= 2);
}

#[tokio::test]
async fn conta_sem_perfil_nao_ganha_bandeja() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![ADULTO.into()],
            ..Default::default()
        },
    );
    rodar(&mut a, 0, 120, false).await;
    assert!(a.sistema_mut().iniciadas.is_empty());
    assert!(m.eventos("TAMPER_SUSPECTED").await.is_empty());
}

#[tokio::test]
async fn sistema_sem_bandeja_nao_e_supervisionado() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sem_ui: true,
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    rodar(&mut a, 0, 120, false).await;
    assert!(a.sistema_mut().iniciadas.is_empty());
    assert!(m.eventos("TAMPER_SUSPECTED").await.is_empty());
}

#[tokio::test]
async fn sessao_que_fecha_e_volta_ganha_a_bandeja_de_novo() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    rodar(&mut a, 0, 20, true).await;
    assert_eq!(a.sistema_mut().iniciadas.len(), 1);
    // a criança sai da sessão: sem sessão, sem alerta de bandeja parada
    a.sistema_mut().sessoes.clear();
    rodar(&mut a, 21, 120, false).await;
    assert!(m.eventos("TAMPER_SUSPECTED").await.is_empty());
    // e volta: a bandeja é iniciada de novo
    a.sistema_mut().sessoes.push(CRIANCA.into());
    a.sistema_mut().batimento = None;
    rodar(&mut a, 121, 125, true).await;
    assert_eq!(a.sistema_mut().iniciadas.len(), 2);
}

#[tokio::test]
async fn os_textos_do_alerta_saem_nos_tres_idiomas() {
    use kiosk_engine::detalhes::{Detalhe, legivel};
    use kiosk_engine::i18n::Idioma;
    let parada = Detalhe::UiStopped { seconds: 16 }.codigo();
    let falha = Detalhe::UiNotStarted { error: "x".into() }.codigo();
    assert!(legivel(&parada, Idioma::PtBr).contains("parou de dar sinal"));
    assert!(legivel(&parada, Idioma::EnUs).contains("stopped responding"));
    assert!(legivel(&parada, Idioma::EsEs).contains("dejó de responder"));
    assert!(legivel(&falha, Idioma::PtBr).contains("não foi possível iniciar a bandeja"));
    assert!(legivel(&falha, Idioma::EnUs).contains("could not start the tray"));
    assert!(legivel(&falha, Idioma::EsEs).contains("no se pudo iniciar la bandeja"));
}

#[tokio::test]
async fn o_tempo_restante_da_sessao_vai_para_a_bandeja_so_quando_muda_de_balde() {
    let m = mundo_com(Some(700)).await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    for t in [0, 300, 600, 602, 620] {
        a.tick(seg(t)).await;
    }
    // acima de 5 min muda a cada minuto; abaixo, a cada 15 s (602 s cai no balde de 600 s)
    let restantes: Vec<_> = a.sistema_mut().estados.iter().map(|(_, r)| *r).collect();
    assert_eq!(restantes, [Some(700), Some(400), Some(100), Some(80)]);
}

#[tokio::test]
async fn sem_limite_de_sessao_a_dica_fica_quieta() {
    let m = mundo().await;
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            ..Default::default()
        },
    );
    for t in [0, 60, 120] {
        a.tick(seg(t)).await;
    }
    assert!(a.sistema_mut().estados.is_empty());
}

#[tokio::test]
async fn aviso_que_a_bandeja_nao_confirmou_vira_falha_ligada_ao_perfil() {
    let m = mundo().await;
    let aviso = Aviso {
        sid: CRIANCA.into(),
        app: Some("jogo.exe".into()),
        restante_segundos: 300,
        motivo: Motivo::LimiteDiarioApp,
    };
    let mut a = agente(
        &m,
        ComUi {
            sessoes: vec![CRIANCA.into()],
            reservas: vec![
                (aviso.clone(), Ok(())),
                (aviso, Err("WTSSendMessageW: sem sessão".into())),
            ],
            ..Default::default()
        },
    );
    let r = a.tick(seg(0)).await;
    assert_eq!(r.erros.len(), 2, "{:?}", r.erros);
    let e = m.eventos("INTERNAL_ERROR").await;
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(e.iter().all(|(p, d)| {
        p.as_deref() == Some(m.perfil.to_string().as_str()) && d.contains("UI_NOT_CONFIRMED")
    }));
    assert!(e[1].1.contains("também falhou"), "{}", e[1].1);
}
