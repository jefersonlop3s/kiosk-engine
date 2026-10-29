#!/usr/bin/env python3
"""Guardrails de agente do harness Sovereign — barra, ANTES de executar, as ações
que já custaram trabalho real num projeto com vários agentes na mesma árvore.

Por que existe
--------------
O enforcement de verdade do método vive no git (pre-commit, pre-push, merge gate):
vale para qualquer agente e para humanos. Mas o git só olha no commit e no push —
tarde demais para `rm` que atravessa symlink, `git checkout --` que descarta
trabalho não commitado, ou a cópia errada do quadro de trabalho. Este guard roda
no momento da ação, dentro do agente (hook PreToolUse do Claude Code e do Codex,
`tool.execute.before` do OpenCode).

Regra de escolha das regras: só entra o que tem incidente medido ou regra vigente
escrita. Gate hostil ligado por padrão é desligado na primeira semana e nunca
reavaliado — por isso cada regra tem um escape NOMEADO, igual ao pre-push.

Uso
---
    harness_guard.py hook              # evento JSON no stdin (Claude Code / Codex)
    harness_guard.py checar --cmd "git add -A" [--cwd DIR]
    harness_guard.py checar --arquivo PATH --acao editar|ler
    harness_guard.py contexto          # texto para o início de sessão
    harness_guard.py regras            # tabela de regras e origem
    harness_guard.py config quadro.canonico   # valor da config efetiva (para scripts)

Saída do `hook`: 0 libera; 2 bloqueia, com o motivo no stderr (o agente lê).
`checar` imprime JSON {"decisao": "liberar"|"bloquear"|"avisar", ...}.

Configuração: `.harness.toml` no diretório atual ou em qualquer ancestral
(ou o caminho em HARNESS_CONFIG). Sem arquivo, valem os padrões do kit.

Somente biblioteca padrão. Python >= 3.11 (tomllib).
"""
from __future__ import annotations

import hashlib
import json
import os
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python < 3.11
    print("harness_guard: Python >= 3.11 é obrigatório (tomllib).", file=sys.stderr)
    sys.exit(1)

VERSAO = "0.7.1"


class ConfigError(ValueError):
    """Uma política solicitada não pode ser substituída por outra em silêncio."""

# --------------------------------------------------------------------------- #
# Configuração
# --------------------------------------------------------------------------- #

PADRAO = {
    "harness": {
        "escape_env": "HARNESS_SKIP",
        "desligadas": [],
        "apenas_avisar": [],
        "bloquear": [],
        "ramos_protegidos": ["main", "master"],
        "merge_gate": "scripts/merge_gate.sh",
        "registro": "",
    },
    "quadro": {"canonico": "", "relativo": ".agents/agents_work.md", "ler": ""},
    "segredos": {"permitidos": [".env.example", ".env.sample", ".env.template"]},
}


def _mesclar(base: dict, extra: dict) -> dict:
    saida = {k: (dict(v) if isinstance(v, dict) else v) for k, v in base.items()}
    for k, v in extra.items():
        if isinstance(v, dict) and isinstance(saida.get(k), dict):
            saida[k] = {**saida[k], **v}
        else:
            saida[k] = v
    return saida


def achar_config(inicio: str) -> Path | None:
    explicito = os.environ.get("HARNESS_CONFIG")
    if explicito:
        p = Path(explicito)
        if not p.is_file():
            raise ConfigError("HARNESS_CONFIG aponta para arquivo inexistente")
        return p
    atual = Path(inicio).resolve()
    for d in (atual, *atual.parents):
        candidato = d / ".harness.toml"
        if candidato.is_file():
            return candidato
    return None


def carregar_config(cwd: str) -> tuple[dict, str]:
    caminho = achar_config(cwd)
    if caminho is None:
        return PADRAO, "padrão do kit"
    try:
        with open(caminho, "rb") as f:
            dados = tomllib.load(f)
    except (OSError, tomllib.TOMLDecodeError) as e:
        raise ConfigError(f"configuração ilegível: {caminho} ({type(e).__name__})") from e
    for section, defaults in PADRAO.items():
        values = dados.get(section, {})
        if not isinstance(values, dict):
            raise ConfigError(f"{section} deve ser tabela")
        for key, value in values.items():
            if key not in defaults:
                raise ConfigError(f"chave desconhecida: {section}.{key}")
            if not isinstance(value, type(defaults[key])) or (isinstance(value, list) and not all(isinstance(item, str) for item in value)):
                raise ConfigError(f"tipo inválido: {section}.{key}")
    ids = {rule.id for rule in REGRAS}
    for key in ("desligadas", "apenas_avisar", "bloquear"):
        if set(dados.get("harness", {}).get(key, [])) - ids:
            raise ConfigError(f"regra desconhecida: harness.{key}")
    escape = dados.get("harness", {}).get("escape_env", PADRAO["harness"]["escape_env"])
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", escape):
        raise ConfigError("harness.escape_env deve ser nome de variável de ambiente")
    return _mesclar(PADRAO, dados), str(caminho)


# --------------------------------------------------------------------------- #
# Git, sem herdar o repositório de quem chamou
# --------------------------------------------------------------------------- #

