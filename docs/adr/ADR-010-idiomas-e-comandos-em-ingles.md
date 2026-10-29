# ADR-010 — Idiomas (en-US, pt-BR, es-ES) e comandos de CLI em inglês

Status: Aceito em 2026-10-29 pelo dono (Jeferson Lopes), com os ajustes da seção "Aceite". Data: 2026-10-29. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; nomes de comandos e opções do `kioskd` e do `kioskctl`; textos mostrados à criança e aos pais; binário registrado no serviço do Windows; [E-012](../epics/E-012-internacionalizacao.md).

## Contexto

Pedido do dono em 2026-10-29:

- **Idiomas**: inglês (en-US), português (pt-BR) e espanhol (es-ES) nas **interfaces, no serviço e nas respostas**. A tradução **não vale para os comandos da CLI**.
- **CLI**: os comandos são em português hoje (`kioskd estado`); o padrão passa a ser o equivalente em inglês (`kioskd status`), "entre outros".

Hoje todo texto está em pt-BR e espalhado pelo código: `src/textos.rs` (avisos à criança), mensagens do `kioskd`, `kioskctl`, `cliente.rs` e `servico_windows.rs`, e os detalhes gravados no log forense. Não há escolha de idioma.

## Aceite (2026-10-29)

O dono aceitou com três ajustes, que **prevalecem** sobre o texto da proposta abaixo:

1. **Português e inglês, os dois para sempre.** Os comandos em português são mantidos, sem aviso nem data de remoção. O inglês é a convenção, e a tabela de equivalentes foi aceita. Localmente, o usuário digita na língua que preferir.
   - **A API fala só inglês**: rotas, ações e campos JSON. Os clientes conversam com ela em inglês. Por isso, o corpo do pareamento passa de `codigo`/`chave_publica`/`nome` para `code`/`public_key`/`name`.
   - **A documentação cita os comandos em inglês**, e uma página em Markdown lista os equivalentes em português.
2. **Os três idiomas** (en-US, pt-BR, es-ES) aprovados. O texto exibido é traduzido na hora, conforme o idioma do perfil do usuário que o vê, seja no cliente, seja no servidor. A criança vê os avisos no idioma da conta dela.
3. **Mensagens da CLI traduzidas** e palavra de confirmação no idioma da mensagem: aprovadas.

## Decisão

Proposta original (os ajustes acima prevalecem):

