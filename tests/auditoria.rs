//! Rastro das tentativas de comando sem privilégio (ADR-011), nas partes que não dependem do
//! Windows: protocolo, limite de taxa, evento gravado e o que a API entrega aos pais. O pipe e o
//! token do chamador são exercitados em `tests/servico_windows.rs`, no CI Windows.

use chrono::{DateTime, Duration, TimeZone, Utc};
use kiosk_engine::auditoria::*;
use kiosk_engine::database_schema::*;
use kiosk_engine::db_manager::{EventContext, KioskDbManager};
use kiosk_engine::detalhes::Detalhe;
use tempfile::TempDir;
use uuid::Uuid;

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap()
}

fn chamador(sid: &str, admin: bool) -> Chamador {
    Chamador {
        sid: sid.into(),
        conta_admin: admin,
    }
}

async fn banco() -> (TempDir, KioskDbManager) {
    let dir = tempfile::tempdir().unwrap();
    let db = KioskDbManager::new(&format!("sqlite://{}", dir.path().join("k.db").display()))
        .await
        .unwrap();
    (dir, db)
}

async fn perfil(db: &KioskDbManager, sid: &str) -> Uuid {
    let p = ChildProfile {
        profile_id: Uuid::new_v4(),
        windows_sid: sid.into(),
        display_name: "Ana".into(),
        daily_total_limit_seconds: None,
    };
    db.create_profile(&EventContext::local(), &p).await.unwrap();
    p.profile_id
}

/// Eventos `AUTHZ_FAIL` e `VALIDATION_FAIL` gravados: (tipo, ação, perfil, detalhe).
async fn eventos(db: &KioskDbManager) -> Vec<(String, String, Option<String>, String)> {
    sqlx::query_as(
        "SELECT event_type, action_attempted, profile_id, rejection_reason_detail FROM security_log \
         WHERE event_type IN ('AUTHZ_FAIL', 'VALIDATION_FAIL') ORDER BY rowid",
    )
    .fetch_all(db.pool())
    .await
    .unwrap()
}

#[test]
fn o_aviso_montado_pelo_cliente_e_aceito_para_todo_comando_da_lista() {
    for c in COMANDOS {
        let linha = montar_aviso(c);
        assert!(linha.len() <= MAX_BYTES && !linha.contains('\n'), "{linha}");
        assert_eq!(decodificar(linha.as_bytes()), Ok(*c), "{linha}");
        // com o \n do fim, como chega pelo pipe, e com \r\n
        assert_eq!(decodificar(format!("{linha}\n").as_bytes()), Ok(*c));
        assert_eq!(decodificar(format!("{linha}\r\n").as_bytes()), Ok(*c));
    }
    assert_eq!(
        montar_aviso("pairing-code"),
        r#"{"v":1,"tipo":"tentativa_sem_privilegio","comando":"pairing-code"}"#
    );
    assert!(resposta_ok(&montar_resposta(true)));
    assert!(!resposta_ok(&montar_resposta(false)));
    assert!(resposta_ok(&format!("{}\n", montar_resposta(true))));
}

