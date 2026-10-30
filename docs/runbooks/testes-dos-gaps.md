# Runbook — stack de testes dos GAPs abertos

Roteiro para o dono validar no PC Windows real os GAPs de Código abertos no [PENDING](../PENDING.md): GC-15, GC-16, GC-17, GC-20 e GC-21. Complementa o [runbook de instalação e administração](api-e-cli.md); os comandos têm fluxograma em [fluxos dos comandos](../architecture/fluxos-dos-comandos.md).

## 0. Qual versão testar

O que está no PC é a **0.0.7**. Ela **não** contém o que entrou depois (GC-08, GC-13, GC-15 mínimo, hash encadeado, GC-19): o `Cargo.toml` ainda diz `0.0.7` e as mudanças estão em "Não lançado" no [CHANGELOG](../../CHANGELOG.md).

Para os testes valerem, é preciso uma **0.0.8**: só assim `kioskd status` mostra `versão` e `versão instalada` distintas da 0.0.7 e o `kioskd version` (commit) prova qual compilação foi exercitada. A publicação passa pelo `release.yml` (tag `v0.0.8`, aprovação do dono).

Antes de cada rodada, confira:

```powershell
kioskd version
kioskd status        # "versão" e "versão instalada" iguais e = 0.0.8
```

## 1. Camadas

| Camada | Onde | Cobre | Como |
|---|---|---|---|
| 1. Automática | CI Ubuntu e Windows | motor, API, banco, CLI, serviço real no runner | `cargo test --locked` |
| 2. Serviço no PC | PowerShell administrador | GC-16, GC-21 | seções 3 e 4 |
| 3. Sessão da criança | PC com 2 contas (pais e criança) | GC-17, GC-20 | seções 5 e 6 |
| 4. Rede | Dispositivo dos pais na LAN | GC-15 | seção 7 |

Preparação única (camada 2 em diante): conta padrão `crianca-teste`, perfil e um app simples (Notepad) cadastrados conforme o [runbook](api-e-cli.md) seções 3 e 4.1. Anote os IDs: `$P` (perfil) e `$A` (app).

Evidência de cada teste: cole no PR a saída de `kioskd version`, o trecho de `kioskctl log --profile $P` e o trecho de `C:\ProgramData\kiosk\kioskd.log`.

## 2. Camada 1: gate automático

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --locked
```

No Windows, para incluir o teste que instala o serviço de verdade e exigir sessão interativa:

```powershell
$env:KIOSK_TESTE_SERVICO = "1"
$env:KIOSK_TESTE_EXIGE_SESSAO = "1"
cargo test --locked
```

`KIOSK_TESTE_PROCESSO_COMO_CONTA=1` só no PC do dono (trava nos runners); cobre o GC-21 com o `kioskd.exe` inteiro como conta padrão.

## 3. GC-16: o agente como SYSTEM aplica regra na conta da criança

1. Janela do administrador: `kioskd rule create --profile $P --app $A --limit 120 --activate` (limite curto; confira antes o uso do dia com `kioskctl usage --profile $P`).
2. Entrar como `crianca-teste` e abrir o app.
3. **Esperado**: avisos no marco configurado, depois o app é encerrado; `kioskctl log --profile $P` mostra o evento de limite e o encerramento; `kioskd.log` registra a ação com o PID do serviço.
4. Revogar a regra (`kioskd rule revoke <id>`) e confirmar que o app abre de novo.

Falhou se: o app segue aberto sem erro no log (fail-loud violado).

## 4. GC-21: recusa com o serviço parado

1. Entrar como `crianca-teste` (conta comum), PowerShell comum: `kioskd pairing-code`. **Esperado**: recusa por falta de administrador; tentativa visível em `kioskctl log --profile $P`.
2. Administrador: `Stop-Service kiosk`.
3. Repetir o passo 1 na conta comum. **Esperado**: recusa dizendo que a tentativa **não** foi registrada.
4. `Start-Service kiosk` e confirmar com `kioskd status`.

## 5. GC-17: avisos pela bandeja, com a caixa de mensagem como reserva

Hoje os avisos saem por `WTSSendMessageW`; o teste mostra o estado atual e serve de linha de base para a etapa 4 da bandeja.

1. Com `crianca-teste` logada, confirmar que `kiosk-ui.exe` está em execução (bandeja) e que o serviço o reinicia ao encerrá-lo.
2. Regra com limite que dispare aviso de 5 e 1 min.
3. Registrar: o aviso apareceu como caixa de mensagem ou pela bandeja? O tempo restante aparece na bandeja?
4. Encerrar `kiosk-ui.exe` antes do aviso e ver se o aviso ainda chega (reserva).

Resultado esperado hoje: caixa de mensagem. A mudança para a bandeja é desenvolvimento, não teste.

## 6. GC-20: logoff após 2 desbloqueios

Repetir o cenário que não reproduziu em 2026-10-29, com a janela do `kioskd serve` visível se rodar em console (pare o serviço antes: os dois disputam a porta).

1. Regra de limite diário total já estourado para `$P`, de modo que a sessão bloqueie ao entrar.
2. Entrar como `crianca-teste`, esperar o bloqueio (1).
3. Aguardar **40 s** e desbloquear com a senha da criança (2); esperar o bloqueio de novo.
4. Repetir uma segunda vez com 40 s de intervalo.
5. **Esperado**: após o segundo desbloqueio, logoff da conta e evento no log.
6. **Se não ocorrer**: guarde o log completo (30 s antes e 7 min depois) e abra a investigação da detecção de sessão reconectada (`WTSSessionInfoEx`), como indicado no PENDING.

## 7. GC-15: modo LAN

Só há HTTP sem TLS (`KIOSK_LAN=1` aceita endereços privados RFC 1918). Teste de comportamento, não de aceite do GC:

1. `$env:KIOSK_LAN = "1"` e `KIOSK_LISTEN` com o IP da LAN; `kioskd serve`.
2. Parear um `kioskctl` de outro dispositivo da LAN.
3. Sem `KIOSK_LAN`, o mesmo endereço deve ser **recusado** na subida.
4. O GC só fecha com TLS (`tokio-rustls`) e revisão de pinning.

## 8. O que ainda não tem como testar

- GC-08 (avisos por regra): não há opção no `kioskd`/`kioskctl` para definir `warning_seconds`; só o banco e os testes automáticos cobrem.
- Hash encadeado ([ADR-012](../adr/ADR-012-hash-encadeado-security-log.md)): `kioskd log verify` está "a propor"; não existe comando para conferir a corrente em campo.
- GC-13 (pareamento por IP): coberto por teste automático; em campo, errar o código 10 vezes de um IP não pode bloquear o pareamento de outro IP.
