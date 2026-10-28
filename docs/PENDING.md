# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Origem: [revisão do estado inicial](insights/2026-10-27-revisao-estado-inicial.md). Todos são resolvidos em [E-001](epics/E-001-fundacao-crate-e-dados.md).

| ID | Mecanismo | Medido? | Próximo passo |
|---|---|---|---|
| GC-02 | A DDL de `db_manager.rs` foi removida na F1 e ainda não existe como migração; o banco não tem tabelas | `cargo check` verde, mas nenhuma tabela criada | E-001 F2 |
| GC-03 | `create_active_rule` é `unimplemented!`; a versão anterior abria transação sem `commit()` (rollback no drop) | Por leitura | E-001 F3, com teste negativo |
| GC-04 | Nomes divergentes: `active_rule` × `active_rules`, `id` × `app_id`/`rule_id`, `status` × `current_state`; `process_name` ausente no DDL | Por leitura | E-001 F2 |
| GC-05 | `src/database_schema.rs`: `daily_limit_seconds` usa `0` como "sem limite", contra a arquitetura (nulo); `effective_until_timestamp` obrigatório; falta `time_window_start/end` (o `#[derive]` em campo foi corrigido na F1) | Por leitura | E-001 F2–F3 |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1 concluída; próxima F2 (migrações) |
| [E-002](epics/E-002-log-forense.md) a [E-005](epics/E-005-api-comando-e-pareamento.md) | proposto | Ver [ROADMAP](ROADMAP.md) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| `.harness.toml` e gates locais | Configurar quando houver build (E-001 F4) |
