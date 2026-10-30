//! E-003 F1 e F3: testes de tabela do motor com instantes fixos (sem SO, sem banco).

use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use kiosk_engine::database_schema::*;
use kiosk_engine::motor::*;
use uuid::Uuid;

fn hms(h: u32, m: u32, s: u32) -> NaiveTime {
    NaiveTime::from_hms_opt(h, m, s).unwrap()
}

fn janela(a: (u32, u32), b: (u32, u32)) -> TimeWindow {
    TimeWindow {
        start: hms(a.0, a.1, 0),
        end: hms(b.0, b.1, 0),
    }
}

fn as_(h: u32, m: u32, s: u32) -> Instante {
    let local = NaiveDate::from_ymd_opt(2026, 3, 9)
        .unwrap()
        .and_time(hms(h, m, s));
    Instante {
        utc: Utc.from_utc_datetime(&local),
        local,
    }
}

struct Cenario {
    perfil: PerfilCtx,
    app_id: Uuid,
}

impl Cenario {
    fn novo() -> Self {
        let perfil = PerfilCtx {
            profile_id: Uuid::new_v4(),
            limite_total_segundos: None,
            janela_geral: None,
        };
        Self {
            perfil,
            app_id: Uuid::new_v4(),
        }
    }

    fn regra(&self, limite: Option<u64>, janela: Option<TimeWindow>) -> ActiveRule {
        ActiveRule {
            rule_id: Uuid::new_v4(),
            profile_id: self.perfil.profile_id,
            app_id: self.app_id,
            creation_timestamp: Utc::now(),
            state: RuleState::Active,
            daily_limit_seconds: limite,
            time_window: janela,
            effective_until_timestamp: None,
            warning_seconds: None,
        }
    }

    fn decidir(
        &self,
        status: ProcessStatus,
        regras: &[ActiveRule],
        agora: Instante,
        consumo: Consumo,
    ) -> Decisao {
        let app = AppCtx {
            app_id: self.app_id,
            status,
            regras,
        };
        decidir(&self.perfil, &app, agora, consumo)
    }

    fn monitorado(&self, regras: &[ActiveRule], agora: Instante, app_segundos: u64) -> Decisao {
        let consumo = Consumo {
            app_segundos,
            total_segundos: app_segundos,
        };
        self.decidir(ProcessStatus::Monitored, regras, agora, consumo)
    }
}

const FIM: Decisao = Decisao::EncerrarApp {
    motivo: Motivo::ForaDaJanelaApp,
};

#[test]
fn sem_regra_o_app_monitorado_e_permitido() {
    let c = Cenario::novo();
    assert_eq!(c.monitorado(&[], as_(3, 0, 0), 99_999), Decisao::Permitir);
}

#[test]
fn janela_do_app_nas_bordas() {
    let c = Cenario::novo();
    let regras = [c.regra(None, Some(janela((8, 0), (20, 0))))];
    let casos = [
        (as_(7, 59, 59), FIM),
        (as_(8, 0, 0), Decisao::Permitir),
        (as_(19, 54, 59), Decisao::Permitir), // 301 s restantes
        (
            as_(19, 55, 0),
            Decisao::Avisar {
                restante_segundos: 300,
                motivo: Motivo::ForaDaJanelaApp,
                thresholds: AVISOS_SEGUNDOS,
            },
        ),
        (
            as_(19, 59, 59),
            Decisao::Avisar {
                restante_segundos: 1,
                motivo: Motivo::ForaDaJanelaApp,
                thresholds: AVISOS_SEGUNDOS,
            },
        ),
        (as_(20, 0, 0), FIM),
        (as_(23, 0, 0), FIM),
    ];
    for (instante, esperado) in casos {
        assert_eq!(
            c.monitorado(&regras, instante, 0),
            esperado,
            "em {}",
            instante.local
        );
    }
}

#[test]
fn janela_geral_bloqueia_a_sessao_fora_do_horario() {
    let mut c = Cenario::novo();
    c.perfil.janela_geral = Some(janela((7, 0), (21, 0)));
    let fora = Decisao::BloquearSessao {
        motivo: Motivo::ForaDaJanelaGeral,
    };
    assert_eq!(c.monitorado(&[], as_(6, 59, 59), 0), fora);
    assert_eq!(c.monitorado(&[], as_(12, 0, 0), 0), Decisao::Permitir);
    assert_eq!(c.monitorado(&[], as_(21, 0, 0), 0), fora);
    assert_eq!(
        c.monitorado(&[], as_(20, 59, 0), 0),
        Decisao::Avisar {
            restante_segundos: 60,
            motivo: Motivo::ForaDaJanelaGeral,
            thresholds: AVISOS_SEGUNDOS,
        }
    );
}

