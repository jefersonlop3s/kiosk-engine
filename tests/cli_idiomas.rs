//! ADR-010: comandos em inglês (convenção) e em português (equivalentes para sempre).

use std::process::{Command, Output};

use kiosk_engine::cli::{COMANDOS, OPCOES, em_ingles};
use tempfile::TempDir;

fn v(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn portugues_vira_ingles_so_no_comando_no_subcomando_e_nas_opcoes() {
    assert_eq!(
        em_ingles(&v(&[
            "perfil", "criar", "--conta", "regra", "--nome", "listar"
        ])),
        v(&[
            "profile",
            "create",
            "--account",
            "regra",
            "--name",
            "listar"
        ]),
        "valores (inclusive palavras que são nomes de comando) passam intactos"
    );
    assert_eq!(em_ingles(&v(&["estado"])), v(&["status"]));
    assert_eq!(em_ingles(&v(&["servico"])), v(&["service"]));
    assert_eq!(
        em_ingles(&v(&["revogar", "limite"])),
        v(&["revoke", "limite"]),
        "`revoke` não é grupo: a 2ª palavra é um valor"
    );
    assert_eq!(
        em_ingles(&v(&["uso", "--perfil", "X", "--data", "2026-10-29"])),
        v(&["usage", "--profile", "X", "--date", "2026-10-29"])
    );
}

#[test]
fn ingles_e_misturado_passam_intactos_ou_normalizados() {
    let ingles = v(&[
        "rule",
        "create",
        "--profile",
        "P",
        "--limit",
        "60",
        "--activate",
    ]);
    assert_eq!(em_ingles(&ingles), ingles);
    assert_eq!(
        em_ingles(&v(&["rule", "criar", "--perfil", "P", "--limit", "60"])),
        v(&["rule", "create", "--profile", "P", "--limit", "60"])
    );
}

#[test]
fn as_tabelas_nao_tem_duplicata_nem_nome_ingles_igual_a_um_portugues() {
    for tabela in [COMANDOS, OPCOES] {
        for (i, (pt, en)) in tabela.iter().enumerate() {
            assert!(
                tabela.iter().skip(i + 1).all(|(p, e)| p != pt && e != en),
                "duplicata: {pt} / {en}"
            );
            assert!(
                tabela.iter().all(|(p, _)| p != en),
                "{en} é inglês e também português: a tradução seria ambígua"
            );
        }
    }
}

fn kioskd(db: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kioskd"))
        .args(args)
        .env("KIOSK_DB", db)
        .env_remove("KIOSK_LISTEN")
        .output()
        .unwrap()
}

fn id(o: &Output) -> String {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8_lossy(&o.stdout)
        .split_whitespace()
        .find(|p| uuid::Uuid::parse_str(p).is_ok())
        .unwrap()
        .to_owned()
}

#[test]
fn o_mesmo_cadastro_em_ingles_e_em_portugues_da_o_mesmo_resultado() {
    let dir = TempDir::new().unwrap();
    let db = format!("sqlite://{}", dir.path().join("k.db").display());

    let perfil = id(&kioskd(
        &db,
        &[
            "profile",
            "create",
            "--sid",
            "S-1-5-21-1-2-3-1001",
            "--name",
            "Ana",
            "--total-limit",
            "7200",
        ],
    ));
    let app = id(&kioskd(&db, &["app", "create", "--process", "jogo.exe"]));
    let regra_en = id(&kioskd(
        &db,
        &[
            "rule",
            "create",
            "--profile",
            &perfil,
            "--app",
            &app,
            "--limit",
            "600",
            "--activate",
        ],
    ));
    // a mesma coisa em português, misturando com inglês
    let app2 = id(&kioskd(&db, &["app", "criar", "--processo", "outro.exe"]));
    let regra_pt = id(&kioskd(
        &db,
        &[
            "regra", "criar", "--perfil", &perfil, "--app", &app2, "--limit", "600", "--ativar",
        ],
    ));

    let en = kioskd(&db, &["rule", "list"]);
    let pt = kioskd(&db, &["regra", "listar"]);
    assert!(en.status.success() && pt.status.success());
    assert_eq!(en.stdout, pt.stdout, "listar nas duas línguas dá o mesmo");
    let lista = String::from_utf8_lossy(&en.stdout);
    assert!(
        lista.contains(&regra_en) && lista.contains(&regra_pt),
        "{lista}"
    );

    let semana_en = kioskd(
        &db,
        &[
            "profile",
            "window",
            &perfil,
            "--days",
            "mon,tue",
            "--window",
            "08:00-20:00",
        ],
    );
    let semana_pt = kioskd(
        &db,
        &[
            "perfil",
            "janela",
            &perfil,
            "--dias",
            "seg,ter",
            "--janela",
            "08:00-20:00",
        ],
    );
    assert!(semana_en.status.success() && semana_pt.status.success());
}

#[test]
fn o_uso_impresso_esta_em_ingles_e_aponta_os_equivalentes() {
    for exe in [env!("CARGO_BIN_EXE_kioskd"), env!("CARGO_BIN_EXE_kioskctl")] {
        let o = Command::new(exe).env_remove("KIOSK_DB").output().unwrap();
        let uso = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success());
        assert!(uso.contains("comandos-em-portugues.md"), "{uso}");
        // só as linhas de comando: a linha de exemplo cita o português de propósito
        for linha in uso.lines().filter(|l| l.starts_with("  kiosk")) {
            let palavras: Vec<&str> = linha.split_whitespace().collect();
            for (pt, _) in COMANDOS.iter().chain(OPCOES) {
                assert!(
                    !palavras.contains(pt),
                    "nome em português no uso: {pt}\n{linha}"
                );
            }
        }
    }
}

