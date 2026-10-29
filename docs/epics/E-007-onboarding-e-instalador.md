# E-007 — Onboarding e instalador

Estado: em andamento desde 2026-10-29 (ADR-007 a ADR-009 aceitas; começa pela etapa 1 da ADR-008). Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Problema**: hoje, pôr o kiosk para funcionar exige seguir o [runbook](../runbooks/api-e-cli.md) comando a comando: descobrir o nome e o SID da conta da criança, copiar IDs entre comandos, definir variáveis, abrir duas janelas do PowerShell e deixar o `kioskd servir` aberto num console. Os testes do dono em 2026-10-28 e 2026-10-29 tropeçaram exatamente nisso (variáveis por janela, banco em outro lugar, comando no executável errado, IDs copiados à mão).

**Resultado observável**: num Windows recém-instalado, os pais rodam **uma vez** um assistente na conta de administrador e terminam com:

- a pasta e o banco criados, com permissão só para SYSTEM e Administradores;
- os perfis das crianças criados a partir de uma **lista das contas locais** do Windows (escolha por seleção, SID resolvido sem digitar nada);
- os programas a controlar ou isentar escolhidos de uma **lista de programas** (instalados ou em execução), sem digitar o nome do `.exe`;
- as **regras iniciais**: limite diário total por criança e, dentro de cada perfil, limite e janela por programa;
- o endereço de escuta configurado (Tailscale e loopback, nunca LAN sem HTTPS);
- o `kioskd` registrado como **serviço do Windows**, iniciando com a máquina (resolve o GC-16 do [PENDING](../PENDING.md));
- o código de pareamento mostrado no fim, para parear o dispositivo dos pais.

- **Inclui**: assistente de primeira configuração, reexecução para ajustar (idempotente), registro e remoção do serviço, criação da pasta e das permissões, lançador.
- **Exclui**: o app mobile ([E-006](E-006-app-mobile-administracao.md)); instalação remota; configuração de Tailscale (pré-requisito).
- **Pressupostos**: os da [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md): contas locais padrão para as crianças e uma conta de administrador para os pais.

## Origem e decisões

- Origem: pedido do dono em 2026-10-29, depois dos testes no próprio PC ([CONCLUSION](../CONCLUSION.md), entradas de 2026-10-28 e 2026-10-29).
- Reaproveita: `kioskd perfil|app|regra` e `sistema_windows::sid_de_usuario` (a lógica já existe; falta o fluxo guiado).
- **ADRs aceitas pelo dono em 2026-10-29**:
  - [ADR-007](../adr/ADR-007-forma-do-assistente-de-configuracao.md): assistente interativo no terminal (`kioskd configurar`), com modo não interativo para CI e reinstalação; interface gráfica depois, junto da bandeja;
  - [ADR-008](../adr/ADR-008-formato-do-instalador.md): primeiro autoinstalação (`kioskd instalar|desinstalar`, serviço SYSTEM no SCM), depois `.msi` com WiX e winget;
  - [ADR-009](../adr/ADR-009-origem-da-lista-de-programas.md): atalhos do menu Iniciar e processos em execução, com entrada manual; nada além das escolhas é gravado.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F1 | Assistente em terminal: pasta e permissões, lista de contas locais para escolher perfis, lista de programas para escolher apps, regras iniciais, endereço de escuta, código de pareamento | ADR-007 e ADR-009 aceitas | Execução de ponta a ponta num Windows limpo, com o fluxograma no padrão do [AGENTS.md](../../AGENTS.md#fluxogramas-dos-comandos) |
| F2 | `kioskd` como serviço do Windows (SCM, conta SYSTEM, recuperação automática) e remoção limpa | F1, [E-004](E-004-agente-windows-enforcement.md), ADR-008 aceita | Serviço sobrevive a reinício; conta da criança não consegue parar o serviço |
| F3 | Instalador e lançador | F2, ADR-008 aceita | Instalação e desinstalação num Windows limpo sem passos manuais |

## Aceites

- Num Windows limpo, do zero ao primeiro bloqueio validado sem abrir o runbook.
- **Controles negativos**: o assistente não oferece a conta de administrador como perfil de criança; não aceita escuta em `0.0.0.0` nem em LAN sem HTTPS; reexecutar não duplica perfis, apps nem regras.
- Textos em pt-BR.

## Encerramento

A preencher.
