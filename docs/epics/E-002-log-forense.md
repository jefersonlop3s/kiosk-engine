# E-002 — Log forense (`Security_Log`)

Estado: em andamento (F1 concluída em 2026-10-28). Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Resultado observável**: todo comando recebido e toda ação de enforcement geram exatamente um evento no `Security_Log`, gravado na mesma transação da mudança de estado que descreve. Os pais conseguem responder "o que aconteceu, quando e por quê".

- **Inclui**:
  - Tipos de evento das camadas da API: `AUTH_FAIL`, `AUTHZ_FAIL`, `VALIDATION_FAIL`, `SUCCESS`.
  - Eventos do motor: `STALE_RULE`.
  - Eventos de enforcement ([ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md)): `WARNING_SHOWN`, `APP_TERMINATED`, `SESSION_LOCKED`, `AGENT_STOPPED`, `TAMPER_SUSPECTED`.
  - Garantia de que eventos são só inseridos (append-only); consulta por período e por perfil.
- **Exclui**: envio remoto dos logs (depende de [E-005](E-005-api-comando-e-pareamento.md)).
- **Pressuposto**: os dados são de crianças. Coletar o mínimo (nome do processo, horário, ação), sem conteúdo de tela, teclas ou URLs, e manter tudo local. A retenção precisa ser definida antes de F3.

## Origem e decisões

- [Arquitetura](../architecture/README.md), Seções I.1, I.3 e III.
- [ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md) (Aceito): novos tipos de evento.
- **Hipótese em aberto**: imutabilidade por hash encadeado (cada evento carrega o hash do anterior). Exige ADR quando F2 for executada; hoje não há decisão.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F1 | Enum de eventos completo + gravação na mesma transação da mudança de estado | [E-001](E-001-fundacao-crate-e-dados.md) F3 | Teste: rollback da mudança também descarta o evento |
| F2 | Proteção contra UPDATE/DELETE (trigger SQLite que aborta) e, se aprovada por ADR, hash encadeado | F1 | Teste negativo: UPDATE é recusado |
| F3 | Política de retenção e consulta por período e por perfil | F1, decisão do dono sobre retenção | Teste com dados de vários dias |

## Aceites

- Não existe caminho de código que altere o estado sem gravar o evento (verificado por teste em cada operação pública).
- `rejection_reason_detail` fica só no log; a resposta externa é genérica (arquitetura, Camada 5).

## Encerramento

A preencher.
