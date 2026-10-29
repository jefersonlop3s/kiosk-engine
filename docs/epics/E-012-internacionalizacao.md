# E-012 — Internacionalização (en-US, pt-BR, es-ES) e CLI em inglês

Estado: em andamento desde 2026-10-29 ([ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md) aceita com ajustes). Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Problema**: tudo está em português, inclusive os nomes de comandos, e não há como escolher o idioma.

**Resultado observável**:

- `kioskd status`, `kioskd install`, `kioskctl pair` e os demais comandos e opções em inglês, que é a convenção da documentação. Os nomes em português continuam valendo para sempre, sem aviso, e ficam listados numa página de equivalentes;
- a API fala só inglês, inclusive o corpo do pareamento;
- avisos à criança no idioma da conta dela (en-US, pt-BR ou es-ES);
- mensagens do `kioskd` e do `kioskctl`, e nome e descrição do serviço no Windows, no idioma de quem usa;
- detalhe do log forense gravado como código neutro e traduzido ao mostrar.

- **Inclui**: o catálogo de mensagens, a escolha de idioma, a renomeação da CLI com apelidos, a tradução dos três idiomas e a atualização de fluxogramas, runbook e README.
- **Exclui**: traduzir os nomes de comandos (fixos em inglês); idiomas além dos três; a documentação em `docs/`, que segue em pt-BR conforme o [AGENTS.md](../../AGENTS.md).

## Origem e decisões

- Origem: pedido do dono em 2026-10-29.
- Decisão estrutural na [ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md), aceita com ajustes do dono: português e inglês permanentes na CLI; API só em inglês; documentação em inglês com página de equivalentes; texto no idioma do perfil de quem o vê; catálogo no código com `match` exaustivo; log forense com códigos.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F1 | CLI em inglês, com os nomes em português equivalentes e permanentes; API só em inglês (corpo do pareamento); `install` registra o serviço como `service`, e o serviço antigo (`servico`) segue subindo; fluxogramas, runbook e README em inglês; página de equivalentes em português | ADR-010 aceita | Testes de CLI nas duas línguas; teste que exige cada equivalente na página e fluxograma para cada comando em inglês |
| F2 | Catálogo de mensagens e escolha de idioma (`KIOSK_LANG`, idioma do Windows, en-US); mensagens do `kioskd`, do `kioskctl` e do serviço traduzidas | F1 | Testes em cada idioma; build falha se faltar tradução |
| F3 | Avisos no idioma da conta da criança e detalhe do log forense como código | F2 | Aviso visto no Windows real numa conta em outro idioma; teste de que o log grava códigos |

## Aceites

- Um usuário que só lê inglês ou espanhol instala, cadastra e pareia sem ver português.
- **Controles negativos**:
  - idioma desconhecido cai em en-US, sem erro;
  - nenhum texto fica sem tradução (garantido pelo build);
  - os nomes de comandos não mudam com o idioma de exibição; português e inglês valem sempre;
  - nenhum código da API ou do log forense é traduzido.
- Evidência por idioma: en-US, pt-BR e es-ES. A tradução es-ES é revisada por falante antes de sair do pré-lançamento.

## Encerramento

A preencher.
