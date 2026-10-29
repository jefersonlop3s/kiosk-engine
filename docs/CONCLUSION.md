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
- **Commit**: `cde74c3` (src) e `f1f48d4` (ADRs), integrados em `main` por fast-forward.

## 2026-10-27 — Publicação no GitHub e fluxo por Pull Request

- **Origem**: decisão do dono de continuar via GitHub, com repositório público `jefersonlop3s/kiosk-engine`.
- **Resultado**:
  - repositório criado e `main` enviada (`f1f48d4`), depois de varrer o histórico atrás de segredos (nada encontrado);
  - o [AGENTS.md](../AGENTS.md) passa a exigir PR com merge feito pelo dono e proíbe dados reais de crianças no Git.
- **Limitações** (na data): não havia CI nem gates locais. Vieram depois: CI em Ubuntu e Windows e harness com 5 gates (entradas de 2026-10-28).
- **Commit**: jefersonlop3s/kiosk-engine#1 (branch `claude/fluxo-github`), mesclado em 2026-10-29 a pedido do dono, com a `main` trazida para o branch por merge e conflitos resolvidos (a linha do PENDING sobre CI ficou obsoleta e saiu).

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

## 2026-10-28 — ADR-003 a ADR-006 propostas (agente Windows e API de comando)

- **Origem**: pedido do dono para redigir as ADRs que bloqueiam E-004 e E-005.
- **Resultado** (todas **Proposto**; o aceite é do dono):
  - [ADR-003](adr/ADR-003-deteccao-de-processos-e-tempo-de-uso.md): polling de 2 s com SID do dono do processo; tempo do app = processo existente, tempo total = sessão ativa (resolve o GC-09);
  - [ADR-004](adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md): named pipe por sessão com DACL, protocolo JSON versionado, UI só recebe e envia batimento, o serviço inicia a UI;
  - [ADR-005](adr/ADR-005-rede-da-api-de-comando.md): sem escuta pública; Tailscale por padrão, LAN opcional com HTTPS e certificado fixado;
  - [ADR-006](adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md): Ed25519 por dispositivo, `timestamp` ±60 s e `nonce`, pareamento presencial por código de uso único.
- **Evidência**: `verificar_adr.py` → `adr em dia`; `verificar_refs.py` → `refs em dia`; épicos E-004 e E-005 apontam para as ADRs.
- **Limitações**: nenhuma alternativa foi exercitada em Windows real; as recomendações são análise, não medição. Cada ADR traz a validação exigida.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-004 (parte SO-independente): núcleo do agente e ligação banco → motor

- **Origem**: pedido do dono para implementar o que fosse possível sem o aceite das ADR-003 a ADR-006. Critério de escolha: só o que sobrevive à rejeição das ADRs sem retrabalho relevante.
- **Resultado**:
  - `KioskDbManager::{perfil_por_sid, app_por_processo, regras_do_app, perfil_ctx, decidir_para, decidir_app}` (fecha GC-10): monta o contexto do motor a partir do banco; `decidir_app` isola as regras do app para que um aviso de sessão não esconda o de app;
  - `src/agente.rs`: `Agente::tick` com a interface `Sistema`. A cada ciclo: agrupa processos por perfil (SID) e app (nome, sem caixa); contabiliza o intervalo desde o tick anterior; decide a sessão e cada app; avisa uma vez por marco (300 s e 60 s) e volta a valer quando a restrição some; encerra apps (tolerância de 5 ticks, desiste após 3 falhas com `TAMPER_SUSPECTED`); bloqueia a sessão com intervalo de 30 s; grava `WARNING_SHOWN`, `APP_TERMINATED`, `SESSION_LOCKED` e as falhas;
  - erros isolados vão em `Relatorio.erros` e não interrompem o enforcement do resto (inclusive após salto de relógio).
- **Evidência**: `cargo test --locked` → 12 (agente) + 9 + 14 + 14 + 11 passando, contra SQLite real e `Sistema` falso; clippy `-D warnings` e fmt limpos. Mutações deliberadas: desligar a deduplicação de aviso reprova 2 testes; desligar a tolerância pós-encerramento reprova 1; ambas revertidas.
- **Decisão do agente**: sessão do perfil é avaliada mesmo sem app controlado em execução (a criança com limite total esgotado é bloqueada abrindo só o Explorer).
- **Limitações**:
  - nada disso foi executado em Windows; o adaptador `Sistema` real, o pipe da UI e a API ficam para depois do aceite das ADRs;
  - GC-11 (sessão "viva" inferida por processos do SID; total ainda é a soma dos apps) e GC-12 (lock pode ser desfeito com a senha da criança) abertos no PENDING; GC-08 e GC-09 seguem abertos;
  - o `ctx` dos eventos do agente é `local` (sem IP).
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Aceite das ADR-003 e ADR-006

- **Origem**: aceite explícito do dono na conversa de 2026-10-28 ("ADRs 3 e 6 aceita, prossiga").
- **Resultado**: [ADR-003](adr/ADR-003-deteccao-de-processos-e-tempo-de-uso.md) (detecção por polling; tempo do app = processo existente; total = sessão ativa) e [ADR-006](adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md) (Ed25519, anti-replay, pareamento presencial) passam a **Aceito**. [ADR-004](adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md) e [ADR-005](adr/ADR-005-rede-da-api-de-comando.md) seguem **Proposto**.
- **Evidência**: `verificar_adr.py` e `verificar_refs.py` sem erros antes do commit.
- **Limitações**: nenhum código novo neste registro; a implementação das duas ADRs vem em commits próprios.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Aceite das ADR-004 e ADR-005 e confirmação de semânticas do motor

- **Origem**: respostas do dono às perguntas da sessão em 2026-10-28.
- **Decidido**:
  - [ADR-004](adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md) e [ADR-005](adr/ADR-005-rede-da-api-de-comando.md) passam a **Aceito** (as quatro ADRs do agente e da API estão aceitas);
  - motor: app `MONITORED` sem regra vigente é **permitido** e dia sem janela cadastrada é **sem restrição de horário** (confirmados; deixam de ser pendência);
  - sessão (GC-12): **lock e, se a criança desbloquear 2 vezes, logoff**.
- **Evidência**: `verificar_adr.py` e `verificar_refs.py` sem erros antes do commit.
- **Limitações**: sem código neste registro; a política de logoff, o tempo de sessão (ADR-003), o protocolo da UI (ADR-004), a verificação de assinatura (ADR-006) e o servidor (ADR-005) vêm em commits próprios.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Tempo de sessão (ADR-003) e política de logoff (GC-12)

- **Origem**: aceite da ADR-003 e resposta do dono sobre lock × logoff em 2026-10-28.
- **Resultado**:
  - `migrations/0005_session_usage.sql` e `KioskDbManager::registrar_sessao`; `consumo_do_dia` passa a devolver o total do perfil pelo tempo de sessão ativa, não pela soma dos apps (fecha GC-09 e GC-11); a guarda de relógio (`TAMPER_SUSPECTED`) é a mesma de `registrar_uso`;
  - `Sistema` ganha `sessoes_ativas` e `encerrar_sessao`; o agente só contabiliza e só age sobre a sessão quando ela está ativa (uma sessão bloqueada não é rebloqueada), e o tempo de sessão exige sessão ativa no tick anterior e no atual;
  - lock e, depois que a criança desbloqueia `MAX_BLOQUEIOS_ANTES_DO_LOGOFF` (2) vezes com a restrição valendo, logoff (`Acao::EncerrouSessao`, evento `SESSION_LOCKED` com detalhe); a contagem zera quando a restrição acaba (fecha GC-12);
  - uma sessão ativa sem nenhum processo visto também é contabilizada e avaliada.
- **Evidência**: `cargo test --locked` → 17 (agente) + 9 + 14 + 14 + 12 passando, contra SQLite real. Mutações deliberadas: logoff nunca (limite 99) reprova 2 testes; não zerar a contagem reprova 1; agir em sessão não ativa reprova 1. Na primeira rodada duas mutações sobreviveram (testes fracos); reforcei os testes e as quatro agora são pegas. Clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - o logoff perde trabalho não salvo (mitigado pelos avisos de 5 e 1 min); o adaptador Windows real de `sessoes_ativas`/`encerrar_sessao` (`WTS*`) não existe ainda;
  - suspensão/hibernação da máquina gera um intervalo maior que 600 s e registra `TAMPER_SUSPECTED` mesmo sem má-fé; o ruído é conhecido;
  - GC-08 (avisos configuráveis por regra) segue aberto.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-005 F2: assinatura de requisições e pareamento (ADR-006)

- **Origem**: aceite da ADR-006 e pedido do dono para prosseguir.
- **Resultado**:
  - `src/assinatura.rs`: string canônica `KIOSK-V1` (método, caminho, `timestamp`, `nonce`, SHA-256 do corpo), assinatura e verificação Ed25519 estrita, nonce de 16 bytes, código de pareamento de 8 caracteres sem símbolos ambíguos (~40 bits);
  - `src/pareamento.rs` e `migrations/0006_dispositivos_e_pareamento.sql`: `autenticar` (formato, janela de ±60 s, dispositivo ativo, assinatura, nonce único; o nonce só é gravado depois da assinatura válida), `autorizar` (papel `ADMIN`), `criar_codigo_pareamento` (validade de 5 min, só o hash no banco), `parear_dispositivo` (uso único, normaliza caixa/hífen, valida a chave pública, bloqueia após 10 falhas em 5 min), `revogar_dispositivo`;
  - recusas devolvem `Rejeicao` ("requisição recusada" / "falha interna") e gravam o motivo no log; os eventos de recusa usam o relógio da chamada.
