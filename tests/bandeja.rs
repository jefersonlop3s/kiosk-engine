//! Textos da bandeja e protocolo do lado da UI (ADR-004, E-004 F3). Puros: rodam em qualquer SO.

use kiosk_engine::bandeja::{MAX_DICA, confirmacao, dica, marco_do_aviso, texto_do_aviso};
use kiosk_engine::i18n::Idioma;
use kiosk_engine::ipc::{
    DaUi, ErroIpc, MAX_MENSAGEM_BYTES, ParaUi, codificar_da_ui, codificar_para_ui,
    decodificar_da_ui, decodificar_para_ui,
};

const TODOS: [Idioma; 3] = [Idioma::EnUs, Idioma::PtBr, Idioma::EsEs];

#[test]
fn a_dica_diz_o_tempo_restante_no_idioma_de_quem_le() {
    assert_eq!(
        dica(Some(4_800), Idioma::PtBr),
        "Controle dos pais: restam 1h 20min 00s"
    );
    assert_eq!(
        dica(Some(4_800), Idioma::EnUs),
        "Parental controls: 1h 20min 00s left"
    );
    assert_eq!(
        dica(Some(4_800), Idioma::EsEs),
        "Control parental: quedan 1h 20min 00s"
    );
    assert_eq!(
        dica(None, Idioma::PtBr),
        "Controle dos pais: sem limite agora"
    );
    assert_eq!(
        dica(None, Idioma::EnUs),
        "Parental controls: no limit right now"
    );
    assert_eq!(
        dica(None, Idioma::EsEs),
        "Control parental: sin límite ahora"
    );
}

#[test]
fn a_dica_sempre_cabe_no_icone_e_nunca_mostra_codigo_nem_sid() {
    for i in TODOS {
        for restante in [
            None,
            Some(0),
            Some(1),
            Some(59),
            Some(86_400),
            Some(u64::MAX),
        ] {
            let d = dica(restante, i);
            assert!(d.chars().count() <= MAX_DICA, "{i:?} {restante:?}: {d}");
            assert!(!d.contains("S-1-") && !d.contains("APP_"), "{d}");
        }
    }
}

#[test]
fn o_aviso_diz_o_app_o_tempo_e_o_que_acontece_nos_tres_idiomas() {
    let esperado = [
        (Idioma::PtBr, "Faltam 5 minutos", "jogo.exe será fechado"),
        (Idioma::EnUs, "5 minutes left", "jogo.exe will be closed"),
        (Idioma::EsEs, "Faltan 5 minutos", "jogo.exe se cerrará"),
    ];
    for (idioma, tempo, depois) in esperado {
        let (titulo, corpo) =
            texto_do_aviso(300, "APP_DAILY_LIMIT", Some("jogo.exe"), idioma).unwrap();
        assert!(!titulo.is_empty());
        assert!(
            corpo.contains(tempo) && corpo.contains(depois),
            "{idioma:?}: {corpo}"
        );
    }
    // aviso de sessão: fala em bloquear a sessão, não em programa
    let (_, corpo) = texto_do_aviso(60, "TOTAL_DAILY_LIMIT", None, Idioma::PtBr).unwrap();
    assert!(corpo.contains("sessão será bloqueada"), "{corpo}");
}

#[test]
fn motivo_desconhecido_nao_vira_texto_inventado() {
    for ruim in ["", "X", "app_daily_limit", "APP_DAILY_LIMIT ", "<script>"] {
        assert_eq!(
            texto_do_aviso(300, ruim, Some("a.exe"), Idioma::PtBr),
            None,
            "{ruim:?}"
        );
    }
}

#[test]
fn o_marco_do_aviso_e_300_acima_de_um_minuto_e_60_dali_para_baixo() {
    assert_eq!(marco_do_aviso(300, None), 300);
    assert_eq!(marco_do_aviso(299, None), 300);
    assert_eq!(marco_do_aviso(61, None), 300);
    assert_eq!(marco_do_aviso(60, None), 60);
    assert_eq!(marco_do_aviso(59, None), 60);
    assert_eq!(marco_do_aviso(0, None), 60);
    // a confirmação é uma mensagem que o serviço aceita
    for restante in [300, 120, 60, 1] {
        let linha = codificar_da_ui(&confirmacao(restante, None));
        assert!(linha.ends_with(b"\n"));
        assert_eq!(decodificar_da_ui(&linha), Ok(confirmacao(restante, None)));
    }
}

#[test]
fn as_mensagens_do_servico_fazem_a_ida_e_volta_na_ui() {
    for msg in [
        ParaUi::Estado {
            restante_segundos: Some(3_600),
            motivo: Some("APP_DAILY_LIMIT".into()),
        },
        ParaUi::Estado {
            restante_segundos: None,
            motivo: None,
        },
        ParaUi::Aviso {
            restante_segundos: 60,
            motivo: "APP_DAILY_LIMIT".into(),
            app: Some("jogo.exe".into()),
        },
    ] {
        assert_eq!(decodificar_para_ui(&codificar_para_ui(&msg)), Ok(msg));
    }
    // o batimento também é uma linha de um objeto só
    let b = codificar_da_ui(&DaUi::Batimento {});
    assert_eq!(b, b"{\"v\":1,\"tipo\":\"batimento\"}\n");
}

#[test]
fn a_ui_confere_o_que_chega_do_servico_do_mesmo_jeito_que_o_servico_confere_a_ui() {
    let grande = vec![b'x'; MAX_MENSAGEM_BYTES + 1];
    assert_eq!(
        decodificar_para_ui(&grande),
        Err(ErroIpc::MuitoGrande(MAX_MENSAGEM_BYTES + 1))
    );
    assert_eq!(decodificar_para_ui(&[0xff, 0xfe]), Err(ErroIpc::NaoEUtf8));
    for ruim in [
        &b"nao e json"[..],
        b"[]",
        b"{}",
        b"{\"v\":1}",
        b"{\"v\":1,\"tipo\":\"comando\"}",
        b"{\"v\":1,\"tipo\":\"estado\",\"restante_segundos\":1,\"motivo\":null,\"extra\":1}",
        b"{\"v\":1,\"tipo\":\"aviso\",\"restante_segundos\":-1,\"motivo\":\"X\",\"app\":null}",
        b"{\"tipo\":\"estado\",\"restante_segundos\":1,\"motivo\":null}",
    ] {
        assert!(
            matches!(decodificar_para_ui(ruim), Err(ErroIpc::Json(_))),
            "{}",
            String::from_utf8_lossy(ruim)
        );
    }
    assert_eq!(
        decodificar_para_ui(
            b"{\"v\":2,\"tipo\":\"estado\",\"restante_segundos\":1,\"motivo\":null}"
        ),
        Err(ErroIpc::VersaoIncompativel(2))
    );
}
