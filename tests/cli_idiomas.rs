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