#[test]
fn limite_diario_do_app_nas_bordas() {
    let c = Cenario::novo();
    let regras = [c.regra(Some(3600), None)];
    let aviso = |r| Decisao::Avisar {
        restante_segundos: r,
        motivo: Motivo::LimiteDiarioApp,
        thresholds: AVISOS_SEGUNDOS,
    };
    assert_eq!(
        c.monitorado(&regras, as_(12, 0, 0), 3299),
        Decisao::Permitir
    );
    assert_eq!(c.monitorado(&regras, as_(12, 0, 0), 3300), aviso(300));
    assert_eq!(c.monitorado(&regras, as_(12, 0, 0), 3599), aviso(1));
    assert_eq!(
        c.monitorado(&regras, as_(12, 0, 0), 3600),
        Decisao::EncerrarApp {
            motivo: Motivo::LimiteDiarioApp
        }
    );
    assert_eq!(
        c.monitorado(&regras, as_(12, 0, 0), 9000),
        Decisao::EncerrarApp {
            motivo: Motivo::LimiteDiarioApp
        }
    );
}

#[test]
fn regra_com_limite_zero_bloqueia_o_app() {
    let c = Cenario::novo();
    let regras = [c.regra(Some(0), None)];
    assert_eq!(
        c.monitorado(&regras, as_(12, 0, 0), 0),
        Decisao::EncerrarApp {
            motivo: Motivo::AppBloqueado
        }
    );
}

#[test]
fn limite_total_do_perfil_bloqueia_a_sessao_mesmo_para_app_isento() {
    let mut c = Cenario::novo();
    c.perfil.limite_total_segundos = Some(7200);
    let consumo = |t| Consumo {
        app_segundos: 10,
        total_segundos: t,
    };
    let sessao = Decisao::BloquearSessao {
        motivo: Motivo::LimiteDiarioTotal,
    };
    assert_eq!(
        c.decidir(
            ProcessStatus::Whitelisted,
            &[],
            as_(12, 0, 0),
            consumo(7200)
        ),
        sessao
    );
    assert_eq!(
        c.decidir(ProcessStatus::Ignored, &[], as_(12, 0, 0), consumo(7300)),
        sessao
    );
    assert_eq!(
        c.decidir(ProcessStatus::Monitored, &[], as_(12, 0, 0), consumo(6000)),
        Decisao::Permitir
    );
    assert_eq!(
        c.decidir(ProcessStatus::Monitored, &[], as_(12, 0, 0), consumo(6900)),
        Decisao::Avisar {
            restante_segundos: 300,
            motivo: Motivo::LimiteDiarioTotal,
            thresholds: AVISOS_SEGUNDOS,
        }
    );
}

#[test]
fn app_isento_nunca_recebe_encerrar_app_mesmo_com_regra_que_o_violaria() {
    let c = Cenario::novo();
    let regras = [c.regra(Some(0), Some(janela((8, 0), (9, 0))))];
    for status in [ProcessStatus::Whitelisted, ProcessStatus::Ignored] {
        for h in 0..24 {
            let d = c.decidir(
                status,
                &regras,
                as_(h, 30, 0),
                Consumo {
                    app_segundos: 99_999,
                    total_segundos: 99_999,
                },
            );
            assert_eq!(d, Decisao::Permitir, "{status:?} às {h}h");
        }
    }
}

// --- F3: precedência e conflitos

#[test]
fn duas_regras_no_mesmo_app_valem_a_mais_restritiva() {
    let c = Cenario::novo();
    let regras = [c.regra(Some(3600), None), c.regra(Some(1800), None)];
    assert_eq!(
        c.monitorado(&regras, as_(12, 0, 0), 1000),
        Decisao::Permitir
    );
    assert_eq!(
        c.monitorado(&regras, as_(12, 0, 0), 2000),
        Decisao::EncerrarApp {
            motivo: Motivo::LimiteDiarioApp
        }
    );
}

#[test]
fn bloqueio_vence_permissao_na_intersecao_de_janelas() {
    let c = Cenario::novo();
    let regras = [
        c.regra(None, Some(janela((8, 0), (20, 0)))),
        c.regra(None, Some(janela((14, 0), (16, 0)))),
    ];
    assert_eq!(c.monitorado(&regras, as_(10, 0, 0), 0), FIM);
    assert_eq!(c.monitorado(&regras, as_(15, 0, 0), 0), Decisao::Permitir);
}

#[test]
fn regra_revogada_pendente_ou_expirada_e_ignorada() {
    let c = Cenario::novo();
    let bloqueia = || c.regra(Some(0), None);
    let mut revogada = bloqueia();
    revogada.state = RuleState::Revoked;
    let mut pendente = bloqueia();
    pendente.state = RuleState::Pending;
    let mut expirada = bloqueia();
    expirada.effective_until_timestamp = Some(as_(11, 59, 59).utc);
    let regras = [revogada, pendente, expirada];
    assert_eq!(c.monitorado(&regras, as_(12, 0, 0), 0), Decisao::Permitir);

    let mut vigente = bloqueia();
    vigente.effective_until_timestamp = Some(as_(12, 0, 1).utc);
    assert_eq!(
        c.monitorado(&[vigente], as_(12, 0, 0), 0),
        Decisao::EncerrarApp {
            motivo: Motivo::AppBloqueado
        }
    );
}

