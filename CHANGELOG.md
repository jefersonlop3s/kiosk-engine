# Changelog

Mudanças do projeto kiosk. Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Não lançado]

### Documentação
- Estrutura documental do método sovereign-harness em `docs/`, com índice, PENDING, ROADMAP e CONCLUSION.
- `AGENTS.md` como fonte única de instruções para agentes; `CLAUDE.md` passa a ser symlink para ele.
- Arquitetura movida para `docs/architecture/README.md`; `ARCHITECTURE.md` vira stub.
- ADR-001 (enforcement ativo e visível) e ADR-002 (perfil por conta Windows, somente Windows) propostas e aceitas em 2026-10-27.

### Corrigido
- Manifesto na raiz com dependências resolvíveis; `cargo check` passa (E-001 F1).

### Chore
- Trabalho em andamento em `src/` (`db_manager.rs`, `security_logger_impl.rs`, `Cargo.toml`) versionado como está; ainda não compila (GC-01 a GC-07).
- Épicos E-001 a E-006 e roadmap de sprints.
- Insights: revisão do estado inicial e crítica ao relatório de hardening Windows (não adotado).
