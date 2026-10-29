#!/usr/bin/env python3
"""Verificador genérico de referências entre documentos Markdown.

Percorre os `*.md` de um diretório de docs e confere que todo alvo local
terminado em `.md` existe de verdade — link `[texto](alvo.md)` (com ou sem
`#âncora`) e caminho em code span `` `pasta/arquivo.md` ``. URL externa
(`http:`, `mailto:` etc.) é ignorada.

Genérico de propósito: não conhece PENDING, ADR nem nenhum layout fixo —
só verifica existência de arquivo. Serve para qualquer projeto, com
qualquer topologia de `docs/`.

    python3 scripts/verificar_refs.py [--docs docs] [--raiz .]

Exit 0: todas as referências resolvem. Exit 1: há alvo ausente (lista
`arquivo:linha: alvo`). Fail-loud, sem exceção silenciosa.
"""
from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

LINK_MD = re.compile(r"\[[^\]]*\]\(([^)\s]+)\)")
CODE_SPAN_MD = re.compile(r"`([^`\s]+\.md(?:#[^`\s]*)?)`", re.IGNORECASE)
ESQUEMA_EXTERNO = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")

EXCLUIDOS = {".git", "node_modules", "target", "dist", "build",
             ".venv", "venv", "__pycache__", ".idea", ".vscode"}


def eh_referencia_resolvivel(alvo: str) -> bool:
    """Só o que dá para conferir: sem esquema externo e sem placeholder `<...>`.
    Placeholder (ex.: `.agents/skills/<nome>/SKILL.md`) é instrução de
    preenchimento, não caminho — verificar quebraria todo template."""
    return bool(alvo) and not ESQUEMA_EXTERNO.match(alvo) and "<" not in alvo and ">" not in alvo


def alvos_de(texto: str) -> list[tuple[int, str]]:
    """(linha, alvo) para cada referência local a .md no texto.

    Code span que é TEXTO de um link ([`arq.md`](destino)) não é alvo: o que
    se confere é o destino. Sem isso, o idioma `[`nome.md`](caminho)` — o
    normal em docs — vira falso positivo."""
    achados: list[tuple[int, str]] = []
    for nro, linha in enumerate(texto.splitlines(), 1):
        links = list(LINK_MD.finditer(linha))
        for m in links:
            alvo = m.group(1).split("#", 1)[0].strip()
            if alvo.lower().endswith(".md") and eh_referencia_resolvivel(alvo):
                achados.append((nro, alvo))
        ocupados = [(m.start(), m.end()) for m in links]
        for m in CODE_SPAN_MD.finditer(linha):
            if any(ini <= m.start() < fim for ini, fim in ocupados):
                continue
            alvo = m.group(1).split("#", 1)[0].strip()
            if alvo.lower().endswith(".md") and eh_referencia_resolvivel(alvo):
                achados.append((nro, alvo))
    return achados


def resolver(alvo: str, origem: Path, raiz: Path) -> Path:
    if alvo.startswith("/"):
        return (raiz / alvo.lstrip("/")).resolve()
    return (origem.parent / alvo).resolve()


def verificar(docs: Path, raiz: Path) -> list[str]:
    erros: list[str] = []
    arquivos = sorted(p for p in docs.rglob("*.md")
                      if not any(parte in EXCLUIDOS for parte in p.parts))
    for arq in arquivos:
        try:
            texto = arq.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as e:
            erros.append(f"{arq}:0: não consegui ler ({e})")
            continue
        for nro, alvo in alvos_de(texto):
            destino = resolver(alvo, arq, raiz)
            if not destino.is_file():
                try:
                    rel = destino.relative_to(Path.cwd())
                except ValueError:
                    rel = destino
                erros.append(f"{arq}:{nro}: alvo ausente `{alvo}` (resolve para {rel})")
    return erros


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description="Confere referências locais entre .md.")
    ap.add_argument("--docs", default="docs",
                    help="diretório de docs a varrer (default: docs)")
    ap.add_argument("--raiz", default=".",
                    help="base para alvos com / inicial (default: diretório atual)")
    args = ap.parse_args(argv)
    docs = Path(args.docs)
    if not docs.is_dir():
        print(f"diretório de docs inexistente: {docs}")
        return 1
    erros = verificar(docs, Path(args.raiz))
    for e in erros:
        print(e)
    if erros:
        print(f"{len(erros)} referência(s) quebrada(s)")
        return 1
    print("refs em dia")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
