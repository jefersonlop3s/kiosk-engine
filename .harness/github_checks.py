#!/usr/bin/env python3
"""Valida a resposta de gh pr checks; erro e ausência de evidência recusam."""
import json
import sys


def validate(exit_code, payload):
    if exit_code != 0:
        raise ValueError(f"gh pr checks terminou com exit {exit_code}")
    checks = json.loads(payload)
    if not isinstance(checks, list) or not checks:
        raise ValueError("nenhum check remoto disponível")
    for check in checks:
        if not isinstance(check, dict) or check.get("bucket") not in ("pass", "skipping"):
            raise ValueError("check reprovado, pendente, cancelado ou resposta desconhecida")
    if not any(check["bucket"] == "pass" for check in checks):
        raise ValueError("todos os checks foram pulados")
    return len(checks)


if __name__ == "__main__":
    try:
        print(f"checks remotos aceitos: {validate(int(sys.argv[1]), sys.stdin.read())}")
    except (ValueError, IndexError) as error:
        print(f"remote: {error}", file=sys.stderr)
        sys.exit(1)