- **Achado na execução**: a contagem de falhas de pareamento comparava o relógio injetado com a hora real dos eventos, então o bloqueio dependia dos dois coincidirem; os eventos de recusa passaram a usar o relógio da chamada (o teste de recuperação após o bloqueio expôs isso).
- **Evidência**: `cargo test --locked` → 15 (assinatura) + 17 + 9 + 14 + 14 + 12 passando, SQLite real e chaves Ed25519 reais. Cinco mutações deliberadas reprovaram os testes e foram revertidas: aceitar qualquer assinatura (3 testes), ignorar nonce repetido (1), janela de 6000 s (2), código reutilizável (1), canônica sem o corpo (2). Clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - não há servidor HTTP: `autenticar` recebe os cabeçalhos como texto; ligação, Camadas 3 a 5 e CLI são E-005 F1/F3;
  - sem segundo fator (já registrado na ADR-006): um dispositivo pareado e desbloqueado na mão da criança comanda tudo;
  - o bloqueio de pareamento por tentativas pode ser usado para atrasar um pareamento novo (GC-13);
  - a chave privada do cliente (arquivo com permissão restrita na CLI) ainda não tem implementação.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Protocolo serviço ↔ UI (ADR-004), parte independente de plataforma

- **Origem**: aceite da ADR-004 e pedido do dono para prosseguir.
- **Resultado**: `src/ipc.rs`:
  - `ParaUi` (estado e aviso, empurrados pelo serviço; sem o SID da sessão) e `DaUi` (batimento e confirmação de aviso), em uma linha JSON com `{"v":1,...}`;
  - `decodificar_da_ui` trata a UI como entrada não confiável: máximo de 4096 bytes, UTF-8, objeto JSON, versão exata, esquema fechado (campo ou tipo desconhecido é recusado) e marco de aviso restrito a 300 e 60;
  - `LimiteDeTaxa` (10 mensagens/s) e `MonitorBatimento` (UI parada após 3 batimentos de 5 s perdidos, alerta uma única vez por parada, recuperação detectada, espera reiniciável após relançar a UI).
- **Achados na execução**: (1) `serde` não combina `flatten` com `deny_unknown_fields`, então a versão e o esquema são conferidos à mão; (2) `deny_unknown_fields` é ignorado em variante unitária de enum com tag interna, e um campo extra em `batimento` passava; `Batimento {}` fecha o esquema.
- **Evidência**: `cargo test --locked` → 8 testes novos em `tests/ipc.rs` e 87 no total, todos passando; três mutações deliberadas (3 → 30 batimentos, sem limite de tamanho, qualquer marco) reprovaram testes e foram revertidas; clippy `-D warnings` e fmt limpos.
- **Limitações**: o named pipe com DACL, a conferência do SID do cliente, o lançamento da UI na sessão (`CreateProcessAsUser`) e a bandeja exigem Windows e não existem; o monitor ainda não está ligado ao agente (o agente não recebe o batimento via `Sistema`).
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-005 F1 e núcleo da F3: servidor da API e ciclo de regras

- **Origem**: aceite das ADR-005 e ADR-006 e pedido do dono para prosseguir.
- **Resultado**:
  - `src/comandos.rs` (Camada 3): `interpretar` valida `add_rule`, `activate_rule` e `revoke_rule` com esquema fechado, `daily_limit_seconds` até 86400, janela `HH:MM` estrita com início anterior ao fim e os dois limites juntos, `effective_until` RFC 3339 no futuro e UUIDs; o motivo exato é devolvido só para o log;
  - `src/api.rs`: `roteador` com `POST /api/windows/{acao}` e as Camadas 0 a 5 em ordem (corpo ≤ 16 KiB → `autenticar` → `autorizar` → validação e existência de perfil/app/regra → operação auditada → `202`); o IP do evento é o da conexão; qualquer recusa responde `403 {"status":"rejected"}` idêntico e falha interna, `500 {"status":"error"}`;
  - política de endereços da ADR-005 em `endereco_permitido`/`vincular`: só loopback e Tailscale; curinga e LAN recusados antes de abrir porta;
  - `existe_perfil`, `existe_app` e `existe_regra` no banco.
- **Evidência**: `cargo test --locked` → 13 testes HTTP reais em `tests/api.rs` (cliente TCP cru contra o serviço em loopback) e 100 no total, todos passando: ciclo criar → ativar → revogar com os eventos e o IP `127.0.0.1`; sem assinatura, replay, corpo adulterado, caminho trocado, timestamp velho, dispositivo revogado, 17 parâmetros inválidos (todos `403` idênticos e `VALIDATION_FAIL` registrado), corpo de 16 KiB + 1 (`413`) com o serviço seguindo de pé, `GET` (`405`) e rota inexistente (`404`). Quatro mutações deliberadas (sem checar referências, caminho fixo, aceitar LAN, IP fixo no log) reprovaram testes e foram revertidas; a primeira tentativa de mutação não compilou e foi refeita. Clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - cobre só `add_rule`, `activate_rule` e `revoke_rule`; faltam `modify_limit`, consultas de uso e log e o CLI dos pais (GC-14);
  - modo LAN com HTTPS não implementado, a API recusa (GC-15);
  - não há `main` que suba o serviço (GC-16);
  - sem TLS: no Tailscale o transporte é cifrado pela VPN (ADR-005); em loopback o tráfego não sai da máquina;
  - testado em Linux; comportamento do firewall do Windows e do Tailscale real não foi exercitado.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-005 F3: serviço executável, pareamento pela API e CLI dos pais

- **Origem**: continuação do pedido do dono; o epic lista o CLI dos pais como parte da F3 e o ciclo só é usável com um executável.
- **Resultado**:
  - `POST /api/pair` (corpo `{codigo, chave_publica, nome}`, esquema fechado): o código de uso único é a credencial; responde `202` com o `device_id` ou o mesmo `403` genérico;
  - `src/cliente.rs`: `post`, `comando`, `parear` (HTTP/1.1 sobre TCP com tempo limite de 10 s) e `Credenciais` (chave privada em arquivo criado já com `0600` no Unix; `Debug` oculta a chave);
  - `kioskd`: `servir` (exige `KIOSK_LISTEN`, valida os endereços antes de abrir qualquer porta), `codigo-pareamento`, `dispositivos`, `revogar <id>`;
  - `kioskctl`: `parear`, `regra criar|ativar|revogar`; erros em pt-BR, recusa do serviço sem detalhe, e não sobrescreve credenciais existentes;
  - [runbook](runbooks/api-e-cli.md) de operação, listado no índice da documentação.
- **Evidência**: `cargo test --locked` → 108 testes passando; `tests/cli.rs` (6) usa os executáveis como processos reais: sobe o `kioskd servir` em loopback, gera o código, pareia com o `kioskctl`, cria → ativa → revoga uma regra (estado conferido no SQLite), revoga o dispositivo e vê o CLI ser recusado sem vazar o identificador; endereços `0.0.0.0`, `[::]`, `192.168.x`, lista com um endereço de LAN e texto inválido recusados na partida; código errado não grava credenciais. Mutações: credenciais com `0644` e chave no `Debug` reprovaram testes e foram revertidas. Clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - o serviço sobe só a API; o agente não está ligado a ele (GC-16);
  - `modify_limit` e consultas de uso e log não existem (GC-14); modo LAN com HTTPS não existe (GC-15);
  - sem TLS: o transporte seguro é o da rede Tailscale (ADR-005);
  - a chave privada fica em arquivo; no Windows a permissão do arquivo depende da ACL da pasta do usuário (não há `0600`);
  - Windows real e Tailscale real não foram exercitados.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — E-005: `modify_limit` e consultas de uso e de log (GC-14)

- **Origem**: GC-14 no [PENDING](PENDING.md); o resultado do E-005 inclui "consulta uso e log pela API".
- **Resultado**:
  - `modify_limit` (`set_rule_limit`, auditado como `MODIFY_LIMIT` com o perfil; recusa regra revogada ou inexistente e limite acima de 86400 s);
  - `usage` (`uso_do_dia`: tempo de sessão ativa e tempo por app de um dia) e `log` (período de até 31 dias, 1 a 500 eventos, filtro por perfil), respondendo `200` com JSON; as consultas exigem as mesmas Camadas 1 a 3 e não geram evento;
  - decisão do agente: na consulta de log o detalhe exato só sai para eventos de enforcement e de operação; o das falhas de API fica só no banco (coerente com o aceite do E-005 de que nenhuma resposta externa vaza `rejection_reason_detail` das falhas);
  - CLI: `kioskctl regra limite`, `kioskctl uso` e `kioskctl log`, com durações legíveis (`1h 30min 00s`); runbook atualizado.
