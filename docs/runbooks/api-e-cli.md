# Runbook — serviço, pareamento e CLI dos pais

Procedimento para subir a API de comando e administrar regras pelo CLI. Decisões de origem: [ADR-005](../adr/ADR-005-rede-da-api-de-comando.md) (rede) e [ADR-006](../adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md) (assinatura e pareamento). Épico: [E-005](../epics/E-005-api-comando-e-pareamento.md).

> Estado em 2026-10-28: o serviço sobe **só a API**. O laço do agente que encerra apps e bloqueia a sessão ([E-004](../epics/E-004-agente-windows-enforcement.md)) ainda não está ligado a ele e depende do adaptador Windows. Validado em Linux (loopback); o firewall do Windows e a rede Tailscale reais não foram exercitados.

## Compilar

```bash
cargo build --release --locked
```

Gera `kioskd` (serviço) e `kioskctl` (CLI dos pais) em `target/release/`.

## Configurar o serviço (conta de administrador do PC)

| Variável | Significado | Padrão |
|---|---|---|
| `KIOSK_DB` | URL do SQLite, por exemplo `sqlite://C:/ProgramData/kiosk/kiosk.db` | `sqlite://kiosk.db` |
| `KIOSK_LISTEN` | endereços de escuta `ip:porta[,ip:porta...]` | **nenhum: o serviço recusa iniciar** |

Endereços aceitos: loopback (`127.0.0.1`, `::1`) e a rede Tailscale (`100.64.0.0/10`, `fd7a:115c:a1e0::/48`). O serviço **recusa** `0.0.0.0`, `::` e qualquer endereço de LAN antes de abrir a porta. Use o IP Tailscale do PC (`tailscale ip -4`).

```bash
KIOSK_DB=sqlite://kiosk.db KIOSK_LISTEN=100.64.0.7:8443 kioskd servir
```

O banco é criado e migrado na primeira execução. O arquivo `.db` contém dados de crianças: proteja-o com permissão só para SYSTEM e Administradores e não o copie para fora do PC.

## Parear o dispositivo dos pais

1. Na conta de administrador do PC, gere o código (uso único, válido por 5 minutos):

   ```bash
   kioskd codigo-pareamento
   ```

2. No dispositivo dos pais:

   ```bash
   kioskctl parear --servidor 100.64.0.7:8443 --codigo ABCD2345 --nome "Celular da mãe"
   ```

   O CLI gera o par de chaves Ed25519, envia só a chave pública e grava `credenciais.json` com a chave privada (permissão `0600` no Unix). Local do arquivo: `KIOSK_CREDENCIAIS`, ou `~/.config/kiosk/credenciais.json` (`%APPDATA%\kiosk\credenciais.json` no Windows).

Dez códigos errados em 5 minutos bloqueiam novos pareamentos por esse período (não afetam dispositivos já pareados).

## Administrar regras

```bash
kioskctl regra criar --perfil <id-do-perfil> --app <id-do-app> --limite 3600 --janela 08:00-20:00
kioskctl regra ativar <id-da-regra>
kioskctl regra revogar <id-da-regra>
```

```bash
kioskctl regra limite <id-da-regra> --segundos 5400      # altera o limite diário (regra não revogada)
kioskctl uso --perfil <id-do-perfil> [--data 2026-03-09] # tempo de sessão e por app do dia
kioskctl log --perfil <id-do-perfil> [--dias 7] [--limite 100]
```

- A regra nasce **pendente**; só `ativar` a põe em vigor.
- `--limite` em segundos (até 86400); `--janela` em `HH:MM-HH:MM`; `--ate` em RFC 3339 (no futuro). Todos opcionais.
- Se o serviço recusar, o CLI diz apenas "recusou o comando": a resposta é genérica de propósito. O motivo exato está no log do PC (`security_log`, eventos `AUTH_FAIL`, `AUTHZ_FAIL` ou `VALIDATION_FAIL`).

## Dispositivos

```bash
kioskd dispositivos          # lista id, nome e estado
kioskd revogar <id>          # vale na próxima requisição
```

Perdeu o celular: revogue o dispositivo no PC e pareie outro.

## Limitações conhecidas

- Sem segundo fator: um dispositivo pareado e desbloqueado nas mãos da criança comanda tudo (ADR-006).
- O `log` mostra o detalhe dos eventos de enforcement, mas não o das falhas de autenticação, autorização e validação (esse fica só no banco do PC).
- Sem modo LAN com HTTPS (GC-15).
- Quem alcança a porta pode errar códigos de pareamento de propósito e bloquear um pareamento novo por 5 minutos (GC-13).
