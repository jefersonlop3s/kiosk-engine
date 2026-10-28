# Runbook — instalar, cadastrar, parear e administrar

Procedimento para pôr o kiosk para funcionar num PC Windows e administrá-lo pelo CLI. Decisões de origem: [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) (uma conta por criança), [ADR-005](../adr/ADR-005-rede-da-api-de-comando.md) (rede) e [ADR-006](../adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md) (assinatura e pareamento). Épicos: [E-004](../epics/E-004-agente-windows-enforcement.md) e [E-005](../epics/E-005-api-comando-e-pareamento.md).

> Pré-lançamento: o agente encerra apps em Windows real, mas bloqueio de sessão, logoff e avisos ainda não foram validados com uma criança logada. O `kioskd` ainda roda como programa de console, não como serviço do Windows. Não use como única proteção.

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
$env:KIOSK_DB = "sqlite://C:/ProgramData/kiosk/kiosk.db"
$env:KIOSK_LISTEN = "100.95.52.48:8443"   # o IP do `tailscale ip -4`, com a porta
```

- `KIOSK_DB`: onde fica o banco. A pasta é criada se faltar. O arquivo tem dados de crianças: deixe a pasta acessível só a SYSTEM e Administradores.
- `KIOSK_LISTEN`: `ip:porta`, obrigatória para `servir`. Só são aceitos loopback (`127.0.0.1`, `::1`) e a rede Tailscale (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`); `0.0.0.0` e endereços de LAN são recusados.
- As variáveis valem só para a janela atual do PowerShell; defina-as de novo em cada janela.

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

```powershell
.\kioskd.exe servir
```

Deve aparecer `escutando em 100.95.52.48:8443` e `agente de enforcement ativo (tick de 2s)`. Deixe a janela aberta. O Windows pode pedir permissão de firewall para a porta: libere só para redes privadas (a Tailscale).

## 5. Parear o dispositivo dos pais

Numa **segunda** janela do PowerShell, com o mesmo `$env:KIOSK_DB`:

```powershell
.\kioskd.exe codigo-pareamento
```

O código é de uso único e vale por 5 minutos. No dispositivo dos pais, com o `kioskctl` do pacote:

```powershell
.\kioskctl.exe parear --servidor 100.95.52.48:8443 --codigo ABCD2345 --nome "Celular da mãe"
```

O CLI gera o par de chaves Ed25519, envia só a chave pública e grava `credenciais.json` com a chave privada (`%APPDATA%\kiosk\credenciais.json` no Windows, `~/.config/kiosk/credenciais.json` no Linux e macOS, ou `KIOSK_CREDENCIAIS`). Dez códigos errados em 5 minutos bloqueiam novos pareamentos por esse período.

## 6. Administrar as regras

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
