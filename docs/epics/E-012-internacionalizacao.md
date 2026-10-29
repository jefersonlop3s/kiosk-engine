# E-012 — Internacionalização (en-US, pt-BR, es-ES) e CLI em inglês

Estado: concluído em 2026-10-29 (início no mesmo dia) ([ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md) aceita com ajustes). Responsável: a definir. Repositório: kiosk.

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

## Progresso

- **F1** (2026-10-29), concluída, CI verde em Ubuntu e Windows:
  - `src/cli.rs` traz as tabelas de equivalentes, e os executáveis traduzem os argumentos para inglês antes de interpretar, então há um caminho só;
  - uso, fluxogramas, runbook e README em inglês;
  - página [comandos em português](../runbooks/comandos-em-portugues.md), conferida contra as tabelas por teste;
  - corpo do pareamento em inglês;
  - serviço registrado como `service`, com `servico` equivalente.

- **F2** (2026-10-29), implementada:
  - `src/i18n.rs`: `Idioma` (en-US, pt-BR, es-ES), escolha por `KIOSK_LANG`, idioma do sistema e en-US, e o catálogo de mensagens gerado por macro. Cada mensagem exige os três idiomas, ou o build falha;
  - mensagens do `kioskd`, do `kioskctl`, do cliente, do serviço (instalação, remoção, estado, log do serviço, nome e descrição no Windows) e o texto de uso nos três idiomas;
  - dias da semana aceitos em inglês, português e espanhol; palavra de confirmação por idioma (`DELETE`, `APAGAR`, `BORRAR`);
  - o `install` grava `KIOSK_LANG` no serviço.
- **F1 e F2 validadas no PC real do dono** (0.0.5, 2026-10-29): comandos em inglês e em português com o mesmo resultado, mensagens vistas em pt-BR, en-US e es-ES, serviço com o idioma de quem o instalou, atualização do serviço pelo `install`.
- **F3** (2026-10-29), implementada e aceita pelo dono com simulação no lugar de nova checagem à mão (o dono já viu os avisos com os horários no PC real):
  - avisos à criança nos três idiomas (`textos`), no idioma da conta dela, lido do perfil carregado (`HKEY_USERS\<SID>`);
  - o detalhe do log forense do enforcement vira código JSON (`detalhes`), traduzido na hora por `kioskctl log`. Linhas antigas aparecem como estão;
  - desvios e o que ficou de fora estão na seção "Implementação e desvios" da [ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md).

## Aceites

- Um usuário que só lê inglês ou espanhol instala, cadastra e pareia sem ver português.
- **Controles negativos**:
  - idioma desconhecido cai em en-US, sem erro;
  - nenhum texto fica sem tradução (garantido pelo build);
  - os nomes de comandos não mudam com o idioma de exibição; português e inglês valem sempre;
  - nenhum código da API ou do log forense é traduzido.
- Evidência por idioma: en-US, pt-BR e es-ES. A tradução es-ES é revisada por falante antes de sair do pré-lançamento.

## Encerramento

2026-10-29. As três fases entregues. F1 e F2 foram vistas no PC real (0.0.5). A F3 não foi vista com a conta da criança em outro idioma: o dono aceitou a **simulação** no CI Windows. `texto_do_aviso` (a ligação entre o idioma da conta e o texto) é exercitada por `tests/windows.rs` contra o registro real do runner, com `es-ES`, `en-US`, `pt-BR`, o idioma de exibição valendo mais que o regional e `fr-FR` caindo no idioma do serviço; os três textos são comparados para provar que diferem. Não coberto: a caixa de mensagem em si (`WTSSendMessageW`) com outro idioma de interface, que só desenha o texto já escolhido; some quando a bandeja do [E-004](E-004-agente-windows-enforcement.md) F3 substituir a caixa. A tradução es-ES continua sem revisão de falante.
