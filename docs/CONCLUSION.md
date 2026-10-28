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

## 2026-10-28 — E-001 F2 e F3: contrato em migração e CRUD transacional testado

- **Origem**: pedido do dono para prosseguir com autonomia; decisão tomada pelo agente: janelas por dia da semana em tabela filha `profile_time_window` (não colunas fixas), por não exigir nova migração se as janelas mudarem.
- **Resultado**:
  - `migrations/0001_contrato_inicial.sql`: `app_metadata`, `child_profile`, `profile_time_window`, `active_rules`, `usage_ledger`, `security_log`; `STRICT`, `CHECK` de estados e de janela, `windows_sid` e `process_name` únicos, FKs; aplicada por `sqlx::migrate!` em `KioskDbManager::new`;
  - `database_schema.rs`: structs alinhadas ao SQL, `daily_limit_seconds`/`effective_until` como `Option`, `ChildProfile`, `TimeWindow`, conversão que falha alto em valor inválido;
  - `db_manager.rs`: `create_app`, `create_profile`, `set_profile_window`, `create_active_rule`, `get_active_rule`, `set_rule_state`, todos com `commit()` explícito;
  - `tests/db_contrato.rs`: 9 testes em SQLite real de arquivo temporário.
- **Controles negativos exercitados**: transação sem `commit()` não persiste; FK para app inexistente recusada; `windows_sid` duplicado recusado; `process_name` duplicado (sem diferenciar caixa) recusado; janela invertida, `weekday` 7 e estado inválido recusados pelo banco; `set_rule_state` de regra inexistente falha.
- **Resolvido**: GC-02 a GC-05.
- **Evidência**: `cargo test` → `9 passed; 0 failed`; `cargo clippy --all-targets -- -D warnings` e `cargo fmt` sem pendências.
- **Limitações**:
  - F4 parcial: `.harness.toml` não criado (o formato depende do plugin, ausente neste container); adicionado o CI `.github/workflows/ci.yml` (fmt, clippy, test em Ubuntu e Windows), ainda não observado rodando; prova negativa do gate pendente;
  - gravação de `Security_Log` e `usage_ledger` não têm API ainda (E-002 F1, E-003);
  - testes só em Linux nesta sessão.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-002 F1: evento gravado na mesma transação da mudança de estado

- **Origem**: pedido do dono para prosseguir ([ROADMAP](ROADMAP.md), sprint 2).
- **Resultado**:
  - `migrations/0002_security_log_event_type.sql`: coluna `event_type` com `CHECK` dos 11 tipos (`SUCCESS`, `AUTH_FAIL`, `AUTHZ_FAIL`, `VALIDATION_FAIL`, `INTERNAL_ERROR`, `STALE_RULE`, `WARNING_SHOWN`, `APP_TERMINATED`, `SESSION_LOCKED`, `AGENT_STOPPED`, `TAMPER_SUSPECTED`);
  - `EventType`, `ActionType` ampliado, `TransactionStatus` e `SecurityEvent::new` em `database_schema.rs`; `insert_event` em `security_logger_impl.rs`, que opera sobre a conexão da transação do chamador;
  - toda operação pública de escrita de `KioskDbManager` (`create_app`, `create_profile`, `set_profile_window`, `create_active_rule`, `set_rule_state`) recebe um `EventContext` (IP) e grava o evento `SUCCESS` antes do `commit()`;
  - em falha, a mudança sofre rollback, um evento `FAILURE`/`INTERNAL_ERROR` com a razão exata é gravado em transação própria e o erro original é devolvido; se o log também falhar, o erro devolvido cita as duas falhas (fail-loud);
  - `record_event` grava eventos sem mudança de estado (autenticação, motor, enforcement).
- **Evidência**: `cargo test` → 9 + 6 testes passando (`tests/db_log_forense.rs`): um evento por operação de escrita, rollback descarta o evento, falha registra a razão e não deixa a regra, tipo desconhecido recusado pelo banco; clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - o evento de falha usa sempre `INTERNAL_ERROR`; `AUTH_FAIL`/`AUTHZ_FAIL`/`VALIDATION_FAIL` serão gravados pelas camadas da API (E-005) via `record_event`;
  - a regra "não existe caminho que altere estado sem evento" vale para a API pública de `KioskDbManager`; `pool()` é público e permite SQL direto (usado nos testes), a restringir quando houver consumidor externo;
  - o log ainda aceita UPDATE/DELETE (E-002 F2); sem retenção nem consulta por período/perfil (F3);
  - `client_ip` é fornecido pelo chamador e não validado.
