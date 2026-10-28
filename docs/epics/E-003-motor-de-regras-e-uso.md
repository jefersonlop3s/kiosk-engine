# E-003 — Motor de regras e contabilidade de uso

Estado: concluído em 2026-10-28 (semântica a confirmar pelo dono, ver PENDING). Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Resultado observável**: uma função pura `decidir(perfil, app, instante, consumo) → Permitir | Avisar{restante} | EncerrarApp{motivo} | BloquearSessão{motivo}`, mais o acumulador de consumo diário que a alimenta.

- **Inclui**:
  - Lista permitida/bloqueada (`App_Metadata.current_status`).
  - Limite diário por app e limite diário total por perfil.
  - Janelas de horário por dia da semana.
  - Precedência entre regras conflitantes e reinício do consumo à meia-noite local.
- **Exclui**: detectar processos e agir no SO ([E-004](E-004-agente-windows-enforcement.md)).
- **Pressuposto**: o relógio vem do sistema. Uma conta padrão não altera a hora do Windows ([ADR-002](../adr/ADR-002-identidade-por-conta-windows.md)); um salto de relógio detectado vira evento `TAMPER_SUSPECTED`, não ajuste silencioso.

## Origem e decisões

- [ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md) (Aceito): dois níveis de ação, avisos de 5 e 1 min, escopo passivo de `STALE_RULE`.
- [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) (Aceito): perfil e `usage_ledger`.
- [Arquitetura](../architecture/README.md), Seção II: ciclo `PENDING → ACTIVE → REVOKED` e `effective_until_timestamp`.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F1 | `decidir` para lista permitida/bloqueada e janelas de horário | [E-001](E-001-fundacao-crate-e-dados.md) F3 | Testes de tabela com instantes fixos |
| F2 | Contabilidade em `usage_ledger` (intervalos, reinício diário, fuso local, horário de verão) e limites | F1 | Testes de virada de dia e de duas regras no mesmo app |
| F3 | Precedência e conflitos (bloqueio vence permissão; regra `REVOKED` ou expirada é ignorada) | F2 | Testes negativos de conflito |

## Aceites

- O motor não chama nenhuma API do SO e é testável no macOS.
- **Controle negativo**: app em lista permitida, dentro da janela e com saldo nunca recebe `EncerrarApp`.

## Encerramento

Entregue em 2026-10-28; evidências na entrada de mesmo título no [CONCLUSION](../CONCLUSION.md). Pendências derivadas: GC-08 a GC-10 no [PENDING](../PENDING.md).
