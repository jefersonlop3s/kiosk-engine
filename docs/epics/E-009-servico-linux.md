# E-009 — Serviço em Linux

Estado: **congelado** desde a criação (2026-10-29, pedido do dono). Responsável: a definir. Repositório: kiosk (a confirmar).

## Resultado e escopo

O `kioskd` aplica as regras num computador **Linux** usado pela criança, com o mesmo modelo do Windows: um perfil por conta, uso por app e por sessão, avisos, encerramento de app, bloqueio e encerramento de sessão, log forense e API de comando para os pais.

- **Inclui**: adaptador `Sistema` para Linux, serviço do sistema, instalação e remoção, CI/CD próprio (testes no Linux real e pacotes).
- **Exclui**: o que o [E-008](E-008-cliente-grafico-dos-pais.md) já entrega (o cliente dos pais roda em Linux desde lá).

## Origem e decisões

- Origem: pedido do dono em 2026-10-29.
- O núcleo já é independente de SO (motor, uso, banco, log forense, API, assinatura); o ponto de extensão é a interface `Sistema` do agente ([E-004](E-004-agente-windows-enforcement.md)). O CI Linux já roda toda a parte comum.
- **GAP Estrutural**: a [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) restringe o alvo ao Windows. Na retomada, uma ADR nova revisa essa restrição para o Linux e decide:
  - identidade: o usuário Unix (UID) no lugar do SID;
  - sessões, bloqueio e logoff via systemd-logind;
  - processos via `/proc`;
  - avisos na sessão gráfica (D-Bus de notificações), com X11 e Wayland;
  - serviço systemd como root;
  - dados em `/var/lib/kiosk` só para root;
  - distribuições suportadas e formato de pacote.

## Condição de retomada

Só começa quando **as duas condições** valerem:

1. **`kioskd` no Windows maduro**: [E-004](E-004-agente-windows-enforcement.md) e [E-007](E-007-onboarding-e-instalador.md) concluídos, sem GAP aberto de enforcement (hoje GC-16, GC-17 e GC-20), e um período de uso real pelo dono sem regressão;
2. **Cliente gráfico maduro** ([E-008](E-008-cliente-grafico-dos-pais.md) F4): uso fluido e consistente em Windows, Linux e macOS.

## Fases, aceites e encerramento

A definir na retomada, junto do CI/CD do épico. Aceite mínimo esperado: a conta da criança não consegue parar o serviço nem ler os dados, com controle negativo no CI, como no Windows.
