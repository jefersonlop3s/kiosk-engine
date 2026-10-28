# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| Hash encadeado no `Security_Log` | Adiado pelo dono em 2026-10-28; retomar só com ADR. O trigger cobre UPDATE/DELETE via SQL, não edição direta do arquivo | dono |
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Origem: [E-003](epics/E-003-motor-de-regras-e-uso.md) e [E-004](epics/E-004-agente-windows-enforcement.md) (núcleo SO-independente), revisão do próprio entregue em 2026-10-28.

| ID | Mecanismo | Medido? | Próximo passo |
|---|---|---|---|
| GC-08 | Avisos de 5 e 1 min são constante no código (`AVISOS_SEGUNDOS`); a ADR-001 os quer configuráveis por regra e `active_rules` não tem coluna para isso | Por leitura | Migração + campo em `ActiveRule` quando o E-004 F2 definir a UX do aviso |
| GC-13 | Pareamento bloqueia por 5 min depois de 10 códigos errados (`MAX_FALHAS_PAREAMENTO`); quem está na rede pode errar de propósito e atrasar o pareamento de um dispositivo novo (não afeta dispositivos já pareados) | Por leitura | Aceitável no MVP; revisar se virar problema real (limite por IP depende do servidor, E-005 F1) |
| GC-14 | A API cobre `add_rule`, `activate_rule`, `revoke_rule` e o pareamento; faltam `modify_limit` (a arquitetura o cita) e as consultas de uso e de log | Por leitura | E-005 F3 |
| GC-15 | O modo LAN da ADR-005 (HTTPS com certificado fixado) não existe; a API recusa qualquer endereço fora de loopback e Tailscale | Por decisão (fail-closed) | Implementar TLS se o dono quiser LAN; senão manter só Tailscale |
| GC-16 | `kioskd servir` sobe só a API; o laço do agente (`Agente::tick` com o adaptador `Sistema` do Windows) não está ligado ao serviço | Por leitura | E-004 F1 (adaptador Win32) |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F4 concluídas; falta o encerramento do épico (conferir o CI no GitHub) |
| [E-004](epics/E-004-agente-windows-enforcement.md) | em andamento | Núcleo SO-independente pronto; F0–F4 dependem de Windows real e do aceite das ADR-003/004 |
| [E-005](epics/E-005-api-comando-e-pareamento.md) | em andamento | F1 a F3 prontas (servidor, assinatura, ciclo criar → ativar → revogar e CLI dos pais); faltam `modify_limit` e as consultas (GC-14) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| Primeira execução do CI (Ubuntu e Windows) | Conferir no GitHub; não foi observada nesta sessão |
| `core.hooksPath` é configuração local do clone | Em cada clone novo: `git config core.hooksPath .githooks` |