/// `kioskd profile limit <não-uuid>` falha na hora, antes de abrir o banco, com uma mensagem que
/// depende do idioma; serve para observar a escolha do idioma em qualquer sistema.
fn mensagem_do_erro(ambiente: &[(&str, &str)]) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_kioskd"));
    c.args(["profile", "limit", "nao-e-uuid", "--seconds", "1"])
        .env_remove("KIOSK_DB")
        .env_remove("KIOSK_LANG")
        .env_remove("LC_ALL")
        .env_remove("LC_MESSAGES")
        .env_remove("LANG");
    for (k, v) in ambiente {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    assert!(!o.status.success());
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn kiosk_lang_escolhe_o_idioma_da_mensagem() {
    for (tag, esperado) in [
        ("en-US", "invalid identifier"),
        ("pt-BR", "identificador inválido"),
        ("es-ES", "identificador no válido"),
        ("pt_BR.UTF-8", "identificador inválido"),
        ("es-MX", "identificador no válido"),
        ("en-GB", "invalid identifier"),
    ] {
        let erro = mensagem_do_erro(&[("KIOSK_LANG", tag)]);
        assert!(erro.contains(esperado), "{tag}: {erro}");
    }
}

#[cfg(not(windows))]
#[test]
fn sem_kiosk_lang_vale_o_idioma_do_sistema_e_na_falta_dele_en_us() {
    // (no Windows o idioma do sistema vem da API do Windows, não destas variáveis)
    let erro = mensagem_do_erro(&[("LANG", "es_ES.UTF-8")]);
    assert!(erro.contains("identificador no válido"), "{erro}");
    let erro = mensagem_do_erro(&[("LC_ALL", "pt_BR.UTF-8"), ("LANG", "es_ES.UTF-8")]);
    assert!(
        erro.contains("identificador inválido"),
        "LC_ALL vale mais: {erro}"
    );
    // idioma desconhecido (francês) em KIOSK_LANG é ignorado e a busca segue, sem erro
    let erro = mensagem_do_erro(&[("KIOSK_LANG", "fr-FR"), ("LANG", "pt_BR.UTF-8")]);
    assert!(erro.contains("identificador inválido"), "{erro}");
    // nada reconhecível: en-US
    let erro = mensagem_do_erro(&[("KIOSK_LANG", "fr-FR"), ("LANG", "C")]);
    assert!(erro.contains("invalid identifier"), "{erro}");
    let erro = mensagem_do_erro(&[]);
    assert!(erro.contains("invalid identifier"), "{erro}");
}

#[test]
fn o_idioma_nunca_muda_o_nome_dos_comandos_nem_os_codigos() {
    // os dois idiomas de comando funcionam em qualquer idioma de exibição e dão a mesma resposta
    for lang in ["en-US", "pt-BR", "es-ES"] {
        let mut erros = Vec::new();
        for args in [["profile", "limit"], ["perfil", "limite"]] {
            let o = Command::new(env!("CARGO_BIN_EXE_kioskd"))
                .args([args[0], args[1], "x", "--seconds", "1"])
                .env("KIOSK_LANG", lang)
                .env_remove("KIOSK_DB")
                .output()
                .unwrap();
            let t = String::from_utf8_lossy(&o.stderr).into_owned();
            assert!(t.contains("\"x\""), "{lang} {args:?}: {t}");
            assert!(
                !t.contains("uso:") && !t.contains("usage:"),
                "{lang} {args:?}: {t}"
            );
            erros.push(t);
        }
        assert_eq!(erros[0], erros[1], "{lang}: inglês e português dão o mesmo");
    }
}

// ---- argumentos: nada é ignorado em silêncio ----

fn kioskd_em(db: &str, lang: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kioskd"))
        .args(args)
        .env("KIOSK_DB", db)
        .env("KIOSK_LANG", lang)
        .env_remove("KIOSK_LISTEN")
        .output()
        .unwrap()
}

#[test]
fn opcao_digitada_errada_e_recusada_com_sugestao_e_nada_e_criado() {
    let dir = TempDir::new().unwrap();
    let db = format!("sqlite://{}", dir.path().join("k.db").display());
    let perfil = id(&kioskd(
        &db,
        &[
            "profile",
            "create",
            "--sid",
            "S-1-5-21-1-2-3-1001",
            "--name",
            "Ana",
        ],
    ));
    let app = id(&kioskd(&db, &["app", "create", "--process", "notepad.exe"]));

    // o caso do teste no PC real: --activete criava uma regra pendente sem avisar
    for (lang, esperado) in [
        (
            "en-US",
            "unknown option --activete for `rule create` (did you mean --activate?)",
        ),
        (
            "pt-BR",
            "opção desconhecida --activete em `rule create` (você quis dizer --activate?)",
        ),
        (
            "es-ES",
            "opción desconocida --activete en `rule create` (¿quiso decir --activate?)",
        ),
    ] {
        let o = kioskd_em(
            &db,
            lang,
            &[
                "rule",
                "create",
                "--profile",
                &perfil,
                "--app",
                &app,
                "--limit",
                "180",
                "--activete",
            ],
        );
        let erro = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success(), "{lang}: aceitou");
        assert!(erro.contains(esperado), "{lang}: {erro}");
        assert!(
            !erro.contains("kioskd: database") && !erro.contains("banco"),
            "{lang}: abriu o banco: {erro}"
        );
    }
    // e em português também (o nome da opção é o traduzido)
    let o = kioskd_em(
        &db,
        "pt-BR",
        &[
            "regra", "criar", "--perfil", &perfil, "--app", &app, "--ativat",
        ],
    );
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("você quis dizer --activate"));

    let lista = kioskd(&db, &["rule", "list"]);
    assert!(
        String::from_utf8_lossy(&lista.stdout).trim().is_empty(),
        "nenhuma regra devia ter sido criada: {}",
        String::from_utf8_lossy(&lista.stdout)
    );
}

