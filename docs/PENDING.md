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
| GC-16 | Implementado: `kioskd instalar`, `desinstalar` e `estado` e o modo serviço (ADR-008, etapa 1), validados no CI Windows (instalação, reinício após queda, conta padrão sem poder parar o serviço nem ler os dados, desinstalação). Instalado e em execução no PC do dono (2026-10-29). Falta ver no PC real: o serviço sobe sozinho após reiniciar o Windows e o agente age como SYSTEM | Por execução no PC real | E-004 F1/F4, E-007 F2 |
| GC-17 | Os avisos à criança usam `WTSSendMessageW` (caixa de mensagem na sessão) como solução provisória; a ADR-004 prevê a UI de bandeja com tempo restante | Por decisão (provisório) | E-004 F3 |
| GC-20 | Logoff após 2 desbloqueios não ocorreu no teste real (2026-10-29): houve dois bloqueios por `LIMITE_DIARIO_TOTAL` com 30 s de intervalo e nenhuma ação nem erro registrados depois, até o limite ser removido 7 min mais tarde | Sim: log do teste do dono | Repetir com intervalo de 40 s entre as entradas e a saída da janela do `servir`; se reproduzir, investigar a detecção da sessão reconectada (`WTSSessionInfoEx`) |
| GC-19 | O pré-push do harness (`.harness/validation.py`) compara o SHA de cada ref enviada com o `HEAD`; uma tag anotada envia o SHA do objeto da tag, não o do commit, e é sempre recusada. Contorno atual: tags leves, ou a tag criada pelo workflow de release | Sim: `16501a3` (tag) × `76d8b8f` (commit) | Corrigir no sovereign-harness (`git rev-parse <sha>^{commit}`) e reinstalar |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F4 concluídas e CI verde em Ubuntu e Windows |
| [E-004](epics/E-004-agente-windows-enforcement.md) | em andamento | Validados em PC real com conta de criança (2026-10-29): encerramento de app, avisos de 5 e 1 min, encerramento ao esgotar o limite do app e bloqueio de sessão; falta o logoff (GC-20), o serviço do Windows (GC-16) e a UI de bandeja (GC-17) |
| [E-005](epics/E-005-api-comando-e-pareamento.md) | em andamento | F1 a F3 concluídas (servidor, assinatura, regras, `modify_limit`, consultas de uso e log, CLI dos pais); sobram o modo LAN (GC-15) e ligar o agente ao serviço (GC-16) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |
| [E-007](epics/E-007-onboarding-e-instalador.md) | em andamento | F2 (serviço do Windows) implementada e validada no CI; falta o reinício no PC real (GC-16). Depois: F1, o assistente `kioskd configurar` |
| [E-008](epics/E-008-cliente-grafico-dos-pais.md) | proposto | F1: listagem remota de perfis, apps e regras na API; ADR do toolkit antes da F2 |
| [E-009](epics/E-009-servico-linux.md) | congelado | Condição de retomada no épico |
| [E-010](epics/E-010-servico-macos.md) | congelado | Condição de retomada no épico |
| [E-011](epics/E-011-servico-android.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| `core.hooksPath` é configuração local do clone | Em cada clone novo: `git config core.hooksPath .githooks` |
