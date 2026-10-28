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
| GC-15 | O modo LAN da ADR-005 (HTTPS com certificado fixado) não existe; a API recusa qualquer endereço fora de loopback e Tailscale | Por decisão (fail-closed) | Implementar TLS se o dono quiser LAN; senão manter só Tailscale |
| GC-16 | O agente roda dentro do `kioskd servir` no Windows, mas o `kioskd` ainda é um processo de console: falta registrá-lo como serviço do Windows (SCM, conta SYSTEM, recuperação automática) e proteger a pasta do banco | Por leitura | E-004 F1/F4 |
| GC-17 | Os avisos à criança usam `WTSSendMessageW` (caixa de mensagem na sessão) como solução provisória; a ADR-004 prevê a UI de bandeja com tempo restante | Por decisão (provisório) | E-004 F3 |
| GC-18 | Bloquear a sessão usa `WTSDisconnectSession` (volta à tela de entrada), não `LockWorkStation`, que só vale para a sessão de quem chama | Por leitura | Confirmar o comportamento em Windows interativo (E-004 F2) |
| GC-19 | O pré-push do harness (`.harness/validation.py`) compara o SHA de cada ref enviada com o `HEAD`; uma tag anotada envia o SHA do objeto da tag, não o do commit, e é sempre recusada. Contorno atual: tags leves, ou a tag criada pelo workflow de release | Sim: `16501a3` (tag) × `76d8b8f` (commit) | Corrigir no sovereign-harness (`git rev-parse <sha>^{commit}`) e reinstalar |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F4 concluídas e CI verde em Ubuntu e Windows |
| [E-004](epics/E-004-agente-windows-enforcement.md) | em andamento | Adaptador Win32 e agente ligado ao `kioskd` prontos; faltam serviço do Windows (SCM), UI de bandeja e teste em sessão interativa |
| [E-005](epics/E-005-api-comando-e-pareamento.md) | em andamento | F1 a F3 concluídas (servidor, assinatura, regras, `modify_limit`, consultas de uso e log, CLI dos pais); sobram o modo LAN (GC-15) e ligar o agente ao serviço (GC-16) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| `core.hooksPath` é configuração local do clone | Em cada clone novo: `git config core.hooksPath .githooks` |
