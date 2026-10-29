# Insight — Aprendizados da sessão de 0.0.4 a 0.0.7 e roteiro de retomada

Data: 2026-10-30. Autoria: agente, a pedido do dono (Jeferson Lopes), ao encerrar a sessão na nuvem. Escopo: o que a sessão entregou, o que ela ensinou (Windows, CI, processo) e por onde retomar. Nada aqui é decisão: o que virar compromisso entra em ADR, épico ou GAP.

## 1. O que a sessão entregou

| Versão / PR | Entrega |
|---|---|
| 0.0.4 | Serviço do Windows (`kioskd install\|uninstall\|status\|service`), pasta de dados protegida, PATH do sistema |
| 0.0.5 | Idiomas en-US, pt-BR e es-ES (ADR-010), comandos em inglês com equivalentes em português, log forense com códigos |
| 0.0.6 | Validação estrita de argumentos, aviso de limite já ultrapassado |
| 0.0.7 | Rastro das tentativas sem privilégio (ADR-011), `kioskd version` e `kioskctl version`, `AGENT_STOPPED` por batimento (migração 0007), correções do pipe de auditoria |
| #20 | `kioskctl log` sem `--profile` (eventos do PC, como o `AGENT_STOPPED`) |
| #21 | `kiosk-ui.exe`, a bandeja da criança |
| #22 (aberto) | O serviço inicia e supervisiona a bandeja; `install` e `uninstall` encerram as bandejas |

Também: E-012 (idiomas) encerrado; ADR-004 com os desvios registrados; ADR-011 aceita, implementada e vista no PC real.

## 2. Estado ao encerrar

- `main` tem tudo até o #21. O **#22** (etapa 3 da bandeja) está aberto, com o conflito do [CHANGELOG](../../CHANGELOG.md) resolvido e o estado mesclado validado (197 testes, clippy limpo em Linux e Windows). O merge é do dono.
- Release publicado: **0.0.7** (pré-lançamento). O código do #21 e do #22 ainda não está em nenhum pacote.
- **Próxima etapa da bandeja (etapa 4)**: os avisos de 5 e 1 minuto e o tempo restante passarem pela bandeja, com a caixa de mensagem do Windows como reserva se a bandeja não confirmar. Fecha o GC-17. O agente ainda não manda o `Estado` (o ícone fica sem o tempo restante até lá).
- **Para ver no PC do dono** (nenhum CI substitui): o desenho do ícone e do balão; a recusa com o serviço parado (GC-21); GC-16 (agente como SYSTEM aplicando regra na conta da criança); GC-20 (logoff após 2 desbloqueios, com 40 s entre as entradas).

## 3. Aprendizados técnicos (Windows)

Cada item custou ao menos uma rodada de CI. Estão aqui para ninguém pagar duas vezes.

**Named pipes**
- `DisconnectNamedPipe` **descarta o que o cliente ainda não leu**. Escrever a resposta e desconectar na sequência perde a resposta de vez em quando (intermitente, só apareceu no CI). O servidor deve esperar o cliente ler e fechar, com prazo.
- Num handle **síncrono**, uma leitura pendente bloqueia qualquer escrita no mesmo handle. Cliente com leitura e escrita simultâneas abre com `FILE_FLAG_OVERLAPPED` (no tokio, `NamedPipeClient::from_raw_handle`). O sintoma é um teste pendurado, não um erro.
- Sem `SECURITY_SQOS_PRESENT` (`SECURITY_IDENTIFICATION`), o Windows entrega ao servidor o token do **processo**, não o da thread personificada. A personificação vale **só para a thread**: uma thread nova usa a identidade do processo.
- `ImpersonateNamedPipeClient` só funciona **depois de o servidor ter lido** do pipe (erro 1368). Uma sondagem que conecta e fecha sem enviar (`Test-Path`, `Path::exists`) não é erro: descartar sem registrar. Ela também **consome uma instância**: o cliente seguinte vê `ERROR_PIPE_BUSY` (231) e precisa repetir.
- `GENERIC_WRITE` inclui `FILE_APPEND_DATA`, que no pipe é `FILE_CREATE_PIPE_INSTANCE`. Para a conta comum conectar sem poder criar instância, pedir só `FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE`.
- Quem tem direito de abrir o pipe é qualquer conta; **quem vale como UI é decidido pelo executável do processo do outro lado** (`GetNamedPipeClientProcessId` + `QueryFullProcessImageNameW`). Sem isso, a criança mata a UI e abre um programa dela que só manda batimentos.

**Identidade e privilégio**
- `CheckTokenMembership(NULL, ...)` sob personificação dá "acesso negado". Ler `TokenGroups` do token da thread com `OpenThreadToken(openasself)`.
- Com o UAC, o administrador sem elevação tem o grupo Administradores **só para negação**. "Elevado" = grupo **ativo** no token efetivo; "conta de administrador" = o grupo presente em qualquer estado.
- Rodar como outra conta: `Start-Process -Credential` **trava** nos runners do GitHub (sem saída em 3 minutos). Use `LogonUser` + personificação da thread dentro do teste.
- `net user` aceita senha de até 14 caracteres; acima disso pergunta S/N e trava o teste.

