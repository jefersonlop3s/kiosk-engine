//! Textos mostrados à criança, nos três idiomas (ADR-001: visível e explicado; ADR-010).

use kiosk_engine::agente::Aviso;
use kiosk_engine::i18n::Idioma::{self, EnUs, EsEs, PtBr};
use kiosk_engine::motor::Motivo;
use kiosk_engine::textos::{aviso, tempo, titulo};

const MOTIVOS: [Motivo; 5] = [
    Motivo::AppBloqueado,
    Motivo::LimiteDiarioApp,
    Motivo::LimiteDiarioTotal,
    Motivo::ForaDaJanelaApp,
    Motivo::ForaDaJanelaGeral,
];

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
    assert_eq!(tempo(300, PtBr), "5 minutos");
    assert_eq!(tempo(60, PtBr), "1 minuto");
    assert_eq!(tempo(100, PtBr), "2 minutos");
    assert_eq!(tempo(59, PtBr), "59 segundos");
    assert_eq!(tempo(1, PtBr), "1 segundo");

    assert_eq!(tempo(300, EnUs), "5 minutes");
    assert_eq!(tempo(60, EnUs), "1 minute");
    assert_eq!(tempo(59, EnUs), "59 seconds");
    assert_eq!(tempo(1, EnUs), "1 second");

    assert_eq!(tempo(300, EsEs), "5 minutos");
    assert_eq!(tempo(60, EsEs), "1 minuto");
    assert_eq!(tempo(1, EsEs), "1 segundo");
}

#[test]
fn aviso_de_app_diz_quanto_falta_por_que_e_o_que_acontece() {
    let (t, msg) = aviso(&a(Some("jogo.exe"), 300, Motivo::LimiteDiarioApp), PtBr);
    assert_eq!(t, titulo(PtBr));
    assert!(msg.contains("Faltam 5 minutos"), "{msg}");
    assert!(msg.contains("tempo de hoje para este programa"), "{msg}");
    assert!(msg.contains("jogo.exe será fechado"), "{msg}");
    assert!(msg.contains("Salve"), "{msg}");

    let (t, msg) = aviso(&a(Some("jogo.exe"), 300, Motivo::LimiteDiarioApp), EnUs);
    assert_eq!(t, "Parental controls");
    assert!(msg.contains("5 minutes left"), "{msg}");
    assert!(msg.contains("today's time for this program"), "{msg}");
    assert!(msg.contains("jogo.exe will be closed"), "{msg}");
    assert!(msg.contains("Save"), "{msg}");

    let (t, msg) = aviso(&a(Some("jogo.exe"), 300, Motivo::LimiteDiarioApp), EsEs);
    assert_eq!(t, "Control parental");
    assert!(msg.contains("Faltan 5 minutos"), "{msg}");
    assert!(msg.contains("el tiempo de hoy para este programa"), "{msg}");
    assert!(msg.contains("jogo.exe se cerrará"), "{msg}");
    assert!(msg.contains("Guarda"), "{msg}");
}

#[test]
fn aviso_de_sessao_fala_em_bloquear_a_sessao_e_nao_em_programa() {
    for motivo in [Motivo::LimiteDiarioTotal, Motivo::ForaDaJanelaGeral] {
        for (idioma, quanto, bloqueio, fechado) in [
            (
                PtBr,
                "Faltam 1 minuto",
                "sessão será bloqueada",
                "será fechado",
            ),
            (
                EnUs,
                "1 minute left",
                "session will be locked",
                "will be closed",
            ),
            (EsEs, "Faltan 1 minuto", "sesión se bloqueará", "se cerrará"),
        ] {
            let (_, msg) = aviso(&a(None, 60, motivo), idioma);
            assert!(msg.contains(quanto) && msg.contains(bloqueio), "{msg}");
            assert!(!msg.contains(fechado), "{msg}");
        }
    }
}

#[test]
fn o_aviso_nunca_mostra_o_sid_nem_codigos_internos() {
    for idioma in Idioma::TODOS {
        for motivo in MOTIVOS {
            let (_, msg) = aviso(&a(Some("jogo.exe"), 300, motivo), idioma);
            assert!(
                !msg.contains("S-1-5") && !msg.contains(motivo.as_str()),
                "{idioma:?} {msg}"
            );
        }
    }
}

#[test]
fn cada_motivo_tem_texto_proprio_em_cada_idioma() {
    // textos diferentes entre os idiomas e entre os motivos: nada ficou em branco ou repetido
    for motivo in MOTIVOS {
        let por_idioma: Vec<String> = Idioma::TODOS
            .iter()
            .map(|i| aviso(&a(Some("x.exe"), 300, motivo), *i).1)
            .collect();
        assert!(por_idioma.iter().all(|m| m.len() > 30), "{por_idioma:?}");
        assert_ne!(por_idioma[0], por_idioma[1]);
        assert_ne!(por_idioma[1], por_idioma[2]);
        assert_ne!(por_idioma[0], por_idioma[2]);
    }
}
