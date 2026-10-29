# ADR-004 — Comunicação serviço ↔ UI por named pipe

Status: Aceito em 2026-10-28 pelo dono (Jeferson Lopes). Data: 2026-10-28. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-004](../epics/E-004-agente-windows-enforcement.md) F3 e F4.

## Contexto

A [ADR-001](ADR-001-enforcement-ativo-e-visivel.md) exige um agente **visível** (bandeja, tempo restante, avisos de 5 e 1 min, motivo do bloqueio) que a conta da criança **não consiga fechar**. O serviço (SYSTEM) tem o banco e o motor; só um processo dentro da sessão do usuário pode desenhar na bandeja. Os dois precisam conversar, e o canal é superfície de ataque: a criança controla um processo na mesma máquina.

Origem: [E-004](../epics/E-004-agente-windows-enforcement.md), seção "ADR a abrir ao iniciar F1".

## Decisão

Proposta (a validar em Windows real):

1. **Named pipe local** `\\.\pipe\kiosk-ui-<id da sessão>`, um por sessão de usuário, criado pelo serviço (`tokio::net::windows::named_pipe`), com `PIPE_REJECT_REMOTE_CLIENTS`.
2. **DACL restritiva**: acesso só a SYSTEM, Administradores e ao SID do perfil daquela sessão. O serviço confere o cliente (`GetNamedPipeClientProcessId` + SID do processo) antes de aceitar mensagens.
3. **Protocolo**: JSON por linha, versionado (`{"v":1,"tipo":...}`). O serviço **empurra** para a UI: estado (tempo restante, motivo), avisos e pedido de exibição. A UI só envia **batimento** (heartbeat) a cada 5 s e confirmação de aviso exibido. A UI não envia comandos que alterem regras ou dados.
4. **O serviço inicia a UI** na sessão da criança (`WTSQueryUserToken` + `CreateProcessAsUser`) e a reinicia se morrer. Ausência de 3 batimentos seguidos gera `TAMPER_SUSPECTED`; parada do serviço, `AGENT_STOPPED`.
5. Mensagens da UI são tratadas como **entrada não confiável**: tamanho máximo, validação de esquema e limite de taxa.

Alternativas rejeitadas:
- **HTTP/TCP em localhost**: qualquer processo local, inclusive da criança, conecta; sem identidade do cliente por padrão. Exigiria token, e o token ficaria na UI, que a criança controla.
- **Arquivos ou o próprio SQLite como canal** (UI lê o banco): obriga abrir leitura do banco à conta da criança, o que vaza dados (regras, log) e dificulta a proteção de arquivos.
- **COM/RPC (ALPC)**: mais pesado, sem ganho para duas mensagens simples.
- **UI iniciada por `Run`/pasta Startup**: a criança remove a entrada; o serviço iniciar a UI mantém o controle de volta.

## Consequências

- Ganho: canal local, com identidade do cliente garantida pelo SO; superfície mínima (a UI não comanda nada).
- Custo: código `unsafe`/Win32 para DACL e criação de processo na sessão; só testável em Windows.
- Risco: a criança pode matar a UI (processo dela). A mitigação é detectar (batimento) e relançar, não impedir; impedir depende de ACL e ficará na F4.
- Reversível: o protocolo é versionado e isolado numa camada; trocar o transporte não muda o laço do agente.
- Validação: Windows real, com teste negativo: um processo de outra conta não abre o pipe; mensagem malformada é descartada e registrada.

## Referências e evolução

- Consome: [E-004](../epics/E-004-agente-windows-enforcement.md) F3 (bandeja) e F4 (antiadulteração).
- Complementa: [ADR-001](ADR-001-enforcement-ativo-e-visivel.md), [ADR-003](ADR-003-deteccao-de-processos-e-tempo-de-uso.md).

## Implementação e desvios (2026-10-29)

A decisão não mudou. Ao implementar o canal (`src/ui_windows.rs`), ficaram estes detalhes, que o dono deve conhecer:

