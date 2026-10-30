//! E-004 (parte independente de SO): o laço do agente contra SQLite real e um `Sistema` falso.

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use kiosk_engine::agente::*;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use kiosk_engine::motor::{Decisao, Instante, Motivo};
use tempfile::TempDir;
use uuid::Uuid;

const SID: &str = "S-1-5-21-1000";
const OUTRO_SID: &str = "S-1-5-18";

#[derive(Default)]
struct Fake {
    processos: Vec<ProcessoVisto>,
    encerrados: Vec<u32>,
    tentativas_encerrar: u32,
    bloqueios: Vec<String>,
    avisos: Vec<Aviso>,
    falha_encerrar: bool,
    falha_avisar: bool,
    falha_listar: bool,
    sessoes: Vec<String>,
    sessoes_encerradas: Vec<String>,
    falha_sessoes: bool,
}

impl Sistema for Fake {
    fn processos(&mut self) -> Result<Vec<ProcessoVisto>, String> {
        if self.falha_listar {
            Err("toolhelp indisponível".into())
        } else {
            Ok(self.processos.clone())
        }
    }
    fn encerrar(&mut self, pid: u32) -> Result<(), String> {
        self.tentativas_encerrar += 1;
        if self.falha_encerrar {
            return Err("acesso negado".into());
        }
        self.encerrados.push(pid);
        Ok(())
    }
    fn bloquear_sessao(&mut self, sid: &str) -> Result<(), String> {
        self.bloqueios.push(sid.to_owned());
        Ok(())
    }
    fn sessoes_ativas(&mut self) -> Result<Vec<String>, String> {
        if self.falha_sessoes {
            Err("wts indisponível".into())
        } else {
            Ok(self.sessoes.clone())
        }
    }
    fn encerrar_sessao(&mut self, sid: &str) -> Result<(), String> {
        self.sessoes_encerradas.push(sid.to_owned());
        Ok(())
    }
    fn avisar(&mut self, aviso: &Aviso) -> Result<(), String> {
        if self.falha_avisar {
            return Err("ui ausente".into());
        }
        self.avisos.push(aviso.clone());
        Ok(())
    }
    fn contas_locais(&mut self) -> Result<Vec<ContaLocal>, String> {
        Ok(vec![])
    }
    fn atalhos_do_menu(&mut self) -> Result<Vec<ProgramaSugerido>, String> {
        Ok(vec![])
    }
}

struct Mundo {
    _dir: TempDir,
    db: KioskDbManager,
    perfil: Uuid,
    jogo: AppMetadata,
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap() // segunda-feira
}

fn proc(pid: u32, nome: &str, sid: &str) -> ProcessoVisto {
    ProcessoVisto {
        pid,
        process_name: nome.into(),
        sid: sid.into(),
    }
}

async fn mundo(limite_total: Option<u64>) -> Mundo {
    let dir = tempfile::tempdir().unwrap();
    let db = KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
        .await
        .unwrap();
    let ctx = EventContext::local();
    let perfil = ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: SID.into(),
        display_name: "Filho".into(),
        daily_total_limit_seconds: limite_total,
    };
    let jogo = AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: "jogo.exe".into(),
        status: ProcessStatus::Monitored,
    };
    db.create_profile(&ctx, &perfil).await.unwrap();
    db.create_app(&ctx, &jogo).await.unwrap();
    Mundo {
        _dir: dir,
        db,
        perfil: perfil.profile_id,
        jogo,
    }
}

impl Mundo {
    async fn regra(&self, app: &AppMetadata, limite: Option<u64>) {
        let r = ActiveRule {
            rule_id: Uuid::new_v4(),
            profile_id: self.perfil,
            app_id: app.app_id,
            creation_timestamp: Utc::now(),
            state: RuleState::Active,
            daily_limit_seconds: limite,
            time_window: None,
            effective_until_timestamp: None,
            warning_seconds: None,
        };
        self.db
            .create_active_rule(&EventContext::local(), &r)
            .await
            .unwrap();
    }

