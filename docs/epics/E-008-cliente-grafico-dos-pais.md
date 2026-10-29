# E-008 — Cliente gráfico dos pais (Windows, Linux e macOS)

Estado: proposto em 2026-10-29 (pedido do dono). Responsável: a definir. Repositório: kiosk (a confirmar na ADR de abertura).

## Resultado e escopo

**Problema**: hoje os pais administram o PC da criança só pelo terminal. Isso inclui parear, criar, ativar, revogar e limitar regras e consultar uso e log com o `kioskctl`. O caminho funciona, mas exige copiar IDs entre comandos e conhecer a sintaxe, o mesmo atrito que o [E-007](E-007-onboarding-e-instalador.md) resolve do lado do PC.

**Resultado observável**: num computador dos pais com Windows, Linux ou macOS, na rede Tailscale, um aplicativo gráfico:

- pareia com o `kioskd` do PC da criança (código de 8 caracteres, [ADR-006](../adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md)) e guarda a credencial no mesmo lugar do `kioskctl`;
- mostra os perfis, os apps e as regras, sem digitar IDs;
- cria, ativa, revoga e altera o limite das regras;
- mostra o uso do dia por criança e por app e o log de eventos, com destaque para `TAMPER_SUSPECTED` e `AGENT_STOPPED`;
- deixa claro quando o PC está fora de alcance e recusa sem fingir sucesso (fail-loud).

- **Inclui**: o app nas três plataformas, empacotamento e CI/CD próprios (build, testes e pacotes por plataforma), extensões da API que o app precisar.
- **Exclui**: o app mobile ([E-006](E-006-app-mobile-administracao.md)); administrar sem Tailscale ou pela LAN sem HTTPS (GC-15 no [PENDING](../PENDING.md)); qualquer enforcement no computador dos pais.
- **Pressupostos**: o `kioskd` no PC Windows segue sendo a única fonte de verdade; o app não abre o banco, só fala com a API de comando ([E-005](E-005-api-comando-e-pareamento.md)), com a mesma assinatura e as mesmas recusas do `kioskctl`.

## Regra de autoridade do pareamento (dono, 2026-10-29)

O cliente só sincroniza com um PC se o pareamento partir de uma **conta de administrador no Windows que roda o serviço** `kioskd`. O código de pareamento é a única porta de entrada de um dispositivo novo ([ADR-006](../adr/ADR-006-assinatura-de-requisicoes-e-pareamento.md)). Por isso:

- `kioskd codigo-pareamento` só pode ser executado por administrador elevado (PowerShell como Administrador) no próprio PC do serviço.
- Uma conta padrão, como a da criança, não gera código e, portanto, não pareia nenhum cliente, nem o próprio.
- **Vários responsáveis**: todo administrador do PC é responsável por todas as contas locais monitoradas, por exemplo pai, mãe ou outro responsável com conta administrativa. Cada um gera o próprio código e pareia o próprio cliente. Não existe vínculo de um administrador com uma criança específica. Vale também para Linux e macOS ([E-009](E-009-servico-linux.md), [E-010](E-010-servico-macos.md)). Delegar a quem não é administrador fica como hipótese no [insight do responsável temporário](../insights/2026-10-29-responsavel-temporario.md).
- **Antecipado a pedido do dono (2026-10-29)**: o `kioskd codigo-pareamento` confere se quem chama é administrador elevado (grupo Administradores ativo, e não só para negação, no token efetivo). Se não for, recusa, grava `AUTHZ_FAIL` com a ação `CREATE_PAIRING_CODE` no log forense e não cria código. Antes disso, a barreira era só a ACL da pasta do banco, que não existe sem o `instalar` ou com `KIOSK_DB` em outro lugar. Testes: controle positivo no runner Windows (administrador elevado) e negativo com uma conta padrão real em `tests/servico_windows.rs`.

## Origem e decisões

- Origem: pedido do dono em 2026-10-29, logo depois da publicação da 0.0.4.
- "Controlando via `kioskctl`": o app usa o mesmo cliente do `kioskctl` (`src/cliente.rs`: credenciais, assinatura e chamadas), não um protocolo novo. A ADR de abertura decide entre embutir essa biblioteca ou chamar o executável `kioskctl`. A recomendação inicial é embutir: assim a credencial não passa por linha de comando e os erros chegam estruturados.
- **Lacuna conhecida na API**: hoje a API só cria, ativa, revoga e limita regras e consulta uso e log. **Não há listagem remota** de perfis, apps e regras (só existe local, no `kioskd perfil|app|regra listar`). Sem isso, o app teria a mesma dependência de IDs copiados. A F1 abre essas ações na API, com as Camadas 0 a 5 e fluxograma por comando ([AGENTS.md](../../AGENTS.md#fluxogramas-dos-comandos)); também servem ao [E-006](E-006-app-mobile-administracao.md).
- **ADR a abrir antes da F2** (nenhuma aceita ainda): toolkit gráfico (por exemplo Tauri, Slint ou egui; critérios: Rust, três plataformas, acessibilidade, tamanho do pacote), repositório e assinatura de código por plataforma (Authenticode, notarização da Apple; custo é decisão do dono).

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F1 | API: listar perfis, apps e regras remotamente; `kioskctl perfil|app|regra listar` usando essas ações. (a exigência de administrador no `codigo-pareamento` já foi antecipada) | [E-005](E-005-api-comando-e-pareamento.md) | Testes de API com assinatura, recusa sem assinatura e sem vazar detalhe; fluxogramas |
| F2 | App mínimo nas três plataformas: parear, listar, criar/ativar/revogar/limitar regra, uso e log | F1, ADR do toolkit aceita | Teste de ponta a ponta contra um `kioskd servir` real no CI de cada plataforma |
| F3 | CI/CD: pacotes por plataforma (`.msi`/`.zip`, AppImage ou `.deb`, `.dmg`) publicados no release, com SHA-256 | F2 | Release com os três pacotes; instalação limpa em cada SO |
| F4 | Uso fluido: atualização automática da tela, PC fora de alcance, alertas de antiadulteração em destaque | F2 | Uso real pelo dono no próprio computador administrando o PC da criança |

O CI atual roda Ubuntu e Windows; a F2 acrescenta o runner macOS.

## Aceites

- Os pais administram o PC Windows de um computador Windows, Linux e macOS sem abrir o terminal.
- **Controles negativos**: conta padrão do Windows não gera código de pareamento; sem pareamento, o app não comanda nada; dispositivo revogado perde o acesso na hora; PC fora de alcance aparece como erro, nunca como sucesso; o app nunca mostra o detalhe que a API omite (AUTH_FAIL, AUTHZ_FAIL, VALIDATION_FAIL, INTERNAL_ERROR).
- Textos em pt-BR.

## Encerramento

A preencher.
