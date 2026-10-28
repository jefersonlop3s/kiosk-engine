# E-006 — App mobile de administração

Estado: **congelado**. Responsável: a definir. Repositório: a definir (provavelmente separado).

## Resultado e escopo

App para os pais administrarem todo o sistema pelo celular: perfis, regras, consumo do dia, log de eventos e alertas (`TAMPER_SUSPECTED`, `AGENT_STOPPED`).

## Origem e decisões

- Requisito do dono em 2026-10-27: "o sistema ganhará futuramente um app mobile".
- Consome a API de [E-005](E-005-api-comando-e-pareamento.md); não fala com o banco diretamente.

## Condição de retomada

[E-005](E-005-api-comando-e-pareamento.md) F3 concluída e estável. Antes disso, decidir em ADR: plataforma (iOS, Android ou multiplataforma), repositório e como o app alcança o PC fora da rede local (depende da ADR de rede de E-005).

## Fases, aceites e encerramento

A definir na retomada.