#[test]
fn outros_erros_de_argumento_sao_explicados() {
    let db = "sqlite://nao-e-aberto.db";
    for (args, esperado) in [
        (
            vec!["rule", "create", "--profile"],
            "--profile needs a value in `rule create`",
        ),
        (
            vec!["rule", "list", "extra"],
            "unexpected argument \"extra\" for `rule list`",
        ),
        (
            vec!["status", "--listen", "x"],
            "unknown option --listen for `status`",
        ),
        (
            vec!["revoke", "a", "b"],
            "unexpected argument \"b\" for `revoke`",
        ),
        (
            vec!["app", "create", "--procces", "x.exe"],
            "did you mean --process?",
        ),
        (
            vec!["profile", "create", "--zzzzzzzz", "x"],
            "unknown option --zzzzzzzz for `profile create`",
        ),
    ] {
        let o = kioskd_em(db, "en-US", &args);
        let erro = String::from_utf8_lossy(&o.stderr);
        assert!(
            !o.status.success() && erro.contains(esperado),
            "{args:?}: {erro}"
        );
    }
    // sem parecido, sem sugestão
    let o = kioskd_em(db, "en-US", &["profile", "create", "--zzzzzzzz", "x"]);
    assert!(!String::from_utf8_lossy(&o.stderr).contains("did you mean"));
    // kioskctl também
    let o = Command::new(env!("CARGO_BIN_EXE_kioskctl"))
        .args([
            "rule",
            "create",
            "--perfil",
            "x",
            "--app",
            "y",
            "--activate",
        ])
        .env("KIOSK_LANG", "pt-BR")
        .output()
        .unwrap();
    let erro = String::from_utf8_lossy(&o.stderr);
    assert!(
        !o.status.success() && erro.contains("opção desconhecida --activate em `rule create`"),
        "{erro}"
    );
}

