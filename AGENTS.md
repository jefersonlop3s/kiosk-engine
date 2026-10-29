# Instruções para agentes de IA — kiosk (controle parental doméstico)

> Fonte única das instruções do projeto. `CLAUDE.md` é symlink para este arquivo.
> Método: [sovereign-harness](https://github.com/personal-digital-sovereignty/sovereign-harness) (plugin `sovereign-harness-guard`, habilitado em `.claude/settings.json`).

## Leitura obrigatória

- Organização documental: [docs/README.md](docs/README.md).
- Arquitetura: [docs/architecture/README.md](docs/architecture/README.md) e as ADRs que a revisam.
- Pendências: [docs/PENDING.md](docs/PENDING.md) — somente trabalho aberto.
- Direção e sequência: [docs/ROADMAP.md](docs/ROADMAP.md).
- Histórico: [docs/CONCLUSION.md](docs/CONCLUSION.md).

## Perfil do projeto

- **Produto**: controle parental para uso doméstico. Pais e responsáveis definem quais programas cada criança/adolescente pode usar, por quanto tempo e em quais horários. Um app mobile de administração virá depois ([E-006](docs/epics/E-006-app-mobile-administracao.md)).
- **Topologia**: repositório único, público em [github.com/jefersonlop3s/kiosk-engine](https://github.com/jefersonlop3s/kiosk-engine) (remote `origin`). Branch principal: `main` (protegida pelo guard).
- **Stack**: Rust (edition 2024), SQLite via `sqlx`, runtime `tokio`.
- **Alvo**: somente Windows 10/11 ([ADR-002](docs/adr/ADR-002-identidade-por-conta-windows.md)). O desenvolvimento acontece em macOS; aceite de enforcement exige Windows real.
- **Validação**: `cargo check` e `cargo test`. Hoje o manifesto não resolve dependências — ver [PENDING](docs/PENDING.md) (GC-01). Até o fim de [E-001](docs/epics/E-001-fundacao-crate-e-dados.md) F1 não há gate executável.
- **Aprovação**: o dono do projeto (Jeferson Lopes) aceita ADRs, merges e mudanças de configuração.

## Comunicação

- Idioma: **sempre português (pt-BR)**, inclusive documentos, commits e respostas.
- Técnico e direto. Revise a própria sugestão contra o histórico do projeto antes de propor.
- Nunca afirme que leu, compilou ou validou algo sem ter executado a ação nesta sessão.

## Segurança

1. **Fail-loud**: falhou, registra no `Security_Log` e devolve o erro; nunca fallback silencioso.
2. **Credenciais** só por ambiente/cofre; nunca no código, fixture, commit ou log.
3. **Dados de crianças**: coletar o mínimo necessário e mantê-los locais; ver [E-002](docs/epics/E-002-log-forense.md).

## Git

1. Uma branch por trabalho: `claude/<slug>` (ou `<agente>/<slug>`). Nunca commitar em `main`.
2. Nunca `git add -A`/`.`; adicione por caminho e confira `git diff --cached --stat`.
3. Commits atômicos, mensagem em português com o quê e o porquê.
4. Todo trabalho chega à `main` por **Pull Request** no GitHub. O merge é do dono, depois da validação aplicável; o agente não faz merge sem pedido explícito.
5. Repositório público: nada de dados reais de crianças, credenciais ou logs de máquina no Git.

## Documentação viva

- Épico, GAP, insight e ADR seguem [docs/README.md](docs/README.md) (comandos `/doc`, `/gap`, `/adr`).
- GAP de Código: só linha no PENDING. GAP Estrutural: ADR antes de executar.
- Toda entrega fecha em PENDING **e** CONCLUSION no mesmo commit; mudanças entram no [CHANGELOG](CHANGELOG.md).
- Antes de commitar docs: `python3 ~/.claude/plugins/marketplaces/sovereign-harness/scripts/verificar_refs.py --docs docs --raiz .` e `verificar_adr.py --adr-dir docs/adr`.

## Fluxogramas dos comandos

1. **Todo e qualquer comando** de CLI (`kioskd`, `kioskctl` e os que vierem) tem um fluxograma **Mermaid** em [docs/architecture/fluxos-dos-comandos.md](docs/architecture/fluxos-dos-comandos.md), numa seção com o título `` ### `<executável> <comando>` ``.
2. O fluxograma mostra a integração e o fluxo de informação entre cliente e serviço: quem age, que dado passa de uma etapa para a próxima, o que é gravado (tabela e evento do log forense), as recusas e o que volta para quem chamou.
3. Comando novo ou alterado entra no mesmo commit que o seu fluxograma novo ou atualizado. O teste `tests/documentacao.rs` lê os comandos impressos pelos próprios executáveis e reprova o build se algum ficar sem fluxograma (ou se o documento citar comando que não existe).
4. Os diagramas precisam renderizar: valide com o `mermaid-cli` antes de commitar.

<!-- rtk-instructions v2 -->
# Command output

Command output here is condensed to save tokens, keeping every signal and
dropping costly noise. Treat it as the complete result: run commands
normally, and batch related commands into one call to avoid extra turns.
Truncated results state their recovery path in their own output. Re-run a
command as `rtk proxy <cmd>` only when its result is unusable: empty when
output was clearly expected, contradicting its exit code, or garbled.
<!-- /rtk-instructions -->
