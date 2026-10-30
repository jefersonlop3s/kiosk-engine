//! ADR-004: protocolo serviço ↔ UI. Entrada da UI é não confiável.

use chrono::{DateTime, Duration, TimeZone, Utc};
use kiosk_engine::agente::Aviso;
use kiosk_engine::ipc::*;
use kiosk_engine::motor::Motivo;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 3, 9, 12, 0, 0).unwrap()
}

#[test]
fn mensagens_da_ui_validas_sao_decodificadas() {
    assert_eq!(
        decodificar_da_ui(br#"{"v":1,"tipo":"batimento"}"#).unwrap(),
        DaUi::Batimento {}
    );
    assert_eq!(
        decodificar_da_ui(b"{\"v\":1,\"tipo\":\"aviso_exibido\",\"marco\":300}\n").unwrap(),
        DaUi::AvisoExibido { marco: 300 }
    );
    assert_eq!(
        decodificar_da_ui(br#"{"v":1,"tipo":"aviso_exibido","marco":60}"#).unwrap(),
        DaUi::AvisoExibido { marco: 60 }
    );
}

#[test]
fn entrada_hostil_da_ui_e_recusada() {
    let grande = vec![b'a'; MAX_MENSAGEM_BYTES + 1];
    assert_eq!(
        decodificar_da_ui(&grande),
        Err(ErroIpc::MuitoGrande(MAX_MENSAGEM_BYTES + 1))
    );
    assert_eq!(
        decodificar_da_ui(&[0xff, 0xfe, b'{']),
        Err(ErroIpc::NaoEUtf8)
    );

    let casos: [(&[u8], &str); 8] = [
        (b"", "vazia"),
        (b"nao e json", "texto"),
        (br#"{"v":1}"#, "sem tipo"),
        (br#"{"tipo":"batimento"}"#, "sem versão"),
        (
            br#"{"v":1,"tipo":"comando","acao":"revogar"}"#,
            "tipo desconhecido",
        ),
        (br#"{"v":1,"tipo":"batimento","extra":true}"#, "campo extra"),
        (
            br#"{"v":1,"tipo":"aviso_exibido","marco":"300"}"#,
            "tipo errado do marco",
        ),
        (br#"[{"v":1,"tipo":"batimento"}]"#, "lista"),
    ];
    for (entrada, nome) in casos {
        assert!(
            matches!(decodificar_da_ui(entrada), Err(ErroIpc::Json(_))),
            "{nome}"
        );
    }
}

#[test]
fn versao_diferente_e_marco_zero_sao_recusados() {
    assert_eq!(
        decodificar_da_ui(br#"{"v":2,"tipo":"batimento"}"#),
        Err(ErroIpc::VersaoIncompativel(2))
    );
    // GC-08: só marco 0 é inválido; qualquer positivo (ex.: 9999) é aceito
    assert_eq!(
        decodificar_da_ui(br#"{"v":1,"tipo":"aviso_exibido","marco":0}"#),
        Err(ErroIpc::MarcoInvalido(0))
    );
}

#[test]
fn o_servico_empurra_estado_e_aviso_em_uma_linha_versionada() {
    let aviso = Aviso {
        sid: "S-1-5-21-1".into(),
        app: Some("jogo.exe".into()),
        restante_segundos: 60,
        motivo: Motivo::LimiteDiarioApp,
    };
    let bytes = codificar_para_ui(&ParaUi::from(&aviso));
    assert_eq!(bytes.last(), Some(&b'\n'));
    let texto = std::str::from_utf8(&bytes).unwrap();
    assert_eq!(
        texto.trim_end(),
        r#"{"v":1,"tipo":"aviso","restante_segundos":60,"motivo":"APP_DAILY_LIMIT","app":"jogo.exe"}"#
    );
    let estado = codificar_para_ui(&ParaUi::Estado {
        restante_segundos: None,
        motivo: None,
    });
    assert_eq!(
        std::str::from_utf8(&estado).unwrap().trim_end(),
        r#"{"v":1,"tipo":"estado","restante_segundos":null,"motivo":null}"#
    );
}

#[test]
fn o_sid_da_sessao_nao_vai_para_a_ui() {
    let aviso = Aviso {
        sid: "S-1-5-21-SEGREDO".into(),
        app: None,
        restante_segundos: 300,
        motivo: Motivo::LimiteDiarioTotal,
    };
    let texto = String::from_utf8(codificar_para_ui(&ParaUi::from(&aviso))).unwrap();
    assert!(!texto.contains("SEGREDO") && !texto.contains("S-1-5"));
}

#[test]
fn limite_de_taxa_recusa_o_excesso_e_libera_na_janela_seguinte() {
    let mut l = LimiteDeTaxa::default();
    for i in 0..MAX_MENSAGENS_POR_SEGUNDO {
        assert!(
            l.permite(t0() + Duration::milliseconds(i64::from(i))),
            "mensagem {i}"
        );
    }
    assert!(!l.permite(t0() + Duration::milliseconds(500)));
    assert!(!l.permite(t0() + Duration::milliseconds(999)));
    assert!(
        l.permite(t0() + Duration::milliseconds(1000)),
        "nova janela"
    );
}

#[test]
fn ui_viva_nunca_gera_alerta_e_parada_gera_um_unico_alerta() {
    let mut m = MonitorBatimento::novo(t0());
    assert_eq!(m.estado(), EstadoUi::Aguardando);
    // batimentos a cada 5 s por 1 minuto
    for s in (0..=60).step_by(5) {
        let agora = t0() + Duration::seconds(s);
        assert_eq!(m.batimento(agora), None);
        assert_eq!(m.verificar(agora), None);
    }
    assert_eq!(m.estado(), EstadoUi::Viva);

    // sem batimento: até 15 s tolera; 16 s dispara uma vez
    let ultimo = t0() + Duration::seconds(60);
    assert_eq!(m.verificar(ultimo + Duration::seconds(15)), None);
    assert_eq!(
        m.verificar(ultimo + Duration::seconds(16)),
        Some(MudancaUi::Perdida {
            segundos_sem_batimento: 16
        })
    );
    assert_eq!(
        m.verificar(ultimo + Duration::seconds(30)),
        None,
        "alerta único"
    );
    assert_eq!(m.estado(), EstadoUi::Parada);

    // a UI volta
    assert_eq!(
        m.batimento(ultimo + Duration::seconds(40)),
        Some(MudancaUi::Recuperada)
    );
    assert_eq!(m.estado(), EstadoUi::Viva);
    assert_eq!(m.verificar(ultimo + Duration::seconds(41)), None);
}

#[test]
fn ui_que_nunca_apareceu_tambem_dispara_depois_da_tolerancia() {
    let mut m = MonitorBatimento::novo(t0());
    assert_eq!(m.verificar(t0() + Duration::seconds(15)), None);
    assert!(matches!(
        m.verificar(t0() + Duration::seconds(16)),
        Some(MudancaUi::Perdida { .. })
    ));
    // depois de relançar a UI, a espera recomeça
    m.reiniciar(t0() + Duration::seconds(20));
    assert_eq!(m.estado(), EstadoUi::Aguardando);
    assert_eq!(m.verificar(t0() + Duration::seconds(30)), None);
}