    async fn eventos(&self, tipo: &str) -> i64 {
        sqlx::query_scalar("SELECT count(*) FROM security_log WHERE event_type = ?")
            .bind(tipo)
            .fetch_one(self.db.pool())
            .await
            .unwrap()
    }
}

fn agente(m: &Mundo) -> Agente<'_, Fake, Utc> {
    // os diagnósticos do relatório saem no idioma do processo; estes testes leem em pt-BR
    kiosk_engine::i18n::definir(kiosk_engine::i18n::Idioma::PtBr);
    Agente::new(&m.db, Fake::default(), Utc)
}

fn seg(n: i64) -> DateTime<Utc> {
    t0() + Duration::seconds(n)
}

#[tokio::test]
async fn processo_de_conta_sem_perfil_e_ignorado() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(0)).await;
    let base = m.eventos("SUCCESS").await; // eventos da preparação
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(10, "jogo.exe", OUTRO_SID)];
    for i in 0..3 {
        let r = a.tick(seg(i * 300)).await;
        assert!(r.acoes.is_empty() && r.erros.is_empty());
    }
    assert!(a.sistema_mut().encerrados.is_empty());
    assert_eq!(m.eventos("SUCCESS").await, base);
    let (n,): (i64,) = sqlx::query_as("SELECT count(*) FROM usage_ledger")
        .fetch_one(m.db.pool())
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn ciclo_completo_do_limite_avisa_uma_vez_por_marco_e_depois_encerra() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(1000)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(42, "JOGO.EXE", SID)]; // caixa diferente do cadastro

    for (quando, nome) in [(0, "t0"), (300, "300"), (600, "600")] {
        let r = a.tick(seg(quando)).await;
        assert!(r.acoes.is_empty() && r.erros.is_empty(), "{nome}: {r:?}");
    }
    let r = a.tick(seg(900)).await; // restam 100 s -> marco de 5 min
    assert_eq!(
        r.acoes,
        [Acao::Avisou {
            sid: SID.into(),
            app: Some("jogo.exe".into()),
            restante_segundos: 100,
            motivo: Motivo::LimiteDiarioApp
        }]
    );
    assert!(
        a.tick(seg(910)).await.acoes.is_empty(),
        "o mesmo marco não repete"
    );
    let r = a.tick(seg(960)).await; // restam 40 s -> marco de 1 min
    assert!(matches!(
        r.acoes[..],
        [Acao::Avisou {
            restante_segundos: 40,
            ..
        }]
    ));
    assert!(a.tick(seg(970)).await.acoes.is_empty());

    let r = a.tick(seg(1040)).await; // estourou
    assert_eq!(
        r.acoes,
        [Acao::Encerrou {
            pid: 42,
            app: "JOGO.EXE".into(),
            motivo: Motivo::LimiteDiarioApp
        }]
    );
    assert_eq!(a.sistema_mut().encerrados, [42]);
    assert_eq!(
        (
            m.eventos("WARNING_SHOWN").await,
            m.eventos("APP_TERMINATED").await
        ),
        (2, 1)
    );
    assert_eq!(a.sistema_mut().avisos.len(), 2);

    // o processo segue na lista: não é morto de novo durante a tolerância e só depois de 5 ticks
    for i in 1..=5 {
        assert!(a.tick(seg(1040 + i * 2)).await.acoes.is_empty());
    }
    let r = a.tick(seg(1040 + 12)).await;
    assert_eq!(r.acoes.len(), 1);
    assert_eq!(a.sistema_mut().encerrados, [42, 42]);
}

