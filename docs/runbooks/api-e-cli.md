# Runbook — instalar, cadastrar, parear e administrar

Procedimento para pôr o kiosk para funcionar num PC Windows e administrá-lo pelo CLI. Decisões de origem: [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) (uma conta por criança), [ADR-005](../adr/ADR-005-rede-da-api-de-comando.md) (rede) e [ADR-006](../adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md) (assinatura e pareamento). Épicos: [E-004](../epics/E-004-agente-windows-enforcement.md) e [E-005](../epics/E-005-api-comando-e-pareamento.md). O fluxograma de cada comando, com o caminho da informação entre o cliente e o serviço, está em [fluxos dos comandos](../architecture/fluxos-dos-comandos.md).

> Pré-lançamento: o agente encerra apps em Windows real, mas bloqueio de sessão, logoff e avisos ainda não foram validados com uma criança logada. O serviço do Windows (`kioskd instalar`, seção 4) é novo e ainda não foi validado num reinício de PC real. Não use como única proteção.

Todos os comandos abaixo são para o **PowerShell aberto como Administrador**, dentro da pasta extraída do pacote `kiosk-engine-v<versão>-windows-x86_64.zip`.

## 1. Pré-requisitos

- Uma conta local **padrão** (sem administrador) para cada criança; os pais usam outra conta, de administrador.
- Tailscale instalado e conectado no PC e no dispositivo dos pais. Veja o IP do PC:

  ```powershell
  tailscale ip -4
  ```

## 2. Variáveis da sessão do PowerShell

> Os exemplos usam `100.95.52.48` como IP Tailscale do PC. É só um exemplo: troque pelo seu. Se esse endereço for de alguém, é chute.

```powershell
$env:KIOSK_LISTEN = "100.95.52.48:8443,127.0.0.1:8443"   # IP do `tailscale ip -4` e o loopback, com a porta
```

