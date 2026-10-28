# E-001 — Fundação do crate e contrato de dados

Estado: em andamento (F1–F4 concluídas em 2026-10-28; CI verde em Ubuntu e Windows). Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Problema**: o projeto não compila. O manifesto falha antes de chegar ao código (GC-01) e os fontes têm erros de sintaxe e de contrato (GC-02 a GC-07 no [PENDING](../PENDING.md)).

**Resultado observável**: `cargo check` e `cargo test` passam na raiz; um banco SQLite novo é criado por migrações versionadas com o contrato completo; operações de escrita são transacionais e testadas contra SQLite real.

- **Inclui**: manifesto na raiz, `lib.rs`, migrações, structs alinhadas ao SQL, repositório CRUD, testes de integração e gates locais (`.harness.toml`).
- **Exclui**: lógica de decisão ([E-003](E-003-motor-de-regras-e-uso.md)), API ([E-005](E-005-api-comando-e-pareamento.md)) e qualquer código que dependa do Windows ([E-004](E-004-agente-windows-enforcement.md)).
- **Pressupostos**: SQLite local num único arquivo, um processo escritor (o serviço).

## Origem e decisões

- [Arquitetura](../architecture/README.md), Seção II: tabelas `App_Metadata`, `Active_Rules`, `Security_Log`.
- [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) (Aceito): `child_profile`, `usage_ledger`, `profile_id` e `time_window_end` em `Active_Rules`.
- [Revisão do estado inicial](../insights/2026-10-27-revisao-estado-inicial.md).

## Fases e dependências

| Fase | Entrega | Depende de | Evidência exigida |
|---|---|---|---|
| F1 | Manifesto na raiz com dependências resolvíveis (`sqlx` com feature `sqlite`), `src/lib.rs` declarando os módulos, binário falso removido. Absorve o trabalho em andamento commitado em `src/` (GC-07) | — | `cargo check` verde (saída colada) |
| F2 | Migrações `sqlx` com nomes únicos de tabela e coluna (fim de `active_rule`/`active_rules`/`app_id`/`id`); `daily_limit_seconds` e `effective_until` como `Option`; `time_window_start/end`; tabelas da ADR-002 | F1, ADR-002 (aceita) | Banco criado do zero; `PRAGMA foreign_keys` ativo |
| F3 | Structs do `database_schema.rs` corrigidas (sem `derive` em campo, sem `SecurityEvent` duplicado) e repositório CRUD com transação e `commit()` explícito | F2 | Testes de integração |
| F4 | `.harness.toml` com gates `cargo fmt --check`, `cargo clippy -D warnings` e `cargo test`; hooks ativados | F3 | Gate reprova num commit propositalmente quebrado (prova negativa) |

## Aceites

- Os testes rodam contra SQLite real em arquivo temporário, sem mock de banco.
- **Controles negativos**: transação sem `commit()` não persiste; FK para `app_id` inexistente é recusada; `windows_sid` duplicado é recusado.
- Sem superfície linguística (não há texto para o usuário).

## Encerramento

A preencher: evidências, commits e entrada no [CONCLUSION](../CONCLUSION.md).