#[test]
fn help_mostra_o_uso_sem_erro() {
    for exe in [env!("CARGO_BIN_EXE_kioskd"), env!("CARGO_BIN_EXE_kioskctl")] {
        for args in [vec!["--help"], vec!["-h"], vec!["rule", "create", "--help"]] {
            let o = Command::new(exe)
                .args(&args)
                .env("KIOSK_LANG", "en-US")
                .output()
                .unwrap();
            assert!(o.status.success(), "{args:?}");
            assert!(
                String::from_utf8_lossy(&o.stdout).contains("usage:"),
                "{args:?}"
            );
        }
    }
}

/// A tabela de argumentos aceitos tem de ser exatamente o que o texto de uso anuncia: uma opção
/// nova no uso sem entrar na tabela seria recusada, e uma na tabela sem estar no uso, não documentada.
#[test]
fn a_tabela_de_argumentos_e_o_texto_de_uso_dizem_o_mesmo() {
    use std::collections::BTreeSet;
    for (uso, exe, formas) in [
        (
            kiosk_engine::i18n::Msg::UsoKioskd,
            "kioskd",
            kiosk_engine::cli::FORMAS_KIOSKD,
        ),
        (
            kiosk_engine::i18n::Msg::UsoKioskctl,
            "kioskctl",
            kiosk_engine::cli::FORMAS_KIOSKCTL,
        ),
    ] {
        for idioma in kiosk_engine::i18n::Idioma::TODOS {
            let texto = uso.em(idioma);
            for f in formas {
                let nome = f.comando.join(" ");
                let linha = texto
                    .lines()
                    .map(str::trim)
                    .find(|l| {
                        l.strip_prefix(&format!("{exe} {nome}"))
                            .is_some_and(|r| r.is_empty() || r.starts_with([' ', '\t']))
                    })
                    .unwrap_or_else(|| panic!("{idioma:?}: `{exe} {nome}` não está no uso"));
                let no_uso: BTreeSet<&str> = linha
                    .split(|c: char| !(c.is_alphanumeric() || c == '-'))
                    .filter(|p| p.starts_with("--"))
                    .collect();
                let na_tabela: BTreeSet<&str> =
                    f.com_valor.iter().chain(f.sem_valor).copied().collect();
                assert_eq!(no_uso, na_tabela, "{idioma:?}: `{exe} {nome}`: {linha}");
            }
        }
    }
}

