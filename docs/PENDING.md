# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| [ADR-001](adr/ADR-001-enforcement-ativo-e-visivel.md) | Proposto | Revisar e aceitar ou rejeitar | dono |
| [ADR-002](adr/ADR-002-identidade-por-conta-windows.md) | Proposto | Revisar e aceitar ou rejeitar; bloqueia E-001 F2 | dono |
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Origem: [revisão do estado inicial](insights/2026-10-27-revisao-estado-inicial.md). Todos são resolvidos em [E-001](epics/E-001-fundacao-crate-e-dados.md).

| ID | Mecanismo | Medido? | Próximo passo |
|---|---|---|---|
| GC-01 | `src/Cargo.toml`: `uuid = "=1.9.2"` não resolve no crates.io; manifesto dentro de `src/`; `[[bin]]` aponta para módulo sem `main`; `sqlx` sem feature `sqlite` | Sim: `cargo check --manifest-path src/Cargo.toml` → `failed to select a version for the requirement uuid = "=1.9.2"` | E-001 F1 |
| GC-02 | `src/db_manager.rs`: raw strings abertas com `r#"` e fechadas com `"`; DDL nunca executada; `Url` não importado; `SqlitePool::connect_url` e `sqlx::Transaction<'_>` sem tipo de banco | Por leitura (não compila por GC-01) | E-001 F1–F3 |
| GC-03 | `create_active_rule` abre transação e não chama `commit()`: o drop faz rollback, então nada persiste | Por leitura | E-001 F3, com teste negativo |
| GC-04 | Nomes divergentes: `active_rule` × `active_rules`, `id` × `app_id`/`rule_id`, `status` × `current_state`; `process_name` ausente no DDL | Por leitura | E-001 F2 |
| GC-05 | `src/database_schema.rs`: `#[derive]` sobre campos (erro de compilação); `daily_limit_seconds` usa `0` como "sem limite", contra a arquitetura (nulo); `effective_until_timestamp` obrigatório; falta `time_window_start` | Por leitura | E-001 F2–F3 |
| GC-06 | `SecurityEvent` definido em `database_schema.rs` e de novo em `security_logger_impl.rs` com campos privados | Por leitura | E-001 F3 |
| GC-07 | Trabalho não commitado do dono em `src/db_manager.rs`, `src/security_logger_impl.rs` e `src/Cargo.toml` | `git status` | Absorver em E-001 F1 |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | proposto | F1: manifesto na raiz e `cargo check` verde |
| [E-002](epics/E-002-log-forense.md) a [E-005](epics/E-005-api-comando-e-pareamento.md) | proposto | Ver [ROADMAP](ROADMAP.md) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| `.harness.toml` e gates locais | Configurar quando houver build (E-001 F4) |
