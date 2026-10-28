# ADR-005 — Topologia de rede da API de comando

Status: Aceito em 2026-10-28 pelo dono (Jeferson Lopes). Data: 2026-10-28. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-005](../epics/E-005-api-comando-e-pareamento.md) F1.

## Contexto

Os pais administram regras e consultam o log por um dispositivo próprio ([E-005](../epics/E-005-api-comando-e-pareamento.md); depois o app de [E-006](../epics/E-006-app-mobile-administracao.md)). A [arquitetura](../architecture/README.md) registra `client_ip: 100.x.x.x`, o que sugere a faixa CGNAT do Tailscale, mas a topologia nunca foi decidida. O serviço roda no PC da criança, na rede doméstica; a máquina é alcançável pela criança e por qualquer aparelho da casa.

Restrições do produto: dados de crianças ficam locais ([E-002](../epics/E-002-log-forense.md)); nenhum servidor de terceiros guarda regras ou log.

## Decisão

Proposta:

1. **A API não escuta em `0.0.0.0` nem na internet.** O endereço de escuta é explícito na configuração, e o serviço recusa iniciar sem ele.
2. **Padrão: rede privada Tailscale (tailnet)** entre o PC e o dispositivo dos pais. O serviço escuta só na interface da tailnet e no loopback (CLI local). O transporte já é cifrado e autenticado por dispositivo (WireGuard); acesso fora de casa funciona sem abrir portas.
3. **Opção: LAN**, ligada pelo dono por configuração explícita, com a API em **HTTPS** com certificado autoassinado fixado (*pinning*) no pareamento, porque a assinatura de requisições ([ADR-006](ADR-006-assinatura-de-requisicoes-e-pareamento.md)) garante autenticidade e integridade, mas não confidencialidade.
4. O `client_ip` registrado no log é o endereço do par na conexão (nunca um cabeçalho enviado pelo cliente).
5. A API **não** é uma fronteira de segurança sozinha: autenticação e autorização são as da ADR-006, em qualquer rede.

Alternativas rejeitadas:
- **Port forwarding / IP público**: expõe um serviço que controla a máquina de uma criança à internet inteira.
- **Relay em nuvem** (app dos pais ↔ servidor ↔ PC): resolve NAT, mas põe regras e log em terceiros e cria um servidor a operar; contraria o princípio de dados locais.
- **Somente LAN**: mais simples e sem dependência externa, mas os pais não administram fora de casa. Fica como modo opcional, não padrão.
- **Outra VPN mesh (WireGuard manual, ZeroTier)**: viável e equivalente; Tailscale foi escolhido pela menor operação e porque a arquitetura já cita a faixa 100.x. A API não depende dele: só do endereço de escuta configurado.

## Consequências

- Ganho: acesso remoto sem exposição pública e sem servidor próprio.
- Custo: os pais precisam de conta Tailscale e do cliente instalado nos dois lados; é uma dependência de terceiros para a **disponibilidade** (não para os dados).
- Risco: se a tailnet cair, a administração remota para; as regras já gravadas continuam sendo aplicadas localmente. A criança, sendo usuária padrão, não altera a rede.
- Reversível: trocar a rede muda só o endereço de escuta e o pareamento.
- Validação: teste HTTP real contra o serviço; teste negativo de que a porta não responde em interfaces fora da lista; em Windows real, conferir o que o firewall expõe.

## Referências e evolução

- Consome: [E-005](../epics/E-005-api-comando-e-pareamento.md) F1.
- Depende de: [ADR-006](ADR-006-assinatura-de-requisicoes-e-pareamento.md).
