//! Idiomas, catálogo e detalhe do log forense (ADR-010).

use kiosk_engine::detalhes::{Detalhe, legivel};
use kiosk_engine::i18n::Idioma::{self, EnUs, EsEs, PtBr};
use kiosk_engine::i18n::Msg;

#[test]
fn tags_do_sistema_viram_o_idioma_suportado_mais_proximo() {
    for (tag, esperado) in [
        ("en-US", Some(EnUs)),
        ("en_GB.UTF-8", Some(EnUs)),
        ("EN", Some(EnUs)),
        ("pt-BR", Some(PtBr)),
        ("pt_PT", Some(PtBr)),
        ("pt", Some(PtBr)),
        ("es-ES", Some(EsEs)),
        ("es_MX.UTF-8", Some(EsEs)),
        ("es-419", Some(EsEs)),
        ("  pt-br  ", Some(PtBr)),
        ("fr-FR", None),
        ("C", None),
        ("POSIX", None),
        ("", None),
    ] {
        assert_eq!(Idioma::de_tag(tag), esperado, "{tag:?}");
    }
    for i in Idioma::TODOS {
        assert_eq!(Idioma::de_tag(i.tag()), Some(i));
    }
}

#[test]
fn palavra_de_confirmacao_e_dias_sao_proprios_de_cada_idioma() {
    assert_eq!(EnUs.palavra_de_confirmacao(), "DELETE");
    assert_eq!(PtBr.palavra_de_confirmacao(), "APAGAR");
    assert_eq!(EsEs.palavra_de_confirmacao(), "BORRAR");
    assert_eq!(EnUs.dia_curto(0), "sun");
    assert_eq!(PtBr.dia_curto(1), "seg");
    assert_eq!(EsEs.dia_curto(1), "lun");
    assert_eq!(EsEs.dia_curto(3), "mie");
}

#[test]
fn a_mesma_mensagem_sai_nos_tres_idiomas_com_os_dados() {
    let m = Msg::PerfilCriado {
        nome: "Ana".into(),
        sid: "S-1-5-21-1".into(),
    };
    assert_eq!(m.em(EnUs), "profile \"Ana\" created for account S-1-5-21-1");
    assert_eq!(m.em(PtBr), "perfil \"Ana\" criado para a conta S-1-5-21-1");
    assert_eq!(
        m.em(EsEs),
        "perfil \"Ana\" creado para la cuenta S-1-5-21-1"
    );
    let m = Msg::ConfirmarApagar {
        palavra: "BORRAR".into(),
    };
    assert!(m.em(EsEs).contains("Escriba BORRAR"));
    assert!(m.em(EnUs).contains("Type BORRAR"));
}

#[test]
fn nenhum_idioma_deixa_o_texto_de_uso_sem_a_pagina_de_equivalentes() {
    for i in Idioma::TODOS {
        for m in [Msg::UsoKioskd, Msg::UsoKioskctl] {
            let t = m.em(i);
            assert!(t.contains("comandos-em-portugues.md"), "{i:?}: {t}");
            assert!(t.contains("KIOSK_LANG"), "{i:?}: {t}");
        }
    }
    // os comandos são os mesmos nos três idiomas: só o texto ao redor muda
    let comandos = |i| -> Vec<String> {
        Msg::UsoKioskd
            .em(i)
            .lines()
            .filter_map(|l| l.trim().strip_prefix("kioskd "))
            .map(|l| l.split_whitespace().next().unwrap().to_owned())
            .collect()
    };
    assert_eq!(comandos(EnUs), comandos(PtBr));
    assert_eq!(comandos(PtBr), comandos(EsEs));
}