1. **Um pipe só, `\\.\pipe\kiosk-ui`, e não um por sessão.** A DACL é a do pipe de auditoria (SYSTEM e Administradores com controle total; usuários autenticados leem e escrevem dados, sem criar instâncias). Quem é cada UI vem do **token da conexão**, e o serviço entrega a cada uma só as mensagens do SID dela. Resultado equivalente, sem criar e destruir pipes a cada logon e sem pôr o SID de cada perfil na DACL.
2. **Só o `kiosk-ui.exe` instalado vale como UI.** O serviço confere o executável do processo do outro lado (`GetNamedPipeClientProcessId` + `QueryFullProcessImageNameW`) contra o `kiosk-ui.exe` ao lado do `kioskd.exe` em `Program Files`, que a conta da criança não consegue trocar. Sem isso, a criança mataria a UI de verdade e abriria um pipe com um programa dela que só manda batimentos, e o `TAMPER_SUSPECTED` nunca dispararia. Outro programa é recusado e vira `AUTHZ_FAIL` (`WRONG_IMAGE`).
3. **A primeira mensagem tem de ser um batimento**, em até 5 s: a identidade só pode ser lida depois de o servidor ter lido do pipe. Mensagem inválida (versão, esquema, marco, linha acima de 4096 bytes) e rajada acima de 10 por segundo derrubam a UI e viram `VALIDATION_FAIL` (`INVALID_MESSAGE`, `RATE_LIMIT`), no máximo 5 por minuto por conta, ligado ao perfil da criança. O detalhe não leva o SID.
4. **Uma UI nova da mesma conta toma o lugar da anterior**, que é largada. A fila do serviço para a UI tem 16 mensagens e a escrita tem prazo de 2 s; se a UI não lê, a conexão cai e o aviso volta para quem chamou (o agente cai na caixa de mensagem do Windows).
5. O `kiosk-ui.exe` existe (etapa 2, abaixo). Falta: o serviço iniciá-lo na sessão da criança, o agente usar o canal para avisos e estado, e o `TAMPER_SUSPECTED` por batimento perdido.

### `kiosk-ui.exe` (etapa 2)

6. **O que a bandeja é**: um ícone com a dica do tempo restante e um balão para cada aviso de 5 e 1 minuto, no idioma da conta da criança. **Sem menu, janela nem "sair"**: quem decide se ela vive é o serviço. Sem console (subsistema Windows). Se a barra de tarefas ainda não existe, o ícone entra quando ela nascer (`TaskbarCreated`).
7. **A UI nunca desiste do serviço**: sem o serviço, tenta de novo a cada 2 s; se ele cai e volta, reconecta. Manda um batimento ao conectar e a cada 5 s. Confere o que chega do serviço com o mesmo rigor que o serviço confere o que chega dela, e não mostra texto de motivo que não conhece.
8. **Confirmação**: depois de o Windows aceitar o balão, a UI manda `aviso_exibido` com o marco (300 ou 60); o agente poderá usar isso para cair na caixa de mensagem se a UI não mostrou.
9. **O que o CI prova e o que não prova**: no CI Windows, o `kiosk-ui.exe` de verdade conecta, recebe estado e aviso, escreve o texto certo no idioma da conta e confirma ao serviço (modo `KIOSK_UI_SEM_BANDEJA`, só para teste), e com a bandeja real sobe, conecta e fica vivo. **O desenho do ícone e do balão só se vê num PC com barra de tarefas**; é o que o dono precisa conferir.

### O serviço supervisiona a bandeja (etapa 3)

10. **O serviço inicia a bandeja** na sessão de cada criança com sessão ativa (`WTSQueryUserToken` + `CreateProcessAsUserW`, na área de trabalho `winsta0\default`, com o ambiente da própria conta), assim que a sessão aparece. O `kiosk-ui.exe` é o de `Program Files\kiosk`.
11. **Três batimentos perdidos (15 s) viram `TAMPER_SUSPECTED`** ligado ao perfil (`UI_STOPPED`), e a bandeja é relançada. Para uma criança que a feche sempre, o alerta sai no máximo uma vez a cada 5 minutos por conta (o relançamento continua a cada 15 s). Falha ao iniciar vira `INTERNAL_ERROR` (`UI_NOT_STARTED`), com o mesmo limite. Só contas com perfil recebem bandeja.
12. **`install` e `uninstall` encerram as bandejas em execução**, depois de parar o serviço (que é quem as relançaria): sem isso, o Windows trava o `kiosk-ui.exe` e a atualização ou a remoção falham.
13. Ainda falta: os avisos e o estado passarem pela bandeja (hoje o aviso segue pela caixa de mensagem do Windows), com a caixa como reserva se a bandeja não confirmar.

