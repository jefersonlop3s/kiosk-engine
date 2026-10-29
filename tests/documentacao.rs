//! Regra do AGENTS.md ("Fluxogramas dos comandos"): todo comando de CLI tem fluxograma Mermaid
//! em docs/architecture/fluxos-dos-comandos.md, e o documento não cita comando inexistente.

use std::collections::BTreeSet;
use std::process::Command;

const DOC: &str = include_str!("../docs/architecture/fluxos-dos-comandos.md");

/// Comandos que o executável anuncia no próprio texto de uso (chamado sem argumentos).
fn comandos(exe: &str, nome: &str) -> BTreeSet<String> {
    let saida = Command::new(exe).env_remove("KIOSK_DB").output().unwrap();
    let uso = String::from_utf8_lossy(&saida.stderr).into_owned();
    uso.lines()
        .filter_map(|l| {
            let resto = l.trim().strip_prefix(&format!("{nome} "))?;
            let palavras: Vec<&str> = resto
                .split_whitespace()
                .take_while(|p| !p.starts_with(['-', '<', '[', '(']))
                .collect();
            (!palavras.is_empty()).then(|| format!("{nome} {}", palavras.join(" ")))
        })
        .collect()
}

/// Seções `### \`...\`` do documento e se cada uma tem um bloco mermaid antes do próximo título.
fn secoes() -> Vec<(String, bool)> {
    let mut saida = Vec::new();
    let partes: Vec<&str> = DOC.split("\n### ").skip(1).collect();
    for parte in partes {
        let titulo = parte.lines().next().unwrap_or_default().trim();
        let Some(cmd) = titulo.strip_prefix('`').and_then(|t| t.strip_suffix('`')) else {
            continue;
        };
        let corpo = parte.split("\n## ").next().unwrap_or_default();
        saida.push((cmd.to_owned(), corpo.contains("```mermaid")));
    }
    saida
}

#[test]
fn todo_comando_tem_fluxograma_e_o_documento_nao_cita_comando_inexistente() {
    let mut esperados = comandos(env!("CARGO_BIN_EXE_kioskd"), "kioskd");
    let ctl = comandos(env!("CARGO_BIN_EXE_kioskctl"), "kioskctl");
    assert!(
        esperados.len() >= 10 && ctl.len() >= 5,
        "uso ilegível: {esperados:?} {ctl:?}"
    );
    esperados.extend(ctl);

    let documentados = secoes();
    let sem_diagrama: Vec<_> = documentados
        .iter()
        .filter(|(_, m)| !m)
        .map(|(c, _)| c)
        .collect();
    assert!(
        sem_diagrama.is_empty(),
        "seção sem bloco mermaid: {sem_diagrama:?}"
    );

    let doc: BTreeSet<String> = documentados.into_iter().map(|(c, _)| c).collect();
    let faltando: Vec<_> = esperados.difference(&doc).collect();
    assert!(
        faltando.is_empty(),
        "comandos sem fluxograma em docs/architecture/fluxos-dos-comandos.md: {faltando:?}"
    );
    let sobrando: Vec<_> = doc.difference(&esperados).collect();
    assert!(
        sobrando.is_empty(),
        "o documento cita comandos que não existem: {sobrando:?}"
    );
}

/// ADR-010: a página de equivalentes em português lista cada par das tabelas do código.
#[test]
fn a_pagina_em_portugues_lista_cada_equivalente_do_codigo() {
    let pagina = include_str!("../docs/runbooks/comandos-em-portugues.md");
    let faltando: Vec<String> = kiosk_engine::cli::COMANDOS
        .iter()
        .chain(kiosk_engine::cli::OPCOES)
        .map(|(pt, en)| format!("| `{pt}` | `{en}` |"))
        .filter(|linha| !pagina.contains(linha.as_str()))
        .collect();
    assert!(
        faltando.is_empty(),
        "faltam em docs/runbooks/comandos-em-portugues.md: {faltando:?}"
    );
}
