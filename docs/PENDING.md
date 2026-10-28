# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Nenhum aberto. GC-01 a GC-07 (origem: [revisão do estado inicial](insights/2026-10-27-revisao-estado-inicial.md)) foram resolvidos no [E-001](epics/E-001-fundacao-crate-e-dados.md); ver [CONCLUSION](CONCLUSION.md).

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F3 concluídas; F4: `.harness.toml` (CI GitHub já criado) |
| [E-002](epics/E-002-log-forense.md) | em andamento | F1 concluída; F2 (proteção contra UPDATE/DELETE) é a próxima |
| [E-003](epics/E-003-motor-de-regras-e-uso.md) a [E-005](epics/E-005-api-comando-e-pareamento.md) | proposto | Ver [ROADMAP](ROADMAP.md) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| `.harness.toml` e hooks locais | Configurar com o plugin sovereign-harness instalado (E-001 F4); o CI `.github/workflows/ci.yml` já roda fmt, clippy e test |
| Primeira execução do CI (Ubuntu e Windows) | Conferir no GitHub após o push; não foi observada nesta sessão |