- **Evidência**: `cargo test --locked` → 114 testes passando; 5 testes novos em `tests/api.rs` (limite, regra revogada e 6 parâmetros inválidos, uso, log com filtro/limite/vazamento de detalhe, período e limite inválidos e consulta sem assinatura) e 1 em `tests/cli.rs` (limite, uso e log pelos executáveis reais). Três mutações deliberadas (vazar o detalhe das falhas, permitir alterar regra revogada, sem limite de período) reprovaram testes e foram revertidas. Clippy `-D warnings` e fmt limpos.
- **Limitações**:
  - uma consulta devolve os eventos mais antigos do período até o limite, sem paginação;
  - as consultas dependem do que o agente grava; sem o agente ligado (GC-16) o uso e os eventos de enforcement ficam vazios;
  - o Windows real não foi exercitado.
- **Encerra o E-005** no que não exige Windows; restam GC-15 e GC-16.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — CI do GitHub verde em Ubuntu e Windows

- **Origem**: pendência aberta desde a F4 do E-001; a validação em Windows nunca tinha acontecido.
- **Achado**: o workflow só disparava em push para `main` e em PR, então a branch de trabalho nunca foi validada (`total_count: 0` execuções). O disparo passou a incluir push em qualquer branch e `workflow_dispatch`.
- **Resultado**: execução [40681673250](https://github.com/jefersonlop3s/kiosk-engine/actions/runs/40681673250) sobre o commit `216f8da`: `gates (ubuntu-latest)` e `gates (windows-latest)` com `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings` e `cargo test --locked` em sucesso. Os 114 testes rodaram em **Windows nativo**, inclusive os de SQLite real em arquivo, os HTTP em loopback e os que sobem `kioskd` e `kioskctl` como processos.
- **Limitações**:
  - o que foi validado em Windows é o código portável com o `Sistema` falso; encerrar processos, bloquear sessão, SIDs, WTS e o firewall reais não foram exercitados (adaptador Win32 não existe);
  - os testes de permissão `0600` do arquivo de credenciais são só de Unix (`cfg(unix)`);
  - não li os logs dos passos, só a conclusão por passo.
- **Commit**: `216f8da` (branch `claude/hopeful-curie-egiw07`).

## 2026-10-28 — E-004: adaptador Win32 e agente ligado ao serviço

- **Origem**: pedido do dono para prosseguir com o adaptador Windows (ADR-002, ADR-003 e ADR-004 aceitas).
- **Resultado**:
  - `src/sistema_windows.rs` (`cfg(windows)`, `windows-sys` 0.59): listar processos com o SID do dono, sessões logadas e desbloqueadas, encerrar processo, bloquear sessão (`WTSDisconnectSession`, GC-18), logoff (`WTSLogoffSession`) e aviso (`WTSSendMessageW`, provisório até a bandeja, GC-17); todo `unsafe` com comentário `SAFETY` e handles fechados por `Drop`;
  - `src/textos.rs`: mensagens pt-BR à criança (quanto falta, por quê, o que acontece; sem SID nem códigos internos);
  - `agente::rodar_em_laco` e `kioskd servir` com o agente no Windows (tick de 2 s) e limpeza diária do log; `--sem-agente`; fora do Windows só a API;
  - `tests/windows.rs`: SID do processo igual ao do `whoami /user`, `TerminateProcess` real num `ping`, falha alta para PID inexistente, consulta de sessões e o agente completo encerrando um `PING.EXE` bloqueado sem tocar num `powershell.exe` isento (controle negativo).
- **Evidência local**: `cargo clippy --all-targets --locked --target x86_64-pc-windows-gnu -- -D warnings` limpo (mingw instalado no container); um erro proposital no módulo foi detectado, provando que ele é compilado; `cargo test --locked` em Linux → 119 passando (4 de textos e 1 do laço novos).
- **Evidência em Windows**: registrada na entrada seguinte, após a execução do CI.
- **Limitações**: o `kioskd` ainda não é um serviço do Windows (GC-16); bloqueio, logoff e aviso não podem ser exercitados no CI (sem sessão interativa); a criança pode renomear o `.exe` (risco aceito na ADR-003).
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Adaptador Win32 validado em Windows real (CI)

- **Origem**: evidência pendente da entrada anterior.
- **Resultado**: execução [40686954735](https://github.com/jefersonlop3s/kiosk-engine/actions/runs/40686954735) sobre `1b4a3e1`, job `gates (windows-latest)` (Rust 1.99.1, `x86_64-pc-windows-msvc`): fmt, clippy `-D warnings` e todos os testes em sucesso. O log mostra `Running tests\windows.rs ... running 5 tests` com os 5 passando:
  - `lista_processos_com_o_sid_do_dono_igual_ao_do_whoami`: o SID lido pelo `TokenUser` bate com o do `whoami /user`;
  - `encerra_um_processo_de_verdade`: `TerminateProcess` encerra um `ping` real;
  - `encerrar_processo_inexistente_falha_alto`;
  - `consulta_de_sessoes_responde_sem_erro`;
  - `o_agente_encerra_o_app_bloqueado_e_nao_toca_no_app_isento`: o agente completo, com SQLite real e `SistemaWindows`, encerra o `PING.EXE` bloqueado e deixa vivo o `powershell.exe` isento (controle negativo da ADR-001 em Windows real).
- **Limitações**: o runner não tem sessão interativa: bloqueio (`WTSDisconnectSession`), logoff, aviso (`WTSSendMessageW`) e a detecção de sessão desbloqueada continuam sem validação de comportamento; a validação em máquina doméstica com conta de criança segue necessária (E-004 F2/F3).
- **Commit**: este registro; código em `1b4a3e1`.

## 2026-10-28 — Preparação da versão 0.0.1 e licença GPL-2.0

- **Origem**: pedido do dono: abrir PR, mesclar e lançar a versão 0.0.1; licença escolhida pelo dono: **GPL v2**.
- **Resultado**:
  - `LICENSE` com o texto da GPL v2 da FSF (339 linhas), obtido do repositório do choosealicense.com e conferido contra a lista da SPDX (`GPL-2.0-only`): as diferenças são só as normalizações da SPDX ("Inc." × "Inc.,", marcadores de modelo). `gnu.org` está bloqueado pela política de rede da sessão;
  - `Cargo.toml`: `version = "0.0.1"` (a `0.1.0` anterior nunca foi lançada) e `license = "GPL-2.0-only"`;
  - [CHANGELOG](../CHANGELOG.md): seção `[0.0.1] - 2026-10-28`, marcada como pré-lançamento;
  - `.github/workflows/release.yml`: na tag `vX.Y.Z`, confere a versão do `Cargo.toml`, roda os testes e compila em Windows e Linux, empacota `kioskd`, `kioskctl`, a licença, o CHANGELOG e o runbook, gera `SHA256SUMS.txt` e publica um release marcado como pré-lançamento com as notas da seção do CHANGELOG.
- **Evidência**: `cargo test --locked` → 119 passando; extração das notas e conferência da versão ensaiadas localmente com `GITHUB_REF_NAME=v0.0.1`.
- **Limitações**: o workflow de release ainda não rodou (só roda com a tag ou manualmente a partir de `main`); "GPL v2" foi registrado como `GPL-2.0-only`.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — PR da 0.0.1 mesclado e publicação pelo workflow

- **Origem**: pedido do dono: abrir PR, mesclar e lançar a 0.0.1.
- **Resultado**:
  - [jefersonlop3s/kiosk-engine#2](https://github.com/jefersonlop3s/kiosk-engine/pull/2) mesclado em `main` com commit de merge (`76d8b8f`), preservando os SHAs citados neste registro; CI verde em Ubuntu e Windows nos dois eventos (push e pull_request) do commit `ab0aa0d`;
  - ensaio manual do `release.yml` em `main` ([40696327422](https://github.com/jefersonlop3s/kiosk-engine/actions/runs/40696327422)): testes, compilação e pacotes `pacote-windows-x86_64` (3,5 MB) e `pacote-linux-x86_64` (3,4 MB) gerados, sem publicar;
  - a tag não pôde ser enviada desta sessão: (1) o pré-push do harness recusa tag anotada (GC-19); (2) com tag leve os 5 gates passaram, mas o proxy git da sessão respondeu `403` (a credencial só envia à branch de trabalho);
  - solução: o `release.yml` ganha `workflow_dispatch` com `publicar: true`, que cria a tag `v<versão do Cargo.toml>` no commit escolhido usando o `GITHUB_TOKEN` do repositório e publica o release; recusa se a tag já existir; os pacotes passam a se chamar pela versão, não pela ref.
- **Evidência**: YAML validado localmente; a publicação é registrada na entrada seguinte.
- **Erro do agente registrado**: a primeira tentativa de merge foi feita em paralelo com um `expectedHeadSha` inventado; o GitHub recusou (`409`) e o merge foi refeito com o SHA real.
- **Commit**: branch `claude/hopeful-curie-egiw07` reiniciada a partir de `main`.

## 2026-10-28 — Versão 0.0.1 publicada

- **Origem**: pedido do dono para lançar a 0.0.1.
- **Resultado**: [jefersonlop3s/kiosk-engine#3](https://github.com/jefersonlop3s/kiosk-engine/pull/3) mesclado (`1559c60`); o `release.yml` disparado em `main` com `publicar: true` ([40698247152](https://github.com/jefersonlop3s/kiosk-engine/actions/runs/40698247152)) testou, compilou e publicou o release [v0.0.1](https://github.com/jefersonlop3s/kiosk-engine/releases/tag/v0.0.1), marcado como **pré-lançamento**, com a tag `v0.0.1` em `1559c60` criada pelo workflow. Arquivos:
  - `kiosk-engine-v0.0.1-windows-x86_64.zip` (3 535 404 bytes, `sha256:a0ac9c5b921fc598a8d9737b50061085b2f15506e0339a1ecfc9a05c45adbb3c`);
  - `kiosk-engine-v0.0.1-linux-x86_64.tar.gz` (3 422 628 bytes, `sha256:4e3d0c9850e0afdca986556570264cab272e692c2e25893b4c901fd10bc7fc56`);
  - `SHA256SUMS.txt`.
  As notas do release são a seção `[0.0.1]` do [CHANGELOG](../CHANGELOG.md).
- **Evidência**: execução do workflow com sucesso; release consultado pela API do GitHub (`prerelease: true`, três arquivos com estado `uploaded`).
- **Limitações**: os pacotes não foram baixados nem executados depois da publicação; a tag está no merge do PR #3 (inclui o ajuste do workflow), não no merge do PR #2; continuam valendo as limitações da versão (sem validação interativa de bloqueio, logoff e avisos; `kioskd` ainda não é serviço do Windows).
- **Commit**: este registro (branch `claude/hopeful-curie-egiw07`).

## 2026-10-28 — Primeiro teste do dono no Windows e preparação da 0.0.2

- **Origem**: o dono tentou subir a 0.0.1 no próprio PC seguindo o [runbook](runbooks/api-e-cli.md) e recebeu erro do PowerShell e do cmd.
- **Achados** (erros do agente):
  - o runbook usava sintaxe de bash (`KIOSK_DB=... kioskd servir`), que não existe no PowerShell nem no cmd, e o exemplo de `KIOSK_LISTEN` não mostrava a porta com clareza;
  - não havia nenhum comando nem endpoint para cadastrar o perfil da criança (SID) e os apps; a API só cria regras para perfis e apps existentes, então o serviço subia sem nada a controlar. Os testes cobriam o fluxo porque cadastravam pela biblioteca, não pelo produto.
- **Resultado**:
  - `kioskd perfil criar|listar|limite|janela` e `kioskd app criar|listar`; `--conta` resolve o SID pelo Windows (`sistema_windows::sid_de_usuario`, recusa grupos), `--sid` explícito nos outros sistemas; `KioskDbManager::{listar_perfis, listar_apps, janelas_do_perfil, set_profile_limit}` (este auditado como `MODIFY_LIMIT`);
  - `kioskd` cria a pasta do banco; `KIOSK_LISTEN` sem porta explica o formato;
  - runbook reescrito em PowerShell, com o fluxo completo do zero (variáveis, cadastro, serviço, pareamento, regras); versão 0.0.2 no `Cargo.toml` e no [CHANGELOG](../CHANGELOG.md).
- **Evidência**: `cargo clippy --all-targets --locked -- -D warnings` limpo em Linux e no alvo `x86_64-pc-windows-gnu`; `tests/cli.rs` com 2 testes novos (cadastro e listagem de perfil, janela, limite e apps pelo `kioskd`, erros de SID, dia, janela, duplicidade, perfil inexistente, criação da pasta do banco e a regra criada pelo `kioskctl` com os ids impressos; endereço sem porta). `tests/windows.rs` ganha o teste de `perfil criar --conta` com o SID do `whoami` e a recusa de conta inexistente e de grupo, que roda no CI em Windows.
- **Limitações**: o fluxo no PC do dono ainda não foi repetido com a 0.0.2.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Dado pessoal do dono publicado por engano (IP Tailscale)

- **Origem**: aviso do dono.
- **O que aconteceu** (erro do agente): ao reescrever o runbook a partir de uma captura de tela do dono, o agente copiou o IP Tailscale real do PC dele para o [runbook](runbooks/api-e-cli.md) (3 vezes) e para `tests/cli.rs` (2 vezes), no commit `4d582b3`, mesclado em `main` por jefersonlop3s/kiosk-engine#5 (`53fccd0`). O repositório é público.
- **Contenção**: o workflow de release da 0.0.2, que empacotaria o runbook, foi cancelado durante o build; release e tag `v0.0.2` não existem. O IPv6 da mesma captura não foi copiado.
- **Correção**: o runbook passa a usar `<ip-tailscale>` e o teste, um endereço genérico da faixa Tailscale. O histórico (`4d582b3`, `53fccd0` e a página do PR #5) continua contendo o IP até o dono decidir sobre reescrever o histórico de `main`.
- **Decisão do dono** (2026-10-28): ele trocou o IP da máquina no Tailscale, o que invalida o valor publicado, e pediu que o endereço antigo fique no runbook como exemplo, com a nota de que qualquer coincidência é chute. O teste segue com um endereço genérico.
- **Regra adotada pelo agente**: nada vindo de capturas de tela, logs ou mensagens do dono (IPs, nomes de conta, caminhos pessoais, SIDs) entra no repositório; exemplos usam marcadores ou endereços genéricos.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Versão 0.0.2 publicada

- **Origem**: pedido do dono para lançar a 0.0.2 (cadastro de perfis e apps e runbook em PowerShell); registro feito junto com a mudança seguinte, a pedido do dono.
- **Resultado**: jefersonlop3s/kiosk-engine#5 e jefersonlop3s/kiosk-engine#6 mesclados; o `release.yml` com `publicar: true` publicou o release [v0.0.2](https://github.com/jefersonlop3s/kiosk-engine/releases/tag/v0.0.2) como pré-lançamento, com a tag em `dc543d6` (depois da correção do runbook) e os arquivos:
  - `kiosk-engine-v0.0.2-windows-x86_64.zip` (3 611 049 bytes, `sha256:9beff387c88a34c044c296f7a96edc6d9202c78780ac0aaa950c6f336aa2b9a3`);
  - `kiosk-engine-v0.0.2-linux-x86_64.tar.gz` (3 501 364 bytes, `sha256:4124a7c9efd9af9fafcf1fafecead906c4a43f8f2408540e9e1fbc690d847f7f`);
  - `SHA256SUMS.txt`.
- **Evidência**: release consultado pela API do GitHub (`prerelease: true`, três arquivos `uploaded`). A primeira tentativa de publicação da 0.0.2 foi cancelada no build (incidente do IP, entrada anterior).
- **Limitações**: os pacotes não foram baixados nem executados depois de publicados.

## 2026-10-28 — Runbook: como identificar a conta da criança

- **Origem**: dúvida do dono ao testar (ID do perfil × nome da conta × nome da pasta em `C:\Users`); ele apontou que o [runbook](runbooks/api-e-cli.md) não esclarecia.
- **Resultado**: a seção 3 do runbook passa a explicar os três identificadores (nome da conta, SID, ID do perfil), que o nome da pasta do perfil pode diferir do nome da conta (conta renomeada, conta Microsoft), como descobrir nome e SID com `Get-LocalUser`, o uso de aspas para nomes com espaço, o cadastro por `--sid` e o caso de conta Microsoft (`whoami /user` na conta da criança).
- **Limitações**: o runbook dentro do pacote da 0.0.2 é o anterior; o texto novo chega aos pacotes na próxima versão.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-28 — Segundo teste do dono no Windows e preparação da 0.0.3

- **Origem**: logs do dono com a 0.0.2 (dados pessoais dos logs não são registrados aqui).
- **Confirmado em Windows real**: `kioskd perfil criar --conta` com nome de conta com espaço resolveu o SID correto da conta da criança; `app criar` funcionou; `servir` escutou no IP Tailscale e no loopback com o agente ativo.
- **Achados**:
  - (defeito, fail-loud violado) sem `KIOSK_DB` na janela, o `kioskd` usava `sqlite://kiosk.db` na pasta atual e criava outro banco em silêncio: na segunda janela o perfil "sumiu" e o app foi cadastrado no banco errado;
  - pareamento recusado (`403`) porque o código digitado não era o gerado por `kioskd codigo-pareamento`; a mensagem do CLI não explicava o código esperado;
  - na 0.0.1 o `servir` falhava sem a pasta do banco (já corrigido na 0.0.2).
- **Resultado**: banco padrão fixo no Windows (`C:\ProgramData\kiosk\kiosk.db`); todo comando do `kioskd` informa o banco em uso; mensagem de pareamento recusado explica o código e o banco; runbook com o banco padrão, o loopback para parear no próprio PC e o código de 8 caracteres; versão 0.0.3.
- **Evidência**: clippy `-D warnings` limpo em Linux e no alvo Windows; testes novos em `tests/cli.rs` (banco informado em todo comando; mensagem do pareamento recusado) e em `tests/windows.rs` (sem `KIOSK_DB`, o padrão é o banco da máquina), este no CI em Windows.
- **Pareamento confirmado em Windows real**: com o banco certo, `codigo-pareamento` + `kioskctl parear` pelo loopback funcionou.
- **Achado seguinte**: o dono tentou `kioskd regra criar`, que não existia (regras só pelo `kioskctl`). Adicionado `kioskd regra criar|ativar|revogar|limite|listar`, que reaproveita `comandos::interpretar` (mesma validação da Camada 3) e as operações auditadas do banco; `listar_regras` no banco; teste de ponta a ponta em `tests/cli.rs` (inclui os eventos `ADD_RULE`, `ACTIVATE_RULE`, `MODIFY_LIMIT`, `REVOKE` no log).
- **Pendente**: o teste de bloqueio do Bloco de Notas na conta da criança ainda não foi feito.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-29 — Enforcement validado em PC real com conta de criança

- **Origem**: teste do dono com a 0.0.2 no próprio PC (identificadores, SID e IP do teste não são registrados aqui).
- **Roteiro**: `kioskd perfil criar --conta` (conta local padrão da criança), `kioskd app criar --processo notepad.exe`, `kioskd servir` num PowerShell de administrador (não como SYSTEM), `kioskd codigo-pareamento` + `kioskctl parear` pelo loopback, `kioskctl regra criar --limite 0` e `kioskctl regra ativar`; depois o Bloco de Notas foi aberto na sessão da criança (troca rápida de usuário).
- **Resultado**: o `kioskctl log` mostrou `ADD_PROFILE`, `ADD_RULE`, `ACTIVATE_RULE` e três `APP_TERMINATED` com o detalhe `app Notepad.exe (pid …) encerrado: APP_BLOQUEADO`, a cerca de 6 s um do outro (três aberturas encerradas). O agente, rodando como administrador comum, leu o SID de processos de outra conta sem precisar de `SeDebugPrivilege`.
- **O que isto valida** (E-004 F2, parcial): detecção por SID em sessão de outro usuário, encerramento real, motor com regra de limite 0, log forense e o ciclo de regra pela API a partir de um dispositivo pareado.
- **Ainda sem validação**: aviso por `WTSSendMessageW`, bloqueio de sessão (`WTSDisconnectSession`), logoff após 2 desbloqueios, limites por tempo (com avisos de 5 e 1 min) e o `kioskd` como serviço do Windows.

## 2026-10-29 — Regra: todo comando tem fluxograma Mermaid

- **Origem**: pedido do dono, antes do merge da 0.0.3: todo comando precisa de fluxograma Mermaid na documentação, para facilitar a cognição e mostrar a integração e o fluxo de informação entre cliente e serviço.
- **Resultado**:
  - regra nova no [AGENTS.md](../AGENTS.md) ("Fluxogramas dos comandos");
  - [fluxos dos comandos](architecture/fluxos-dos-comandos.md): visão geral, o pipeline comum das Camadas 0 a 5 e um fluxograma por comando, 15 do `kioskd` e 7 do `kioskctl`, com quem age, o que é gravado (tabela e evento), as recusas e o retorno; ligado à arquitetura e ao runbook;
  - `tests/documentacao.rs` transforma a regra em gate: lê os comandos que os próprios executáveis imprimem no texto de uso e reprova se algum não tiver seção com bloco `mermaid`, ou se o documento citar comando inexistente.
- **Evidência**: os 24 blocos renderizados com `@mermaid-js/mermaid-cli` 11.4.2 e o Chromium do container, sem erro; três mutações (apagar a seção de um comando, citar comando inexistente, trocar `mermaid` por `text`) reprovaram o teste com a mensagem certa e foram revertidas.
- **Limitações**: o teste confere a existência do fluxograma, não a fidelidade ao código; a renderização não roda no CI (só localmente, antes do commit, como diz a regra).
- **Commit**: branch `claude/hopeful-curie-egiw07`.

## 2026-10-29 — Versão 0.0.3 publicada

- **Origem**: pedido do dono para mesclar o jefersonlop3s/kiosk-engine#8 e publicar.
- **Resultado**: o `release.yml` com `publicar: true` publicou o release [v0.0.3](https://github.com/jefersonlop3s/kiosk-engine/releases/tag/v0.0.3) como pré-lançamento, com a tag em `518ebfb` e os arquivos:
  - `kiosk-engine-v0.0.3-windows-x86_64.zip` (3 674 075 bytes, `sha256:16e8a58c7a75b3d5aebfd9dcdee841b888b9715e86435432d009450dbbd31307`);
  - `kiosk-engine-v0.0.3-linux-x86_64.tar.gz` (3 585 994 bytes, `sha256:65911d090121c265c3301535f1fde47d9c1e27ffe9e4703bedcbb6a7e9b2c5a9`);
  - `SHA256SUMS.txt`.
- **Evidência**: release consultado pela API do GitHub; o dono usou o pacote no próprio PC (entrada seguinte).

## 2026-10-29 — Testes do dono no Windows com a 0.0.3: aviso, limite, bloqueio e logoff

- **Origem**: roteiro sugerido pelo agente, executado pelo dono no próprio PC com uma conta local padrão de criança (identificadores, SID e IP não são registrados aqui).
- **Validado em Windows real**:
  - sem `KIOSK_DB`, o `kioskd` abriu o banco padrão `C:\ProgramData\kiosk\kiosk.db` com os cadastros feitos antes; `kioskd regra criar --ativar` criou a regra já ativa; `regra revogar` e `regra limite` auditados (`REVOKE`, `MODIFY_LIMIT`);
  - **avisos**: com limite de 360 s no Bloco de Notas, `WARNING_SHOWN` com 299 s restantes, outro com 59 s restantes 4 min 9 s depois, e `APP_TERMINATED` por `LIMITE_DIARIO_APP` 60 s depois; cada aviso apareceu uma vez e o dono confirmou as duas caixas de mensagem na tela da criança (`WTSSendMessageW` validado);
  - **limite do app esgotado**: o Bloco de Notas reaberto foi encerrado na hora, três vezes, por `LIMITE_DIARIO_APP`;
  - **bloqueio de sessão**: com limite total de 60 s no perfil, dois `SESSION_LOCKED` por `LIMITE_DIARIO_TOTAL` com 30 s de intervalo; o dono confirmou que a tela voltou para a senha (`WTSDisconnectSession` validado; fecha o GC-18).
- **Não validado**: o **logoff** após 2 desbloqueios não ocorreu: depois do segundo bloqueio não houve ação nem erro registrados até o limite ser removido, 7 minutos depois. O que o dono percebeu como logoff foi a desconexão. Aberto como GC-20 no [PENDING](PENDING.md).

## 2026-10-29 — E-007 Onboarding e instalador proposto

- **Origem**: ideia do dono depois dos testes: um lançador de primeira execução que crie pastas e configuração, liste as contas locais para escolher os perfis de criança, liste os programas para escolher os controlados e crie as regras iniciais (limite total por criança e limites por app dentro do perfil). Não é para agora.
- **Resultado**: [E-007](epics/E-007-onboarding-e-instalador.md) registrado como proposto, com resultado, fases (assistente, serviço do Windows, instalador), aceites e as decisões que exigirão ADR; incluído no [ROADMAP](ROADMAP.md) e no PENDING. O índice de épicos, que ainda mostrava E-001 a E-005 como propostos, foi atualizado.
- **Commit**: branch `claude/hopeful-curie-egiw07` (PR só de documentação).

## 2026-10-29 — ADR-007 a ADR-009 propostas para o E-007

- **Origem**: pedido do dono para registrar já as ADRs que o E-007 deixou em aberto.
- **Resultado** (todas **Proposto**; o aceite é do dono):
  - [ADR-007](adr/ADR-007-forma-do-assistente-de-configuracao.md): assistente interativo no terminal (`kioskd configurar`), idempotente, com modo não interativo (`--respostas`) para CI e reinstalação; só orquestra as operações existentes; interface gráfica depois, junto da bandeja da ADR-004;
  - [ADR-008](adr/ADR-008-formato-do-instalador.md): etapa 1 com autoinstalação (`kioskd instalar|desinstalar`: Program Files, pasta do banco com ACL, serviço SYSTEM no SCM com reinício automático), etapa 2 com `.msi` (WiX) e winget; assinatura de código registrada como risco e decisão do dono;
  - [ADR-009](adr/ADR-009-origem-da-lista-de-programas.md): atalhos do menu Iniciar e processos em execução, com entrada manual; a lista não é guardada, só as escolhas; histórico de processos rejeitado por coletar mais do que as regras pedem.
- **Evidência**: `verificar_adr.py` e `verificar_refs.py` sem erros; E-007, índice de épicos, ROADMAP e PENDING apontam para as ADRs.
- **Limitações**: nenhuma alternativa foi exercitada; são análise, não medição.

## 2026-10-29 — Aceite das ADR-007 a ADR-009 e início do E-007

- **Origem**: aceite explícito do dono ("aceito.. inicie").
- **Resultado**: [ADR-007](adr/ADR-007-forma-do-assistente-de-configuracao.md), [ADR-008](adr/ADR-008-formato-do-instalador.md) e [ADR-009](adr/ADR-009-origem-da-lista-de-programas.md) passam a **Aceito**; o [E-007](epics/E-007-onboarding-e-instalador.md) entra em andamento, começando pela etapa 1 da ADR-008 (autoinstalação e serviço do Windows).
- **Evidência**: `verificar_adr.py` e `verificar_refs.py` sem erros antes do commit.


## 2026-10-29 — Serviço do Windows: `kioskd instalar`, `desinstalar` e `estado` (E-007 F2, ADR-008 etapa 1)

- **Origem**: [ADR-008](adr/ADR-008-formato-do-instalador.md) aceita; GC-16 do [PENDING](PENDING.md).
- **Resultado**: `src/servico_windows.rs` implementa a autoinstalação (cópia para `%ProgramFiles%\kiosk`, pasta de dados com dono e acesso só de SYSTEM e Administradores, serviço `kiosk` LocalSystem com início automático, reinício em falha e DACL que só deixa a conta padrão consultar, `KIOSK_LISTEN` no `Environment` do serviço) e o modo serviço (`kioskd servico`, despachante do SCM, stderr para `kioskd.log`). Fluxogramas dos quatro comandos; runbook com a seção 4.1.
- **Evidência local**: `cargo clippy -D warnings` no Linux e cruzado para `x86_64-pc-windows-gnu`, 127 testes passando no Linux, 28 diagramas validados com o mermaid-cli.
- **Evidência de aceite**: `tests/servico_windows.rs` no runner Windows do CI (`KIOSK_TESTE_SERVICO=1`), com controles negativos (escuta fora da ADR-005, conta padrão tentando parar e iniciar o serviço e ler os dados, `--apagar-dados` sem confirmação).
- **Pendente**: reinício do PC real (GC-16).

## 2026-10-29 — E-008 a E-011: cliente gráfico e serviço em outras plataformas

- **Origem**: pedido do dono depois da publicação da 0.0.4.
- **Resultado**:
  - [E-008](epics/E-008-cliente-grafico-dos-pais.md) proposto: app gráfico dos pais para Windows, Linux e macOS na Tailscale, sobre o mesmo cliente do `kioskctl`. Registra a lacuna da API (não há listagem remota de perfis, apps e regras), que vira a F1;
  - [E-009](epics/E-009-servico-linux.md), [E-010](epics/E-010-servico-macos.md) e [E-011](epics/E-011-servico-android.md) congelados, com condição de retomada objetiva: `kioskd` no Windows maduro e cliente gráfico maduro. Cada um exige uma ADR que revise a [ADR-002](adr/ADR-002-identidade-por-conta-windows.md) (GAP Estrutural) e terá CI/CD próprio.
- **Requisitos do dono, mesma data**:
  - o cliente só pareia a partir de uma conta de administrador no Windows que roda o serviço. Registrado no E-008, com a lacuna atual (a barreira hoje vem só da ACL da pasta, depois do `instalar`) e a correção na F1;
  - no Linux e no macOS, `sudo` para gerar o código e para definir os **responsáveis** por cada conta monitorada (conceito novo). Registrado no E-009 e no E-010.
- **Evidência**: `verificar_refs.py` e `verificar_adr.py` sem erros. Sem código.

## 2026-10-29 — Só administrador gera código de pareamento (antecipado do E-008)

- **Origem**: regra do dono (o cliente só pareia a partir de uma conta de administrador no PC do serviço) e pedido para antecipá-la, antes do resto do [E-008](epics/E-008-cliente-grafico-dos-pais.md).
- **Resultado**: `sistema_windows::administrador_elevado` (grupo Administradores ativo no token efetivo) e `KioskDbManager::recusar_codigo_pareamento` (`AUTHZ_FAIL`, ação `CREATE_PAIRING_CODE`). O `kioskd codigo-pareamento` recusa quem não é administrador elevado e não cria código. Fluxograma do comando e runbook atualizados.
- **Evidência local**: clippy `-D warnings` no Linux e cruzado para Windows, 128 testes no Linux (incluindo a gravação do `AUTHZ_FAIL` sem código criado) e 28 diagramas validados.
- **Evidência no CI Windows**: o runner, administrador elevado, passa (controle positivo); a conta padrão real de `tests/servico_windows.rs` dá `false` (controle negativo).
- **Limitação**: o teste negativo exercita a checagem sob personificação da conta padrão, não um processo `kioskd` inteiro aberto por ela.
- **Erro corrigido no CI**: a primeira versão usava `CheckTokenMembership(NULL, ...)`, que sob personificação abre o token com a identidade da conta padrão e recebe "acesso negado". O teste negativo pegou isso como erro, sem passar por administrador. A versão final abre o token efetivo com a identidade do processo e lê os grupos (`TokenGroups`).

## 2026-10-29 — README, maturidade por épico, responsáveis e insight do responsável temporário

- **Origem**: pedidos e decisões do dono na mesma data.
- **Resultado**:
  - `README.md` na raiz (não existia): o que é o kiosk, como funcionam o serviço e o cliente, conceitos, plataformas e dois fluxogramas (arquitetura e "o que acontece quando o tempo acaba");
  - [ROADMAP](ROADMAP.md) com a seção "Maturidade por épico": porcentagem com critério declarado (média das fases; concluída 100%, parcial 50%, não iniciada 0%);
  - responsáveis = administradores, todos sobre todas as contas monitoradas, sem vínculo por criança; vale para Windows, Linux e macOS ([E-008](epics/E-008-cliente-grafico-dos-pais.md), [E-009](epics/E-009-servico-linux.md), [E-010](epics/E-010-servico-macos.md));
  - [insight do responsável temporário](insights/2026-10-29-responsavel-temporario.md): delegação de até 5 dias a contas não administrativas, com revisão ao fim. O dono registrou que o raciocínio ainda não fecha, então fica sem épico.
- **Evidência**: os dois diagramas do README renderizados no mermaid-cli e conferidos visualmente (um rótulo sobreposto foi corrigido); `verificar_refs.py` e `verificar_adr.py` sem erros.

## 2026-10-29 — Serviço do Windows instalado no PC real do dono (0.0.4)

- **Origem**: teste do dono com o pacote da 0.0.4, num PowerShell como administrador.
- **Resultado**: o `kioskd instalar` protegeu `C:\ProgramData\kiosk`, copiou os executáveis para `C:\Program Files\kiosk`, registrou o serviço e o iniciou. O `kioskd estado` mostrou o serviço em execução, com início automático, conta LocalSystem, o binário instalado em modo `servico` e o `KIOSK_LISTEN` no ambiente do serviço (Tailscale e loopback).
- **Ainda não visto no PC real**: o serviço subir sozinho depois de reiniciar o Windows e o agente agir como SYSTEM sobre a conta da criança (GC-16 segue aberto). O logoff (GC-20) também segue aberto.
- **Observação**: definir `KIOSK_DB` e `KIOSK_LISTEN` na janela antes do `instalar` não é necessário; o endereço vai para o serviço pelo `--escutar`, e o banco usa o padrão.
- Nenhum dado do log do dono (IP, conta, caminhos pessoais) foi copiado para o repositório.

## 2026-10-29 — Serviço volta sozinho após reiniciar o Windows; pasta do kiosk no PATH

- **Origem**: teste de reinício do dono com a 0.0.4 e pedido para registrar o diretório de instalação no PATH do sistema (o `kioskd` não era encontrado fora da pasta extraída).
- **Resultado**:
  - depois de reiniciar, o `kioskd estado` mostrou o serviço em execução com um PID diferente do da instalação, início automático e conta LocalSystem. Com isso, a [E-007](epics/E-007-onboarding-e-instalador.md) F2 está concluída e o GC-16 se reduz a ver o agente, como SYSTEM, aplicar uma regra na conta da criança;
  - `kioskd instalar` põe `%ProgramFiles%\kiosk` no `Path` da máquina (`HKLM\...\Session Manager\Environment`), sem duplicar, preservando as outras entradas e o tipo do valor, e avisa o Windows com `WM_SETTINGCHANGE`. `desinstalar` tira a entrada; `estado` mostra se ela está lá. A pasta só aceita escrita de SYSTEM e Administradores, então a entrada no PATH não permite que a criança plante um executável;
  - fluxogramas de `instalar`, `desinstalar` e `estado` e o runbook atualizados; ROADMAP: E-004 em 70% e E-007 em 35%.
- **Evidência**: log do dono (nenhum dado pessoal copiado); `tests/servico_windows.rs` no CI Windows confere, pelo `reg query`, que a pasta entra uma vez, que a reinstalação não duplica e que o PATH volta ao original depois do `desinstalar`.

## 2026-10-29 — E-012 e ADR-010: idiomas e CLI em inglês (proposta)

- **Origem**: pedido do dono: en-US, pt-BR e es-ES nas interfaces, no serviço e nas respostas; comandos da CLI com equivalentes em inglês como padrão (`kioskd estado` → `kioskd status`).
- **Resultado**: [E-012](epics/E-012-internacionalizacao.md) e [ADR-010](adr/ADR-010-idiomas-e-comandos-em-ingles.md) **propostos**. A ADR traz a tabela completa de nomes, os apelidos em português até a 0.1.0 (o serviço instalado na 0.0.4 está registrado como `servico`), o catálogo no código com `match` exaustivo, a ordem de escolha do idioma e o log forense com códigos. Nada implementado antes do aceite.
- **Evidência**: `verificar_refs.py` e `verificar_adr.py` sem erros.

## 2026-10-29 — Aceite da ADR-010 com ajustes do dono

- **Origem**: respostas do dono aos três pontos da [ADR-010](adr/ADR-010-idiomas-e-comandos-em-ingles.md).
- **Resultado**: ADR-010 **Aceita**, com os ajustes registrados na própria ADR:
  - comandos em português e em inglês permanentes, sem obsolescência;
  - API só em inglês, inclusive o corpo do pareamento;
  - documentação em inglês, com uma página de equivalentes em português;
  - texto no idioma do perfil de quem o vê;
  - mensagens da CLI traduzidas e palavra de confirmação no idioma da mensagem.

  O [E-012](epics/E-012-internacionalizacao.md) entra em andamento pela F1.
- **Evidência**: `verificar_adr.py` e `verificar_refs.py` sem erros.

## 2026-10-29 — E-012 F1: comandos em inglês, português equivalente e API só em inglês

- **Origem**: [ADR-010](adr/ADR-010-idiomas-e-comandos-em-ingles.md), aceita com ajustes do dono.
- **Resultado**:
  - `src/cli.rs` traz as tabelas português → inglês de comandos e opções. O `kioskd` e o `kioskctl` traduzem os argumentos antes de interpretar, então as duas línguas, inclusive misturadas, seguem o mesmo código;
  - uso impresso, fluxogramas, runbook e README em inglês; página [comandos em português](runbooks/comandos-em-portugues.md) gerada das tabelas;
  - corpo do pareamento em inglês (`code`, `public_key`, `name`), uma quebra registrada no CHANGELOG;
  - `install` registra o serviço como `service`, e o `servico` da 0.0.4 segue valendo.
- **Evidência local**:
  - 134 testes no Linux, entre eles `tests/cli_idiomas.rs`: tradução só de comando, subcomando e opção, valores intactos, tabelas sem ambiguidade, o mesmo cadastro nas duas línguas e uso sem português nas linhas de comando;
  - página conferida contra as tabelas em `tests/documentacao.rs`;
  - os testes de CLI anteriores, ainda em português, seguem passando;
  - clippy no Linux e cruzado para Windows; diagramas validados no mermaid-cli.
- **Evidência no CI Windows**: o teste do serviço instala em inglês e reinstala em português, e confere o registro como `kioskd.exe" service`.

## 2026-10-29 — E-012 F2 e F3: mensagens em en-US, pt-BR e es-ES e log com códigos

- **Origem**: [ADR-010](adr/ADR-010-idiomas-e-comandos-em-ingles.md), aceita com ajustes do dono; pedido de seguir com as próximas fases.
- **Resultado**:
  - `i18n` (idiomas, escolha, catálogo por macro com os três idiomas obrigatórios) e `detalhes` (detalhe do log como código JSON, traduzido ao ler);
  - `kioskd`, `kioskctl`, cliente e serviço com todas as mensagens ao usuário no catálogo; texto de uso, nome e descrição do serviço também;
  - avisos à criança nos três idiomas, no idioma da conta dela (`sistema_windows::idioma_da_conta`, perfil carregado em `HKEY_USERS`);
  - o `install` grava `KIOSK_LANG` no serviço;
  - desvios e itens fora do escopo registrados na ADR-010 (seção "Implementação e desvios").
- **Evidência local**: 146 testes no Linux, entre eles `tests/i18n.rs` (tags, catálogo, código do detalhe e tradução), `tests/cli_idiomas.rs` (escolha do idioma por `KIOSK_LANG` e pelo sistema, nomes de comando intactos), o log traduzido nos três idiomas em `tests/cli.rs` e o log do agente em código em `tests/agente.rs`; clippy no Linux e cruzado para Windows; 30 diagramas validados.
- **Evidência no CI Windows**: leitura do idioma da conta pelo registro (`tests/windows.rs`), serviço instalado com `KIOSK_LANG` e nome de exibição no idioma de quem instala (`tests/servico_windows.rs`).
- **Falta**: ver no PC real o aviso à criança no idioma da conta dela.
- **Quebra**: o log gravado pela 0.0.4 tem texto em português; o `kioskctl log` o mostra como está.

## 2026-10-29 — v0.0.5 publicada e validada no PC real do dono

- **Origem**: publicação da 0.0.5 ([release](https://github.com/jefersonlop3s/kiosk-engine/releases/tag/v0.0.5), pré-lançamento, gerada de `01c8556`) e teste do dono no próprio PC.
- **Resultado**:
  - **Pacote**: o SHA-256 do `.zip` do Windows bateu com o do `SHA256SUMS.txt`.
  - **Atualização**: `kioskd install` sobre a 0.0.4 parou o serviço, trocou os executáveis em `Program Files\kiosk`, pôs a pasta no PATH do sistema, registrou o serviço como `service` e o reiniciou, sem outro passo.
  - **PATH**: em janela nova do PowerShell, `kioskd` funciona de qualquer pasta.
  - **Comandos**: `kioskd status` e `kioskd estado` dão o mesmo resultado, com `KIOSK_LANG=pt-BR` (o idioma de quem instalou) no ambiente do serviço.
  - **Idiomas**: a saída muda na hora com `KIOSK_LANG` e foi vista em pt-BR, en-US e es-ES.
  - **Segurança do pareamento**: sem elevação, `kioskd pairing-code` recusa; elevado, gera o código, nos dois idiomas.
- **Achado**: a recusa sem elevação **não foi registrada no log forense**, porque a conta comum não abre o banco protegido. Registrado como GC-21 no [PENDING](PENDING.md).
- **Ainda por ver no PC real**: o aviso à criança no idioma da conta dela ([E-012](epics/E-012-internacionalizacao.md) F3) e o agente aplicando uma regra na conta da criança como SYSTEM (GC-16).
- Nenhum dado do log do dono (IP, conta, caminhos pessoais) foi copiado para o repositório.

## 2026-10-29 — ADR-011 proposta: rastro das tentativas sem privilégio (GC-21)

- **Origem**: achado do teste da 0.0.5 (a recusa de `pairing-code` sem elevação não chega ao log) e pedido do dono para seguir com uma ADR para o canal entre o comando e o serviço.
- **Resultado**: [ADR-011](adr/ADR-011-rastro-de-tentativas-sem-privilegio.md) **proposta**, ainda sem código:
  - o comando avisa o serviço por named pipe local (`\\.\pipe\kiosk-audit`) e o serviço grava no banco, ligado ao perfil da criança quando o chamador é um;
  - o chamador é descoberto pelo token da conexão e nunca pelo conteúdo da mensagem;
  - esquema fechado, limite de taxa por conta, falha alta quando o serviço não responde;
  - todo comando que abre o banco confere a elevação primeiro e explica a recusa, no lugar do erro cru do SQLite (outro achado do teste);
  - alternativas rejeitadas (Log de Eventos do Windows, pasta de entrada, HTTP sem assinatura, abrir o banco) e o plano de testes no CI Windows.
- **Evidência**: `verificar_adr.py` e `verificar_refs.py` sem erros.

## 2026-10-29 — Opção desconhecida deixa de ser ignorada; aviso de limite já ultrapassado

- **Origem**: log do dono no teste da 0.0.5: `--activete` (digitado errado) criou uma regra pendente sem erro, e a regra ativa de 3 min fechava o Bloco de Notas na hora porque ele já tinha 11min09s de uso hoje. As revogações (`d9712d1f` e `be76246e`) eram de regras **pendentes**, que nunca valeram; a que valia (`14b04946`) continuava ativa. Não era defeito da revogação.
- **Resultado**:
  - `cli::validar` e as tabelas `FORMAS_KIOSKD` e `FORMAS_KIOSKCTL`: opção desconhecida, opção sem valor e argumento sobrando são recusados, com sugestão por distância de edição, antes de abrir o banco ou gravar. `--help` e `-h` mostram o uso;
  - `kioskd rule create`, `rule activate`, `rule limit` e `profile limit` avisam quando a regra ativa (ou o limite total) já nasce com limite menor ou igual ao uso de hoje;
  - a duração das listagens passa de `0h03` para `3min 00s`;
  - runbook com a explicação (limite por dia, uso já contado, revogar por ID) e fluxogramas atualizados.
- **Evidência local**: 151 testes; um teste confere a tabela de argumentos contra o texto de uso nos três idiomas (uma opção nova só no uso, ou só na tabela, reprova o build); o caso do `--activete` em três idiomas e em português; aviso de limite em cada idioma, sem aviso para regra pendente, limite folgado e limite 0; clippy no Linux e cruzado para Windows; 30 diagramas validados.
- **ADR-011**: o dono confirmou as duas decisões abertas (registrar também o administrador sem elevação; `sc stop` fora do escopo). O aceite da ADR como um todo está pendente.
- Nenhum dado do log do dono (IDs, SID, conta) foi copiado.

## 2026-10-29 — ADR-011 aceita

- **Origem**: aceite explícito do dono ("ADR-011 aceita"), depois de confirmar as duas decisões abertas.
- **Resultado**: [ADR-011](adr/ADR-011-rastro-de-tentativas-sem-privilegio.md) passa a **Aceito**. O GC-21 segue aberto até a implementação (named pipe `kiosk-audit`, personificação do cliente para ler o SID, evento `AUTHZ_FAIL` com o perfil da criança, limite de taxa por conta, conferência de elevação em todo comando que abre o banco) e a validação no CI Windows.

## 2026-10-29 — v0.0.6 publicada; ADR-011 implementada (GC-21)

- **v0.0.6** ([release](https://github.com/jefersonlop3s/kiosk-engine/releases/tag/v0.0.6), pré-lançamento, de `de25d0b`): opção desconhecida deixa de ser ignorada, aviso de limite já ultrapassado, duração em `3min 00s`. O SHA-256 do `.zip` do Windows está no `SHA256SUMS.txt` do release.
- **ADR-011 implementada** (ainda sem versão publicada):
  - `auditoria` (protocolo de uma linha JSON de esquema fechado, limite de taxa por conta, evento) e `auditoria_windows` (servidor do pipe no serviço, com DACL e personificação para ler o token, e o cliente no `kioskd`);
  - `kioskd` confere a elevação de todo comando administrativo antes de abrir o banco; a API entrega o detalhe da tentativa aos pais, sem o SID;
  - desvios registrados na ADR (conferência do servidor pelo PID do serviço, direitos do pipe, nível de personificação).
- **Evidência local**: 164 testes no Linux, entre eles 13 de `tests/auditoria.rs` (entrada hostil, limite de taxa, perfil da criança, administrador sem elevação, inundação) e o log da API sem o SID; clippy no Linux e cruzado para Windows; 28 diagramas validados.
- **Evidência no CI Windows**: conta padrão real conecta ao pipe, o serviço grava o evento com o SID do **token**, a mensagem forjada com outro SID é recusada, o evento leva o perfil quando a conta é uma criança cadastrada, a inundação para em 5 por minuto. O `kioskd.exe` inteiro rodando como a conta padrão **não** é exercitado no CI (`Start-Process -Credential` trava nos runners); fica atrás de `KIOSK_TESTE_PROCESSO_COMO_CONTA=1` para rodar num PC real.
- **Falta**: ver no PC do dono, numa conta comum, a tentativa aparecer em `kioskctl log --profile` (GC-21 segue aberto até lá).

## 2026-10-29 — GC-21 visto no PC real; `version`; sondagem do pipe

- **GC-21 validado no PC real** (build de ensaio do commit `fe2461d`, sem release): uma conta comum rodando `pairing-code` e `profile list` gerou `AUTHZ_FAIL` em `kioskctl log --profile`, ligado ao perfil da criança e sem SID; na rajada, só 5 eventos por minuto; o administrador elevado gera o código normalmente; o serviço parou e voltou com `Stop-Service`/`Start-Service`. Não foi visto no PC: a recusa com o **serviço parado** ("NÃO registrada"). Os dados do teste (IP, conta, IDs) ficam fora do repositório.
- **`kioskd version` e `kioskctl version`** (pedido do dono): `src/versao.rs` e `build.rs` gravam commit, data, alvo e perfil no binário. `status` e `install` mostram a versão; o `status` lê também a do executável instalado. Fluxogramas novos (`kioskd version`, `kioskctl version`) e atualizados (`kioskd status`, `kioskd install`), 30 diagramas validados com o mermaid-cli. Testes: `tests/versao.rs` (7) e, no CI Windows, versão no `install` e no `status`.
- **Sondagem do pipe**: o erro 1368 no `kioskd.log` do PC real vinha de uma conexão que fechou sem enviar nada (o `Test-Path` do roteiro). `ler_linha` agora trata "nenhum byte" como encerramento sem erro; o teste Windows conecta e fecha e confere que o log não registra falha.
- Fica para a 0.0.7, a pedido do dono, junto das correções que o teste em andamento achar.

## 2026-10-29 — E-012 concluído

- O dono aceitou encerrar o E-012: os avisos com os horários já foram vistos no PC real, e o idioma da conta da criança passa a ser **simulado** no CI Windows.
- `sistema_windows::texto_do_aviso` (extraída do `avisar`) liga o idioma lido de `HKEY_USERS\<SID>` ao texto do aviso. `tests/windows.rs` cobre es-ES, en-US, pt-BR, a precedência do idioma de exibição e o idioma não traduzido (`fr-FR`), no registro real do runner.
- Fica fora das fases: revisão da tradução es-ES por falante.

## 2026-10-29 — `AGENT_STOPPED` (E-004 F4) e resposta perdida no pipe

- **`AGENT_STOPPED`**: `agent_heartbeat` (migração 0007), `Agente::conferir_parada_anterior` e `bater`, `Detalhe::AgentWasDown`, marca de parada normal gravada por `servir` ao receber o pedido do Windows. Detalhes e desvio da ADR-004 no E-004. 9 testes em `tests/agente_parada.rs` (primeira execução, agente que sumiu com o intervalo exato, parada de propósito, reinício rápido, limiar de 3 batimentos, frequência do batimento, desmarcar a parada normal, texto nos idiomas, banco sem a tabela) e, no CI Windows, parada de propósito e `taskkill` com o intervalo acima de 30 s.
- **Correção no pipe de auditoria** (ADR-011, item 7): o servidor desconectava antes de o cliente ler; achado em duas rodadas intermitentes do CI ("o serviço não confirmou").
- **Correção ao que eu disse**: o salto de relógio já estava coberto desde o E-003 (intervalos negativos ou acima de 600 s viram `TAMPER_SUSPECTED` e não são contados); não é GAP novo.
- Vai no mesmo PR do encerramento do E-012.

## 2026-10-29 — Bandeja, etapa 1: canal serviço ↔ UI (E-004 F3)

- `src/ui_windows.rs`: servidor do pipe `kiosk-ui`, `GerenteUi` e conferência do executável do cliente (`imagem_do_cliente_do_pipe`). `Detalhe::UiRejected` e mensagens nos três idiomas. Ainda não ligado ao `kioskd`: nada muda para quem usa a 0.0.6.
- Desvios da ADR-004 (um pipe só; conferência do executável) registrados na própria ADR.
- `tests/ui_windows.rs` (7 testes) no CI Windows; a conta padrão de teste foi extraída para `tests/comum`.

## 2026-10-30 — Bandeja, etapa 2: `kiosk-ui.exe` (E-004 F3)

- `kiosk-ui.exe` (sem console, sem menu, sem "sair"), `bandeja` (dica e aviso nos três idiomas, puros), `bandeja_windows` (ícone e balão) e `ui_cliente_windows` (conexão com batimento, reconexão e entrega). O protocolo ganhou `codificar_da_ui` e `decodificar_para_ui`.
- `install` e o pacote do Windows passam a levar o `kiosk-ui.exe`. O serviço ainda não o inicia (etapa 3).
- Testes: `tests/bandeja.rs` (textos e protocolo, no Linux); no CI Windows, o cliente contra o servidor real (espera o serviço, batimento, aviso, confirmação, serviço que cai e volta) e o `kiosk-ui.exe` de verdade, no modo de teste (texto no idioma da conta e confirmação) e com a bandeja real (sobe, conecta, fica vivo).
- **Falta ver no PC**: o desenho do ícone e do balão (o runner do CI não tem como mostrar).

## 2026-10-30 — Bandeja, etapa 3: o serviço supervisiona a UI (E-004 F3 e F4)

- `lancador_windows` (`WTSQueryUserToken` + `CreateProcessAsUserW`), `Sistema` ganha `tem_ui`, `batimento_da_ui` e `iniciar_ui`, e o agente supervisiona a bandeja de cada criança com sessão ativa. `SistemaWindows` passou a carregar o `GerenteUi` e o caminho do `kiosk-ui.exe` (`com_ui`). `kioskd serve` escuta o pipe `kiosk-ui`. `install` e `uninstall` encerram as bandejas em execução.
- Testes: `tests/agente_ui.rs` (10, com tempo simulado) cobre início, bandeja saudável, bandeja que some (alerta com perfil e relançamento), limite de alertas, falha ao iniciar, conta sem perfil, sistema sem bandeja e sessão que fecha e volta. No CI Windows, o serviço real como SYSTEM inicia a bandeja na sessão do runner, o batimento chega por mais de 15 s sem alerta, e a bandeja morta é relançada e registrada (se o runner não tiver sessão interativa, o teste diz e pula essa parte).
- **Ainda não passa pela bandeja**: o aviso de 5 e 1 minuto (segue na caixa de mensagem) e o estado do tempo restante (o agente ainda não o manda). É a etapa 4.

## 2026-10-30 — Fechamento da sessão (0.0.4 a 0.0.7 e bandeja)

- Entregas e estado ao encerrar, aprendizados técnicos e de processo, e o roteiro de retomada estão no [insight da sessão](insights/2026-10-30-aprendizados-da-sessao-de-0.0.4-a-0.0.7.md).
- O conflito do [CHANGELOG](../CHANGELOG.md) do #22 foi resolvido comparando o resultado com os dois lados do merge: nada se perdeu (o "Corrigido" do `kioskctl log` e a bandeja estão os dois).

