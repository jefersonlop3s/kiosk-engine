# ADR-011 — Rastro das tentativas de comando administrativo sem privilégio

Status: Aceito em 2026-10-29 pelo dono (Jeferson Lopes). Data: 2026-10-29. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; `kioskd` (comandos que abrem o banco), serviço do Windows, leitura do log pela API; GC-21 do [PENDING](../PENDING.md); [E-004](../epics/E-004-agente-windows-enforcement.md) F4.

## Contexto

Depois do `kioskd install`, a pasta do banco só abre para SYSTEM e Administradores ([ADR-008](ADR-008-formato-do-instalador.md)), e `kioskd pairing-code` recusa quem não é administrador elevado (regra do dono, [E-008](../epics/E-008-cliente-grafico-dos-pais.md)). No teste da 0.0.5 no PC real (2026-10-29), uma janela sem elevação foi recusada, mas **a tentativa não ficou no log forense**: quem recusa é o próprio `kioskd`, que só consegue gravar abrindo o banco, e a conta sem privilégio não o abre. O comando avisa isso na mensagem ("a recusa não foi registrada no log forense"). O bloqueio funciona; falta o rastro.

Isso importa porque a conta da criança é justamente a que não tem privilégio. Uma criança que tenta gerar um código de pareamento, criar ou revogar regras pelo terminal é um sinal que os pais deveriam ver ([ADR-001](ADR-001-enforcement-ativo-e-visivel.md): o controle é visível e auditável).

Dois fatos que o teste mostrou e esta ADR também resolve:

- Só o `pairing-code` confere a elevação. Os outros comandos que usam o banco (`devices`, `revoke`, `profile *`, `app *`, `rule *`) falham pelo erro cru do SQLite (`unable to open database file (code: 14)`), sem explicar e sem rastro.
- O serviço (SYSTEM) já é dono do banco e escuta a API, mas a API é só para dispositivos pareados e assinados ([ADR-006](ADR-006-assinatura-de-requisicoes-e-pareamento.md)); uma conta local sem privilégio não tem como falar com ele.

## Confirmações do dono (2026-10-29)

O dono confirmou as duas decisões que a proposta deixou abertas, e elas valem como estão escritas abaixo: **(1)** o administrador que esqueceu de abrir o PowerShell elevado também é registrado, marcado como `admin_account: true`; **(2)** o que a conta da criança tenta contra o serviço pelo gerenciador de serviços (por exemplo `sc stop`) fica fora desta ADR. A ADR foi aceita como um todo no mesmo dia.

## Decisão


1. **O comando avisa o serviço, e o serviço grava.** Quando um comando que exige administrador roda sem elevação, o `kioskd` recusa como hoje e, antes, manda um aviso curto ao serviço por um **named pipe local** `\\.\pipe\kiosk-audit`. O serviço, que é SYSTEM, grava o evento no banco, que fica no mesmo lugar e sob a mesma proteção do resto do log.
2. **O serviço descobre quem chamou; o cliente não informa.** Para cada conexão, o serviço usa `ImpersonateNamedPipeClient` + `OpenThreadToken`, lê o SID do chamador e volta à própria identidade (`RevertToSelf`). Nada no conteúdo da mensagem diz quem é o chamador, então ele não pode ser forjado. (A [ADR-004](ADR-004-comunicacao-servico-ui-por-named-pipe.md) usa `GetNamedPipeClientProcessId` + SID do processo; aqui a personificação é preferida porque não depende de o PID ainda ser o mesmo.)
3. **O pipe.**
   - Criado pelo serviço com `FILE_FLAG_FIRST_PIPE_INSTANCE` (ninguém ocupa o nome antes) e `PIPE_REJECT_REMOTE_CLIENTS`.
   - DACL: leitura e escrita para usuários autenticados (sem permissão de criar instâncias) e controle total para SYSTEM e Administradores. É de propósito que a conta da criança consiga conectar: é ela que queremos registrar.
   - O cliente confere antes de enviar que o servidor do pipe é o SYSTEM (`GetNamedPipeServerProcessId` + SID do processo), para não entregar nada a um falso servidor.
