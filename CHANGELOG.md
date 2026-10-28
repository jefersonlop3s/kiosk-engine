# Changelog

Mudanças do projeto kiosk. Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Não lançado]

### Documentação
- Estrutura documental do método sovereign-harness em `docs/`, com índice, PENDING, ROADMAP e CONCLUSION.
- `AGENTS.md` como fonte única de instruções para agentes; `CLAUDE.md` passa a ser symlink para ele.
- Arquitetura movida para `docs/architecture/README.md`; `ARCHITECTURE.md` vira stub.
- ADR-001 (enforcement ativo e visível) e ADR-002 (perfil por conta Windows, somente Windows) propostas e aceitas em 2026-10-27.

### Adicionado
- Migração inicial com o contrato do dado (ADR-002), structs alinhadas e CRUD transacional com testes em SQLite real (E-001 F2–F3).
- CI no GitHub: `cargo fmt --check`, `clippy -D warnings` e `cargo test` em Ubuntu e Windows.
- Log forense: tipo de evento (migração 0002) e gravação do evento na mesma transação de cada escrita; falhas registram a razão em evento próprio (E-002 F1).
- `Security_Log` append-only: triggers recusam UPDATE e DELETE (migração 0003, E-002 F2).
- Harness Sovereign: guard do agente, gates de git (pre-commit/pre-push) e contrato de validação em `.harness.toml` (E-001 F4).

### Corrigido
- `build.rs` recompila o crate quando `migrations/` muda; antes, uma migração nova podia ser ignorada pelo binário.
- Manifesto na raiz com dependências resolvíveis; `cargo check` passa (E-001 F1).

### Chore
- Trabalho em andamento em `src/` (`db_manager.rs`, `security_logger_impl.rs`, `Cargo.toml`) versionado como está; ainda não compila (GC-01 a GC-07).
- Épicos E-001 a E-006 e roadmap de sprints.
- Insights: revisão do estado inicial e crítica ao relatório de hardening Windows (não adotado).
