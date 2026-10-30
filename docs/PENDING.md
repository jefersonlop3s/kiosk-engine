# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Roteiro de validação no PC: [testes dos GAPs](runbooks/testes-dos-gaps.md) (versão 0.0.8).

Origem: [E-003](epics/E-003-motor-de-regras-e-uso.md) e [E-004](epics/E-004-agente-windows-enforcement.md) (núcleo SO-independente), revisão do próprio entregue em 2026-10-28.

| ID | Mecanismo | Medido? | Próximo passo |
|---|---|---|---|
| GC-15 | Modo LAN da ADR-005 com HTTPS: implementado o mínimo (`KIOSK_LAN=1` permite endereços privados RFC 1918). **Sem TLS ainda** — depende da adição de `tokio-rustls` ao crate | Parcial (HTTP sem TLS) | Adicionar TLS quando `tokio-rustls` estiver disponível; rever pinning de certificado |
| GC-16 | Serviço do Windows validado no PC do dono: instalação (2026-10-29) e **retorno sozinho depois de reiniciar o Windows** (novo PID, em execução, início automático). Falta ver o agente, rodando como SYSTEM, aplicar uma regra na conta da criança (por exemplo um limite curto) | Por execução no PC real | E-004 F1/F4, E-007 F2 |
| GC-17 | Os avisos à criança ainda usam `WTSSendMessageW` (caixa de mensagem). A bandeja já existe e é iniciada e supervisionada pelo serviço (#21 e #22); falta os avisos e o tempo restante passarem por ela, com a caixa de mensagem como reserva se ela não confirmar, e ver o desenho no PC | Por decisão (provisório) | E-004 F3 (etapa 4) |
| GC-22 | Hash encadeado (ADR-012) não encadeia: `hash_chain` devolve `None` quando o `prev_hash` anterior é `None`, e o primeiro evento tem `None`, então **todo** `prev_hash` fica NULL (visto só pela leitura de `src/security_logger_impl.rs`, sem teste). Os triggers de UPDATE e DELETE valem; o `kioskd log verify` não existe | Por leitura do código | Escrever teste que reprova; decidir na ADR-012 o valor do primeiro elo (por exemplo, string vazia) e implementar `log verify` |
| GC-20 | Logoff após 2 desbloqueios não ocorreu no teste real (2026-10-29): houve dois bloqueios por `LIMITE_DIARIO_TOTAL` com 30 s de intervalo e nenhuma ação nem erro registrados depois, até o limite ser removido 7 min mais tarde | Sim: log do teste do dono | Repetir com intervalo de 40 s entre as entradas e a saída da janela do `servir`; se reproduzir, investigar a detecção da sessão reconectada (`WTSSessionInfoEx`) |
| GC-21 | Visto no PC real (2026-10-29): a tentativa de uma conta comum aparece em `kioskctl log --profile`, o limite de 5 por minuto vale e o administrador elevado gera o código. **Falta ver** a recusa com o serviço parado (`Stop-Service kiosk`): a mensagem deve dizer que a tentativa **não** foi registrada. O `kioskd.exe` inteiro como a conta padrão não roda no CI (`Start-Process -Credential` trava nos runners): só com `KIOSK_TESTE_PROCESSO_COMO_CONTA=1` | Por execução no PC real | [ADR-011](adr/ADR-011-rastro-de-tentativas-sem-privilegio.md), E-004 F4 |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F4 concluídas e CI verde em Ubuntu e Windows |
| [E-004](epics/E-004-agente-windows-enforcement.md) | em andamento | Validados em PC real: encerramento de app, avisos de 5 e 1 min, limite do app, bloqueio de sessão. `AGENT_STOPPED` por batimento visto na 0.0.7. Bandeja: canal, `kiosk-ui.exe` e supervisão prontos e provados no CI com o serviço real (#21, #22); falta a etapa 4 (avisos e estado pela bandeja), o logoff (GC-20) e ver o agente como SYSTEM (GC-16) no PC |
| [E-005](epics/E-005-api-comando-e-pareamento.md) | em andamento | F1 a F3 concluídas (servidor, assinatura, regras, `modify_limit`, consultas de uso e log, CLI dos pais); sobram o modo LAN (GC-15) e ligar o agente ao serviço (GC-16) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |
| [E-007](epics/E-007-onboarding-e-instalador.md) | em andamento | F2 concluída (serviço validado no CI e no PC real, inclusive após reinício; `instalar` põe a pasta no PATH). Próximo: F1, o assistente `kioskd configurar` |
| [E-008](epics/E-008-cliente-grafico-dos-pais.md) | proposto | F1: listagem remota de perfis, apps e regras na API; ADR do toolkit antes da F2 |
| [E-009](epics/E-009-servico-linux.md) | congelado | Condição de retomada no épico |
| [E-010](epics/E-010-servico-macos.md) | congelado | Condição de retomada no épico |
| [E-011](epics/E-011-servico-android.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| `core.hooksPath` é configuração local do clone | Em cada clone novo: `git config core.hooksPath .githooks` |