#[test]
fn entrada_hostil_e_recusada() {
    let base = |comando: &str| {
        format!(r#"{{"v":1,"tipo":"tentativa_sem_privilegio","comando":"{comando}"}}"#)
    };
    // comando fora da lista, inclusive variações que um filtro frouxo deixaria passar
    for ruim in [
        "",
        "PAIRING-CODE",
        "pairing-code ",
        "rule",
        "serve",
        "status",
        "rule create; x",
        "../x",
        "\\u0000",
    ] {
        assert_eq!(
            decodificar(base(ruim).as_bytes()),
            Err(ErroAviso::ComandoDesconhecido),
            "{ruim:?}"
        );
    }
    // o chamador nunca vem da mensagem: campo extra (sid, device...) é recusado
    for extra in [
        r#","sid":"S-1-5-18""#,
        r#","conta_admin":true"#,
        r#","perfil":"x""#,
    ] {
        let linha = format!(
            r#"{{"v":1,"tipo":"tentativa_sem_privilegio","comando":"pairing-code"{extra}}}"#
        );
        assert!(
            matches!(decodificar(linha.as_bytes()), Err(ErroAviso::Json(_))),
            "{extra}"
        );
    }
    assert_eq!(
        decodificar(br#"{"v":2,"tipo":"tentativa_sem_privilegio","comando":"pairing-code"}"#),
        Err(ErroAviso::Versao(2))
    );
    assert_eq!(
        decodificar(br#"{"v":1,"tipo":"outra","comando":"pairing-code"}"#),
        Err(ErroAviso::Tipo)
    );
    assert!(matches!(
        decodificar(b"nao e json"),
        Err(ErroAviso::Json(_))
    ));
    assert!(matches!(decodificar(b"{}"), Err(ErroAviso::Json(_))));
    assert!(matches!(decodificar(b""), Err(ErroAviso::Json(_))));
    assert_eq!(decodificar(&[0xff, 0xfe, 0xfd]), Err(ErroAviso::NaoEUtf8));
    // acima do tamanho máximo, mesmo que o começo seja válido
    let grande = format!("{}{}", montar_aviso("pairing-code"), " ".repeat(MAX_BYTES));
    assert_eq!(decodificar(grande.as_bytes()), Err(ErroAviso::Grande));
    assert_eq!(
        decodificar(&vec![b'a'; MAX_BYTES + 1]),
        Err(ErroAviso::Grande)
    );
    // só a primeira linha vale: duas mensagens coladas não são uma
    let duas = format!("{}\n{}", montar_aviso("devices"), montar_aviso("revoke"));
    assert!(decodificar(duas.as_bytes()).is_err());
}

#[test]
fn a_resposta_do_servico_so_vale_se_for_exatamente_ok() {
    for ruim in [
        "",
        "ok",
        r#"{"v":1,"ok":false}"#,
        r#"{"v":2,"ok":true}"#,
        r#"{"v":1,"ok":true,"x":1}"#,
        "{",
    ] {
        assert!(!resposta_ok(ruim), "{ruim:?}");
    }
}

#[test]
fn limite_de_taxa_por_conta_conta_o_excedente_e_o_reporta_no_proximo() {
    let mut l = Limitador::default();
    let a = "S-1-5-21-1-1-1-1001";
    for i in 0..MAX_AVISOS_POR_MINUTO {
        assert_eq!(
            l.admitir(a, t0() + Duration::seconds(i64::from(i))),
            Admissao::Aceito { suprimidos: 0 },
            "{i}"
        );
    }
    // o excedente do minuto é descartado e contado
    for _ in 0..3 {
        assert_eq!(
            l.admitir(a, t0() + Duration::seconds(30)),
            Admissao::Descartado
        );
    }
    // outra conta tem a própria cota
    assert_eq!(
        l.admitir("S-1-5-21-1-1-1-1002", t0() + Duration::seconds(30)),
        Admissao::Aceito { suprimidos: 0 }
    );
    // passado o minuto, volta a valer e leva a contagem do que foi descartado
    assert_eq!(
        l.admitir(a, t0() + Duration::seconds(61)),
        Admissao::Aceito { suprimidos: 3 }
    );
    assert_eq!(
        l.admitir(a, t0() + Duration::seconds(62)),
        Admissao::Aceito { suprimidos: 0 },
        "a contagem é entregue uma vez só"
    );
}

#[test]
fn o_limitador_nao_cresce_sem_limite() {
    let mut l = Limitador::default();
    // muito mais contas do que o teto: as paradas e sem nada a reportar saem da memória
    for i in 0..3000 {
        l.admitir(
            &format!("S-1-5-21-{i}"),
            t0() + Duration::minutes(i64::from(i) / 100),
        );
    }
    // continua funcionando para a conta atual
    assert_eq!(
        l.admitir("S-1-5-21-novo", t0() + Duration::minutes(60)),
        Admissao::Aceito { suprimidos: 0 }
    );
}

#[tokio::test]
async fn conta_comum_sem_perfil_grava_authz_fail_com_o_sid_do_chamador() {
    let (_d, db) = banco().await;
    let mut l = Limitador::default();
    let r = registrar(
        &db,
        &mut l,
        &chamador("S-1-5-21-9-9-9-1005", false),
        montar_aviso("pairing-code").as_bytes(),
        t0(),
    )
    .await;
    assert_eq!(r, Ok(Registro::Gravado));
    let e = eventos(&db).await;
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].0.as_str(), e[0].1.as_str(), e[0].2.as_deref()),
        ("AUTHZ_FAIL", "CREATE_PAIRING_CODE", None)
    );
    assert_eq!(
        Detalhe::de_texto(&e[0].3),
        Some(Detalhe::AdminRequired {
            command: "pairing-code".into(),
            admin_account: false,
            caller_sid: Some("S-1-5-21-9-9-9-1005".into()),
            suppressed: None
        })
    );
    // o horário é o da gravação, e o evento fica "local"
    let (ip, ts): (String, String) = sqlx::query_as(
        "SELECT client_ip, timestamp FROM security_log WHERE event_type = 'AUTHZ_FAIL'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(ip, "local");
    assert!(ts.starts_with("2026-10-29T12:00:00"), "{ts}");
}

#[tokio::test]
async fn perfil_de_crianca_leva_o_perfil_e_nao_o_sid() {
    let (_d, db) = banco().await;
    let sid = "S-1-5-21-1-2-3-1004";
    let p = perfil(&db, sid).await;
    let mut l = Limitador::default();
    registrar(
        &db,
        &mut l,
        &chamador(sid, false),
        montar_aviso("rule create").as_bytes(),
        t0(),
    )
    .await
    .unwrap();
    let e = eventos(&db).await;
    assert_eq!(
        (e[0].0.as_str(), e[0].1.as_str()),
        ("AUTHZ_FAIL", "ADD_RULE")
    );
    assert_eq!(e[0].2.as_deref(), Some(p.to_string().as_str()));
    assert!(
        !e[0].3.contains(sid),
        "o SID não precisa estar no detalhe: {}",
        e[0].3
    );
    assert_eq!(
        Detalhe::de_texto(&e[0].3),
        Some(Detalhe::AdminRequired {
            command: "rule create".into(),
            admin_account: false,
            caller_sid: None,
            suppressed: None
        })
    );
}

#[tokio::test]
async fn administrador_sem_elevacao_e_registrado_e_marcado() {
    let (_d, db) = banco().await;
    let mut l = Limitador::default();
    registrar(
        &db,
        &mut l,
        &chamador("S-1-5-21-1-2-3-1001", true),
        montar_aviso("devices").as_bytes(),
        t0(),
    )
    .await
    .unwrap();
    let e = eventos(&db).await;
    assert_eq!(e[0].1, "SYSTEM", "devices não tem ação própria");
    match Detalhe::de_texto(&e[0].3).unwrap() {
        Detalhe::AdminRequired { admin_account, .. } => assert!(admin_account),
        outro => panic!("{outro:?}"),
    }
}

#[tokio::test]
async fn cada_comando_da_lista_mapeia_para_a_acao_certa() {
    let (_d, db) = banco().await;
    let mut l = Limitador::default();
    // uma conta por comando, para não bater no limite de taxa
    for (i, c) in COMANDOS.iter().enumerate() {
        let r = registrar(
            &db,
            &mut l,
            &chamador(&format!("S-1-5-21-0-0-0-{}", 2000 + i), false),
            montar_aviso(c).as_bytes(),
            t0(),
        )
        .await;
        assert_eq!(r, Ok(Registro::Gravado), "{c}");
    }
    let e = eventos(&db).await;
    assert_eq!(e.len(), COMANDOS.len());
    for ((_, acao, _, detalhe), c) in e.iter().zip(COMANDOS) {
        assert_eq!(acao, acao_do_comando(c).as_str(), "{c}");
        match Detalhe::de_texto(detalhe).unwrap() {
            Detalhe::AdminRequired { command, .. } => assert_eq!(&command, c),
            outro => panic!("{outro:?}"),
        }
    }
    assert_eq!(
        acao_do_comando("pairing-code").as_str(),
        "CREATE_PAIRING_CODE"
    );
    assert_eq!(acao_do_comando("rule revoke").as_str(), "REVOKE");
    assert_eq!(acao_do_comando("rule limit").as_str(), "MODIFY_LIMIT");
    assert_eq!(acao_do_comando("rule activate").as_str(), "ACTIVATE_RULE");
}

#[tokio::test]
async fn inundacao_vira_cinco_eventos_e_a_contagem_do_resto() {
    let (_d, db) = banco().await;
    let mut l = Limitador::default();
    let c = chamador("S-1-5-21-1-2-3-1004", false);
    let linha = montar_aviso("pairing-code");
    let mut gravados = 0;
    for i in 0..9 {
        let r = registrar(
            &db,
            &mut l,
            &c,
            linha.as_bytes(),
            t0() + Duration::seconds(i),
        )
        .await
        .unwrap();
        gravados += usize::from(r == Registro::Gravado);
    }
    assert_eq!(gravados, 5);
    assert_eq!(eventos(&db).await.len(), 5);
    // o minuto seguinte: um evento que diz quantos ficaram de fora
    let r = registrar(
        &db,
        &mut l,
        &c,
        linha.as_bytes(),
        t0() + Duration::seconds(65),
    )
    .await;
    assert_eq!(r, Ok(Registro::Gravado));
    let e = eventos(&db).await;
    assert_eq!(e.len(), 6);
    match Detalhe::de_texto(&e[5].3).unwrap() {
        Detalhe::AdminRequired { suppressed, .. } => assert_eq!(suppressed, Some(4)),
        outro => panic!("{outro:?}"),
    }
}

#[tokio::test]
async fn aviso_invalido_vira_validation_fail_e_erro_sem_dar_a_voz_ao_chamador() {
    let (_d, db) = banco().await;
    let mut l = Limitador::default();
    let c = chamador("S-1-5-21-1-2-3-1004", false);
    // tenta se passar por SYSTEM e por perfil de outra criança
    let r = registrar(
        &db,
        &mut l,
        &c,
        br#"{"v":1,"tipo":"tentativa_sem_privilegio","comando":"pairing-code","sid":"S-1-5-18"}"#,
        t0(),
    )
    .await;
    assert!(r.is_err());
    let e = eventos(&db).await;
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].0.as_str(), e[0].2.as_deref()),
        ("VALIDATION_FAIL", None)
    );
    assert!(e[0].3.starts_with("invalid audit notice:"), "{}", e[0].3);
    assert!(
        !e[0].3.contains("S-1-5-18"),
        "o texto do chamador não entra no log: {}",
        e[0].3
    );
    // o lixo também gasta a cota: não dá para encher o log com mensagens inválidas
    let mut gravados = 1;
    for i in 1..20 {
        let _ = registrar(&db, &mut l, &c, b"lixo", t0() + Duration::seconds(i)).await;
        gravados = eventos(&db).await.len();
    }
    assert_eq!(gravados, MAX_AVISOS_POR_MINUTO as usize);
}