#[test]
fn regra_que_ja_nasce_acima_do_uso_do_dia_avisa_em_cada_idioma() {
    use kiosk_engine::db_manager::{EventContext, KioskDbManager};
    let rt = tokio::runtime::Runtime::new().unwrap();
    let dir = TempDir::new().unwrap();
    let db = format!("sqlite://{}", dir.path().join("k.db").display());
    let perfil = id(&kioskd(
        &db,
        &[
            "profile",
            "create",
            "--sid",
            "S-1-5-21-1-2-3-1001",
            "--name",
            "Ana",
            "--total-limit",
            "600",
        ],
    ));
    let app = id(&kioskd(&db, &["app", "create", "--process", "notepad.exe"]));
    // 11min09s de Bloco de Notas e 15 min de sessão hoje, como no teste real
    rt.block_on(async {
        let d = KioskDbManager::new(&db).await.unwrap();
        let fim = chrono::Utc::now();
        let ini = fim - chrono::Duration::seconds(500);
        let (p, a) = (perfil.parse().unwrap(), app.parse().unwrap());
        let tz = chrono::Local;
        // dois intervalos de até 10 min, dentro do mesmo dia local
        d.registrar_uso(&EventContext::local(), p, a, ini, fim, &tz)
            .await
            .unwrap();
        d.registrar_sessao(&EventContext::local(), p, ini, fim, &tz)
            .await
            .unwrap();
        d.registrar_uso(
            &EventContext::local(),
            p,
            a,
            ini,
            ini + chrono::Duration::seconds(169),
            &tz,
        )
        .await
        .unwrap();
    });
    // uso de hoje do app: 669 s (se a virada do dia local cair no meio do teste, o limite abaixo ainda vale)
    for (lang, esperado) in [
        ("en-US", "warning: notepad.exe has already used"),
        ("pt-BR", "atenção: notepad.exe já usou"),
        ("es-ES", "atención: notepad.exe ya usó"),
    ] {
        let o = kioskd_em(
            &db,
            lang,
            &[
                "rule",
                "create",
                "--profile",
                &perfil,
                "--app",
                &app,
                "--limit",
                "180",
                "--activate",
            ],
        );
        let erro = String::from_utf8_lossy(&o.stderr);
        assert!(o.status.success(), "{lang}: {erro}");
        assert!(
            erro.contains(esperado) && erro.contains("3min 00s"),
            "{lang}: {erro}"
        );
    }
    // pendente não avisa (ainda não vale), e limite acima do uso também não
    let o = kioskd_em(
        &db,
        "en-US",
        &[
            "rule",
            "create",
            "--profile",
            &perfil,
            "--app",
            &app,
            "--limit",
            "180",
        ],
    );
    assert!(!String::from_utf8_lossy(&o.stderr).contains("warning"));
    let o = kioskd_em(
        &db,
        "en-US",
        &[
            "rule",
            "create",
            "--profile",
            &perfil,
            "--app",
            &app,
            "--limit",
            "86400",
            "--activate",
        ],
    );
    assert!(!String::from_utf8_lossy(&o.stderr).contains("warning"));
    // limite 0 é bloqueio de propósito: sem aviso
    let o = kioskd_em(
        &db,
        "en-US",
        &[
            "rule",
            "create",
            "--profile",
            &perfil,
            "--app",
            &app,
            "--limit",
            "0",
            "--activate",
        ],
    );
    assert!(!String::from_utf8_lossy(&o.stderr).contains("warning"));

    // o limite total do perfil (600 s) também: a sessão já passou de 500 s? ainda não, então sem aviso;
    // abaixo do uso, avisa
    let o = kioskd_em(
        &db,
        "pt-BR",
        &["profile", "limit", &perfil, "--seconds", "300"],
    );
    let erro = String::from_utf8_lossy(&o.stderr);
    assert!(
        o.status.success() && erro.contains("atenção: a conta já ficou"),
        "{erro}"
    );
    let o = kioskd_em(
        &db,
        "en-US",
        &["profile", "limit", &perfil, "--seconds", "90000"],
    );
    assert!(!String::from_utf8_lossy(&o.stderr).contains("warning"));
}
