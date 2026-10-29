# Comandos em português

Os comandos do `kioskd` e do `kioskctl` são documentados em inglês, que é a convenção. Os nomes em português abaixo **equivalem para sempre** e podem ser misturados livremente ([ADR-010](../adr/ADR-010-idiomas-e-comandos-em-ingles.md)): `kioskd estado` é o mesmo que `kioskd status`, e `kioskd perfil criar --conta Ana` é o mesmo que `kioskd profile create --account Ana`.

A tradução vale para o comando, para o subcomando (depois de `profile`, `app` ou `rule`) e para os nomes de opção. Os valores (nomes, IDs, horários) não mudam. Dias da semana aceitam `mon,tue,...`, `seg,ter,...` ou `0` a `6`.

A API fala só inglês; isto vale apenas para quem digita os comandos.

## Comandos e subcomandos

| Português | Inglês |
|---|---|
| `servir` | `serve` |
| `codigo-pareamento` | `pairing-code` |
| `dispositivos` | `devices` |
| `revogar` | `revoke` |
| `perfil` | `profile` |
| `criar` | `create` |
| `listar` | `list` |
| `limite` | `limit` |
| `janela` | `window` |
| `regra` | `rule` |
| `ativar` | `activate` |
| `instalar` | `install` |
| `desinstalar` | `uninstall` |
| `estado` | `status` |
| `versao` | `version` |
| `servico` | `service` |
| `parear` | `pair` |
| `uso` | `usage` |

## Opções

| Português | Inglês |
|---|---|
| `--sem-agente` | `--no-agent` |
| `--conta` | `--account` |
| `--nome` | `--name` |
| `--limite-total` | `--total-limit` |
| `--segundos` | `--seconds` |
| `--sem-limite` | `--no-limit` |
| `--dias` | `--days` |
| `--janela` | `--window` |
| `--processo` | `--process` |
| `--isento` | `--exempt` |
| `--ignorar` | `--ignore` |
| `--perfil` | `--profile` |
| `--limite` | `--limit` |
| `--ate` | `--until` |
| `--ativar` | `--activate` |
| `--escutar` | `--listen` |
| `--apagar-dados` | `--purge-data` |
| `--servidor` | `--server` |
| `--codigo` | `--code` |
| `--data` | `--date` |

## Exemplos

| Português | Inglês |
|---|---|
| `kioskd servir` | `kioskd serve` |
| `kioskd codigo-pareamento` | `kioskd pairing-code` |
| `kioskd perfil criar --conta Ana --nome Ana --limite-total 7200` | `kioskd profile create --account Ana --name Ana --total-limit 7200` |
| `kioskd regra criar --perfil ID --app ID --limite 3600 --ativar` | `kioskd rule create --profile ID --app ID --limit 3600 --activate` |
| `kioskd instalar --escutar 127.0.0.1:8443` | `kioskd install --listen 127.0.0.1:8443` |
| `kioskd estado` | `kioskd status` |
| `kioskd versao` | `kioskd version` |
| `kioskctl parear --servidor 127.0.0.1:8443 --codigo ABCD2345` | `kioskctl pair --server 127.0.0.1:8443 --code ABCD2345` |
| `kioskctl uso --perfil ID` | `kioskctl usage --profile ID` |

Fluxograma de cada comando: [fluxos dos comandos](../architecture/fluxos-dos-comandos.md). Passo a passo: [runbook](api-e-cli.md).