fn exemplos() -> Vec<Detalhe> {
    vec![
        Detalhe::WarningShown {
            marker: 300,
            remaining: 298,
            reason: "APP_DAILY_LIMIT".into(),
            app: Some("jogo.exe".into()),
        },
        Detalhe::WarningShown {
            marker: 60,
            remaining: 60,
            reason: "TOTAL_DAILY_LIMIT".into(),
            app: None,
        },
        Detalhe::AppTerminated {
            process: "jogo.exe".into(),
            pid: 7,
            reason: "APP_BLOCKED".into(),
        },
        Detalhe::SessionLocked {
            reason: "OUTSIDE_WINDOW".into(),
        },
        Detalhe::SessionLoggedOff {
            locks: 2,
            reason: "TOTAL_DAILY_LIMIT".into(),
        },
        Detalhe::ProcessWontTerminate {
            process: "x.exe".into(),
            pid: 9,
            attempts: 3,
        },
        Detalhe::TerminateFailed {
            process: "x.exe".into(),
            pid: 9,
            attempt: 1,
            error: "Access is denied. (os error 5) \"com aspas\" e = sinais".into(),
        },
        Detalhe::PairingCodeNotAdmin,
        Detalhe::RetentionPurged { removed: 2 },
    ]
}

#[test]
fn o_detalhe_do_log_e_um_codigo_estavel_que_volta_igual() {
    for d in exemplos() {
        let texto = d.codigo();
        assert!(texto.starts_with("{\"code\":\""), "{texto}");
        assert!(!texto.contains('\n'), "uma linha só: {texto}");
        assert_eq!(Detalhe::de_texto(&texto), Some(d.clone()), "{texto}");
    }
    assert_eq!(
        Detalhe::AppTerminated {
            process: "jogo.exe".into(),
            pid: 4242,
            reason: "APP_DAILY_LIMIT".into()
        }
        .codigo(),
        r#"{"code":"APP_TERMINATED","process":"jogo.exe","pid":4242,"reason":"APP_DAILY_LIMIT"}"#
    );
    assert_eq!(
        Detalhe::PairingCodeNotAdmin.codigo(),
        r#"{"code":"PAIRING_CODE_NOT_ADMIN"}"#
    );
}

#[test]
fn o_detalhe_e_traduzido_na_hora_de_ler_e_nunca_mostra_o_codigo_cru() {
    for d in exemplos() {
        let texto = d.codigo();
        let por_idioma: Vec<String> = Idioma::TODOS.iter().map(|i| legivel(&texto, *i)).collect();
        for t in &por_idioma {
            assert!(!t.starts_with('{') && !t.contains("\"code\""), "{t}");
            assert!(
                !t.contains("APP_DAILY_LIMIT") && !t.contains("TOTAL_DAILY_LIMIT"),
                "{t}"
            );
        }
        assert_ne!(por_idioma[0], por_idioma[1], "{texto}");
        assert_ne!(por_idioma[1], por_idioma[2], "{texto}");
    }
    let aviso = Detalhe::WarningShown {
        marker: 300,
        remaining: 298,
        reason: "APP_DAILY_LIMIT".into(),
        app: Some("jogo.exe".into()),
    }
    .codigo();
    assert_eq!(
        legivel(&aviso, PtBr),
        "aviso de 300s exibido: restam 298s (limite diário do app), app jogo.exe"
    );
    assert_eq!(
        legivel(&aviso, EnUs),
        "300s warning shown: 298s left (app daily limit), app jogo.exe"
    );
    assert_eq!(
        legivel(&aviso, EsEs),
        "aviso de 300s mostrado: quedan 298s (límite diario de la app), app jogo.exe"
    );
}

#[test]
fn linhas_antigas_ou_de_outra_origem_aparecem_como_foram_gravadas() {
    for texto in [
        "app jogo.exe encerrado: LIMITE_DIARIO_APP", // log da 0.0.4, em português
        "",
        "{\"code\":\"CODIGO_QUE_NAO_EXISTE\"}",
        "{\"code\":\"APP_TERMINATED\"}", // faltam campos
        "{\"code\":",
    ] {
        for i in Idioma::TODOS {
            assert_eq!(legivel(texto, i), texto);
        }
    }
    // motivo desconhecido (versão futura) aparece como veio, dentro da frase
    let d = Detalhe::SessionLocked {
        reason: "FUTURE_REASON".into(),
    }
    .codigo();
    assert_eq!(legivel(&d, EnUs), "session locked: FUTURE_REASON");
}