#[tokio::test]
async fn controle_negativo_app_isento_nunca_e_encerrado() {
    let m = mundo(None).await;
    let video = AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: "video.exe".into(),
        status: ProcessStatus::Whitelisted,
    };
    m.db.create_app(&EventContext::local(), &video)
        .await
        .unwrap();
    m.regra(&video, Some(0)).await; // regra que o bloquearia, se não fosse isento
    let antes = m.eventos("APP_TERMINATED").await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(7, "video.exe", SID)];
    for i in 0..10 {
        let r = a.tick(seg(i * 300)).await;
        assert!(r.acoes.is_empty() && r.erros.is_empty(), "{r:?}");
    }
    assert!(a.sistema_mut().encerrados.is_empty());
    assert_eq!(m.eventos("APP_TERMINATED").await, antes);
}

#[tokio::test]
async fn falha_repetida_ao_encerrar_vira_tamper_e_o_agente_para_de_tentar() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(0)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(9, "jogo.exe", SID)];
    a.sistema_mut().falha_encerrar = true;

    for i in 0..3 {
        let r = a.tick(seg(i * 2)).await;
        assert!(r.acoes.is_empty());
        assert!(!r.erros.is_empty(), "falha deve ser reportada no tick {i}");
    }
    assert_eq!(a.sistema_mut().tentativas_encerrar, MAX_TENTATIVAS_ENCERRAR);
    assert_eq!(m.eventos("TAMPER_SUSPECTED").await, 1);
    assert!(m.eventos("INTERNAL_ERROR").await >= 3);

    for i in 3..8 {
        a.tick(seg(i * 2)).await;
    }
    assert_eq!(
        a.sistema_mut().tentativas_encerrar,
        MAX_TENTATIVAS_ENCERRAR,
        "desistiu"
    );
    assert_eq!(m.eventos("TAMPER_SUSPECTED").await, 1, "registrado uma vez");
}

#[tokio::test]
async fn limite_total_avisa_uma_vez_bloqueia_duas_vezes_e_depois_encerra_a_sessao() {
    let m = mundo(Some(700)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(1, "jogo.exe", SID)]; // sem regra de app: só a sessão conta
    a.sistema_mut().sessoes = vec![SID.into()];

    assert!(a.tick(seg(0)).await.acoes.is_empty());
    assert!(a.tick(seg(300)).await.acoes.is_empty());
    let r = a.tick(seg(600)).await; // sessão ativa por 600 s: restam 100
    assert_eq!(
        r.acoes,
        [Acao::Avisou {
            sid: SID.into(),
            app: None,
            restante_segundos: 100,
            motivo: Motivo::LimiteDiarioTotal
        }]
    );
    let r = a.tick(seg(900)).await; // 900 s >= 700: primeiro bloqueio
    assert_eq!(
        r.acoes,
        [Acao::Bloqueou {
            sid: SID.into(),
            motivo: Motivo::LimiteDiarioTotal
        }]
    );
    assert!(
        a.tick(seg(910)).await.acoes.is_empty(),
        "dentro do intervalo de rebloqueio"
    );
    let r = a.tick(seg(940)).await; // a criança desbloqueou: segundo bloqueio
    assert!(matches!(r.acoes[..], [Acao::Bloqueou { .. }]));
    assert!(a.tick(seg(960)).await.acoes.is_empty());
    let r = a.tick(seg(970)).await; // desbloqueou de novo: logoff
    assert_eq!(
        r.acoes,
        [Acao::EncerrouSessao {
            sid: SID.into(),
            motivo: Motivo::LimiteDiarioTotal
        }]
    );
    let f = a.sistema_mut();
    assert_eq!((f.bloqueios.len(), f.sessoes_encerradas.len()), (2, 1));
    assert_eq!(
        (
            m.eventos("SESSION_LOCKED").await,
            m.eventos("WARNING_SHOWN").await
        ),
        (3, 1)
    );
}