- **Commit**: branch `claude/hopeful-curie-egiw07`.



## 2026-10-28 — E-001 F4: harness instalado e gates locais ativos

- **Origem**: pedido do dono para instalar o [sovereign-harness](https://github.com/personal-digital-sovereignty/sovereign-harness) e prosseguir.
- **Resultado**:
  - marketplace `sovereign-harness` e plugin `sovereign-harness-guard` 0.7.1 instalados (escopo de projeto);
  - `install.sh --agentes claude --sem-docs --sem-skills`: criados `.harness/`, `.harness.toml` e `.githooks/` (pre-commit e pre-push), sem conflito com a documentação existente; adaptadores de Codex, OpenCode e Antigravity e cópia de skills ficaram de fora por escolha;
  - contrato de validação em `.harness.toml` com 5 gates: `docs-refs`, `docs-adr`, `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked`;
  - `core.hooksPath` apontado para `.githooks` neste clone.
- **Evidência**: `python3 .harness/validation.py check` → `gates aprovados: 5`. Prova negativa: com `src/lib.rs` desformatado de propósito, o mesmo comando → `gates reprovados: fmt (exit 1)`; o arquivo foi restaurado.
- **Limitações**:
  - `hooksPath` não é versionado: cada clone precisa ativá-lo;
  - o guard do agente é heurístico, não é fronteira de segurança (documentação do harness);
  - o pre-push do harness valida o checkout local; o CI do GitHub segue como segunda barreira e não foi observado;
  - o plugin de método (skills) não foi instalado; os comandos `/doc`, `/gap` e `/adr` vêm do plugin guard.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-002 F2: `Security_Log` append-only por trigger

- **Origem**: pedido do dono para prosseguir ([ROADMAP](ROADMAP.md), sprint 2). Decisão do agente: executar só o trigger; o hash encadeado segue sem decisão e exige ADR.
- **Resultado**:
  - `migrations/0003_security_log_append_only.sql`: triggers `BEFORE UPDATE` e `BEFORE DELETE` em `security_log` com `RAISE(ABORT)`;
  - `build.rs` com `rerun-if-changed=migrations`.
- **Achado na execução**: os dois testes novos falharam na primeira rodada (`unwrap_err()` sobre `Ok`, `changes: 1`). Causa: `sqlx::migrate!` embute as migrações na compilação e o cargo não recompilou a lib ao surgir `0003`, então o teste rodou com o esquema antigo e o UPDATE passou. Depois de `touch src/db_manager.rs`, 9 + 9 passaram; o `build.rs` corrige a causa (arquivo novo em `migrations/` → `Compiling kiosk-engine`).
- **Evidência**: `cargo test --locked` → 9 + 9 passando (inclui UPDATE recusado com registro intacto, DELETE recusado, INSERT seguindo funcionando); clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - o trigger não protege contra quem tem acesso ao arquivo (`DROP TRIGGER`, `DROP TABLE`, edição fora do SQLite): sem hash encadeado, adulteração por esse caminho não é detectável;
  - a retenção (F3) não pode usar DELETE enquanto o trigger existir: precisa de caminho próprio, a definir com a política de retenção.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-002 F3: retenção de 90 dias e consulta por período e por perfil

- **Origem**: decisões do dono na conversa de 2026-10-28: (1) hash encadeado **adiado**; (2) retenção de **3 meses** (adotado: 90 dias).
- **Resultado**:
  - `migrations/0004_security_log_retencao_e_perfil.sql`: coluna `profile_id` (sem chave estrangeira, para o log aceitar evento de perfil inexistente), índice `(profile_id, timestamp)` e o trigger de DELETE substituído por `security_log_retention_only_delete`, que só deixa sair evento com mais de 90 dias. A regra vive no banco: SQL direto também é recusado para evento recente;
  - `KioskDbManager::purge_expired_events` remove o vencido e registra a limpeza (`SYSTEM`, com a contagem) na mesma transação; sem nada a remover, não registra;
  - `KioskDbManager::security_events(from, to, profile_id)`: consulta por período e, opcionalmente, por perfil, em ordem cronológica;
  - todo evento de operação leva o `profile_id` (inclusive o de falha); timestamps do log passam a largura fixa em UTC com microssegundos e `Z`, ordenáveis como texto;
  - `SecurityEvent` ganha `profile_id`; `ActionType`, `EventType` e `TransactionStatus` ganham `parse`, que falha alto em valor desconhecido.
- **Evidência**: `cargo test --locked` → 9 + 14 passando, com dados de vários dias: consulta por período e por perfil, retenção com eventos de 100, 91, 89 e 1 dia (saem 2), DELETE de evento de 89 dias recusado por SQL direto e de 91 dias aceito, UPDATE recusado até em evento de 200 dias; clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - o prazo de 90 dias está no SQL (trigger e `purge_expired_events`); mudar exige nova migração, e um teste de fronteira (89 × 91 dias) acusa divergência entre os dois;
  - a limpeza não roda sozinha: alguém (o serviço do E-004) precisa chamar `purge_expired_events` periodicamente;
  - o prazo usa o relógio do sistema (`'now'` do SQLite); a ADR-002 assume conta padrão sem direito de alterar a hora;
  - eventos gravados antes da migração 0004 não têm perfil (não há dados em produção).
- **Encerra o E-002**; o hash encadeado fica fora do épico.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-003 F1–F3: motor de decisão e contabilidade de uso

- **Origem**: pedido do dono para prosseguir ([ROADMAP](ROADMAP.md), sprint 3). As decisões de semântica abaixo foram tomadas pelo agente por serem reversíveis (função pura) e estão no PENDING para confirmação.
- **Resultado**:
  - `src/motor.rs`: `decidir(perfil, app, instante, consumo) → Permitir | Avisar{restante, motivo} | EncerrarApp{motivo} | BloquearSessao{motivo}`, pura (sem SO, banco ou relógio próprio), testável no macOS;
  - semântica: `WHITELISTED` e `IGNORE` isentos de regras de app; `MONITORED` só é restringido por regras `ACTIVE` e vigentes do perfil e do app, e sem regra é permitido; limite 0 bloqueia o app; várias regras no mesmo app valem a mais restritiva (menor limite, dentro de todas as janelas); regra `REVOKED`, `PENDING`, expirada ou de outro app/perfil é ignorada; gravidade `BloquearSessao` > `EncerrarApp` > `Avisar` > `Permitir`, e entre avisos o de menor restante; aviso a partir de 300 s; dia sem janela = sem restrição de horário;
  - `src/uso.rs`: `dividir_por_dia_local` divide o intervalo pelos dias locais de um fuso, em segundos reais (dia de 23 e de 25 horas; meia-noite inexistente);
  - `KioskDbManager::registrar_uso` (UPSERT em `usage_ledger`, uma transação) e `consumo_do_dia` (app e total do perfil);
  - guarda de relógio: intervalo negativo ou acima de 600 s grava `TAMPER_SUSPECTED`, devolve erro e não contabiliza (nunca ajuste silencioso).
- **Evidência**: `cargo test --locked` → 9 + 14 + 14 + 11 passando. O controle negativo do épico varre 08:00–19:53 com consumo 0, 1000 e 3299 s e nunca obtém `EncerrarApp`/`BloquearSessao`; app isento nunca é encerrado em nenhuma hora. Mutação deliberada: trocar `>=` por `>` no limite reprova 5 testes do motor; trocar o dia local pelo dia UTC reprova 4 testes de uso; ambas revertidas. Clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - `registrar_uso` não grava evento por chamada (medição de alta frequência); só anomalias e, no E-004, ações de enforcement geram evento;
  - o relógio e o fuso vêm de quem chama (`chrono::Local` no agente); testado com `chrono-tz` (Nova York e São Paulo), não com `Local` no Windows;
  - GC-08 a GC-10 abertos no PENDING;
  - sem integração ainda: nada lê perfil/regras do banco para montar a decisão.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

