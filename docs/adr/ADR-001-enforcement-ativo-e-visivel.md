# ADR-001 — Enforcement ativo e visível para a criança

Status: Proposto. Data: 2026-10-27. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [arquitetura](../architecture/README.md) Seção I.2 e objetivo principal.

## Contexto

A [especificação de arquitetura](../architecture/README.md) (2026-09-06) traz duas regras incompatíveis com o produto definido em 2026-10-27, um controle parental doméstico:

1. **Seção I.2, "Regra da Discrepância"**: quando o `Task Killer` encontra processo rodando cuja regra diz o contrário, a ação é **passiva**: registra `STALE_RULE` e *não* encerra o processo. Com isso nenhum bloqueio de app ou de tempo seria aplicado de fato.
2. **Objetivo principal**: "invisibilidade ao usuário alvo". Para crianças e adolescentes isso significa bloqueio sem aviso nem explicação.

Origem: [revisão do estado inicial](../insights/2026-10-27-revisao-estado-inicial.md).

## Decisão

Escolha do dono em 2026-10-27, registrada aqui como proposta:

- **Bloqueio ativo em dois níveis**:
  - **Por aplicativo**: app fora da lista permitida, fora da janela de horário ou com limite diário esgotado recebe aviso e, ao fim do prazo, tem o processo encerrado.
  - **Por sessão**: esgotado o limite diário total do perfil ou fora da janela geral, a sessão do Windows é bloqueada.
- **Avisos antes de agir**: padrão de 5 min e 1 min, configurável por regra. Encerrar ou bloquear sem aviso só para app proibido ao ser aberto.
- **Visível**: agente na bandeja com tempo restante e motivo de cada bloqueio. Não pode ser fechado pela conta da criança.
- **A Regra da Discrepância muda de escopo**: continua passiva apenas para divergência *interna* do motor (estado lógico × físico sem regra vigente que justifique ação). Nesse caso registra `STALE_RULE` e alerta os pais. Regra ativa e válida é aplicada.
- **Toda ação de enforcement** gera evento no `Security_Log` ([E-002](../epics/E-002-log-forense.md)).

Alternativas rejeitadas:
- *Manter passivo* (só monitorar): não atende ao requisito de bloquear.
- *Só bloqueio de sessão*: impede a granularidade por aplicativo pedida.
- *Invisível*: a criança perde o aviso antes do encerramento, o que aumenta trabalho perdido e conflito; também dificulta a explicação do bloqueio.

## Consequências

- Ganho: o produto cumpre o objetivo (limitar uso) e a criança entende o que acontece.
- Custo: encerrar processos pode perder trabalho não salvo; os avisos mitigam, mas não eliminam.
- Custo: exige um componente de UI por sessão, além do serviço, o que aumenta a superfície de [E-004](../epics/E-004-agente-windows-enforcement.md).
- Risco: com o agente visível, a criança sabe que ele existe e tentará contorná-lo; as proteções contra adulteração passam a ser requisito ([E-004](../epics/E-004-agente-windows-enforcement.md) F4).
- Reversível: a política de ação por regra (avisar/encerrar/bloquear) fica configurável no dado, não no código.
- Validação: só em Windows real; teste negativo de que um app permitido nunca é encerrado.

## Referências e evolução

- Épicos que consomem: [E-003](../epics/E-003-motor-de-regras-e-uso.md), [E-004](../epics/E-004-agente-windows-enforcement.md), [E-002](../epics/E-002-log-forense.md).
- Revisa (não apaga) a Seção I.2 e o objetivo de invisibilidade da [arquitetura](../architecture/README.md).