#[tokio::test]
async fn sessao_ja_bloqueada_nao_e_rebloqueada_e_cada_desbloqueio_conta() {
    let m = mundo(Some(700)).await;
    let ctx = EventContext::local();
    let base = seg(0) - Duration::hours(2);
    for k in 0..2 {
        let ini = base + Duration::seconds(k * 400);
        m.db.registrar_sessao(&ctx, m.perfil, ini, ini + Duration::seconds(400), &Utc)
            .await
            .unwrap();
    }
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(3, "explorer.exe", SID)]; // a sessão bloqueada segue com processos
    a.sistema_mut().sessoes = vec![SID.into()];
    assert!(matches!(
        a.tick(seg(0)).await.acoes[..],
        [Acao::Bloqueou { .. }]
    ));

    a.sistema_mut().sessoes = vec![]; // bloqueada: não está mais ativa
    assert!(a.tick(seg(60)).await.acoes.is_empty());
    assert!(a.tick(seg(120)).await.acoes.is_empty());

    a.sistema_mut().sessoes = vec![SID.into()]; // a criança desbloqueou (1)
    assert!(matches!(
        a.tick(seg(150)).await.acoes[..],
        [Acao::Bloqueou { .. }]
    ));
    a.sistema_mut().sessoes = vec![];
    assert!(a.tick(seg(200)).await.acoes.is_empty());
    a.sistema_mut().sessoes = vec![SID.into()]; // desbloqueou (2)
    assert!(matches!(
        a.tick(seg(250)).await.acoes[..],
        [Acao::EncerrouSessao { .. }]
    ));
    assert_eq!(a.sistema_mut().bloqueios.len(), 2);
}

#[tokio::test]
async fn restricao_que_acaba_zera_a_contagem_e_o_proximo_bloqueio_nao_vira_logoff() {
    let m = mundo(Some(700)).await;
    let ctx = EventContext::local();
    let base = seg(0) - Duration::hours(2);
    for k in 0..2 {
        let ini = base + Duration::seconds(k * 400);
        m.db.registrar_sessao(&ctx, m.perfil, ini, ini + Duration::seconds(400), &Utc)
            .await
            .unwrap();
    }
    let mut a = agente(&m);
    a.sistema_mut().sessoes = vec![SID.into()];
    assert!(matches!(
        a.tick(seg(0)).await.acoes[..],
        [Acao::Bloqueou { .. }]
    )); // dia 1, bloqueio 1
    a.sistema_mut().sessoes = vec![];
    a.tick(seg(50)).await;
    a.sistema_mut().sessoes = vec![SID.into()];
    assert!(matches!(
        a.tick(seg(100)).await.acoes[..],
        [Acao::Bloqueou { .. }]
    )); // dia 1, bloqueio 2 (a contagem chega ao limite)

    a.sistema_mut().sessoes = vec![];
    a.tick(seg(150)).await;
    let dia2 = 86_400 + 3_600;
    a.sistema_mut().sessoes = vec![SID.into()];
    assert!(
        a.tick(seg(dia2)).await.acoes.is_empty(),
        "dia novo: total zerado, permitido"
    );

    let base2 = seg(dia2) - Duration::seconds(800); // 800 s de sessão no dia 2
    for k in 0..2 {
        let ini = base2 + Duration::seconds(k * 400);
        m.db.registrar_sessao(&ctx, m.perfil, ini, ini + Duration::seconds(400), &Utc)
            .await
            .unwrap();
    }
    let r = a.tick(seg(dia2 + 100)).await;
    assert!(matches!(r.acoes[..], [Acao::Bloqueou { .. }]), "{r:?}");
    assert!(a.sistema_mut().sessoes_encerradas.is_empty());
}

#[tokio::test]
async fn tempo_de_sessao_so_conta_com_a_sessao_ativa_nos_dois_ticks() {
    let m = mundo(None).await;
    let mut a = agente(&m);
    let dia = Instante::em(seg(0), &Utc).local.date();
    a.sistema_mut().sessoes = vec![SID.into()];
    a.tick(seg(0)).await;
    a.tick(seg(300)).await; // +300
    a.sistema_mut().sessoes = vec![];
    a.tick(seg(600)).await; // bloqueada: não conta
    a.sistema_mut().sessoes = vec![SID.into()];
    a.tick(seg(900)).await; // acabou de voltar: ainda não conta
    a.tick(seg(1200)).await; // +300
    let c =
        m.db.consumo_do_dia(m.perfil, m.jogo.app_id, dia)
            .await
            .unwrap();
    assert_eq!(c.total_segundos, 600);
}

