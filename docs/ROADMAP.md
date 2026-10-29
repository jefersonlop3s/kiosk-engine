# Roadmap

Direção e sequência, sem duplicar a [fila de pendências](PENDING.md). Datas não foram decididas; as sprints abaixo são ordem de execução, não prazo.

## Objetivo

Controle parental doméstico para Windows: os pais definem quais programas cada criança pode usar, por quanto tempo e em quais horários. O agente avisa e aplica as regras de forma visível ([ADR-001](adr/ADR-001-enforcement-ativo-e-visivel.md), [ADR-002](adr/ADR-002-identidade-por-conta-windows.md)). Um app mobile de administração vem depois.

## Sequência proposta

```
E-001 Fundação ──┬─> E-002 Log forense ──┐
                 └─> E-003 Motor de regras ─┴─> E-004 Agente Windows ──> E-005 API + pareamento ──> E-006 App mobile (congelado)
                                                      └──────────────┬──────────┘
                                                                     ├─> E-007 Onboarding e instalador
                                                                     └─> E-008 Cliente gráfico dos pais (Windows, Linux, macOS)

E-004 + E-007 maduros e E-008 maduro ──> E-009 Serviço Linux, E-010 Serviço macOS, E-011 Serviço Android (congelados)
```

| Sprint | Épico/fases | Resultado ao fim |
|---|---|---|
| 1 | [E-001](epics/E-001-fundacao-crate-e-dados.md) F1–F2 (+ aceite das ADR-001/002) | Crate compila; banco criado por migrações |
| 2 | E-001 F3–F4, [E-002](epics/E-002-log-forense.md) F1 | CRUD transacional testado; gates ativos; evento gravado na mesma transação |
| 3 | [E-003](epics/E-003-motor-de-regras-e-uso.md) F1–F3, E-002 F2 | Motor decide e contabiliza uso; log append-only |
| 4 | [E-004](epics/E-004-agente-windows-enforcement.md) F0–F2 | Serviço no Windows encerra app bloqueado e bloqueia sessão |
| 5 | E-004 F3–F4, E-002 F3 | Bandeja visível com avisos; antiadulteração |
| 6 | [E-005](epics/E-005-api-comando-e-pareamento.md) F1–F3 | Pais administram via CLI pareado |
| 7 | [E-007](epics/E-007-onboarding-e-instalador.md) F1–F3 | Primeira configuração guiada, `kioskd` como serviço do Windows e instalador |
| 8 | [E-008](epics/E-008-cliente-grafico-dos-pais.md) F1–F4 | Pais administram pelo app gráfico em Windows, Linux e macOS, na Tailscale |
| — | [E-006](epics/E-006-app-mobile-administracao.md) | Retomada após E-005 estável |
| — | [E-009](epics/E-009-servico-linux.md), [E-010](epics/E-010-servico-macos.md), [E-011](epics/E-011-servico-android.md) | Serviço em outras plataformas, depois do `kioskd` Windows e do cliente gráfico maduros |

## Maturidade por épico

Critério: cada fase do épico vale 100% concluída, 50% parcial e 0% não iniciada. A porcentagem é a média das fases, arredondada para múltiplos de 5. Uma fase só conta como concluída com a evidência exigida no épico; o que depende de Windows real e ainda não foi visto lá conta como parcial. Atualizada a cada entrega que muda o estado de uma fase.

| Épico | Estado | Fases | % | Falta |
|---|---|---|---|---|
| [E-001](epics/E-001-fundacao-crate-e-dados.md) Fundação e dados | fases concluídas | F1–F4 concluídas | 100% | — |
| [E-002](epics/E-002-log-forense.md) Log forense | concluído | F1–F3 concluídas | 100% | Hash encadeado, adiado pelo dono (fora das fases) |
| [E-003](epics/E-003-motor-de-regras-e-uso.md) Motor e uso | concluído | F1–F3 concluídas | 100% | — |
| [E-004](epics/E-004-agente-windows-enforcement.md) Agente Windows | em andamento | F0 e F1 concluídas; F2, F3 e F4 parciais | 70% | Agente como SYSTEM na conta da criança (GC-16), logoff (GC-20), bandeja no lugar das caixas de mensagem (GC-17), batimento da UI |
| [E-005](epics/E-005-api-comando-e-pareamento.md) API e pareamento | concluído nas fases | F1–F3 concluídas | 100% | Modo LAN com HTTPS (GC-15, fora das fases) |
| [E-006](epics/E-006-app-mobile-administracao.md) App mobile dos pais | congelado | fases a definir | 0% | Condição de retomada |
| [E-007](epics/E-007-onboarding-e-instalador.md) Onboarding e instalador | em andamento | F2 concluída; F1 e F3 não iniciadas | 35% | Assistente `kioskd configurar`, `.msi` |
| [E-008](epics/E-008-cliente-grafico-dos-pais.md) Cliente gráfico dos pais | proposto | F1–F4 não iniciadas | 0% | Listagem remota na API (F1); a exigência de administrador no pareamento já foi antecipada |
| [E-009](epics/E-009-servico-linux.md) Serviço Linux | congelado | fases a definir | 0% | Condição de retomada |
| [E-010](epics/E-010-servico-macos.md) Serviço macOS | congelado | fases a definir | 0% | Condição de retomada |
| [E-011](epics/E-011-servico-android.md) Serviço Android | congelado | fases a definir | 0% | Condição de retomada |

Leitura rápida: a base comum (dados, log, motor, API) está pronta; o produto no Windows depende de fechar o E-004 e o E-007; o resto vem depois.

Marco do MVP: fim da sprint 5. Nela, um PC Windows com duas crianças tem regras aplicadas de verdade, configuradas localmente na conta de admin.

## Condições de retomada

- E-006 congelado até E-005 F3.
- E-009, E-010 e E-011 congelados até: `kioskd` no Windows maduro (E-004 e E-007 concluídos, sem GAP de enforcement aberto, uso real sem regressão) **e** cliente gráfico maduro (E-008 F4). Cada um revisa a ADR-002 numa ADR nova e traz o próprio CI/CD.
- E-007 em execução: ADR-007 (forma do assistente), ADR-008 (formato do instalador) e ADR-009 (origem da lista de programas) aceitas em 2026-10-29. As ADRs de detecção, IPC, rede e assinatura (ADR-003 a ADR-006) já foram aceitas.
