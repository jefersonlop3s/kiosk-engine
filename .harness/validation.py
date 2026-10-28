#!/usr/bin/env python3
"""Executa gates declarados pelo projeto, sem inferir linguagem, forge ou branch."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys
import tomllib


def environment():
    return {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}


def git(root, *args):
    return subprocess.run(["git", "-C", str(root), *args], env=environment(),
                          check=True, capture_output=True, text=True).stdout.strip()


def within(root, name):
    if not isinstance(name, str):
        raise ValueError("cwd/membro deve ser caminho relativo")
    path = Path(name)
    resolved = (root / path).resolve(strict=True)
    if path.is_absolute() or not resolved.is_relative_to(root) or not resolved.is_dir():
        raise ValueError(f"diretório fora do projeto: {name}")
    return resolved


def configuration(root):
    with (root / ".harness.toml").open("rb") as stream:
        config = tomllib.load(stream)
    project = config.get("project", {})
    if not isinstance(project, dict):
        raise ValueError("project deve ser uma tabela")
    topology = project.get("topology", "repository")
    if topology not in ("repository", "monorepo", "meta-repository"):
        raise ValueError("project.topology: repository, monorepo ou meta-repository")
    members = project.get("members", [])
    if not isinstance(members, list) or not all(isinstance(member, str) for member in members):
        raise ValueError("project.members deve ser lista de caminhos")
    if topology == "meta-repository" and not members:
        raise ValueError("metarrepositório exige membros explícitos")
    for member in members:
        path = within(root, member)
        if topology == "meta-repository" and Path(git(path, "rev-parse", "--show-toplevel")).resolve() != path:
            raise ValueError(f"membro não é repositório independente: {member}")
    validation = config.get("validation", {})
    if not isinstance(validation, dict):
        raise ValueError("validation deve ser uma tabela")
    if validation.get("configured") is not True:
        raise ValueError("defina validation.configured=true somente após configurar os gates reais do projeto")
    gates = validation.get("gates", [])
    if not isinstance(gates, list) or not gates:
        raise ValueError("validation.gates exige ao menos um gate explícito")
    ids = set()
    normalized = []
    for gate in gates:
        if not isinstance(gate, dict):
            raise ValueError("cada gate deve ser uma tabela")
        name, argv = gate.get("id"), gate.get("argv")
        if not isinstance(name, str) or not name or name in ids:
            raise ValueError("ID de gate vazio ou duplicado")
        if not isinstance(argv, list) or not argv or not all(isinstance(arg, str) and arg for arg in argv):
            raise ValueError(f"{name}: argv deve ser lista não vazia; shell implícito não é permitido")
        cwd = within(root, gate.get("cwd", "."))
        if topology == "meta-repository" and cwd != root and not any(cwd.is_relative_to(within(root, member)) for member in members):
            raise ValueError(f"{name}: cwd não pertence a membro declarado")
        ids.add(name)
        normalized.append((name, argv, cwd))
    return normalized


def snapshot(roots):
    result = {}
    for root in roots:
        repo = Path(git(root, "rev-parse", "--show-toplevel")).resolve()
        if git(repo, "status", "--porcelain", "--untracked-files=no"):
            raise ValueError(f"checkout tem alterações rastreadas: {repo}")
        result[repo] = git(repo, "rev-parse", "HEAD")
    return result


def execute(root, gates, pushed=None):
    roots = {root, *(cwd for _, _, cwd in gates)}
    before = snapshot(roots) if pushed is not None else None
    if pushed is not None:
        head = git(root, "rev-parse", "HEAD")
        if any(sha != head for sha in pushed):
            raise ValueError("push contém ref diferente do HEAD validado; valide e envie cada checkout correspondente")
    failed = []
    for name, argv, cwd in gates:
        print(f"gate {name}: {cwd}", flush=True)
        try:
            result = subprocess.run(argv, cwd=cwd, env=environment(), check=False)
            if result.returncode:
                failed.append(f"{name} (exit {result.returncode})")
        except OSError as error:
            print(f"{name}: {error}", file=sys.stderr)
            failed.append(name)
    if before is not None and snapshot(roots) != before:
        raise ValueError("HEAD mudou durante a validação; execute novamente")
    if failed:
        print("gates reprovados: " + ", ".join(failed), file=sys.stderr)
        return 1
    print(f"gates aprovados: {len(gates)}")
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path.cwd())
    parser.add_argument("phase", choices=("check", "pre-push"))
    args = parser.parse_args()
    try:
        root = Path(git(args.root, "rev-parse", "--show-toplevel")).resolve()
        pushed = None
        if args.phase == "pre-push":
            lines = sys.stdin.read().splitlines()
            if not lines:
                raise ValueError("pre-push sem refs no stdin")
            pushed = []
            for line in lines:
                fields = line.split()
                if len(fields) != 4:
                    raise ValueError("ref de pre-push inválida")
                sha = fields[1]
                if sha and set(sha) != {"0"}:
                    pushed.append(sha)
            if not pushed:
                print("deleção de refs: nenhum conteúdo enviado")
                return 0
        gates = configuration(root)
        return execute(root, gates, pushed)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"validation: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