#[tokio::test]
async fn uso_simultaneo_de_dois_apps_nao_conta_em_dobro_no_total() {
    let m = mundo(None).await;
    let video = AppMetadata {
        app_id: Uuid::new_v4(),
        process_name: "video.exe".into(),
        status: ProcessStatus::Monitored,
    };
    m.db.create_app(&EventContext::local(), &video)
        .await
        .unwrap();
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(1, "jogo.exe", SID), proc(2, "video.exe", SID)];
    a.sistema_mut().sessoes = vec![SID.into()];
    a.tick(seg(0)).await;
    a.tick(seg(300)).await;
    let dia = Instante::em(seg(0), &Utc).local.date();
    let j =
        m.db.consumo_do_dia(m.perfil, m.jogo.app_id, dia)
            .await
            .unwrap();
    let v =
        m.db.consumo_do_dia(m.perfil, video.app_id, dia)
            .await
            .unwrap();
    assert_eq!((j.app_segundos, v.app_segundos), (300, 300));
    assert_eq!(
        j.total_segundos, 300,
        "o total é o tempo de sessão, não 600"
    );
}

#[tokio::test]
async fn falha_ao_listar_sessoes_e_reportada_e_o_enforcement_de_apps_continua() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(0)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(5, "jogo.exe", SID)];
    a.sistema_mut().falha_sessoes = true;
    let r = a.tick(seg(0)).await;
    assert!(
        r.erros.iter().any(|e| e.contains("sessões")),
        "{:?}",
        r.erros
    );
    assert!(matches!(r.acoes[..], [Acao::Encerrou { .. }]));
    assert_eq!(m.eventos("INTERNAL_ERROR").await, 1);
}

#[tokio::test]
async fn sessao_e_bloqueada_mesmo_sem_nenhum_app_controlado_em_execucao() {
    let m = mundo(Some(700)).await;
    let ctx = EventContext::local();
    let dia = seg(0) - Duration::hours(2);
    for k in 0..2 {
        let ini = dia + Duration::seconds(k * 400);
        m.db.registrar_sessao(&ctx, m.perfil, ini, ini + Duration::seconds(400), &Utc)
            .await
            .unwrap();
    }
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(3, "explorer.exe", SID)]; // app desconhecido
    a.sistema_mut().sessoes = vec![SID.into()];
    let r = a.tick(seg(0)).await;
    assert_eq!(
        r.acoes,
        [Acao::Bloqueou {
            sid: SID.into(),
            motivo: Motivo::LimiteDiarioTotal
        }]
    );
}

#[tokio::test]
async fn aviso_de_app_nao_e_escondido_por_aviso_de_sessao_com_menos_tempo() {
    let m = mundo(Some(1000)).await;
    m.regra(&m.jogo, Some(1200)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(1, "jogo.exe", SID)];
    a.sistema_mut().sessoes = vec![SID.into()];
    for i in 0..3 {
        a.tick(seg(i * 300)).await;
    }
    // total = app = 600: sessão restam 400, app restam 600. Em 900 s: sessão 100, app 300.
    let r = a.tick(seg(900)).await;
    let motivos: Vec<Motivo> = r
        .acoes
        .iter()
        .filter_map(|x| {
            if let Acao::Avisou { motivo, .. } = x {
                Some(*motivo)
            } else {
                None
            }
        })
        .collect();
    assert!(
        motivos.contains(&Motivo::LimiteDiarioTotal) && motivos.contains(&Motivo::LimiteDiarioApp),
        "{motivos:?}"
    );
}

