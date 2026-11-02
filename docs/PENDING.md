# Pendências

Somente trabalho aberto. Detalhes nos [épicos](epics/README.md) e [gaps](gaps/README.md); sequência no [ROADMAP](ROADMAP.md). O que for fechado vai para o [CONCLUSION](CONCLUSION.md).

## Decisões aguardando o dono

| ID / documento | Estado | Próximo passo | Responsável |
|---|---|---|---|
| AGENTS global em `~/.claude/` | Criado pela sessão `3a84a5f7` fora do repositório; sem efeito | Apagar (a regra já está no AGENTS.md do projeto) | dono |

## GAPs de Código

Roteiro de validação no PC: [testes dos GAPs](runbooks/testes-dos-gaps.md) (versão 0.0.9).

Origem: [E-003](epics/E-003-motor-de-regras-e-uso.md) e [E-004](epics/E-004-agente-windows-enforcement.md) (núcleo SO-independente), revisão do próprio entregue em 2026-10-28.

| ID | Mecanismo | Medido? | Próximo passo |
|---|---|---|---|
| GC-15 | Modo LAN da ADR-005 com HTTPS: implementado o mínimo (`KIOSK_LAN=1` permite endereços privados RFC 1918). **Sem TLS ainda** — depende da adição de `tokio-rustls` ao crate. No PC real (2026-11-02) o `install` com `KIOSK_LAN=1` deixou o **serviço parado**: `KIOSK_LAN` não era gravada no ambiente do serviço (corrigido no `install` na 0.0.9, falta ver no PC real com `scripts/testes-gaps.ps1 -Fase GC15`) | Parcial (HTTP sem TLS; a LAN não foi alcançada) | Reinstalar com `KIOSK_LAN=1`, ver a porta responder na LAN; TLS quando `tokio-rustls` estiver disponível; rever pinning de certificado |
| GC-24 | Os executáveis não são assinados: o Smart App Control do Windows 11 bloqueia o `kioskctl.exe` ("uma política de Controle de Aplicativo bloqueou este arquivo"), mesmo com `Unblock-File`, na pasta extraída e na instalada. `kioskd.exe` e `kiosk-ui.exe` rodam (2026-11-02) | Sim: log do dono | Decidir a assinatura de código (certificado, custo, passo no `release.yml`) — ADR antes de executar |
| GC-25 | Sem o `kioskctl`, o administrador do PC não tem como ler o log forense (`log` e `usage` existem só no `kioskctl`; o `kioskd.log` traz só início e parada) | Sim: GC-21 e GC-20 de 2026-11-02 sem evidência no log | Avaliar `kioskd log` e `kioskd usage` locais (somente leitura, administrador elevado) |
| GC-23 | Hash encadeado (ADR-012) não encadeia: `hash_chain` devolve `None` quando o `prev_hash` anterior é `None`, e o primeiro evento tem `None`, então **todo** `prev_hash` fica NULL (visto só pela leitura de `src/security_logger_impl.rs`, sem teste). Os triggers de UPDATE e DELETE valem; o `kioskd log verify` não existe | Por leitura do código | Escrever teste que reprova; decidir na ADR-012 o valor do primeiro elo (por exemplo, string vazia) e implementar `log verify` |

## Épicos abertos

| Épico | Estado | Próximo passo |
|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) | em andamento | F1–F4 concluídas e CI verde em Ubuntu e Windows |
| [E-004](epics/E-004-agente-windows-enforcement.md) | em andamento | Validados em PC real: encerramento de app, avisos de 5 e 1 min, limite do app, bloqueio de sessão, serviço como SYSTEM aplicando regra (GC-16, 2026-11-02), avisos e tempo restante pela bandeja (GC-17, GC-22), logoff após 2 desbloqueios com 40 s de intervalo (GC-20). Faltam: ler o log forense no PC (GC-24, GC-25) e os critérios de aceite finais do épico |
| [E-005](epics/E-005-api-comando-e-pareamento.md) | em andamento | F1 a F3 concluídas (servidor, assinatura, regras, `modify_limit`, consultas de uso e log, CLI dos pais); sobram o modo LAN (GC-15) e ligar o agente ao serviço (GC-16) |
| [E-006](epics/E-006-app-mobile-administracao.md) | congelado | Condição de retomada no épico |
| [E-007](epics/E-007-onboarding-e-instalador.md) | em andamento | F2 concluída (serviço validado no CI e no PC real, inclusive após reinício; `instalar` põe a pasta no PATH). Próximo: F1, o assistente `kioskd configurar` |
| [E-008](epics/E-008-cliente-grafico-dos-pais.md) | em andamento | F1 concluída (listagem remota na API e no `kioskctl`); ADR do toolkit e do repositório antes da F2 |
| [E-009](epics/E-009-servico-linux.md) | congelado | Condição de retomada no épico |
| [E-010](epics/E-010-servico-macos.md) | congelado | Condição de retomada no épico |
| [E-011](epics/E-011-servico-android.md) | congelado | Condição de retomada no épico |

## Infraestrutura do harness

| Item | Próximo passo |
|---|---|
| `core.hooksPath` é configuração local do clone | Em cada clone novo: `git config core.hooksPath .githooks` |
