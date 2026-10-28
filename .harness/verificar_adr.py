#!/usr/bin/env python3
"""Validador opcional de índice e identidade de ADRs.

Confere, num diretório de ADRs, o que o verificador genérico de refs não
vê — porque dois arquivos diferentes podem existir e, ainda assim, colidir:

1. **Identidade**: nenhum número se repete (`ADR-001-a.md` + `ADR-001-b.md`
   é colisão, mesmo com slugs diferentes).
2. **Índice**: todo `ADR-NNN-*.md` tem linha no índice (default `ADR-INDEX.md`).
3. **Próximo livre**: o ponteiro `Próximo número livre: ADR-NNN` do índice é
   `max(NNN) + 1` (ignorado quando não há ADR numerada).

Arquivos sem número (`TEMPLATE.md`, o próprio índice) são ignorados.
Opt-in de propósito: o instalador não o impõe; o projeto o adota no CI
quando passa a numerar ADRs.

    python3 scripts/verificar_adr.py [--adr-dir docs/adr] [--indice docs/adr/ADR-INDEX.md]

Exit 0: índice íntegro. Exit 1: duplicado, sem linha ou aritmética errada,
sempre com arquivo e par esperado/medido.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ARQUIVO_ADR = re.compile(r"^ADR-([0-9]+)-.*\.md$", re.IGNORECASE)
# "Próximo número livre" em pt-BR com ou sem acento.
PROXIMO_LIVRE = re.compile(r"ximo[^0-9]*ADR-([0-9]+)", re.IGNORECASE)


def numeradas(adr_dir: Path) -> dict[int, list[str]]:
    """número -> arquivos que o reivindicam, em ordem."""
    ids: dict[int, list[str]] = {}
    for p in sorted(adr_dir.iterdir()):
        if not p.is_file():
            continue
        m = ARQUIVO_ADR.match(p.name)
        if m:
            ids.setdefault(int(m.group(1)), []).append(p.name)
    return ids


def verificar(adr_dir: Path, indice: Path) -> list[str]:
    erros: list[str] = []
    if not adr_dir.is_dir():
        return [f"diretório de ADRs inexistente: {adr_dir}"]
    if not indice.is_file():
        return [f"índice inexistente: {indice}"]
    ids = numeradas(adr_dir)
    for num in sorted(ids):
        if len(ids[num]) > 1:
            erros.append(
                f"ID ADR-{num:03d} duplicado: " + ", ".join(ids[num]))
    try:
        texto = indice.read_text(encoding="utf-8")
    except OSError as e:
        return erros + [f"{indice}: não consegui ler ({e})"]
    for num in sorted(ids):
        for nome in ids[num]:
            if nome not in texto and f"ADR-{num:03d}" not in texto:
                erros.append(f"{indice.name}: sem linha para {nome}")
    m = PROXIMO_LIVRE.search(texto)
    if ids and m and int(m.group(1)) != max(ids) + 1:
        erros.append(
            f"{indice.name}: próximo livre diz ADR-{m.group(1)}, "
            f"mas o maior ID é ADR-{max(ids):03d}")
    return erros


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Confere IDs e índice de ADRs.")
    ap.add_argument("--adr-dir", default="docs/adr",
                    help="diretório das ADRs (default: docs/adr)")
    ap.add_argument("--indice", default=None,
                    help="arquivo de índice (default: <adr-dir>/ADR-INDEX.md)")
    args = ap.parse_args(argv)
    adr_dir = Path(args.adr_dir)
    indice = Path(args.indice) if args.indice else adr_dir / "ADR-INDEX.md"
    erros = verificar(adr_dir, indice)
    for e in erros:
        print(e)
    if erros:
        print(f"{len(erros)} problema(s) de índice/identidade de ADR")
        return 1
    print("adr em dia")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
