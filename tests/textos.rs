//! Textos pt-BR mostrados à criança (ADR-001: visível e explicado).

use kiosk_engine::agente::Aviso;
use kiosk_engine::motor::Motivo;
use kiosk_engine::textos::{TITULO, aviso, tempo};

fn a(app: Option<&str>, restante: u64, motivo: Motivo) -> Aviso {
    Aviso {
        sid: "S-1-5-21-1".into(),
        app: app.map(Into::into),
        restante_segundos: restante,
        motivo,
    }
}

#[test]
fn tempo_restante_em_linguagem_simples() {
    assert_eq!(tempo(300), "5 minutos");
    assert_eq!(tempo(60), "1 minuto");
    assert_eq!(tempo(100), "2 minutos");
    assert_eq!(tempo(59), "59 segundos");
    assert_eq!(tempo(1), "1 segundo");
}

#[test]
fn aviso_de_app_diz_quanto_falta_por_que_e_o_que_acontece() {
    let (titulo, msg) = aviso(&a(Some("jogo.exe"), 300, Motivo::LimiteDiarioApp));
    assert_eq!(titulo, TITULO);
    assert!(msg.contains("Faltam 5 minutos"), "{msg}");
    assert!(msg.contains("tempo de hoje para este programa"), "{msg}");
    assert!(msg.contains("jogo.exe será fechado"), "{msg}");
    assert!(msg.contains("Salve"), "{msg}");
}

#[test]
fn aviso_de_sessao_fala_em_bloquear_a_sessao_e_nao_em_programa() {
    for motivo in [Motivo::LimiteDiarioTotal, Motivo::ForaDaJanelaGeral] {
        let (_, msg) = aviso(&a(None, 60, motivo));
        assert!(
            msg.contains("Faltam 1 minuto") && msg.contains("sessão será bloqueada"),
            "{msg}"
        );
        assert!(!msg.contains("será fechado"), "{msg}");
    }
}

#[test]
fn o_aviso_nunca_mostra_o_sid_nem_codigos_internos() {
    for motivo in [
        Motivo::AppBloqueado,
        Motivo::LimiteDiarioApp,
        Motivo::LimiteDiarioTotal,
        Motivo::ForaDaJanelaApp,
        Motivo::ForaDaJanelaGeral,
    ] {
        let (_, msg) = aviso(&a(Some("jogo.exe"), 300, motivo));
        assert!(
            !msg.contains("S-1-5") && !msg.contains(motivo.as_str()),
            "{msg}"
        );
    }
}