- **Banco:** no Windows o padrão é `C:\ProgramData\kiosk\kiosk.db`, igual em qualquer janela e pasta; a pasta é criada se faltar. Todo comando do `kioskd` mostra na primeira linha qual banco está usando (`kioskd: banco ...`): confira que é sempre o mesmo. Só defina `$env:KIOSK_DB` se quiser outro lugar e, nesse caso, defina-o **em toda janela** (as variáveis `$env:` valem só para a janela em que foram definidas). O arquivo tem dados de crianças: o `kioskd instalar` (seção 4) deixa a pasta acessível só a SYSTEM e Administradores; a partir daí, os comandos do `kioskd` só funcionam num PowerShell **aberto como Administrador**.
- `KIOSK_LISTEN`: `ip:porta[,ip:porta...]`, obrigatória para `servir`. Só são aceitos loopback (`127.0.0.1`, `::1`) e a rede Tailscale (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`); `0.0.0.0` e endereços de LAN são recusados. O `127.0.0.1` permite parear um `kioskctl` no próprio PC.

## 3. Cadastrar a criança e os apps

### 3.1 Três identificadores diferentes

| O quê | Exemplo | De onde vem | Onde é usado |
|---|---|---|---|
| **Nome da conta** | `Ana` ou `Nome da Conta` | nome de login no Windows | `perfil criar --conta` |
| **SID da conta** | `S-1-5-21-…-1001` | identificador interno do Windows | `perfil criar --sid` (alternativa ao nome) |
| **ID do perfil** | `3f2a9c1e-…` (UUID) | gerado pelo `kioskd` ao criar o perfil | `perfil janela`, `perfil limite`, `kioskctl regra criar --perfil`, `uso`, `log` |

O `--nome` do `perfil criar` é só o nome de exibição (o que aparece nas listagens); não precisa ser igual ao da conta.

### 3.2 Descobrir o nome e o SID da conta da criança

**O nome da pasta em `C:\Users\` não é necessariamente o nome da conta.** Se a conta foi renomeada, a pasta mantém o nome antigo; em conta Microsoft, a pasta costuma ter só as primeiras letras do e-mail. Confira o nome real:

```powershell
Get-LocalUser | Select-Object Name, SID, Enabled
```

- Use o valor da coluna `Name` em `--conta`, **entre aspas** se tiver espaço.
- Ou use o valor da coluna `SID` em `--sid`, que não depende do nome.

**Conta Microsoft** (login por e-mail): ela não aparece como as contas locais no `Get-LocalUser`, e o projeto parte do pressuposto de contas locais ([ADR-002](../adr/ADR-002-identidade-por-conta-windows.md)). Para usá-la, entre na conta da criança, rode `whoami /user` e cadastre pelo SID mostrado, com `--sid`.

### 3.3 Cadastrar

```powershell
.\kioskd.exe perfil criar --conta "Nome da Conta" --nome "Ana" --limite-total 7200
# ou, pelo SID:
.\kioskd.exe perfil criar --sid S-1-5-21-XXXXXXXXXX-XXXXXXXXXX-XXXXXXXXXX-1001 --nome "Ana" --limite-total 7200

.\kioskd.exe perfil listar
```

O `perfil criar` imprime o **ID do perfil**. Confira no `perfil listar` que o SID mostrado é o mesmo do `Get-LocalUser` (ou do `whoami /user`). Contas inexistentes e grupos são recusados; o mesmo SID não pode ter dois perfis.

```powershell
.\kioskd.exe perfil janela <id-do-perfil> --dias seg,ter,qua,qui,sex --janela 08:00-20:00
.\kioskd.exe app criar --processo Roblox.exe
.\kioskd.exe app criar --processo escola.exe --isento
.\kioskd.exe app listar
```

- `--limite-total` (segundos) é o tempo de computador por dia; `perfil limite <id-do-perfil> --segundos N` ou `--sem-limite` muda depois.
- Dia sem janela cadastrada não tem restrição de horário.
- `app criar` também imprime o ID do app, usado em `kioskctl regra criar --app`.
- App **monitorado** (padrão) só é restringido por regras; `--isento` nunca é encerrado; `--ignorar` fica fora do controle.
- O nome do processo é o do `.exe` (veja no Gerenciador de Tarefas, aba Detalhes). Renomear o `.exe` burla a identificação: limitação conhecida.

## 4. Subir o serviço

### 4.1 Como serviço do Windows (recomendado)

```powershell
.\kioskd.exe instalar --escutar "100.95.52.48:8443,127.0.0.1:8443"   # IP do `tailscale ip -4` e o loopback
```

Num só passo, o `instalar`:

- copia `kioskd.exe` e `kioskctl.exe` para `C:\Program Files\kiosk`;
- deixa `C:\ProgramData\kiosk` (banco e log) acessível só a SYSTEM e Administradores;
- registra o serviço `kiosk`: conta SYSTEM, início automático com o Windows e reinício 5 s depois de uma falha. Uma conta padrão, como a da criança, consegue consultar o serviço, mas não pará-lo;
- grava o endereço de escuta no próprio serviço, sem precisar de `$env:KIOSK_LISTEN`;
- inicia o serviço.

Rodar de novo atualiza a instalação, por exemplo com uma versão nova do pacote ou outro endereço. Para conferir:

```powershell
.\kioskd.exe estado          # estado, pid, início automático, conta, endereço e onde fica o log
Get-Content C:\ProgramData\kiosk\kioskd.log -Tail 20
```

Para remover, use `.\kioskd.exe desinstalar`: para e apaga o serviço e os executáveis, e **mantém** o banco. Com `--apagar-dados`, apaga também `C:\ProgramData\kiosk`, mas só depois de você digitar `APAGAR`. O Windows pode pedir permissão de firewall para a porta: libere só para redes privadas (a Tailscale).

Depois de instalado, use `C:\Program Files\kiosk\kioskd.exe` (ou o da pasta extraída) para os cadastros, sempre num PowerShell como Administrador.

### 4.2 Num console, para testes

```powershell
.\kioskd.exe servir
```

Precisa de `$env:KIOSK_LISTEN` (seção 2). Deve aparecer `escutando em 100.95.52.48:8443` e `agente de enforcement ativo (tick de 2s)`. Funciona só enquanto a janela estiver aberta. Não rode junto com o serviço instalado, porque os dois disputam a mesma porta.

## 5. Parear o dispositivo dos pais

Numa janela do PowerShell como Administrador (confira que a linha `kioskd: banco ...` mostra o mesmo banco do serviço; com o serviço instalado, é o padrão):

```powershell
.\kioskd.exe codigo-pareamento
```

Só um administrador elevado gera o código: numa janela sem "Executar como administrador", o comando recusa e registra a tentativa no log forense. O comando imprime um código de **8 caracteres**, de uso único, válido por 5 minutos. Use exatamente esse código; qualquer outro é recusado com `HTTP 403`. No dispositivo dos pais, com o `kioskctl` do pacote:

```powershell
.\kioskctl.exe parear --servidor 100.95.52.48:8443 --codigo ABCD2345 --nome "Celular da mãe"
# ou, no próprio PC:
.\kioskctl.exe parear --servidor 127.0.0.1:8443 --codigo ABCD2345 --nome "PC admin"
```

O CLI gera o par de chaves Ed25519, envia só a chave pública e grava `credenciais.json` com a chave privada (`%APPDATA%\kiosk\credenciais.json` no Windows, `~/.config/kiosk/credenciais.json` no Linux e macOS, ou `KIOSK_CREDENCIAIS`). Dez códigos errados em 5 minutos bloqueiam novos pareamentos por esse período.

## 6. Administrar as regras

**No próprio PC** (PowerShell de administrador, sem precisar parear nada):

```powershell
.\kioskd.exe regra criar --perfil <id-do-perfil> --app <id-do-app> --limite 0 --ativar
.\kioskd.exe regra listar [--perfil <id-do-perfil>]
.\kioskd.exe regra ativar <id-da-regra>
.\kioskd.exe regra limite <id-da-regra> --segundos 5400
.\kioskd.exe regra revogar <id-da-regra>
```

Sem `--ativar`, a regra nasce **pendente**. No PC os erros vêm com o motivo exato (por exemplo, `perfil inexistente`).

**De outro dispositivo** (celular ou notebook dos pais, já pareado), com o `kioskctl`:

```powershell
.\kioskctl.exe regra criar --perfil <id-do-perfil> --app <id-do-app> --limite 3600 --janela 08:00-20:00
.\kioskctl.exe regra ativar <id-da-regra>
.\kioskctl.exe regra limite <id-da-regra> --segundos 5400
.\kioskctl.exe regra revogar <id-da-regra>
.\kioskctl.exe uso --perfil <id-do-perfil> [--data 2026-03-09]
.\kioskctl.exe log --perfil <id-do-perfil> [--dias 7] [--limite 100]
```

- A regra nasce **pendente**; só `ativar` a põe em vigor. Regra com `--limite 0` bloqueia o app.
- `--limite` em segundos (até 86400); `--janela` em `HH:MM-HH:MM`; `--ate` em RFC 3339 (no futuro).
- Se o serviço recusar, o CLI diz apenas que recusou: a resposta é genérica de propósito. O motivo exato fica no log do PC.
- O `log` mostra o detalhe dos eventos de enforcement, mas não o das falhas de autenticação, autorização e validação.

## 7. Dispositivos

```powershell
.\kioskd.exe dispositivos          # lista id, nome e estado
.\kioskd.exe revogar <id>          # vale na próxima requisição
```

Perdeu o celular: revogue o dispositivo no PC e pareie outro.

## Limitações conhecidas

- Sem segundo fator: um dispositivo pareado e desbloqueado nas mãos da criança comanda tudo (ADR-006).
- Sem modo LAN com HTTPS (GC-15 no [PENDING](../PENDING.md)).
- Avisos são caixas de mensagem do Windows até existir a bandeja (GC-17); bloqueio desconecta a sessão (GC-18).
- Quem alcança a porta pode errar códigos de pareamento de propósito e bloquear um pareamento novo por 5 minutos (GC-13).
