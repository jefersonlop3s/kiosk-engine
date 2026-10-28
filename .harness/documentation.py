#!/usr/bin/env python3
"""Migração explícita de documentos e verificação de links locais, só stdlib.

Plano JSON: {"version": 1, "moves": {"old/path": "new/path"}}.
Sem --apply, migrate imprime o plano e não escreve. Não classifica por nome,
não infere decisões e não atravessa symlinks. O Git guarda o histórico.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

LINK = re.compile(r"(!?\[[^\]\n]*\]\()(<[^>\n]+>|[^\s)]+)([^)\n]*\))")
REFERENCE = re.compile(r"^(\s*\[[^\]\n]+\]:\s*)(<[^>\n]+>|\S+)(.*)$", re.M)


def tracked(root: Path) -> list[str]:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    result = subprocess.run(["git", "-C", str(root), "ls-files", "-z"],
                            env=env, check=True, capture_output=True)
    return result.stdout.decode("utf-8").rstrip("\0").split("\0") if result.stdout else []


def local_path(root: Path, name: str) -> Path:
    path = Path(name)
    if path.is_absolute() or ".." in path.parts or not name or ".git" in path.parts:
        raise ValueError(f"caminho relativo inválido: {name!r}")
    full = root / path
    for part in (full, *full.parents):
        if part == root:
            break
        if part.is_symlink():
            raise ValueError(f"symlink não permitido na migração: {part}")
    # Compara resolvido com resolvido: no macOS o tmpdir vive sob /private
    # via /tmp (ou /var), então full.resolve() nunca é relativo a root cru —
    # medido na matriz macos do CI (todos os testes de documentation).
    if not full.resolve().is_relative_to(root.resolve()):
        raise ValueError(f"caminho fora da raiz: {name}")
    return full


def target_path(root: Path, source: str, target: str, aliases: list[str]) -> tuple[str | None, str]:
    target = target.removeprefix("<").removesuffix(">")
    parsed = urlsplit(target)
    if (parsed.scheme and parsed.scheme != "file") or target.startswith(("#", "//")):
        return None, ""
    name = unquote(parsed.path)
    suffix = ("?" + parsed.query if parsed.query else "") + ("#" + parsed.fragment if parsed.fragment else "")
    if not name:
        return None, suffix
    for alias in [str(root), *aliases]:
        if name.startswith(alias.rstrip("/") + "/"):
            name = name[len(alias.rstrip("/")) + 1:]
            return name, suffix
    if name.startswith("/"):
        return None, suffix  # externo: o projeto decide como validar
    return os.path.normpath(str(Path(source).parent / name)), suffix


def rewrite(root: Path, source: str, destination: str, text: str,
            moves: dict[str, str], aliases: list[str]) -> str:
    def link(match: re.Match) -> str:
        old, suffix = target_path(root, source, match[2], aliases)
        if old is None:
            return match[0]
        # Caminho absoluto para recurso não migrado (ex.: quadro canônico ou
        # repositório irmão) conserva sua autoridade e não vira cópia do worktree.
        if old not in moves and urlsplit(match[2].strip("<>")).path.startswith("/"):
            return match[0]
        new = moves.get(old, old)
        relative = os.path.relpath(new, str(Path(destination).parent))
        target = relative + suffix
        if match[2].startswith("<") or " " in target:
            target = "<" + target + ">"
        return match[1] + target + match[3]

    # Links primeiro: resolver a origem antes de alterar referências textuais.
    text = LINK.sub(link, text)
    text = REFERENCE.sub(link, text)
    # Caminhos completos em comandos/prosa também são consumidores. Substituição
    # simultânea evita que o destino de um movimento seja substituído de novo.
    if moves:
        pattern = re.compile("|".join(re.escape(k) for k in sorted(moves, key=len, reverse=True)))
        text = pattern.sub(lambda m: moves[m[0]], text)
    return text


def migrate(root: Path, plan: Path, apply: bool, aliases: list[str]) -> dict:
    data = json.loads(plan.read_text("utf-8"))
    if data.get("version") != 1 or not isinstance(data.get("moves"), dict):
        raise ValueError("plano deve conter version=1 e moves={origem:destino}")
    moves = data["moves"]
    excluded = data.get("exclude", [])
    if not isinstance(excluded, list) or not all(isinstance(p, str) for p in excluded):
        raise ValueError("exclude deve ser lista de caminhos relativos")
    if set(excluded) & set(moves):
        raise ValueError("origem excluída não pode estar em moves")
    if len(set(moves.values())) != len(moves):
        raise ValueError("dois arquivos têm o mesmo destino")
    files = tracked(root)
    for old, new in moves.items():
        src, dst = local_path(root, old), local_path(root, new)
        if old not in files or not src.is_file():
            raise ValueError(f"origem não é arquivo versionado: {old}")
        if dst.exists() and old != new:
            raise ValueError(f"destino já existe: {new}")
    originals: dict[str, bytes] = {}
    outputs: dict[str, tuple[str, bytes]] = {}
    for old in files:
        if old in excluded:
            continue
        if (root / old).is_symlink() or not (root / old).is_file():
            continue
        content = (root / old).read_bytes()
        new = moves.get(old, old)
        updated = content
        # Processar só texto conhecido; nunca reescrever anexos binários.
        if Path(old).suffix in (".md", ".py", ".sh", ".toml", ".yml", ".yaml", ".json") or old in (".gitignore", ".githooks/pre-commit"):
            try:
                updated = rewrite(root, old, new, content.decode("utf-8"), moves, aliases).encode("utf-8")
            except UnicodeDecodeError:
                if old.endswith(".md"):
                    raise ValueError(f"Markdown não é UTF-8: {old}") from None
        if new != old or updated != content:
            local_path(root, old)
            local_path(root, new)
            originals[old] = content
            outputs[old] = (new, updated)
    report = {"version": 1, "files": [
        {"source": old, "destination": new,
         "before": hashlib.sha256(originals[old]).hexdigest(),
         "after": hashlib.sha256(content).hexdigest()}
        for old, (new, content) in outputs.items()]}
    if not apply:
        return report
    # Checar toda a pré-condição antes da primeira escrita. Recusar concorrência.
    for old, before in originals.items():
        if local_path(root, old).read_bytes() != before:
            raise ValueError(f"origem mudou durante planejamento: {old}")
    completed = []
    try:
        for old, (new, content) in outputs.items():
            src, dst = local_path(root, old), local_path(root, new)
            dst.parent.mkdir(parents=True, exist_ok=True)
            if old != new:
                # x impede sobrescrever um destino surgido depois do preflight.
                with dst.open("xb") as stream:
                    completed.append(old)
                    stream.write(content)
                os.chmod(dst, src.stat().st_mode)
                src.unlink()
            else:
                completed.append(old)
                dst.write_bytes(content)
    except BaseException:
        for old in reversed(completed):
            new, _ = outputs[old]
            (root / old).write_bytes(originals[old])
            if old != new:
                (root / new).unlink(missing_ok=True)
        raise
    return report


def check(root: Path, directories: list[str], aliases: list[str]) -> list[dict]:
    failures = []
    candidates = set(tracked(root))
    # Incluir documentos novos antes do primeiro commit, sem percorrer repositórios
    # filhos nem seguir links de diretório.
    for directory in directories:
        base = local_path(root, directory)
        for parent, dirs, names in os.walk(base, followlinks=False):
            dirs[:] = [d for d in dirs if not (Path(parent) / d).is_symlink() and d != ".git"]
            candidates.update(str((Path(parent) / name).relative_to(root)) for name in names if name.endswith(".md"))
    for name in sorted(candidates):
        if not name.endswith(".md") or not any(name == d or name.startswith(d.rstrip("/") + "/") for d in directories):
            continue
        path = root / name
        if not path.is_file() or path.is_symlink():
            continue
        fenced = False
        for number, line in enumerate(path.read_text("utf-8").splitlines(), 1):
            if line.lstrip().startswith(("```", "~~~")):
                fenced = not fenced
                continue
            if fenced or line.startswith("    "):
                continue
            # Exemplos literais de Markdown não são links navegáveis.
            line = re.sub(r"(`+)(?!`)(.+?)(?<!`)\1(?!`)", "", line)
            for match in [*LINK.finditer(line), *REFERENCE.finditer(line)]:
                target, _ = target_path(root, name, match[2], aliases)
                if target is not None and not (root / target).exists():
                    failures.append({"source": name, "line": number, "target": target})
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("--alias", action="append", default=[], help="raiz absoluta anterior do projeto")
    sub = parser.add_subparsers(dest="command", required=True)
    migration = sub.add_parser("migrate")
    migration.add_argument("plan", type=Path)
    migration.add_argument("--apply", action="store_true")
    validation = sub.add_parser("check")
    validation.add_argument("directories", nargs="*", default=["docs", "AGENTS.md"])
    args = parser.parse_args()
    try:
        root = args.root.resolve(strict=True)
        if args.command == "migrate":
            result = migrate(root, args.plan, args.apply, args.alias)
            print(json.dumps(result, ensure_ascii=False, indent=2))
            return 0
        failures = check(root, args.directories, args.alias)
        print(json.dumps({"broken_links": failures}, ensure_ascii=False, indent=2))
        return 1 if failures else 0
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"documentation: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
