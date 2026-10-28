# E-004 — Agente Windows de enforcement

Estado: proposto. Responsável: a definir. Repositório: kiosk.

## Resultado e escopo

**Resultado observável**: no Windows 10/11, com uma criança logada na própria conta, o agente:

- detecta os processos que ela abre;
- consulta o motor ([E-003](E-003-motor-de-regras-e-uso.md));
- mostra o tempo restante na bandeja;
- avisa antes de agir;
- encerra o app ou bloqueia a sessão;
- registra tudo ([E-002](E-002-log-forense.md)).

- **Inclui**: serviço Windows (conta SYSTEM) com o banco e o motor; processo de UI por sessão de usuário (bandeja e avisos); mapeamento processo → SID → perfil; ações de encerrar processo e bloquear sessão; proteções contra adulteração.
- **Exclui**: comandos remotos ([E-005](E-005-api-comando-e-pareamento.md)) e bloqueio de sites ou conteúdo web.
- **Pressupostos**: os de [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md). Os pais têm a única conta administradora.

## Origem e decisões

- [ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md) (Aceito): enforcement ativo e visível.
- [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md) (Aceito): perfil por SID, somente Windows.
- **ADRs propostas (2026-10-28), aguardando aceite do dono**:
  - [ADR-003](../adr/ADR-003-deteccao-de-processos-e-tempo-de-uso.md): detecção por polling de 2 s e definição de tempo de uso (app enquanto o processo existe; total enquanto a sessão está ativa);
  - [ADR-004](../adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md): serviço ↔ UI por named pipe com DACL, o serviço inicia a UI e o batimento detecta adulteração.
- [Insight sobre o relatório de hardening](../insights/2026-10-27-relatorio-hardening-windows.md): os poucos itens aproveitáveis entram em F4.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F0 | Ambiente: cross-compilação macOS → `x86_64-pc-windows-msvc` ou VM/máquina Windows para testes | — | Binário roda numa VM Windows |
| F1 | Serviço instalável, detecção de processos e mapeamento para o SID | F0, [E-001](E-001-fundacao-crate-e-dados.md) | Log de processos de duas contas distintas |
| F2 | Ações: encerrar processo e bloquear sessão, guiadas por `decidir` | F1, [E-003](E-003-motor-de-regras-e-uso.md) F1 | Vídeo/log em Windows real; app permitido intocado |
| F3 | UI de bandeja: tempo restante, avisos de 5 e 1 min, motivo do bloqueio | F2 | Captura de tela em Windows real |
| F4 | Antiadulteração: conta da criança não para o serviço nem mata a UI (ACL do serviço, recuperação automática, heartbeat UI ↔ serviço gerando `TAMPER_SUSPECTED`/`AGENT_STOPPED`) | F2 | Teste negativo: `sc stop` e `taskkill` como usuário padrão falham ou são registrados |

## Aceites

- Todo aceite deste épico exige Windows real ou VM; build no macOS não certifica comportamento.
- Textos da UI em pt-BR; outros idiomas fora do escopo do MVP.

## Encerramento

A preencher.