#[tokio::test]
async fn salto_de_relogio_gera_tamper_sem_contabilizar_mas_o_enforcement_continua() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(0)).await;
    let mut a = agente(&m);
    a.tick(seg(0)).await; // nada em execução ainda
    a.sistema_mut().processos = vec![proc(5, "jogo.exe", SID)];
    let r = a.tick(seg(3600)).await; // uma hora entre dois ticks
    assert!(
        r.erros.iter().any(|e| e.contains("uso não contabilizado")),
        "{:?}",
        r.erros
    );
    assert_eq!(m.eventos("TAMPER_SUSPECTED").await, 1);
    assert!(!r.acoes.is_empty(), "o app bloqueado ainda é encerrado");
    let dia = Instante::em(seg(3600), &Utc).local.date();
    assert_eq!(
        m.db.consumo_do_dia(m.perfil, m.jogo.app_id, dia)
            .await
            .unwrap()
            .app_segundos,
        0
    );
}

#[tokio::test]
async fn aviso_que_falha_nao_conta_como_exibido_e_e_tentado_de_novo() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(1000)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(1, "jogo.exe", SID)];
    for i in 0..3 {
        a.tick(seg(i * 300)).await;
    }
    a.sistema_mut().falha_avisar = true;
    let r = a.tick(seg(900)).await;
    assert!(r.acoes.is_empty() && !r.erros.is_empty());
    assert_eq!(m.eventos("WARNING_SHOWN").await, 0);

    a.sistema_mut().falha_avisar = false;
    let r = a.tick(seg(902)).await;
    assert!(matches!(r.acoes[..], [Acao::Avisou { .. }]));
    assert_eq!(m.eventos("WARNING_SHOWN").await, 1);
}

#[tokio::test]
async fn aviso_volta_a_valer_depois_que_a_restricao_sai_e_a_proxima_se_aproxima() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(1000)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(1, "jogo.exe", SID)];
    for i in 0..4 {
        a.tick(seg(i * 300)).await; // 900 s usados: aviso de 5 min dado em 900
    }
    assert_eq!(a.sistema_mut().avisos.len(), 1);

    // os pais revogam a regra: o app volta a ser permitido e o aviso anterior deixa de valer
    let regras = m.db.regras_do_app(m.perfil, m.jogo.app_id).await.unwrap();
    m.db.set_rule_state(
        &EventContext::local(),
        regras[0].rule_id,
        RuleState::Revoked,
    )
    .await
    .unwrap();
    assert!(
        a.tick(seg(902)).await.acoes.is_empty(),
        "sem regra vigente: permitido"
    );

    // nova regra com 1300 s: sobram ~400 s, sem aviso; ao chegar perto do fim, avisa de novo
    m.regra(&m.jogo, Some(1300)).await;
    assert!(a.tick(seg(904)).await.acoes.is_empty());
    for i in 1..=3 {
        a.tick(seg(904 + i * 100)).await; // 1204 s usados: restam 96
    }
    assert_eq!(
        a.sistema_mut().avisos.len(),
        2,
        "o aviso foi repetido para a nova restrição"
    );
}

#[tokio::test]
async fn falha_ao_listar_processos_e_reportada_e_registrada_sem_derrubar_o_agente() {
    let m = mundo(None).await;
    let mut a = agente(&m);
    a.sistema_mut().falha_listar = true;
    let r = a.tick(seg(0)).await;
    assert_eq!(r.erros.len(), 1);
    assert_eq!(m.eventos("INTERNAL_ERROR").await, 1);
    a.sistema_mut().falha_listar = false;
    assert!(a.tick(seg(2)).await.erros.is_empty());
}

