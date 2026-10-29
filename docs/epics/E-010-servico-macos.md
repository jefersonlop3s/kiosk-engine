# E-010 — Serviço em macOS

Estado: **congelado** desde a criação (2026-10-29, pedido do dono). Responsável: a definir. Repositório: kiosk (a confirmar).

## Resultado e escopo

O `kioskd` aplica as regras num **Mac** usado pela criança, com o mesmo modelo do Windows: perfil por conta, uso, avisos, encerramento de app, bloqueio e encerramento de sessão, log forense e API de comando.

- **Inclui**: adaptador `Sistema` para macOS, daemon do sistema, instalação e remoção, CI/CD próprio (runner macOS, pacote assinado e notarizado).
- **Exclui**: o cliente dos pais em macOS, que vem no [E-008](E-008-cliente-grafico-dos-pais.md).

## Origem e decisões

- Origem: pedido do dono em 2026-10-29.
- Reaproveita o núcleo independente de SO e a interface `Sistema` ([E-004](E-004-agente-windows-enforcement.md)).
- **GAP Estrutural**: revisar a [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) para o macOS, numa ADR nova na retomada. Ela decide:
  - identidade pelo usuário macOS;
  - LaunchDaemon como root, mais um LaunchAgent por sessão para os avisos;
  - como bloquear e encerrar a sessão por APIs públicas;
  - lista de processos;
  - permissões do TCC;
  - se usa Endpoint Security, que exige autorização da Apple.
- **Decisão do dono na retomada**: conta de desenvolvedor Apple (Developer ID e notarização) tem custo anual. Sem ela, o Gatekeeper bloqueia o pacote.
- Concorre com o Tempo de Uso nativo da Apple; a ADR registra o que o kiosk oferece além dele (regras pela mesma API dos PCs Windows, log forense local).

## Autoridade local: elevação e responsáveis (dono, 2026-10-29)

Requisito para a retomada, equivalente à regra do Windows (só administrador gera código; ver [E-008](E-008-cliente-grafico-dos-pais.md)):

- **Código de pareamento** só com elevação de privilégio (`sudo kioskd codigo-pareamento`); um usuário comum é recusado, com evento no log forense.
- **Responsáveis**: também com `sudo`, define-se quais usuários do sistema são **responsáveis** por quais contas monitoradas, as das crianças e adolescentes. É um conceito que o modelo de dados não tem hoje: o perfil liga uma conta a regras, mas não registra quem responde por ela. A ADR de retomada decide:
  - onde a relação responsável ↔ perfil fica gravada, com evento no log forense a cada mudança;
  - o que ela libera, por exemplo quem pode gerar código para qual perfil e quem recebe os alertas;
  - se o mesmo conceito passa a valer no Windows, para os administradores.
- **Controles negativos**: a conta monitorada não gera código, não se torna responsável de ninguém e não altera a lista de responsáveis.

## Condição de retomada

As mesmas do [E-009](E-009-servico-linux.md): `kioskd` no Windows maduro (E-004 e E-007 concluídos, sem GAP de enforcement aberto, uso real sem regressão) **e** cliente gráfico maduro ([E-008](E-008-cliente-grafico-dos-pais.md) F4).

## Fases, aceites e encerramento

A definir na retomada, junto do CI/CD do épico. Aceite mínimo esperado: a conta padrão da criança não consegue parar o daemon nem ler os dados, com controle negativo no CI.
