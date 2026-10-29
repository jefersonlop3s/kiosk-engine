//! `kioskd version` / `kioskctl version`: versão e informações da compilação.

use std::process::Command;

use kiosk_engine::cli::em_ingles;
use kiosk_engine::versao::{atual, da_primeira_linha};

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(ToString::to_string).collect()
}

fn rodar(exe: &str, a: &[&str], lang: &str) -> (bool, String, String) {
    let o = Command::new(exe)
        .args(a)
        .env("KIOSK_LANG", lang)
        .env_remove("KIOSK_DB")
        .env_remove("KIOSK_LISTEN")
        .output()
        .unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn a_versao_e_a_do_cargo_e_o_build_tem_todos_os_campos() {
    let b = atual();
    assert_eq!(b.versao, env!("CARGO_PKG_VERSION"));
    assert!(!b.commit.is_empty());
    assert!(!b.alvo.is_empty() && b.alvo != "unknown", "{}", b.alvo);
    assert!(["debug", "release"].contains(&b.perfil), "{}", b.perfil);
    // AAAA-MM-DD
    let partes: Vec<&str> = b.data.split('-').collect();
    assert_eq!(partes.len(), 3, "{}", b.data);
    assert_eq!(partes[0].len(), 4);
    assert!(partes[0].parse::<u32>().unwrap() >= 2026);
    assert!((1..=12).contains(&partes[1].parse::<u32>().unwrap()));
    assert!((1..=31).contains(&partes[2].parse::<u32>().unwrap()));
}

#[test]
fn a_primeira_linha_tem_formato_fixo_e_volta_a_ser_lida() {
    let b = atual();
    let saida = b.completa("kioskd");
    assert_eq!(
        saida.lines().next().unwrap(),
        format!("kioskd {} ({})", b.versao, b.commit)
    );
    assert_eq!(da_primeira_linha(&saida, "kioskd"), Some(b.curta()));
    // o resto tem commit, data, alvo e perfil
    assert_eq!(saida.lines().count(), 5, "{saida}");
}

#[test]
fn leitura_da_primeira_linha_recusa_o_que_nao_e_do_programa() {
    assert_eq!(
        da_primeira_linha("kioskd 0.0.6 (abc1234)\nx", "kioskd"),
        Some("0.0.6 (abc1234)".into())
    );
    assert_eq!(da_primeira_linha("kioskctl 0.0.6 (abc)", "kioskd"), None);
    assert_eq!(da_primeira_linha("kioskd", "kioskd"), None);
    assert_eq!(da_primeira_linha("kioskd ", "kioskd"), None);
    assert_eq!(da_primeira_linha("", "kioskd"), None);
    assert_eq!(
        da_primeira_linha("erro: algo\nkioskd 1.0 (a)", "kioskd"),
        None
    );
}

#[test]
fn versao_em_portugues_e_as_opcoes_curtas_valem_como_version() {
    assert_eq!(em_ingles(&args(&["versao"])), ["version"]);
    assert_eq!(em_ingles(&args(&["version"])), ["version"]);
    assert_eq!(em_ingles(&args(&["--version"])), ["version"]);
    assert_eq!(em_ingles(&args(&["-V"])), ["version"]);
    // só vale sozinho: dentro de outro comando continua sendo uma opção desconhecida
    assert_eq!(
        em_ingles(&args(&["status", "--version"])),
        ["status", "--version"]
    );
}

#[test]
fn os_dois_executaveis_imprimem_a_versao_sem_banco_nem_credenciais() {
    for (exe, nome) in [
        (env!("CARGO_BIN_EXE_kioskd"), "kioskd"),
        (env!("CARGO_BIN_EXE_kioskctl"), "kioskctl"),
    ] {
        for forma in [&["version"][..], &["versao"], &["--version"], &["-V"]] {
            let (ok, saida, erro) = rodar(exe, forma, "pt-BR");
            assert!(ok, "{nome} {forma:?}: {erro}");
            assert_eq!(
                da_primeira_linha(&saida, nome),
                Some(atual().curta()),
                "{nome} {forma:?}: {saida}"
            );
            // não abriu banco nem leu credenciais
            assert!(
                !erro.contains("banco") && !erro.contains("database"),
                "{erro}"
            );
        }
    }
}

#[test]
fn os_campos_saem_no_idioma_de_quem_le_e_a_primeira_linha_nao_muda() {
    let exe = env!("CARGO_BIN_EXE_kioskd");
    let (_, pt, _) = rodar(exe, &["version"], "pt-BR");
    let (_, en, _) = rodar(exe, &["version"], "en-US");
    let (_, es, _) = rodar(exe, &["version"], "es-ES");
    assert!(pt.contains("data da compilação:"), "{pt}");
    assert!(en.contains("build date:"), "{en}");
    assert!(es.contains("fecha de compilación:"), "{es}");
    let primeira = |t: &str| t.lines().next().unwrap().to_owned();
    assert_eq!(primeira(&pt), primeira(&en));
    assert_eq!(primeira(&pt), primeira(&es));
}

#[test]
fn version_nao_aceita_argumento_sobrando() {
    let (ok, _, erro) = rodar(env!("CARGO_BIN_EXE_kioskd"), &["version", "extra"], "en-US");
    assert!(!ok, "aceitou argumento sobrando");
    assert!(erro.contains("extra"), "{erro}");
}
