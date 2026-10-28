# Conclusões

Log cronológico de entregas, decisões, resoluções e descartes, com data, origem, resultado, evidência e commit. A [fila](PENDING.md) mantém só o que continua aberto.

## 2026-10-27 — Estrutura documental sovereign-harness e revisão do estado inicial

- **Origem**: pedido do dono para revisar as sessões anteriores e estruturar os épicos antes de implementar.
- **Resultado**:
  - criada a árvore `docs/` (README, PENDING, ROADMAP, CONCLUSION, architecture, adr, epics, insights, gaps, templates), o [AGENTS.md](../AGENTS.md) do projeto e o [CHANGELOG](../CHANGELOG.md);
  - [CLAUDE.md](../CLAUDE.md) passa a ser symlink para o AGENTS.md;
  - a arquitetura foi movida da raiz ([stub](../ARCHITECTURE.md)) para [architecture/README.md](architecture/README.md). Há outro stub em [20260906133311.md](20260906133311.md), que era cópia idêntica.
- **Decidido pelo dono** (escolha direta, registrada como ADR Proposta):
  - bloqueio por app e por sessão, agente visível ([ADR-001](adr/ADR-001-enforcement-ativo-e-visivel.md));
  - somente Windows, uma conta por criança ([ADR-002](adr/ADR-002-identidade-por-conta-windows.md)).
- **Descartado**:
  - `EPICS_OVERVIEW` (raiz) e `src/data_models.rs`, gerados sem base na arquitetura e nunca versionados;
  - o relatório de hardening colado na conversa ([insight](insights/2026-10-27-relatorio-hardening-windows.md)).
- **Evidência**: [revisão do estado inicial](insights/2026-10-27-revisao-estado-inicial.md); `cmp ARCHITECTURE.md docs/20260906133311.md` sem diferença; `cargo check` falha em GC-01; `verificar_refs.py` e `verificar_adr.py` sem erros antes do commit.
- **Limitações**: nenhum código foi alterado ou compilado; os GAPs GC-02 a GC-06 foram vistos só por leitura.
- **Commit**: `e7ccaa5` (`docs: estrutura documental sovereign-harness e revisão do estado inicial`), integrado em `main` por fast-forward.

## 2026-10-27 — Aceite das ADR-001 e ADR-002 e commit do trabalho em andamento

- **Origem**: aceite explícito do dono na conversa de 2026-10-27 ("aceito.. commit e mergeie tudo").
- **Resultado**:
  - [ADR-001](adr/ADR-001-enforcement-ativo-e-visivel.md) e [ADR-002](adr/ADR-002-identidade-por-conta-windows.md) passam a **Aceito**; E-001 F2 fica desbloqueada;
  - o trabalho em andamento do dono em `src/` foi commitado como está, num commit próprio. Ele não compila e segue rastreado como GC-07 no [PENDING](PENDING.md).
- **Limitações**: nenhum código foi corrigido; o build continua quebrado até E-001 F1.
- **Commit**: branch `claude/aceite-adrs`, integrada em `main` por fast-forward.

## 2026-10-28 — E-001 F1: manifesto na raiz e `cargo check` verde

- **Origem**: pedido do dono para iniciar a implementação; primeira fase do [E-001](epics/E-001-fundacao-crate-e-dados.md).
- **Resultado**:
  - `Cargo.toml` movido para a raiz, sem pino `=1.9.2`/`=0.4.38`, sem `[[bin]]` falso e sem `[workspace]`; `sqlx` com `sqlite`, `migrate`, `chrono` e `uuid`; `tokio` e `thiserror` adicionados;
  - `src/lib.rs` declara os três módulos;
  - fontes corrigidos só até compilar: `#[derive]` fora de campos, `Clone` nos enums, `SecurityEvent` único (reexportado em `security_logger_impl`), `KioskDbManager::new` com `SqliteConnectOptions` e `foreign_keys(true)`.
- **Resolvido**: GC-01, GC-06 e GC-07; GC-02, GC-03 e GC-05 reduzidos (restante em F2/F3).
- **Evidência**: antes, `cargo check --manifest-path src/Cargo.toml` → `failed to select a version for the requirement uuid = "=1.9.2"`; depois, `cargo check` → `Finished dev profile`; `cargo test` → `0 passed; 0 failed`.
- **Limitações**: não há testes ainda (F3); `create_active_rule` é `unimplemented!` até a F3; validado em Linux, não em Windows.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