**Serviço e processos**
- Arquivo em uso não é substituído nem apagado: `install` (atualização) e `uninstall` precisam encerrar as bandejas **depois** de parar o serviço (que é quem as relançaria).
- Reinício após falha: `sc failure kiosk reset= 0 actions= restart/60000`. A sintaxe para "sem ação" é duvidosa; prefira um atraso longo.
- A janela da bandeja tem de ser **de nível superior** (escondida), não só de mensagens: `TaskbarCreated` é transmitido só às de nível superior, e o ícone some quando a barra de tarefas renasce.
- Soltar um runtime do tokio dentro de código assíncrono é proibido; use `shutdown_background`.
- O PowerShell 5 lê UTF-8 como ANSI: `Get-Content ... -Encoding UTF8` (o `kioskd.log` aparece com `Ã§` sem isso).
- `Get-Acl` quebra no runner (pwsh 7 muda o `PSModulePath`): `GetNamedSecurityInfoW` direto.

## 4. Aprendizados de processo

- **O CI é o oráculo do Windows**; o macOS/Linux só compila. O que mais rendeu: **mensagens de falha com os dados que decidem a causa** (grupos do token, SID esperado, detalhe gravado, fim do `kioskd.log`). Duas rodadas resolveram o que adivinhação não resolvia.
- **Teste que passa pulando é teste que não provou nada.** A prova da bandeja precisa de sessão interativa; em vez de pular em silêncio, `KIOSK_TESTE_EXIGE_SESSAO=1` no CI faz a falta virar falha. Resultado: o runner **tem** sessão interativa, e a cadeia inteira foi provada com o serviço real como SYSTEM.
- Job sem prazo prende o runner por horas: `timeout-minutes: 25` nos jobs. Log de job **em andamento** não é lido pela API: cancele o run para ler o que já saiu.
- Testes que mexem no mesmo `HKCU` precisam ser serializados (mutex), senão um derruba o outro.
- Dois lados de um merge: ao resolver conflito, comparar o resultado com **cada pai** (linhas removidas) e conferir, por conteúdo, uma marca de cada lado. Aqui as remoções eram só as linhas que cada lado reescreveu, e os dois lados estavam inteiros.
- PRs empilhados funcionam (base no branch anterior), mas a base precisa ser trocada para a `main` depois do merge, e o [CHANGELOG](../../CHANGELOG.md) conflita sempre. Prefira PRs pequenos e na ordem.
- **Verificar antes de afirmar.** Erros desta sessão: disse que o salto de relógio não estava coberto (estava, no E-003); disse que tinha aberto um PR que só tinha sido enviado como branch; li o estado do CI errado uma vez. Todos foram corrigidos na hora, mas o padrão é: consultar a fonte (código, API do GitHub) antes de dar o resultado.
- **Dados reais do dono** (IP, conta, IDs de perfil, códigos de pareamento) aparecem em logs colados no chat e **nunca** entram no repositório (público). `git diff --cached | grep` por esses padrões antes de cada commit.
- Harness: `git add` só por caminho; `sleep` encadeado é bloqueado (esperar o CI com laço em segundo plano); o pré-push roda 5 gates (docs-refs, docs-adr, fmt, clippy `-D warnings`, test).
- Corte de versão: PR de bump (`Cargo.toml`, `Cargo.lock`, [CHANGELOG](../../CHANGELOG.md)) + merge + workflow `release` com `publicar: true`. Para o dono testar sem cortar versão: o mesmo workflow com `publicar: false` gera os pacotes como artefato do run.
- O `mermaid-cli` para validar os fluxogramas ficou instalado no scratchpad da sessão (`mmd/node_modules/.bin/mmdc`, com `--no-sandbox` e o Chromium em `/opt/pw-browsers/chromium`); numa sessão nova, instale de novo.

## 5. Decisões e desvios que merecem ADR se o dono quiser formalizar

- ADR-004: um pipe só (`kiosk-ui`) com roteamento pelo SID do token, e conferência do executável do cliente. Já registrado na própria ADR.
- Eventos do PC sem perfil (`AGENT_STOPPED`, recusa de administrador sem elevação) só aparecem em `kioskctl log` **sem** `--profile`. Vale decidir se o app gráfico (E-008) os destaca por padrão.
- Limite de alertas da bandeja (um por 5 min por conta): número escolhido, não medido.

## 6. Pendências por ordem de valor

1. Etapa 4 da bandeja (avisos e estado pela bandeja, com reserva) e ver o desenho no PC.
2. GC-16, GC-20 e a recusa com o serviço parado (GC-21), no PC.
3. E-007 F1: assistente `kioskd configurar` (ADR-007 aceita).
4. E-008 F1: listagem remota de perfis, apps e regras na API (pré-requisito do app gráfico).
5. GC-08 (avisos configuráveis por regra), GC-15 (LAN com HTTPS, só se o dono quiser), GC-19 (tag anotada no pré-push do harness).
6. Revisão da tradução es-ES por falante.

## Referências

[E-004](../epics/E-004-agente-windows-enforcement.md), [E-012](../epics/E-012-internacionalizacao.md), [ADR-004](../adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md), [ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md), [ADR-011](../adr/ADR-011-rastro-de-tentativas-sem-privilegio.md), [PENDING](../PENDING.md), [CONCLUSION](../CONCLUSION.md).