4. **Protocolo mínimo.** Uma linha JSON, versionada, esquema fechado, no mesmo molde da [ADR-004](ADR-004-comunicacao-servico-ui-por-named-pipe.md), reaproveitando a decodificação defensiva do módulo `ipc`: `{"v":1,"tipo":"tentativa_sem_privilegio","comando":"pairing-code"}`. O campo `comando` é de uma **lista fechada** (os nomes em inglês dos comandos administrativos); qualquer outro valor é recusado. Tamanho máximo e limite de taxa como na ADR-004. Resposta: `{"v":1,"ok":true}`. Texto livre nunca entra no log.
5. **O evento.**
   - `event_type = AUTHZ_FAIL`; `action_attempted` é a ação do comando quando existe (por exemplo `CREATE_PAIRING_CODE`) e `SYSTEM` nos demais; `client_ip = local`.
   - O serviço resolve o SID do chamador: se for um **perfil de criança cadastrado**, o evento leva o `profile_id` dela e aparece em `kioskctl log --profile`. Se não for, o detalhe leva o SID (`caller_sid`) para quem examinar o banco no PC.
   - Detalhe em código ([ADR-010](ADR-010-idiomas-e-comandos-em-ingles.md)): `{"code":"ADMIN_REQUIRED","command":"pairing-code","admin_account":false}`. O serviço também diz, a partir do token do chamador, se a conta é de administrador sem elevação (`admin_account: true`: um pai que esqueceu de abrir o PowerShell como administrador) ou uma conta comum (`false`), para os pais distinguirem esquecimento de tentativa. Um administrador sem elevação também é registrado; é inofensivo e o evento diz o que foi.
   - O `kioskctl log` passa a mostrar o detalhe deste código, sem o `caller_sid`. Os demais detalhes de `AUTHZ_FAIL` continuam omitidos ([ADR-006](ADR-006-assinatura-de-requisicoes-e-pareamento.md)).
6. **Limite de taxa por conta.** No máximo 5 avisos por minuto por SID; o excedente é descartado e contado, e a contagem (`suppressed`) vai no próximo evento aceito. Uma criança não consegue inundar o log nem o banco.
7. **Falha alta.** Sem o serviço (parado, ou pipe inexistente), o comando recusa do mesmo jeito e diz explicitamente que a tentativa **não foi registrada** e por quê. Nada de fallback silencioso.
8. **Todo comando que abre o banco confere a elevação primeiro**, não só o `pairing-code`, e recusa com a mensagem clara no idioma de quem lê, em vez do erro cru do SQLite. `install` e `uninstall` já têm a própria mensagem de acesso negado do gerenciador de serviços e passam a usar o mesmo aviso.

Alternativas rejeitadas:

- **Log de Eventos do Windows**: funcionaria mesmo com o serviço parado e é nativo, mas deixa o rastro em outro lugar, que o `kioskctl log` e a API dos pais não leem sem trabalho extra, e a retenção e a proteção ficam fora do que o projeto já controla (append-only, 90 dias, por perfil). Pode ser acrescentado depois como reforço (ver "Evolução").
- **Pasta de entrada onde qualquer usuário só cria arquivos, que o serviço consome**: não exige servidor sempre escutando e sobrevive a um serviço parado por um instante, e o dono do arquivo identifica o chamador. Mas depende de uma ACL delicada (criar sem listar, ler nem apagar), não tem limite de taxa (a criança enche a pasta) e adiciona sondagem e limpeza. A superfície do pipe é menor e já é um padrão do projeto.
- **Endpoint HTTP sem assinatura em loopback**: qualquer processo local conecta, e o TCP em loopback não dá o SID do chamador de forma confiável. É o que a ADR-004 já rejeitou.
- **Abrir a pasta do banco a todos para o próprio `kioskd` gravar**: derrota a proteção dos dados da criança ([E-002](../epics/E-002-log-forense.md)) e deixaria a criança escrever (e ler) o log.
- **Não registrar**: o bloqueio já funciona, mas a ausência de rastro é justamente o buraco do GC-21.

## Consequências

- Ganho: a tentativa fica no log forense, ligada ao perfil da criança, visível para os pais, sem abrir o banco a ninguém; o chamador não pode ser forjado; todo comando administrativo explica a recusa.
- Custo: código Win32 novo (servidor de pipe com DACL e personificação no serviço, cliente no `kioskd`), só testável em Windows; uma superfície nova escutando para entrada **não confiável** de contas locais.
- Riscos:
  - o servidor de pipe é entrada hostil: mitigado por esquema fechado, tamanho, limite de taxa por SID e, no serviço, por nunca confiar em nenhum campo para identificar o chamador;
  - a personificação de um cliente não confiável por um serviço SYSTEM: o serviço só lê o token e reverte na hora, e usa o nível mais baixo de personificação (identificação);
  - o serviço parado não registra nada, o que é dito na mensagem (item 7) e é coberto por `AGENT_STOPPED`/antiadulteração ([E-004](../epics/E-004-agente-windows-enforcement.md) F4).
