# Changelog

Mudanças do projeto kiosk. Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Não lançado]

### Documentação
- Estrutura documental do método sovereign-harness em `docs/`, com índice, PENDING, ROADMAP e CONCLUSION.
- `AGENTS.md` como fonte única de instruções para agentes; `CLAUDE.md` passa a ser symlink para ele.
- Arquitetura movida para `docs/architecture/README.md`; `ARCHITECTURE.md` vira stub.
- ADR-003 a ADR-006 propostas (detecção de processos, IPC serviço↔UI, rede da API, assinatura e pareamento); as quatro aceitas pelo dono em 2026-10-28.
- ADR-001 (enforcement ativo e visível) e ADR-002 (perfil por conta Windows, somente Windows) propostas e aceitas em 2026-10-27.

### Adicionado
- Migração inicial com o contrato do dado (ADR-002), structs alinhadas e CRUD transacional com testes em SQLite real (E-001 F2–F3).
- CI no GitHub: `cargo fmt --check`, `clippy -D warnings` e `cargo test` em Ubuntu e Windows, em push de qualquer branch (verde em 2026-10-28).
- Log forense: tipo de evento (migração 0002) e gravação do evento na mesma transação de cada escrita; falhas registram a razão em evento próprio (E-002 F1).
- `Security_Log` append-only: triggers recusam UPDATE e DELETE (migração 0003, E-002 F2).
- Retenção de 90 dias do log forense imposta por trigger, `purge_expired_events` e consulta de eventos por período e por perfil (migração 0004, E-002 F3).
- Motor de decisão puro (`decidir`) e contabilidade de uso por dia local, com guarda de relógio (`TAMPER_SUSPECTED`) (E-003 F1–F3).
- Núcleo do agente independente de SO (`Agente`, interface `Sistema`): avisos de 5 e 1 min, encerramento de apps e bloqueio de sessão com log forense, e leituras do banco para o motor (E-004, parte SO-independente).
- Tempo total do perfil pela sessão ativa (`session_usage`, ADR-003) e política de lock seguido de logoff após 2 desbloqueios.
- Assinatura Ed25519 de requisições, anti-replay por nonce, pareamento presencial por código de uso único e revogação de dispositivos (ADR-006, E-005 F2).
- Protocolo serviço ↔ UI com decodificação defensiva, limite de taxa e monitor de batimento (ADR-004, parte independente de plataforma).
- API de comando `POST /api/windows/{ação}` com as Camadas 0 a 5 (criar, ativar e revogar regras), respostas genéricas e escuta restrita a loopback e Tailscale (ADR-005, E-005 F1/F3).
- Serviço `kioskd` (servir, código de pareamento, dispositivos, revogar), CLI dos pais `kioskctl` (parear e criar, ativar e revogar regras), endpoint `POST /api/pair` e runbook de operação (E-005 F3).
- `modify_limit` e consultas de uso e de log pela API e pelo CLI (`regra limite`, `uso`, `log`).
- Harness Sovereign: guard do agente, gates de git (pre-commit/pre-push) e contrato de validação em `.harness.toml` (E-001 F4).

### Corrigido
- `build.rs` recompila o crate quando `migrations/` muda; antes, uma migração nova podia ser ignorada pelo binário.
- Manifesto na raiz com dependências resolvíveis; `cargo check` passa (E-001 F1).

### Chore
- Trabalho em andamento em `src/` (`db_manager.rs`, `security_logger_impl.rs`, `Cargo.toml`) versionado como está; ainda não compila (GC-01 a GC-07).
- Épicos E-001 a E-006 e roadmap de sprints.
- Insights: revisão do estado inicial e crítica ao relatório de hardening Windows (não adotado).
