# ADR-002 — Perfil por conta Windows (SID) e alvo somente Windows

Status: Proposto. Data: 2026-10-27. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [arquitetura](../architecture/README.md) Seção II (contrato do dado).

## Contexto

O contrato de dados da [arquitetura](../architecture/README.md) e o código em `src/database_schema.rs` têm três tabelas: `App_Metadata`, `Active_Rules` e `Security_Log`. Faltam dois elementos sem os quais o produto não funciona:

1. **Quem** está usando: não existe perfil de criança, então uma regra não se aplica a uma pessoa.
2. **Quanto** já foi usado: `daily_limit_seconds` não tem onde acumular o consumo do dia.

A arquitetura cita `.exe`, WinAPI e `POST /api/windows/{action}`, mas não declara a plataforma. O desenvolvimento acontece em macOS.

Origem: [revisão do estado inicial](../insights/2026-10-27-revisao-estado-inicial.md).

## Decisão

Escolha do dono em 2026-10-27, registrada aqui como proposta:

- **Alvo somente Windows 10/11** no MVP. macOS e Linux ficam fora do escopo, sem núcleo portável preventivo.
- **Uma conta local Windows padrão (sem admin) por criança.** O perfil (`child_profile`) é vinculado ao **SID** da conta. Os pais usam uma conta administradora separada.
- **Novas tabelas** no contrato (forma final em [E-001](../epics/E-001-fundacao-crate-e-dados.md)):
  - `child_profile`: `profile_id`, `windows_sid` (único), `display_name`, `daily_total_limit_seconds` (nulo = sem limite), janelas de horário por dia da semana.
  - `usage_ledger`: consumo por `profile_id` × `app_id` × dia local, acumulado em segundos.
  - `Active_Rules` ganha `profile_id` (FK) e `time_window_end`.
- Processo sem SID mapeado (conta de pais, sistema) não é controlado.

Alternativas rejeitadas:
- *Conta compartilhada com PIN*: a criança troca de perfil ou usa o PIN de um irmão; o agente teria que manter a própria autenticação.
- *Uma criança por computador*: não atende famílias com mais de um filho no mesmo PC.
- *Windows + macOS no MVP*: dobra o enforcement ([E-004](../epics/E-004-agente-windows-enforcement.md)) sem demanda declarada.

## Consequências

- Ganho: o Windows já isola as contas; um usuário padrão não instala software para todos, não para serviços nem altera a hora do sistema.
- Custo: os pais precisam criar uma conta por filho; o onboarding deve guiar isso.
- Risco: a criança que conhece a senha de administrador burla tudo. Fica fora do alcance do software e é registrado como pressuposto.
- **Pressuposto declarado**: uma máquina Windows, várias contas locais, pais com a única conta administradora. Conta Microsoft/família online e domínio corporativo não são tratados.

## Referências e evolução

- Épicos que consomem: [E-001](../epics/E-001-fundacao-crate-e-dados.md), [E-003](../epics/E-003-motor-de-regras-e-uso.md), [E-004](../epics/E-004-agente-windows-enforcement.md).
- Complementa a Seção II da [arquitetura](../architecture/README.md).