def _git(cwd: str, *args: str) -> str | None:
    # Lição de 2026-09-08 (hermeticidade): GIT_DIR herdado faz o git operar no
    # repositório errado sem nenhum erro. O guard resolve tudo pelo cwd.
    env = {k: v for k, v in os.environ.items() if k not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE")}
    try:
        r = subprocess.run(["git", "-C", cwd, *args], capture_output=True, text=True, timeout=4, env=env)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


@dataclass
class Repo:
    topo: str | None
    ramo: str | None
    worktree_vinculado: bool
    tem_remote: bool
    em_merge: bool


def info_repo(cwd: str) -> Repo:
    if not os.path.isdir(cwd):
        return Repo(None, None, False, False, False)
    topo = _git(cwd, "rev-parse", "--show-toplevel")
    if topo is None:
        return Repo(None, None, False, False, False)
    ramo = _git(cwd, "branch", "--show-current") or None
    git_dir = _git(cwd, "rev-parse", "--absolute-git-dir")
    comum = _git(cwd, "rev-parse", "--path-format=absolute", "--git-common-dir")
    vinculado = bool(git_dir and comum and os.path.realpath(git_dir) != os.path.realpath(comum))
    remotes = _git(cwd, "remote") or ""
    em_merge = bool(git_dir and os.path.exists(os.path.join(git_dir, "MERGE_HEAD")))
    return Repo(topo, ramo, vinculado, bool(remotes.strip()), em_merge)


# --------------------------------------------------------------------------- #
# Leitura do comando de shell
# --------------------------------------------------------------------------- #

_HEREDOC = re.compile(r"(?<!<)<<(?!<)-?\s*(['\"]?)([A-Za-z_][A-Za-z0-9_]*)\1")


def remover_heredocs(texto: str) -> str:
    """Tira o CORPO dos heredocs: é dado, não comando. Sem isso, um commit que
    descreve a regra (`git add -A` dentro de um heredoc de mensagem) seria barrado."""
    linhas = texto.split("\n")
    saida: list[str] = []
    fim: str | None = None
    for linha in linhas:
        if fim is not None:
            if linha.strip() == fim:
                fim = None
            continue
        saida.append(linha)
        m = _HEREDOC.search(linha)
        if m:
            fim = m.group(2)
    return "\n".join(saida)


def quebras_viram_separador(texto: str) -> str:
    """Nova linha fora de aspas separa comandos, como `;`. Continuação `\\\n` junta."""
    texto = texto.replace("\\\n", " ")
    saida = []
    aspas: str | None = None
    escapado = False
    for c in texto:
        if escapado:
            saida.append(c)
            escapado = False
            continue
        if c == "\\" and aspas != "'":
            escapado = True
            saida.append(c)
            continue
        if aspas:
            if c == aspas:
                aspas = None
        elif c in ("'", '"'):
            aspas = c
        elif c == "\n":
            saida.append(" ; ")
            continue
        saida.append(c)
    return "".join(saida)


SEPARADORES = {"&&", "||", ";", "|", "&", "(", ")", ";;", "|&"}
PREFIXOS_TRANSPARENTES = {"rtk", "command", "nohup", "time", "exec", "builtin", "nice"}
_ATRIB = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*=")


@dataclass
class Segmento:
    argv: list[str]
    env: dict[str, str]
    cwd: str
    redirecoes: list[str] = field(default_factory=list)


def segmentar(comando: str, cwd: str) -> list[Segmento] | None:
    texto = quebras_viram_separador(remover_heredocs(comando))
    try:
        lex = shlex.shlex(texto, posix=True, punctuation_chars=";&|()<>")
        lex.whitespace_split = True
        tokens = list(lex)
    except ValueError:
        return None  # aspas desbalanceadas: quem chamou decide (avisa, não bloqueia)

    brutos: list[list[str]] = [[]]
    for t in tokens:
        if t in SEPARADORES:
            brutos.append([])
        else:
            brutos[-1].append(t)

    segmentos: list[Segmento] = []
    cwd_atual = cwd
    env_persistente: dict[str, str] = {}
    for bruto in brutos:
        if not bruto:
            continue
        env = dict(env_persistente)
        redirecoes: list[str] = []
        argv: list[str] = []
        i = 0
        while i < len(bruto):
            t = bruto[i]
            if t in (">", ">>", "<", ">|", "&>", ">&", "<>") and i + 1 < len(bruto):
                if argv and argv[-1].isdigit():  # `2>&1`: o descritor não é argumento
                    argv.pop()
                if t != "<":
                    redirecoes.append(bruto[i + 1])
                i += 2
                continue
            argv.append(t)
            i += 1
        # atribuições de ambiente na frente (`VAR=x cmd`) e `export VAR=x`
        while argv and _ATRIB.match(argv[0]):
            k, _, v = argv.pop(0).partition("=")
            env[k] = v
        if argv and argv[0] == "export":
            for a in argv[1:]:
                if _ATRIB.match(a):
                    k, _, v = a.partition("=")
                    env_persistente[k] = v
                    env[k] = v
            continue
        if argv and argv[0] == "env":
            argv.pop(0)
            while argv and (_ATRIB.match(argv[0]) or argv[0].startswith("-")):
                a = argv.pop(0)
                if _ATRIB.match(a):
                    k, _, v = a.partition("=")
                    env[k] = v
        while argv and argv[0] in PREFIXOS_TRANSPARENTES:
            argv.pop(0)
            if argv and argv[0] == "proxy":  # `rtk proxy <cmd>`
                argv.pop(0)
        if not argv:
            continue
        if argv[0] == "cd":
            destino = argv[1] if len(argv) > 1 else os.path.expanduser("~")
            cwd_atual = os.path.normpath(os.path.join(cwd_atual, os.path.expanduser(destino)))
            continue
        # `bash -c "..."` / `sh -c` / `zsh -c`: o comando real está no argumento
        if argv[0] in ("bash", "sh", "zsh", "/bin/bash", "/bin/sh", "/usr/bin/bash") and "-c" in argv[1:]:
            idx = argv.index("-c")
            if idx + 1 < len(argv):
                internos = segmentar(argv[idx + 1], cwd_atual)
                if internos:
                    for s in internos:
                        s.env = {**env, **s.env}
                    segmentos.extend(internos)
                continue
        segmentos.append(Segmento(argv, env, cwd_atual, redirecoes))
    return segmentos


@dataclass
class Git:
    sub: str
    args: list[str]
    cwd: str
    config: list[str]


def ler_git(seg: Segmento) -> Git | None:
    if not seg.argv or os.path.basename(seg.argv[0]) != "git":
        return None
    cwd = seg.cwd
    config: list[str] = []
    a = seg.argv[1:]
    while a and a[0].startswith("-"):
        op = a.pop(0)
        if op == "-C" and a:
            cwd = os.path.normpath(os.path.join(cwd, os.path.expanduser(a.pop(0))))
        elif op == "-c" and a:
            config.append(a.pop(0))
        elif op in ("--git-dir", "--work-tree", "--namespace", "--exec-path") and a:
            a.pop(0)
        elif op.startswith("--config-env") and "=" not in op and a:
            a.pop(0)
    if not a:
        return None
    return Git(a[0], a[1:], cwd, config)


# Opções que consomem o token seguinte: o valor delas (uma mensagem "-nada", por
# exemplo) não pode ser lido como flag.
_COM_VALOR = {"-m", "--message", "-F", "--file", "-C", "-c", "-t", "--template", "--author", "--date",
              "--fixup", "--squash", "--trailer", "--cleanup", "-S", "--gpg-sign", "--reuse-message"}


def _curtas(args: list[str], commit: bool = False) -> set[str]:
    """Letras de flags curtas agrupadas (`-am` → {a, m}). Com `commit=True`, pula
    os valores das opções do `git commit` que consomem o token seguinte."""
    letras: set[str] = set()
    pular = False
    for t in args:
        if pular:
            pular = False
            continue
        if t == "--":
            break
        if commit and t in _COM_VALOR:
            pular = True
            continue
        if re.fullmatch(r"-[A-Za-z]+", t):
            letras.update(t[1:])
            if commit and t[-1] in "mFCct":  # `-am "msg"`: a última letra consome o valor
                pular = True
    return letras


# --------------------------------------------------------------------------- #
# Regras
# --------------------------------------------------------------------------- #

@dataclass
class Violacao:
    regra: str
    motivo: str
    correto: str


@dataclass
class Contexto:
    cfg: dict
    segmentos: list[Segmento]
    arquivo: str | None
    acao: str  # "bash" | "editar" | "ler"
    cwd: str
    cache_repo: dict = field(default_factory=dict)

    def repo(self, cwd: str) -> Repo:
        if cwd not in self.cache_repo:
            self.cache_repo[cwd] = info_repo(cwd)
        return self.cache_repo[cwd]


@dataclass
class Regra:
    id: str
    titulo: str
    origem: str
    checar: object  # Callable[[Contexto], list[tuple[Violacao, dict]]]
    severidade: str = "bloquear"  # padrão da regra; a config promove (`bloquear`) ou rebaixa (`apenas_avisar`)


def _gits(ctx: Contexto, sub: str):
    for s in ctx.segmentos:
        g = ler_git(s)
        if g and g.sub == sub:
            yield s, g


def _caminho_amplo(caminho: str, cwd: str) -> bool:
    if caminho in (".", ":/", ":", "*", "./") or any(c in caminho for c in "*?[") or caminho.startswith(":("):
        return True
    pasta = os.path.join(cwd, os.path.expanduser(caminho))
    if not os.path.isdir(pasta):
        return False
    # Diretório inteiramente novo (nada rastreado dentro) só contém o que você criou:
    # equivale a nominal. Com arquivo rastreado dentro, varre mudança de qualquer um.
    rastreados = _git(cwd, "ls-files", "--", caminho)
    return bool(rastreados)


def r_add_tudo(ctx: Contexto):
    out = []
    for s, g in _gits(ctx, "add"):
        caminhos = [a for a in g.args if not a.startswith("-")]
        amplo_por_caminho = any(_caminho_amplo(c, g.cwd) for c in caminhos)
        flag_ampla = bool({"-A", "--all", "--update", "-u"} & set(g.args) or {"A", "u"} & _curtas(g.args))
        # `-A`/`-u` com caminhos NOMINAIS (arquivos) é o que a regra pede; amplo é sem
        # caminho, ou com diretório/glob/`.` — medido no histórico real (2026-10-12).
        if amplo_por_caminho or (flag_ampla and not caminhos):
            out.append((Violacao("add-tudo", "`git add` amplo leva junto o que outra sessão deixou na mesma árvore.",
                                 "Adicione os arquivos pelo nome e confira `git diff --cached --stat` antes do commit."), s.env))
    for s, g in _gits(ctx, "commit"):
        if "--all" in g.args or "a" in _curtas(g.args, commit=True):
            out.append((Violacao("add-tudo", "`git commit -a` faz stage de todo arquivo rastreado modificado, inclusive alheio.",
                                 "Faça stage nominal (`git add <arquivo>`) e commite sem `-a`."), s.env))
    return out


def r_no_verify(ctx: Contexto):
    out = []
    for s in ctx.segmentos:
        g = ler_git(s)
        if not g:
            continue
        pula = "--no-verify" in g.args or (g.sub == "commit" and "n" in _curtas(g.args, commit=True))
        if pula and g.sub in ("commit", "push", "merge", "rebase", "am", "cherry-pick", "revert"):
            out.append((Violacao("no-verify", f"`git {g.sub} --no-verify` desliga os gates em silêncio.",
                                 "Se um gate reprovou, ou o código está errado ou o gate está — corrija um dos dois. "
                                 "Exceção consciente é o escape nomeado do próprio hook, que aparece no terminal."), s.env))
    return out


def _hookspath_desligado(valor: str) -> bool:
    return valor.strip() in ("", "/dev/null", "/dev/null/")


def r_hooks_off(ctx: Contexto):
    out = []
    v = Violacao("hooks-off", "Desligar `core.hooksPath` é um override silencioso: nenhum gate roda e nada fica registrado.",
                 "Hook bloqueou o que deveria passar? O defeito é do hook — corrija o hook. Exceção pontual: escape nomeado.")
    for s in ctx.segmentos:
        g = ler_git(s)
        if not g:
            continue
        for c in g.config:
            k, _, val = c.partition("=")
            if k.lower() == "core.hookspath" and _hookspath_desligado(val):
                out.append((v, s.env))
        if g.sub == "config":
            args = [a for a in g.args if not a.startswith("--") or a == "--unset"]
            low = [a.lower() for a in args]
            if "core.hookspath" in low:
                i = low.index("core.hookspath")
                if "--unset" in low or (i + 1 < len(args) and _hookspath_desligado(args[i + 1])):
                    out.append((v, s.env))
    return out


_SKIP_CI = re.compile(r"\[(skip ci|ci skip|no ci|skip actions|actions skip)\]|skip-checks:\s*true", re.I)


def _mensagens_commit(args: list[str]) -> list[str]:
    msgs = []
    i = 0
    while i < len(args):
        t = args[i]
        if t in ("-m", "--message") and i + 1 < len(args):
            msgs.append(args[i + 1]); i += 2; continue
        if t.startswith("--message="):
            msgs.append(t.split("=", 1)[1])
        elif re.fullmatch(r"-[A-Za-z]*m", t) and i + 1 < len(args):
            msgs.append(args[i + 1]); i += 2; continue
        elif t.startswith("-m") and len(t) > 2:
            msgs.append(t[2:])
        i += 1
    return msgs


def r_skip_ci(ctx: Contexto):
    out = []
    for s, g in _gits(ctx, "commit"):
        if any(_SKIP_CI.search(m) for m in _mensagens_commit(g.args)):
            out.append((Violacao("skip-ci", "`[skip ci]` num commit de PR desliga o único sinal remoto da esteira.",
                                 "Commite sem o marcador. Sem gatilho de push, ele não economiza nada."), s.env))
    return out


def r_pkill_f(ctx: Contexto):
    out = []
    for s in ctx.segmentos:
        if os.path.basename(s.argv[0]) == "pkill" and ("-f" in s.argv or "f" in _curtas(s.argv[1:]) or "--full" in s.argv):
            out.append((Violacao("pkill-f", "`pkill -f` casa a própria linha de comando do shell que o chamou — já derrubou a sessão.",
                                 "Encerre pelo PID (`pidof <executável>` ou `pgrep -x`) e confira o alvo antes."), s.env))
    return out


def r_stash_nu(ctx: Contexto):
    out = []
    for s, g in _gits(ctx, "stash"):
        a = g.args
        sub = a[0] if a and not a[0].startswith("-") else "push"
        resto = a[1:] if a and not a[0].startswith("-") else a
        com_ref = any(re.fullmatch(r"stash@\{\d+\}|[0-9a-f]{7,40}|\d+", t) for t in resto)
        tem_msg = "-m" in resto or "--message" in resto or any(t.startswith("--message=") for t in resto)
        ruim = (
            (sub == "push" and not tem_msg)
            or sub in ("save", "pop", "clear")
            or (sub in ("drop", "apply") and not com_ref)
        )
        if ruim:
            out.append((Violacao("stash-nu", "A pilha de stash é compartilhada entre todos os worktrees e sessões: "
                                 "`git stash`/`pop`/`apply` sem referência pode pegar o trabalho de outra.",
                                 "Prefira commit WIP. Se precisar: `git stash push -u -m <tag-unica>`, guarde o SHA, "
                                 "`git stash apply <sha>` e depois `drop` pela referência achada pela tag."), s.env))
    return out


def r_sudo(ctx: Contexto):
    out = []
    for s in ctx.segmentos:
        if os.path.basename(s.argv[0]) in ("sudo", "doas", "pkexec"):
            out.append((Violacao("sudo", "Agente não roda `sudo`: dependência de sistema é decisão do dono.",
                                 "Reproduza o erro exato, identifique o pacote e entregue o comando pronto para o dono rodar."), s.env))
    return out


_LEITORES = {"cat", "less", "more", "head", "tail", "bat", "batcat", "grep", "egrep", "fgrep", "rg", "sed", "awk",
             "xxd", "od", "hexdump", "strings", "nl", "tac", "cut", "sort", "uniq", "base64", "jq", "diff", "vim",
             "vi", "nano", "view"}


def _e_segredo(caminho: str, cfg: dict) -> bool:
    nome = os.path.basename(caminho.rstrip("/"))
    if nome in cfg["segredos"]["permitidos"]:
        return False
    return bool(re.fullmatch(r"\.env(\..+)?", nome))


def r_segredo(ctx: Contexto):
    out = []
    v = Violacao("segredo-env", "Conteúdo de `.env` não pode chegar ao stdout, ao log da sessão nem ao contexto do agente.",
                 "Confira a PRESENÇA de uma variável sem imprimir valor (`grep -c '^NOME=' .env`), liste só os nomes "
                 "(`cut -d= -f1 .env`) ou carregue-a no processo que precisa dela.")
    if ctx.acao == "ler" and ctx.arquivo and _e_segredo(ctx.arquivo, ctx.cfg):
        out.append((v, {}))
    for s in ctx.segmentos:
        cmd = os.path.basename(s.argv[0])
        if cmd not in _LEITORES:
            continue
        if cmd == "cut" and ({"-d=", "--delimiter=="} & set(s.argv) or "-d" in s.argv and "=" in s.argv) \
                and ({"-f1", "--fields=1"} & set(s.argv) or "-f" in s.argv and "1" in s.argv):
            continue  # só os nomes das variáveis, nunca os valores
        if cmd in ("grep", "egrep", "fgrep", "rg") and ({"q", "c", "l", "L"} & _curtas(s.argv[1:])
                                                     or {"--quiet", "--count", "--files-with-matches"} & set(s.argv)):
            continue
        if any(_e_segredo(t, ctx.cfg) for t in s.argv[1:] if not t.startswith("-")):
            out.append((v, s.env))
    return out


def r_commit_protegida(ctx: Contexto):
    out = []
    protegidos = set(ctx.cfg["harness"]["ramos_protegidos"])
    for s, g in _gits(ctx, "commit"):
        if "--dry-run" in g.args:
            continue
        r = ctx.repo(g.cwd)
        if r.ramo in protegidos and not r.em_merge:
            out.append((Violacao("commit-protegida",
                                 f"Commit direto em `{r.ramo}` ({r.topo}). Outra sessão pode ter movido o HEAD do diretório "
                                 "desde que você criou sua branch — foi assim que um commit caiu em `main` em 2026-09-07.",
                                 "Confirme `git branch --show-current` e commite na sua branch `<agente>/<slug>`."), s.env))
    return out


def r_worktree_main_local(ctx: Contexto):
    out = []
    protegidos = set(ctx.cfg["harness"]["ramos_protegidos"])
    for s, g in _gits(ctx, "worktree"):
        if not g.args or g.args[0] != "add":
            continue
        pos: list[str] = []
        a = g.args[1:]
        i = 0
        while i < len(a):
            t = a[i]
            if t in ("-b", "-B", "--reason") and i + 1 < len(a):
                i += 2; continue
            if t.startswith("-"):
                i += 1; continue
            pos.append(t); i += 1
        base = pos[1] if len(pos) > 1 else None
        r = ctx.repo(g.cwd)
        # Sem remote, o `main` local É a verdade (não existe `origin/main` para derivar).
        if not r.tem_remote:
            continue
        if (base in protegidos) or (base is None and r.ramo in protegidos):
            out.append((Violacao("worktree-main-local",
                                 "Worktree derivado do `main` LOCAL herda o atraso dele: o `main` local de um diretório que "
                                 "está na branch de outra sessão não é atualizado.",
                                 "`git fetch origin && git worktree add -b <agente>/<slug> <caminho> origin/main`"), s.env))
    return out


def _raiz_logica(cwd: str, topo: str) -> str:
    """Raiz do repo no mesmo plano (lógico) do cwd: sobe procurando `.git`.
    O `git rev-parse --show-toplevel` devolve o caminho resolvido (getcwd do
    kernel); o chamador passa o caminho lógico ($PWD, ~, /tmp). Comparar os
    dois planos nunca casa — no macOS (/tmp e /var sob /private) isso virava
    falso LIBERAR na rm-symlink: o `rm` digitado "dentro" nunca casava com o
    topo resolvido. Fallback: topo como veio."""
    atual = cwd if os.path.isabs(cwd) else os.path.abspath(cwd)
    while True:
        if os.path.lexists(os.path.join(atual, ".git")):
            return atual
        pai = os.path.dirname(atual)
        if pai == atual:
            return topo
        atual = pai


def _sai_da_arvore(alvo: str, cwd: str, topo: str) -> bool:
    textual = os.path.normpath(os.path.join(cwd, os.path.expanduser(alvo)))
    raiz = _raiz_logica(cwd, topo)
    if not (textual == raiz or textual.startswith(raiz.rstrip("/") + "/")):
        return False  # caminho já é de fora textualmente: não é travessia de symlink
    topo_real = os.path.realpath(topo)
    pai_real = os.path.realpath(os.path.dirname(textual))
    return not (pai_real == topo_real or pai_real.startswith(topo_real.rstrip("/") + "/"))


def r_rm_symlink(ctx: Contexto):
    out = []
    for s in ctx.segmentos:
        if os.path.basename(s.argv[0]) not in ("rm", "mv", "unlink", "rmdir"):
            continue
        r = ctx.repo(s.cwd)
        if not r.topo:
            continue
        alvos = [t for t in s.argv[1:] if not t.startswith("-")]
        if os.path.basename(s.argv[0]) == "mv" and alvos:
            alvos = alvos[:-1]
        fora = [t for t in alvos if _sai_da_arvore(t, s.cwd, r.topo)]
        if fora:
            out.append((Violacao("rm-symlink",
                                 f"`{s.argv[0]}` em {', '.join(fora)}: o caminho atravessa um symlink e o efeito cai FORA da "
                                 "sua árvore (ex.: `node_modules` de worktree aponta para o diretório principal).",
                                 "Resolva antes com `readlink -f`. Para remover só o link, aponte para o link, não para dentro dele."),
                        s.env))
    return out


_NPM_SUBS = {"install", "i", "ci", "update", "up", "upgrade", "add", "remove", "uninstall", "rm", "un", "link", "dedupe", "prune"}


def r_npm_worktree(ctx: Contexto):
    out = []
    for s in ctx.segmentos:
        if os.path.basename(s.argv[0]) not in ("npm", "pnpm", "yarn", "bun") or len(s.argv) < 2:
            continue
        if s.argv[1] not in _NPM_SUBS:
            continue
        pasta = s.cwd
        if "--prefix" in s.argv:
            i = s.argv.index("--prefix")
            if i + 1 < len(s.argv):
                pasta = os.path.normpath(os.path.join(s.cwd, s.argv[i + 1]))
        r = ctx.repo(pasta)
        if r.worktree_vinculado and os.path.islink(os.path.join(pasta, "node_modules")):
            out.append((Violacao("npm-worktree",
                                 f"`{s.argv[0]} {s.argv[1]}` num worktree cujo `node_modules` é symlink: substitui o link por "
                                 "diretório real e quebra a árvore compartilhada (24 erros de tipagem falsos, 2026-09-09).",
                                 "Instale dependências no diretório principal do repositório, nunca no worktree."), s.env))
    return out


def r_merge_direto(ctx: Contexto):
    out = []
    script = ctx.cfg["harness"].get("merge_gate") or ""
    if not script:
        return out
    for s in ctx.segmentos:
        if s.argv[:3] == ["gh", "pr", "merge"]:
            r = ctx.repo(s.cwd)
            if r.topo and os.path.exists(os.path.join(r.topo, script)):
                out.append((Violacao("merge-direto",
                                     "`gh pr merge` direto pula o merge gate — no GitHub Free com repo privado, nada mais recusa "
                                     "merge vermelho.",
                                     f"`{script} <PR>` (valida o RESULTADO do merge e só então chama o gh)."), s.env))
    return out


def _e_copia_do_quadro(caminho: str, cfg: dict, cwd: str) -> bool:
    canonico = cfg["quadro"].get("canonico") or ""
    rel = cfg["quadro"].get("relativo") or ""
    if not canonico or not rel:
        return False
    absoluto = os.path.normpath(os.path.join(cwd, os.path.expanduser(caminho)))
    if not absoluto.endswith("/" + rel.lstrip("/")):
        return False
    return os.path.realpath(absoluto) != os.path.realpath(canonico)


def r_quadro_copia(ctx: Contexto):
    out = []
    v = Violacao("quadro-copia", "Esta é a cópia CONGELADA do quadro de trabalho (worktree/branch): ninguém lê o que se "
                 "escreve aqui, e ela ainda conflita no merge.",
                 "Edite o caminho canônico ({canonico}) e commite pelo helper do quadro.")
    v.correto = v.correto.format(canonico=ctx.cfg["quadro"].get("canonico") or "?")
    if ctx.acao == "editar" and ctx.arquivo and _e_copia_do_quadro(ctx.arquivo, ctx.cfg, ctx.cwd):
        out.append((v, {}))
    for s in ctx.segmentos:
        alvos = list(s.redirecoes)
        base = os.path.basename(s.argv[0])
        if base == "tee":
            alvos += [t for t in s.argv[1:] if not t.startswith("-")]
        if base in ("sed", "perl") and any(t.startswith("-i") or t == "-pi" for t in s.argv[1:]):
            alvos += [t for t in s.argv[1:] if not t.startswith("-")][-1:]
        if base in ("cp", "mv") and len(s.argv) > 2:
            alvos.append(s.argv[-1])
        if any(_e_copia_do_quadro(a, ctx.cfg, s.cwd) for a in alvos):
            out.append((v, s.env))
    return out


def r_gitconfig_global(ctx: Contexto):
    out = []
    v = Violacao("gitconfig-global", "Identidade e credenciais do git são do dono: autoria de commit é sempre dele, e "
                 "`gh auth setup-git` reescreve o `~/.gitconfig`.",
                 "Não altere config global. Falha de chave SSH: diagnostique o ssh-agent antes de qualquer workaround.")
    for s in ctx.segmentos:
        if s.argv[:3] == ["gh", "auth", "setup-git"]:
            out.append((v, s.env))
        g = ler_git(s)
        if g and g.sub == "config" and ({"--global", "--system"} & set(g.args)):
            chaves = [a for a in g.args if not a.startswith("-")]
            if chaves and chaves[0].lower() in ("user.name", "user.email") and len(chaves) > 1:
                out.append((v, s.env))
    return out


def _tem_mudanca_rastreada(cwd: str, caminhos: list[str] | None = None) -> bool:
    alvo = ["--", *caminhos] if caminhos else []
    saida = _git(cwd, "status", "--porcelain", "--untracked-files=no", *alvo)
    return bool(saida)  # None (fora de repo/erro) conta como "nada a perder"


def r_descarte_amplo(ctx: Contexto):
    out = []
    v = Violacao("descarte-amplo", "Descarta trabalho não commitado da árvore inteira, sem confirmação e sem undo.",
                 "Antes: commit WIP (ou `git stash push -u -m <tag>` guardando o SHA). Árvore limpa não é bloqueada.")
    for s in ctx.segmentos:
        g = ler_git(s)
        if not g:
            continue
        if g.sub == "reset" and "--hard" in g.args and _tem_mudanca_rastreada(g.cwd):
            out.append((v, s.env))
        elif g.sub in ("checkout", "restore") and [a for a in g.args if a != "--" and not a.startswith("-")] == ["."] \
                and _tem_mudanca_rastreada(g.cwd):
            out.append((v, s.env))
        elif g.sub == "clean" and ("f" in _curtas(g.args) or "--force" in g.args):
            flags = [a for a in g.args if a.startswith("-") and a not in ("-f", "--force")]
            simulado = _git(g.cwd, "clean", "-n", *[f.replace("f", "") for f in flags if f.replace("f", "") not in ("-", "")])
            if simulado:
                out.append((v, s.env))
    return out


def r_descarte_arquivo(ctx: Contexto):
    out = []
    for s in ctx.segmentos:
        g = ler_git(s)
        if not g:
            continue
        caminhos: list[str] = []
        if g.sub == "checkout" and "--" in g.args[:-1]:
            caminhos = [a for a in g.args[g.args.index("--") + 1:] if a != "."]
        elif g.sub == "restore" and not ({"--staged", "-S"} & set(g.args)):
            caminhos = [a for a in g.args if not a.startswith("-") and a not in (".", "--")]
        if caminhos and _tem_mudanca_rastreada(g.cwd, caminhos):
            out.append((Violacao("descarte-arquivo",
                                 f"`git {g.sub}` vai descartar mudança não commitada em {', '.join(caminhos)} — sem undo. "
                                 "Já apagou uma migração inteira ao \"desfazer um engano\" (2026-09-07).",
                                 "Se é a restauração de um teste de mutação, siga. Se há trabalho seu ali: `cp` ou commit WIP antes."),
                        s.env))
    return out


REGRAS: list[Regra] = [
    Regra("add-tudo", "`git add -A`/`.`/`-u` e `git commit -a`", "AGENTS.md › Coexistência multi-agente, regra 3", r_add_tudo),
    Regra("no-verify", "`--no-verify` em commit/push/merge", "AGENTS.md › CI e hooks: \"não desative o hook\"", r_no_verify),
    Regra("hooks-off", "`core.hooksPath` vazio ou `/dev/null`", "CONCLUSION 2026-09-12: bypass por antecipação num merge", r_hooks_off),
    Regra("skip-ci", "`[skip ci]` em mensagem de commit", "ADR-041 item 8; AGENTS.md › CI regra 5", r_skip_ci),
    Regra("pkill-f", "`pkill -f`", "skill medicao-comportamento-modelo §8; CONCLUSION 2026-10-08/10", r_pkill_f),
    Regra("stash-nu", "`git stash` sem mensagem/referência, `pop`, `clear`", "pilha de stash compartilhada entre worktrees", r_stash_nu),
    Regra("sudo", "`sudo`/`doas`/`pkexec`", "AGENTS.md › Ferramentas de build, regra 5.4", r_sudo),
    Regra("segredo-env", "ler/imprimir `.env`", "skill manuseio-segredos", r_segredo),
    Regra("commit-protegida", "`git commit` em `main`/`master`", "CONCLUSION 2026-09-07: HEAD movido por outra sessão", r_commit_protegida),
    Regra("worktree-main-local", "`git worktree add` a partir do `main` local", "CONCLUSION 2026-09-09, erro #3", r_worktree_main_local),
    Regra("rm-symlink", "`rm`/`mv` que atravessa symlink para fora da árvore", "CONCLUSION 2026-09-09, erro #1", r_rm_symlink),
    Regra("npm-worktree", "`npm install/update` em worktree com `node_modules` symlinkado", "CONCLUSION 2026-09-09, erro #2", r_npm_worktree),
    Regra("merge-direto", "`gh pr merge` sem o merge gate", "ADR-041 item 9; AGENTS.md › CI regra 5", r_merge_direto),
    Regra("quadro-copia", "editar a cópia não canônica do quadro", "CONCLUSION 2026-09-12: incidente do rascunho", r_quadro_copia),
    Regra("gitconfig-global", "`git config --global user.*`, `gh auth setup-git`", "AGENTS.md › Autoria; skill diagnostico-ssh-agent-git", r_gitconfig_global),
    Regra("descarte-amplo", "`reset --hard`/`checkout .`/`clean -f` com algo a perder", "skill vies-de-plausibilidade §7", r_descarte_amplo),
    Regra("descarte-arquivo", "`checkout --`/`restore` de arquivo com mudança não commitada (avisa)", "skill vies-de-plausibilidade §7",
          r_descarte_arquivo, severidade="avisar"),
]


# --------------------------------------------------------------------------- #
# Decisão
# --------------------------------------------------------------------------- #

def _escapes(cfg: dict, env_seg: dict) -> set[str]:
    nome = cfg["harness"]["escape_env"]
    vals = [os.environ.get(nome, ""), env_seg.get(nome, "")]
    # Sem `all`: no guard o escape é sempre pela regra, nominal.
    return {p.strip() for v in vals for p in v.split(",") if p.strip() and p.strip() != "all"}


def decidir(comando: str | None, arquivo: str | None, acao: str, cwd: str) -> dict:
    cfg, origem_cfg = carregar_config(cwd)
    segmentos: list[Segmento] = []
    avisos: list[str] = []
    if comando:
        lidos = segmentar(comando, cwd)
        if lidos is None:
            avisos.append("comando com aspas desbalanceadas: o guard não conseguiu ler; os gates do git continuam valendo.")
        else:
            segmentos = lidos
    ctx = Contexto(cfg, segmentos, arquivo, acao, cwd)
    desligadas = set(cfg["harness"]["desligadas"])
    so_aviso = set(cfg["harness"]["apenas_avisar"])
    promovidas = set(cfg["harness"].get("bloquear") or [])
    bloqueios, escapados = [], []
    for regra in REGRAS:
        if regra.id in desligadas:
            continue
        for violacao, env_seg in regra.checar(ctx):
            if violacao.regra in _escapes(cfg, env_seg):
                escapados.append(violacao.regra)
            elif violacao.regra in so_aviso or (regra.severidade == "avisar" and violacao.regra not in promovidas):
                avisos.append(f"[{violacao.regra}] {violacao.motivo} {violacao.correto}")
            else:
                bloqueios.append(violacao)
    decisao = "bloquear" if bloqueios else ("avisar" if avisos else "liberar")
    resultado = {
        "decisao": decisao,
        "bloqueios": [v.__dict__ for v in bloqueios],
        "avisos": avisos,
        "escapados": sorted(set(escapados)),
        "config": origem_cfg,
        "escape_env": cfg["harness"]["escape_env"],
    }
    if bloqueios:
        resultado["mensagem"] = texto_bloqueio(resultado)
    _registrar(cfg, resultado, comando or arquivo or "", acao, cwd)
    return resultado


def _registrar(cfg: dict, res: dict, alvo: str, acao: str, cwd: str) -> None:
    """Medir antes de endurecer: cada bloqueio/escape vira uma linha, sem o comando
    em claro (pode conter valor sensível) — só o hash."""
    if res["decisao"] == "liberar" and not res["escapados"]:
        return
    caminho = cfg["harness"].get("registro") or os.path.join(
        os.environ.get("XDG_STATE_HOME", os.path.expanduser("~/.local/state")), "sovereign-harness", "decisoes.jsonl")
    try:
        os.makedirs(os.path.dirname(caminho), exist_ok=True)
        with open(caminho, "a", encoding="utf-8") as f:
            f.write(json.dumps({
                "ts": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "decisao": res["decisao"],
                "regras": [b["regra"] for b in res["bloqueios"]],
                "escapados": res["escapados"],
                "acao": acao,
                "cwd": cwd,
                "alvo_sha256": hashlib.sha256(alvo.encode()).hexdigest()[:16],
            }, ensure_ascii=False) + "\n")
    except OSError:
        pass  # registro é instrumentação; a decisão já foi tomada e impressa


def texto_bloqueio(res: dict) -> str:
    linhas = ["⛔ harness: ação bloqueada antes de executar."]
    for b in res["bloqueios"]:
        linhas.append(f"  • [{b['regra']}] {b['motivo']}")
        linhas.append(f"    Faça assim: {b['correto']}")
    ids = ",".join(b["regra"] for b in res["bloqueios"])
    linhas.append(f"  Exceção consciente (nomeada, fica registrada): prefixe o comando com {res['escape_env']}={ids}")
    linhas.append("  Antes do escape, decida: o defeito é da ação ou da regra? Se for da regra, corrija a regra.")
    return "\n".join(linhas)


# --------------------------------------------------------------------------- #
# Adaptadores de entrada
# --------------------------------------------------------------------------- #

_EDITORES = {"edit", "write", "multiedit", "notebookedit", "apply_patch", "patch", "str_replace_editor", "create"}
_LEITURA = {"read", "view", "notebookread"}


def _do_patch(texto: str) -> list[str]:
    return re.findall(r"^\*\*\* (?:Update|Add|Delete) File: (.+)$", texto or "", re.M)


def evento_para_alvos(ev: dict) -> list[tuple[str | None, str | None, str]]:
    """Normaliza o JSON de hook (Claude Code / Codex) em (comando, arquivo, ação)."""
    nome = str(ev.get("tool_name") or ev.get("tool") or "").lower()
    entrada = ev.get("tool_input") or ev.get("args") or {}
    if isinstance(entrada, str):
        entrada = {"command": entrada}
    cmd = entrada.get("command") or entrada.get("cmd")
    if isinstance(cmd, list):
        cmd = shlex.join(str(c) for c in cmd)
    arquivo = entrada.get("file_path") or entrada.get("filePath") or entrada.get("notebook_path") or entrada.get("path")
    alvos: list[tuple[str | None, str | None, str]] = []
    if cmd and isinstance(cmd, str):
        alvos.append((cmd, None, "bash"))
    if nome in _EDITORES or "patch" in nome:
        patch = entrada.get("patch") or entrada.get("input") or ""
        for p in _do_patch(patch if isinstance(patch, str) else ""):
            alvos.append((None, p, "editar"))
        if arquivo:
            alvos.append((None, str(arquivo), "editar"))
    elif nome in _LEITURA and arquivo:
        alvos.append((None, str(arquivo), "ler"))
    return alvos


def cmd_hook() -> int:
    bruto = sys.stdin.read()
    try:
        ev = json.loads(bruto) if bruto.strip() else {}
    except json.JSONDecodeError:
        print("harness_guard: evento de hook ilegível — liberado; os gates do git continuam valendo.", file=sys.stderr)
        return 0
    cwd = ev.get("cwd") or os.getcwd()
    for comando, arquivo, acao in evento_para_alvos(ev):
        res = decidir(comando, arquivo, acao, cwd)
        if res["decisao"] == "bloquear":
            print(texto_bloqueio(res), file=sys.stderr)
            return 2
        for a in res["avisos"]:
            print(f"⚠️  harness: {a}", file=sys.stderr)
    return 0


def cmd_contexto() -> int:
    cwd = os.getcwd()
    cfg, origem = carregar_config(cwd)
    r = info_repo(cwd)
    linhas = [f"Harness Sovereign {VERSAO} ativo (config: {origem})."]
    if r.topo:
        tipo = "worktree vinculado" if r.worktree_vinculado else "checkout principal"
        linhas.append(f"Repositório: {r.topo} · branch `{r.ramo or 'HEAD destacado'}` · {tipo}.")
    ler = cfg["quadro"].get("ler")
    if ler:
        try:
            q = subprocess.run(shlex.split(ler), capture_output=True, text=True, timeout=5)
            emandamento = re.search(r"^## [^\n]*Em andamento[^\n]*\n(.*?)(?=^## )", q.stdout, re.S | re.M)
            titulos = re.findall(r"^### (.+)$", emandamento.group(1), re.M) if emandamento else []
            linhas.append(f"Quadro de trabalho: {len(titulos)} entrada(s) em andamento. Leia o quadro inteiro antes de "
                          "começar e registre a sua (paths sob edição) — comando: " + ler)
            for t in titulos[:8]:
                linhas.append(f"  · {t[:140]}")
        except (OSError, subprocess.TimeoutExpired, ValueError):
            linhas.append("Quadro de trabalho configurado, mas não foi possível lê-lo agora: " + ler)
    linhas.append("Guardrails no agente: " + ", ".join(rg.id for rg in REGRAS if rg.id not in cfg["harness"]["desligadas"])
                  + f". Escape nominal: {cfg['harness']['escape_env']}=<regra>.")
    print("\n".join(linhas))
    return 0


def cmd_regras() -> int:
    for rg in REGRAS:
        print(f"{rg.id:<20} {rg.titulo}\n{'':<20} origem: {rg.origem}")
    return 0


def main(argv: list[str]) -> int:
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    if argv[0] == "--version":
        print(VERSAO)
        return 0
    if argv[0] == "hook":
        return cmd_hook()
    if argv[0] == "contexto":
        return cmd_contexto()
    if argv[0] == "regras":
        return cmd_regras()
    if argv[0] == "config" and len(argv) == 2:
        cfg, _ = carregar_config(os.getcwd())
        valor: object = cfg
        for parte in argv[1].split("."):
            if not isinstance(valor, dict) or parte not in valor:
                print(f"chave inexistente: {argv[1]}", file=sys.stderr)
                return 1
            valor = valor[parte]
        print(" ".join(map(str, valor)) if isinstance(valor, list) else valor)
        return 0
    if argv[0] == "checar":
        opts = {"--cmd": None, "--arquivo": None, "--acao": None, "--cwd": os.getcwd()}
        i = 1
        while i < len(argv):
            if argv[i] in opts and i + 1 < len(argv):
                opts[argv[i]] = argv[i + 1]
                i += 2
            else:
                print(f"argumento inesperado: {argv[i]}", file=sys.stderr)
                return 1
        acao = opts["--acao"] or ("bash" if opts["--cmd"] else "editar")
        res = decidir(opts["--cmd"], opts["--arquivo"], acao, opts["--cwd"])
        print(json.dumps(res, ensure_ascii=False))
        return 0
    print(f"subcomando desconhecido: {argv[0]}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except ConfigError as error:
        print(f"harness_guard: {error}; ação recusada, corrija a política.", file=sys.stderr)
        sys.exit(2)
