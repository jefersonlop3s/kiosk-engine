# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| Hash encadeado no `Security_Log` | Adiado pelo dono em 2026-10-28; retomar só com ADR. O trigger cobre UPDATE/DELETE via SQL, não edição direta do arquivo | dono |
| Semântica do motor (E-003) a confirmar: (a) app `MONITORED` **sem regra vigente é permitido** (o oposto, "só roda o que está na lista", exigiria modo allowlist por perfil); (b) dia da semana **sem janela cadastrada = sem restrição de horário** (bloquear o dia inteiro hoje só com uma janela mínima); (c) `WHITELISTED` e `IGNORE` isentos de regras de app | Aprovar ou pedir o modo oposto; é mudança local em `src/motor.rs` | dono |
| [ADR-003](adr/ADR-003-deteccao-de-processos-e-tempo-de-uso.md), [ADR-004](adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md), [ADR-005](adr/ADR-005-rede-da-api-de-comando.md), [ADR-006](adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md) | Propostas em 2026-10-28; recomendação em cada uma | Aceitar, rejeitar ou pedir alternativa; o aceite desbloqueia E-004 F1/F3/F4 e E-005 F1/F2 | dono |
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Origem: [E-003](epics/E-003-motor-de-regras-e-uso.md), revisão do próprio entregue em 2026-10-28.

| ID | Mecanismo | Medido? | Próximo passo |
|---|---|---|---|
| GC-08 | Avisos de 5 e 1 min são constante no código (`AVISOS_SEGUNDOS`); a ADR-001 os quer configuráveis por regra e `active_rules` não tem coluna para isso | Por leitura | Migração + campo em `ActiveRule` quando o E-004 F2 definir a UX do aviso |
| GC-09 | `consumo_do_dia` soma os apps do perfil como "total": dois apps em uso simultâneo contam em dobro contra o limite total | Por leitura | [ADR-003](adr/ADR-003-deteccao-de-processos-e-tempo-de-uso.md) propõe o total pelo tempo de sessão ativa (tabela `session_usage`); depende do aceite |
| GC-10 | Nada liga as peças: ler perfil, janelas do dia, regras e consumo e chamar `decidir` ainda é montado à mão por quem chama | Por leitura | E-004 F1 (laço do agente), com leitura de `child_profile` e `profile_time_window` |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F4 concluídas; falta o encerramento do épico (conferir o CI no GitHub) |
| [E-004](epics/E-004-agente-windows-enforcement.md) a [E-005](epics/E-005-api-comando-e-pareamento.md) | proposto | Ver [ROADMAP](ROADMAP.md) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| Primeira execução do CI (Ubuntu e Windows) | Conferir no GitHub; não foi observada nesta sessão |
| `core.hooksPath` é configuração local do clone | Em cada clone novo: `git config core.hooksPath .githooks` |
