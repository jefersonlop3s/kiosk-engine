# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| Hash encadeado no `Security_Log` | Adiado pelo dono em 2026-10-28; retomar só com ADR. O trigger cobre UPDATE/DELETE via SQL, não edição direta do arquivo | dono |
| Semântica do motor (E-003) a confirmar: (a) app `MONITORED` **sem regra vigente é permitido** (o oposto, "só roda o que está na lista", exigiria modo allowlist por perfil); (b) dia da semana **sem janela cadastrada = sem restrição de horário** (bloquear o dia inteiro hoje só com uma janela mínima); (c) `WHITELISTED` e `IGNORE` isentos de regras de app | Aprovar ou pedir o modo oposto; é mudança local em `src/motor.rs` | dono |
| [ADR-004](adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md), [ADR-005](adr/ADR-005-rede-da-api-de-comando.md) | Propostas em 2026-10-28; ADR-003 e ADR-006 aceitas pelo dono na mesma data | Aceitar, rejeitar ou pedir alternativa; o aceite desbloqueia E-004 F3/F4 e E-005 F1 | dono |
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Origem: [E-003](epics/E-003-motor-de-regras-e-uso.md) e [E-004](epics/E-004-agente-windows-enforcement.md) (núcleo SO-independente), revisão do próprio entregue em 2026-10-28.

| ID | Mecanismo | Medido? | Próximo passo |
|---|---|---|---|
| GC-08 | Avisos de 5 e 1 min são constante no código (`AVISOS_SEGUNDOS`); a ADR-001 os quer configuráveis por regra e `active_rules` não tem coluna para isso | Por leitura | Migração + campo em `ActiveRule` quando o E-004 F2 definir a UX do aviso |
| GC-09 | `consumo_do_dia` soma os apps do perfil como "total": dois apps em uso simultâneo contam em dobro contra o limite total | Por leitura | [ADR-004](adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md), [ADR-005](adr/ADR-005-rede-da-api-de-comando.md) | Propostas em 2026-10-28; ADR-003 e ADR-006 aceitas pelo dono na mesma data | Aceitar, rejeitar ou pedir alternativa; o aceite desbloqueia E-004 F3/F4 e E-005 F1 | dono |
| GC-11 | O agente considera a sessão do perfil "viva" quando há qualquer processo do SID na lista; a [ADR-003](adr/ADR-003-deteccao-de-processos-e-tempo-de-uso.md) propõe `WTSEnumerateSessions`. Enquanto isso, sessão bloqueada ou sem processos não conta e o total segue sendo a soma dos apps (GC-09) | Por leitura | Trocar a fonte do "total" quando a ADR-003 for aceita (tabela `session_usage`) |
| GC-12 | Bloquear a sessão (lock) pode ser desfeito pela criança com a própria senha; o agente rebloqueia a cada 30 s. Se o lock basta ou se é preciso encerrar a sessão (logoff) não foi decidido | Por leitura | Decidir em E-004 F2 com teste em Windows real; muda só o adaptador `Sistema::bloquear_sessao` |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F4 concluídas; falta o encerramento do épico (conferir o CI no GitHub) |
| [E-004](epics/E-004-agente-windows-enforcement.md) | em andamento | Núcleo SO-independente pronto; F0–F4 dependem de Windows real e do aceite das ADR-003/004 |
| [E-005](epics/E-005-api-comando-e-pareamento.md) | proposto | Depende do aceite das ADR-005/006 |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| Primeira execução do CI (Ubuntu e Windows) | Conferir no GitHub; não foi observada nesta sessão |
| `core.hooksPath` é configuração local do clone | Em cada clone novo: `git config core.hooksPath .githooks` |