- Reversível: o protocolo é versionado e isolado; remover o pipe volta ao comportamento atual (recusa sem rastro).
- Validação, no CI Windows, com uma conta padrão real (já usada em `tests/servico_windows.rs`):
  - a conta comum conecta, envia um aviso e o banco ganha o `AUTHZ_FAIL` com o SID **do chamador**, mesmo que a mensagem tente dizer outro;
  - perfil de criança cadastrado: o evento leva o `profile_id` e aparece na consulta de log por perfil, e o `caller_sid` não sai pela API;
  - mensagem acima do tamanho, com campo extra, com versão errada ou com comando fora da lista é recusada;
  - o sexto aviso no mesmo minuto é descartado e contado;
  - com o serviço parado, o comando recusa e diz que não registrou;
  - um processo que não é SYSTEM não consegue ocupar o nome do pipe antes do serviço, e o cliente recusa um servidor que não é SYSTEM;
  - os comandos `devices`, `profile list` etc. sem elevação dão a mensagem clara.
  Fluxograma de cada comando alterado no mesmo commit ([AGENTS.md](../../AGENTS.md#fluxogramas-dos-comandos)).

## Referências e evolução

- Consome: GC-21 do [PENDING](../PENDING.md); [E-004](../epics/E-004-agente-windows-enforcement.md) F4 (antiadulteração).
- Relaciona-se com: [ADR-001](ADR-001-enforcement-ativo-e-visivel.md), [ADR-004](ADR-004-comunicacao-servico-ui-por-named-pipe.md) (mesmo molde de pipe e de protocolo, canal e confiança diferentes), [ADR-006](ADR-006-assinatura-de-requisicoes-e-pareamento.md) (o que a API dos pais mostra do log), [ADR-008](ADR-008-formato-do-instalador.md) (proteção da pasta de dados), [ADR-010](ADR-010-idiomas-e-comandos-em-ingles.md) (detalhe em código).
- Fora do escopo: o que a conta da criança faz contra o serviço pelo gerenciador de serviços (por exemplo `sc stop`, que a DACL do serviço já nega). Registrar essas negativas depende da auditoria de acesso a objetos do próprio Windows, que fica como possível reforço em E-004 F4, junto com o Log de Eventos do Windows como segundo destino.

## Implementação e desvios (2026-10-29)

A decisão não mudou. Ao implementar, ficaram estes detalhes:

1. **O cliente confere o servidor pelo PID do serviço, e não pelo SID do processo.** A ADR previa `GetNamedPipeServerProcessId` + SID do processo, mas uma conta comum não consegue abrir o processo SYSTEM do serviço para ler o token. O cliente compara o PID do servidor do pipe com o PID que o gerenciador de serviços informa para o serviço `kiosk` (qualquer conta consulta, a DACL do serviço deixa). Um processo que ocupou o nome antes não tem o PID do serviço, e o cliente recusa. O serviço continua criando a primeira instância com `FILE_FLAG_FIRST_PIPE_INSTANCE`.
2. **Direitos da conta comum no pipe**: `FILE_GENERIC_READ` + `FILE_WRITE_DATA` + `FILE_WRITE_ATTRIBUTES` (`0x12018b`), e não `GENERIC_WRITE`, porque o `FILE_GENERIC_WRITE` inclui `FILE_APPEND_DATA`, que no pipe é o `FILE_CREATE_PIPE_INSTANCE` e deixaria qualquer conta criar instâncias. Por isso o cliente também **não pode** abrir o pipe com leitura e escrita genéricas (`OpenOptions::read(true).write(true)` pede `GENERIC_WRITE` e recebe "acesso negado", visto no primeiro teste no CI Windows): ele pede só `FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | SYNCHRONIZE`.
3. **A personificação abre o token com a identidade do serviço** (`OpenThreadToken` com `openasself`) e o cliente abre o pipe no nível `SECURITY_IDENTIFICATION`, o mínimo para o serviço saber quem é. Foi o mesmo erro que o teste do `administrador_elevado` já tinha mostrado: abrir o token com a identidade da conta comum dá "acesso negado".
4. **Mensagem inválida também gasta a cota** do limite de taxa (e vira `VALIDATION_FAIL`), para não dar para encher o log com lixo. Aviso descartado por excesso responde `ok:true`, porque o que importa ao `kioskd` é que o serviço está no ar; a contagem sai no próximo evento.
5. **A lista fechada de comandos** inclui `install` e `uninstall`: sem elevação, eles também recusam antes de tocar no gerenciador de serviços. Sem o serviço no ar (primeira instalação), a recusa diz que a tentativa não foi registrada.
6. **`Detalhe::PairingCodeNotAdmin`** continua existindo só para ler linhas gravadas pela 0.0.5; nada novo o grava.