1. **Comandos e opções da CLI em inglês, fixos e iguais em qualquer idioma** (tabela abaixo). O texto de uso, os fluxogramas ([AGENTS.md](../../AGENTS.md#fluxogramas-dos-comandos)), o runbook e o README passam aos nomes em inglês.
   - **Os nomes em português continuam aceitos como apelidos** até a 0.1.0, sem aparecer no uso e com um aviso de "obsoleto" no stderr. Assim, o serviço já instalado (registrado como `kioskd.exe servico`) e os hábitos atuais não quebram. Na 0.1.0, os apelidos saem.
   - O `instalar`/`install` passa a registrar o serviço como `kioskd.exe service`; reinstalar atualiza o registro.
2. **Textos traduzidos**: avisos à criança, mensagens e erros do `kioskd` e do `kioskctl`, nome de exibição e descrição do serviço no Windows e, no futuro, a bandeja e o cliente gráfico ([E-008](../epics/E-008-cliente-grafico-dos-pais.md)).
   - **Catálogo no próprio código**: um enum de mensagens e uma função por idioma com `match` exaustivo. Se faltar uma tradução em qualquer dos três idiomas, o build falha. Sem arquivos externos para carregar ou perder, e sem dependência nova.
   - **Não traduzidos**: códigos da API e do log forense (`status`, `event_type`, `action_attempted`), que já são neutros, e os nomes de comandos e opções.
3. **Escolha do idioma**, nesta ordem:
   1. `KIOSK_LANG` (`en-US`, `pt-BR`, `es-ES`), para a CLI e para testes;
   2. para os **avisos à criança**, o idioma de exibição da conta dela no Windows; um campo opcional no perfil pode fixar outro;
   3. para o **serviço e a CLI**, o idioma de exibição do Windows de quem roda o comando, ou o do sistema, para o serviço;
   4. sem correspondência (por exemplo francês), **en-US**.
   - `pt-*` cai em pt-BR, e `es-*` cai em es-ES.
4. **Log forense**: o detalhe gravado passa a ser um código estável mais parâmetros (por exemplo `PAIRING_CODE_NOT_ADMIN`), traduzido só na hora de mostrar. Linhas antigas, com texto em pt-BR, ficam como estão, porque o log é append-only.

### Tabela de nomes (proposta)

| Hoje (pt) | Inglês (padrão) |
|---|---|
| `kioskd servir [--sem-agente]` | `kioskd serve [--no-agent]` |
| `kioskd codigo-pareamento` | `kioskd pairing-code` |
| `kioskd dispositivos` / `revogar <id>` | `kioskd devices` / `revoke <id>` |
| `kioskd perfil criar --conta --nome --limite-total` | `kioskd profile create --account --name --total-limit` |
| `kioskd perfil listar` / `limite --segundos --sem-limite` / `janela --dias --janela` | `kioskd profile list` / `limit --seconds --no-limit` / `window --days --window` |
| `kioskd app criar --processo [--isento\|--ignorar]` / `app listar` | `kioskd app create --process [--exempt\|--ignore]` / `app list` |
| `kioskd regra criar --perfil --app --limite --janela --ate --ativar` | `kioskd rule create --profile --app --limit --window --until --activate` |
| `kioskd regra ativar\|revogar\|limite\|listar` | `kioskd rule activate\|revoke\|limit\|list` |
| `kioskd instalar --escutar` / `desinstalar [--apagar-dados]` | `kioskd install --listen` / `uninstall [--purge-data]` |
| `kioskd estado` / `servico` | `kioskd status` / `service` |
| `kioskctl parear --servidor --codigo --nome` | `kioskctl pair --server --code --name` |
| `kioskctl regra criar\|ativar\|revogar\|limite` | `kioskctl rule create\|activate\|revoke\|limit` |
| `kioskctl uso --perfil --data` / `log --perfil --dias --limite` | `kioskctl usage --profile --date` / `log --profile --days --limit` |

- `--sid` e `--app` não mudam.
- Dias da semana: `mon,tue,wed,thu,fri,sat,sun`. `seg,ter,...` e os números de 0 a 6 continuam aceitos.
- A palavra de confirmação do `--purge-data` é a do idioma da mensagem: `DELETE`, `APAGAR` ou `BORRAR`. O texto mostra qual digitar.

Alternativas rejeitadas:

- **Comandos traduzidos por idioma**: o mesmo script quebraria em outra máquina, e o pedido exclui a CLI da tradução.
- **Trocar os nomes sem apelidos**: quebraria o serviço já instalado e o runbook em uso.
- **Fluent (`fluent-rs`) ou gettext com arquivos externos**: mais recursos, como plurais complexos, mas a falta de uma tradução só aparece em execução. Com três idiomas e algumas dezenas de mensagens, o `match` exaustivo dá a garantia no build. Pode ser revisto no cliente gráfico, se o volume crescer.
- **Idioma só por variável de ambiente**: o serviço roda como SYSTEM, e a criança veria os avisos no idioma do serviço, não no dela.

## Consequências

- Ganhos:
  - projeto utilizável por quem não lê português;
  - comandos estáveis para scripts e documentação;
  - nenhuma tradução esquecida, porque o build não deixa.
- Custos:
  - renomear comandos, testes, fluxogramas, runbook e README;
  - extrair todos os textos para o catálogo;
  - traduzir para en-US e es-ES. A tradução es-ES precisa de revisão por falante.
- Riscos: durante a transição, haverá dois nomes por comando. Os apelidos ficam fora do uso impresso para o teste de fluxogramas continuar exigindo um diagrama por comando real.
- Reversível: os apelidos permitem voltar atrás até a 0.1.0.
- Validação:
  - testes dos três idiomas (completude garantida pelo build e uma amostra de cada texto);
  - aviso mostrado no idioma da conta da criança no Windows real;
  - `kioskd estado` ainda funcionando, com o aviso de obsoleto, durante a transição;
  - serviço instalado na 0.0.4 continuando a subir depois de atualizar.

## Referências e evolução

- Consome: [E-012](../epics/E-012-internacionalizacao.md).
- Relaciona-se com: [ADR-001](ADR-001-enforcement-ativo-e-visivel.md) (avisos visíveis, agora no idioma da criança), [ADR-008](ADR-008-formato-do-instalador.md) (binário registrado no serviço), [E-008](../epics/E-008-cliente-grafico-dos-pais.md) (o cliente gráfico usa o mesmo catálogo).
