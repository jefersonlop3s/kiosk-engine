# E-012 — Internacionalização (en-US, pt-BR, es-ES) e CLI em inglês

Estado: proposto em 2026-10-29 (pedido do dono). Aguarda o aceite da [ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md). Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Problema**: tudo está em português, inclusive os nomes de comandos, e não há como escolher o idioma.

**Resultado observável**:

- `kioskd status`, `kioskd install`, `kioskctl pair` e os demais comandos e opções em inglês. Os nomes em português seguem aceitos, com aviso de obsoleto, até a 0.1.0;
- avisos à criança no idioma da conta dela (en-US, pt-BR ou es-ES);
- mensagens do `kioskd` e do `kioskctl`, e nome e descrição do serviço no Windows, no idioma de quem usa;
- detalhe do log forense gravado como código neutro e traduzido ao mostrar.

- **Inclui**: o catálogo de mensagens, a escolha de idioma, a renomeação da CLI com apelidos, a tradução dos três idiomas e a atualização de fluxogramas, runbook e README.
- **Exclui**: traduzir os nomes de comandos (fixos em inglês); idiomas além dos três; a documentação em `docs/`, que segue em pt-BR conforme o [AGENTS.md](../../AGENTS.md).

## Origem e decisões

- Origem: pedido do dono em 2026-10-29.
- Decisão estrutural na [ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md) (proposta): catálogo no código com `match` exaustivo, ordem de escolha do idioma, apelidos em português até a 0.1.0, log forense com códigos.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F1 | CLI em inglês com apelidos em português e aviso de obsoleto; `install` registra o serviço como `service` e o serviço antigo (`servico`) segue subindo; fluxogramas, runbook e README nos nomes novos | ADR-010 aceita | Testes de CLI com nomes novos e apelidos; teste de documentação exigindo fluxograma para cada nome novo; serviço da 0.0.4 atualizado no CI Windows |
| F2 | Catálogo de mensagens e escolha de idioma (`KIOSK_LANG`, idioma do Windows, en-US); mensagens do `kioskd`, do `kioskctl` e do serviço traduzidas | F1 | Testes em cada idioma; build falha se faltar tradução |
| F3 | Avisos no idioma da conta da criança e detalhe do log forense como código | F2 | Aviso visto no Windows real numa conta em outro idioma; teste de que o log grava códigos |

## Aceites

- Um usuário que só lê inglês ou espanhol instala, cadastra e pareia sem ver português.
- **Controles negativos**:
  - idioma desconhecido cai em en-US, sem erro;
  - nenhum texto fica sem tradução (garantido pelo build);
  - os nomes de comandos não mudam com o idioma;
  - nenhum código da API ou do log forense é traduzido.
- Evidência por idioma: en-US, pt-BR e es-ES. A tradução es-ES é revisada por falante antes de sair do pré-lançamento.

## Encerramento

A preencher.