#[tokio::test]
async fn leituras_do_banco_para_o_motor_perfil_por_sid_app_sem_caixa_e_janela_do_dia() {
    let m = mundo(None).await;
    assert_eq!(
        m.db.perfil_por_sid(SID).await.unwrap().unwrap().profile_id,
        m.perfil
    );
    assert!(m.db.perfil_por_sid(OUTRO_SID).await.unwrap().is_none());
    assert_eq!(
        m.db.app_por_processo("JoGo.ExE")
            .await
            .unwrap()
            .unwrap()
            .app_id,
        m.jogo.app_id
    );
    assert!(m.db.app_por_processo("nada.exe").await.unwrap().is_none());

    // janela geral só às segundas (weekday 1)
    let janela = TimeWindow {
        start: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        end: NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
    };
    m.db.set_profile_window(&EventContext::local(), m.perfil, 1, &janela)
        .await
        .unwrap();
    let segunda_7h = Instante::em(Utc.with_ymd_and_hms(2026, 3, 9, 7, 0, 0).unwrap(), &Utc);
    let terca_7h = Instante::em(Utc.with_ymd_and_hms(2026, 3, 10, 7, 0, 0).unwrap(), &Utc);
    assert_eq!(
        m.db.decidir_para(m.perfil, None, segunda_7h).await.unwrap(),
        Decisao::BloquearSessao {
            motivo: Motivo::ForaDaJanelaGeral
        }
    );
    assert_eq!(
        m.db.decidir_para(m.perfil, None, terca_7h).await.unwrap(),
        Decisao::Permitir
    );
}

#[tokio::test]
async fn o_laco_roda_ate_ser_interrompido_e_pode_rodar_em_outra_tarefa() {
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(0)).await;
    let db = std::sync::Arc::new(m.db);
    let db2 = db.clone();
    // tokio::spawn exige Send: prova de compilação de que o serviço pode rodar o laço em segundo plano
    let tarefa = tokio::spawn(async move {
        let fake = Fake {
            processos: vec![proc(5, "jogo.exe", SID)],
            ..Default::default()
        };
        let mut ticks = 0;
        let mut encerramentos = 0;
        rodar_em_laco(
            &db2,
            fake,
            Utc,
            std::time::Duration::from_millis(10),
            |rel| {
                ticks += 1;
                encerramentos += rel
                    .acoes
                    .iter()
                    .filter(|a| matches!(a, Acao::Encerrou { .. }))
                    .count();
                ticks < 3
            },
        )
        .await;
        (ticks, encerramentos)
    });
    let (ticks, encerramentos) = tarefa.await.unwrap();
    assert_eq!(ticks, 3);
    assert_eq!(
        encerramentos, 1,
        "encerra uma vez e respeita a tolerância nos ticks seguintes"
    );
    let n: i64 =
        sqlx::query_scalar("SELECT count(*) FROM security_log WHERE event_type = 'APP_TERMINATED'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(n, 1);
}

#[tokio::test]
async fn o_log_forense_recebe_codigos_e_nao_texto_em_idioma_nenhum() {
    use kiosk_engine::detalhes::Detalhe;
    let m = mundo(None).await;
    m.regra(&m.jogo, Some(1000)).await;
    let mut a = agente(&m);
    a.sistema_mut().processos = vec![proc(42, "JOGO.EXE", SID)];
    for quando in [0, 300, 600, 900, 960, 1040] {
        a.tick(seg(quando)).await;
    }
    let detalhes: Vec<(String, String)> =
        sqlx::query_as("SELECT event_type, rejection_reason_detail FROM security_log WHERE event_type IN ('WARNING_SHOWN', 'APP_TERMINATED') ORDER BY rowid")
            .fetch_all(m.db.pool())
            .await
            .unwrap();
    let lidos: Vec<Detalhe> = detalhes
        .iter()
        .map(|(_, d)| Detalhe::de_texto(d).unwrap_or_else(|| panic!("não é código: {d}")))
        .collect();
    assert_eq!(
        lidos,
        [
            Detalhe::WarningShown {
                marker: 300,
                remaining: 100,
                reason: "APP_DAILY_LIMIT".into(),
                app: Some("jogo.exe".into())
            },
            Detalhe::WarningShown {
                marker: 60,
                remaining: 40,
                reason: "APP_DAILY_LIMIT".into(),
                app: Some("jogo.exe".into())
            },
            Detalhe::AppTerminated {
                process: "JOGO.EXE".into(),
                pid: 42,
                reason: "APP_DAILY_LIMIT".into()
            },
        ]
    );
}