#[test]
fn regra_de_outro_app_ou_de_outro_perfil_nao_se_aplica() {
    let c = Cenario::novo();
    let mut outro_app = c.regra(Some(0), None);
    outro_app.app_id = Uuid::new_v4();
    let mut outro_perfil = c.regra(Some(0), None);
    outro_perfil.profile_id = Uuid::new_v4();
    assert_eq!(
        c.monitorado(&[outro_app, outro_perfil], as_(12, 0, 0), 0),
        Decisao::Permitir
    );
}

#[test]
fn sessao_vence_app_e_encerrar_vence_avisar() {
    let mut c = Cenario::novo();
    c.perfil.limite_total_segundos = Some(1000);
    let regras = [c.regra(Some(0), None)];
    let d = c.decidir(
        ProcessStatus::Monitored,
        &regras,
        as_(12, 0, 0),
        Consumo {
            app_segundos: 0,
            total_segundos: 1000,
        },
    );
    assert_eq!(
        d,
        Decisao::BloquearSessao {
            motivo: Motivo::LimiteDiarioTotal
        }
    );

    c.perfil.limite_total_segundos = Some(1000); // sessão só em aviso (restante 100)
    let d = c.decidir(
        ProcessStatus::Monitored,
        &regras,
        as_(12, 0, 0),
        Consumo {
            app_segundos: 0,
            total_segundos: 900,
        },
    );
    assert_eq!(
        d,
        Decisao::EncerrarApp {
            motivo: Motivo::AppBloqueado
        }
    );
}

#[test]
fn entre_avisos_vale_o_de_menor_tempo_restante() {
    let mut c = Cenario::novo();
    c.perfil.limite_total_segundos = Some(1000);
    let regras = [c.regra(Some(3600), None)];
    // total: restante 250; app: restante 100
    let d = c.decidir(
        ProcessStatus::Monitored,
        &regras,
        as_(12, 0, 0),
        Consumo {
            app_segundos: 3500,
            total_segundos: 750,
        },
    );
    assert_eq!(
        d,
        Decisao::Avisar {
            restante_segundos: 100,
            motivo: Motivo::LimiteDiarioApp,
            thresholds: AVISOS_SEGUNDOS,
        }
    );
}

#[test]
fn controle_negativo_dentro_da_janela_e_com_saldo_nunca_encerra_nem_bloqueia() {
    let mut c = Cenario::novo();
    c.perfil.limite_total_segundos = Some(14_400);
    c.perfil.janela_geral = Some(janela((7, 0), (21, 0)));
    let regras = [c.regra(Some(3600), Some(janela((8, 0), (20, 0))))];
    for minutos in (8 * 60)..(19 * 60 + 54) {
        for usado in [0, 1000, 3299] {
            let d = c.decidir(
                ProcessStatus::Monitored,
                &regras,
                as_(minutos / 60, minutos % 60, 0),
                Consumo {
                    app_segundos: usado,
                    total_segundos: usado,
                },
            );
            assert_eq!(
                d,
                Decisao::Permitir,
                "às {}:{:02} com {usado}s",
                minutos / 60,
                minutos % 60
            );
        }
    }
}

#[test]
fn restante_da_sessao_e_o_menor_entre_o_limite_e_a_janela_e_diz_qual() {
    use kiosk_engine::motor::{Consumo, Motivo, restante_da_sessao};
    let janela = TimeWindow {
        start: NaiveTime::from_hms_opt(8, 0, 0).unwrap(),
        end: NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
    };
    let perfil = |limite, janela| PerfilCtx {
        profile_id: Uuid::nil(),
        limite_total_segundos: limite,
        janela_geral: janela,
    };
    let c = |total| Consumo {
        app_segundos: 0,
        total_segundos: total,
    };
    // 19:00: a janela fecha em 3600 s; o limite (7200 - 600) ainda está longe
    assert_eq!(
        restante_da_sessao(&perfil(Some(7200), Some(janela)), as_(19, 0, 0), c(600)),
        Some((3600, Motivo::ForaDaJanelaGeral))
    );
    // o limite vem primeiro
    assert_eq!(
        restante_da_sessao(&perfil(Some(1000), Some(janela)), as_(19, 0, 0), c(600)),
        Some((400, Motivo::LimiteDiarioTotal))
    );
    // sem limite nem janela: nada a mostrar
    assert_eq!(
        restante_da_sessao(&perfil(None, None), as_(19, 0, 0), c(600)),
        None
    );
    // estourado ou fora da janela: zero, sem estourar a subtração
    assert_eq!(
        restante_da_sessao(&perfil(Some(500), None), as_(19, 0, 0), c(600)),
        Some((0, Motivo::LimiteDiarioTotal))
    );
    assert_eq!(
        restante_da_sessao(&perfil(None, Some(janela)), as_(21, 0, 0), c(0)),
        Some((0, Motivo::ForaDaJanelaGeral))
    );
}
