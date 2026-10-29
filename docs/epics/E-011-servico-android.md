# E-011 — Serviço em Android

Estado: **congelado** desde a criação (2026-10-29, pedido do dono). Responsável: a definir. Repositório: a definir (provavelmente separado, por causa da toolchain Android).

## Resultado e escopo

Um app no **celular ou tablet Android da criança** aplica as regras definidas pelos pais: uso por app e total, avisos, bloqueio de app e do aparelho fora do horário, log forense local e a mesma API de comando, alcançada pela Tailscale.

- **Inclui**: o app de enforcement, o pareamento com o cliente dos pais, distribuição e CI/CD próprio (build, testes em emulador e aparelho, assinatura do APK/AAB).
- **Exclui**: o app dos pais no celular, que é o [E-006](E-006-app-mobile-administracao.md). Este épico é o lado da criança.

## Origem e decisões

- Origem: pedido do dono em 2026-10-29.
- **GAP Estrutural maior que nos desktops**: o Android não tem várias contas por aparelho no uso comum, nem um serviço root, nem um "encerrar processo de outro usuário". A ADR de retomada revisa a [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) (e possivelmente a [ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md)) e decide:
  - identidade: o aparelho é da criança, ou perfis de usuário do Android;
  - mecanismo: Device Owner/Android Enterprise (DevicePolicyManager), ou UsageStats com Acessibilidade;
  - reaproveitamento do núcleo Rust (motor, uso e assinatura via JNI) contra reescrita em Kotlin;
  - política da Play Store para apps de controle parental, ou distribuição fora dela;
  - antiadulteração: impedir que a criança desinstale o app ou desligue a permissão.
- Concorre com o Family Link do Google; a ADR registra o que o kiosk oferece além dele.

## Condição de retomada

As mesmas do [E-009](E-009-servico-linux.md): `kioskd` no Windows maduro **e** cliente gráfico maduro ([E-008](E-008-cliente-grafico-dos-pais.md) F4). Como o lado dos pais no celular é o [E-006](E-006-app-mobile-administracao.md), avaliar na retomada se ele vem antes.

## Fases, aceites e encerramento

A definir na retomada, junto do CI/CD do épico. Aceite mínimo esperado: a criança não consegue desinstalar o app nem revogar suas permissões sem o consentimento dos pais, com controle negativo em aparelho real.
