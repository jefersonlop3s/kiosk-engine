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
  - `CHANGELOG.md`: seção `[0.0.1] - 2026-10-28`, marcada como pré-lançamento;
  - `.github/workflows/release.yml`: na tag `vX.Y.Z`, confere a versão do `Cargo.toml`, roda os testes e compila em Windows e Linux, empacota `kioskd`, `kioskctl`, `LICENSE`, `CHANGELOG.md` e o runbook, gera `SHA256SUMS.txt` e publica um release marcado como pré-lançamento com as notas da seção do CHANGELOG.
- **Evidência**: `cargo test --locked` → 119 passando; extração das notas e conferência da versão ensaiadas localmente com `GITHUB_REF_NAME=v0.0.1`.
- **Limitações**: o workflow de release ainda não rodou (só roda com a tag ou manualmente a partir de `main`); "GPL v2" foi registrado como `GPL-2.0-only`.
- **Commit**: branch `claude/hopeful-curie-egiw07`.