#[test]
fn o_detalhe_para_os_pais_tira_o_sid_e_so_vale_para_este_codigo() {
    use kiosk_engine::detalhes::para_os_pais;
    let com_sid = Detalhe::AdminRequired {
        command: "pairing-code".into(),
        admin_account: false,
        caller_sid: Some("S-1-5-21-9-9-9-1005".into()),
        suppressed: Some(2),
    }
    .codigo();
    let saida = para_os_pais(&com_sid).unwrap();
    assert!(
        !saida.contains("S-1-5") && !saida.contains("caller_sid"),
        "{saida}"
    );
    assert_eq!(
        Detalhe::de_texto(&saida),
        Some(Detalhe::AdminRequired {
            command: "pairing-code".into(),
            admin_account: false,
            caller_sid: None,
            suppressed: Some(2)
        })
    );
    // qualquer outro detalhe de AUTHZ_FAIL (cita dispositivos) continua só no banco
    assert_eq!(
        para_os_pais("device 1234 lacks permission for ADD_RULE"),
        None
    );
    assert_eq!(para_os_pais(&Detalhe::PairingCodeNotAdmin.codigo()), None);
    assert_eq!(para_os_pais(""), None);
}

#[test]
fn o_texto_para_os_pais_diz_quem_tentou_o_que_em_cada_idioma() {
    use kiosk_engine::i18n::Idioma::{EnUs, EsEs, PtBr};
    let comum = Detalhe::AdminRequired {
        command: "pairing-code".into(),
        admin_account: false,
        caller_sid: None,
        suppressed: None,
    };
    assert_eq!(
        comum.texto(PtBr),
        "`pairing-code` exige administrador elevado; tentativa de uma conta comum"
    );
    assert_eq!(
        comum.texto(EnUs),
        "`pairing-code` needs an elevated administrator; attempted by a standard account"
    );
    assert_eq!(
        comum.texto(EsEs),
        "`pairing-code` exige un administrador elevado; intento de una cuenta estándar"
    );
    let admin = Detalhe::AdminRequired {
        command: "rule create".into(),
        admin_account: true,
        caller_sid: None,
        suppressed: Some(3),
    };
    assert_eq!(
        admin.texto(PtBr),
        "`rule create` exige administrador elevado; tentativa de uma conta de administrador sem elevação (+3 tentativas no último minuto não foram registradas)"
    );
    assert!(
        admin
            .texto(EnUs)
            .contains("an administrator account that was not elevated")
    );
    assert!(admin.texto(EsEs).contains("sin elevar"));
}
