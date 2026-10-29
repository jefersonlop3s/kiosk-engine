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
