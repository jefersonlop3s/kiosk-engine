# Roadmap

Direção e sequência, sem duplicar a [fila de pendências](PENDING.md). Datas não foram decididas; as sprints abaixo são ordem de execução, não prazo.

## Objetivo

Controle parental doméstico para Windows: os pais definem quais programas cada criança pode usar, por quanto tempo e em quais horários. O agente avisa e aplica as regras de forma visível ([ADR-001](adr/ADR-001-enforcement-ativo-e-visivel.md), [ADR-002](adr/ADR-002-identidade-por-conta-windows.md)). Um app mobile de administração vem depois.

## Sequência proposta

```
E-001 Fundação ──┬─> E-002 Log forense ──┐
                 └─> E-003 Motor de regras ─┴─> E-004 Agente Windows ──> E-005 API + pareamento ──> E-006 App mobile (congelado)
                                                      └──────────────┬──────────┘
                                                                     └─> E-007 Onboarding e instalador
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
| — | [E-006](epics/E-006-app-mobile-administracao.md) | Retomada após E-005 estável |

Marco do MVP: fim da sprint 5. Nela, um PC Windows com duas crianças tem regras aplicadas de verdade, configuradas localmente na conta de admin.

## Condições de retomada

- E-006 congelado até E-005 F3.
- E-007 em execução: ADR-007 (forma do assistente), ADR-008 (formato do instalador) e ADR-009 (origem da lista de programas) aceitas em 2026-10-29. As ADRs de detecção, IPC, rede e assinatura (ADR-003 a ADR-006) já foram aceitas.
