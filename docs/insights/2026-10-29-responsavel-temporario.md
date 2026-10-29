# Insight — Responsável temporário por conta infantil

Data: 2026-10-29. Autoria: ideia do dono (Jeferson Lopes), registrada pelo agente. Escopo: delegar, por tempo limitado, parte da autoridade sobre uma conta monitorada a quem não é administrador.

## Evidências e hipóteses

- **Hoje** (decisão do dono, mesma data): os responsáveis são os administradores da máquina. Todos gerem todas as contas monitoradas, e só administrador gera código de pareamento ([E-008](../epics/E-008-cliente-grafico-dos-pais.md)).
- **Ideia**: uma conta **não administrativa** recebe a função de responsável temporário por uma conta infantil, por exemplo avós, babá ou um tio que cuida da criança por alguns dias. Essa conta pode **liberar tempo de uso e de app por até 5 dias**.
- **Ao fim do período**: as regras criadas pelo responsável temporário são **revisadas** por um administrador, que as torna definitivas ou as revoga. A revogação volta ao padrão de antes delas.
- O próprio dono registrou que o raciocínio ainda não fecha e que não quer transformar algo simples em complexo. Por isso fica como insight, sem épico.

## Alternativas e crítica

- **Delegação com prazo** (a ideia): resolve a ausência dos pais sem dar administrador a terceiros. Custos:
  - modelo de dados novo (quem delegou, para quem, sobre qual perfil, até quando);
  - credencial de pareamento com escopo e validade;
  - estado "pendente de revisão" para as regras criadas no período;
  - restauração exata do estado anterior.
- **Sem delegação**: o administrador libera tempo remotamente pelo cliente ([E-008](../epics/E-008-cliente-grafico-dos-pais.md)). Não exige nada novo, mas depende de os pais estarem disponíveis.
- **Regras com validade, sem delegação**: o administrador cria de antemão regras com `--ate` (já existe: `effective_until`) para o período em que vai estar fora. Cobre parte do caso com o que já há.
- Pontos em aberto que podem refutar a ideia:
  - o que impede o responsável temporário de liberar tudo nos 5 dias;
  - como a revisão acontece se nenhum administrador a fizer;
  - como "voltar ao padrão" convive com regras alteradas pelos administradores no mesmo período.

## Encaminhamento

- Decidido: nada. Permanece hipótese.
- Pode virar épico (e ADR, porque mexe na autoridade da [ADR-006](../adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md)) quando o dono fechar o raciocínio. Ponto de partida natural: depois do [E-008](../epics/E-008-cliente-grafico-dos-pais.md), que traz a administração remota.
