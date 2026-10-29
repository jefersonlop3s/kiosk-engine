# E-004 — Agente Windows de enforcement

Estado: em andamento (adaptador Win32 e agente no serviço em 2026-10-28; faltam SCM, bandeja e teste em sessão interativa). Responsável: a definir. Repositório: kiosk.

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
- **ADR aceita (2026-10-29)**: [ADR-011](../adr/ADR-011-rastro-de-tentativas-sem-privilegio.md), rastro das tentativas de comando administrativo sem privilégio (GC-21), por named pipe local que o serviço escuta e grava no banco. Implementada na F4 e validada no CI Windows; falta ver no PC real.
- [Insight sobre o relatório de hardening](../insights/2026-10-27-relatorio-hardening-windows.md): os poucos itens aproveitáveis entram em F4.

## Fases e dependências

| Fase | Entrega | Depende de | Evidência |
|---|---|---|---|
| F0 | Ambiente: cross-compilação macOS → `x86_64-pc-windows-msvc` ou VM/máquina Windows para testes | — | Binário roda numa VM Windows |
| F1 | Serviço instalável, detecção de processos e mapeamento para o SID | F0, [E-001](E-001-fundacao-crate-e-dados.md) | Log de processos de duas contas distintas |
| F2 | Ações: encerrar processo e bloquear sessão, guiadas por `decidir` | F1, [E-003](E-003-motor-de-regras-e-uso.md) F1 | Vídeo/log em Windows real; app permitido intocado |
| F3 | UI de bandeja: tempo restante, avisos de 5 e 1 min, motivo do bloqueio | F2 | Captura de tela em Windows real |
| F4 | Antiadulteração: conta da criança não para o serviço nem mata a UI (ACL do serviço, recuperação automática, heartbeat UI ↔ serviço gerando `TAMPER_SUSPECTED`/`AGENT_STOPPED`) | F2 | Teste negativo: `sc stop` e `taskkill` como usuário padrão falham ou são registrados |

## Entregue sem depender de Windows nem das ADRs (2026-10-28)

- `agente::Agente` com a interface `Sistema` (listar processos, encerrar, bloquear sessão, avisar): contabilidade por tick, avisos nos marcos de 5 e 1 min sem repetir, encerramento com tolerância e desistência após 3 falhas (`TAMPER_SUSPECTED`), bloqueio de sessão com intervalo de rebloqueio e eventos de log em cada ação.
- Leituras do banco para o motor: perfil por SID, app por nome do executável (sem caixa), janela do dia da semana e regras do app (`decidir_para`, `decidir_app`).
- Tempo de sessão ativa (`session_usage`, [ADR-003](../adr/ADR-003-deteccao-de-processos-e-tempo-de-uso.md)): o total do perfil deixa de ser a soma dos apps; só conta quando a sessão estava ativa no tick anterior e no atual.
- Política de sessão decidida pelo dono: lock e, após 2 desbloqueios com a restrição valendo, logoff (`Sistema::encerrar_sessao`).
- Protocolo serviço ↔ UI ([ADR-004](../adr/ADR-004-comunicacao-servico-ui-por-named-pipe.md)) independente de transporte: mensagens JSON por linha versionadas, decodificação defensiva da entrada da UI, limite de taxa e monitor de batimento (3 batimentos perdidos = alerta único). O named pipe com DACL e a UI de bandeja exigem Windows.
- Testes com `Sistema` falso contra SQLite real. O adaptador Win32, o pipe da UI e o total por sessão ficam para depois do aceite das ADRs.

## Adaptador Win32 (2026-10-28)

- `sistema_windows::SistemaWindows` implementa `Sistema`: processos por `CreateToolhelp32Snapshot` com o SID do dono (`TokenUser`); sessões logadas e desbloqueadas por `WTSEnumerateSessionsW` + `WTSSessionInfoEx`; `TerminateProcess`; bloqueio por `WTSDisconnectSession`; logoff por `WTSLogoffSession`; avisos por `WTSSendMessageW` (provisório até a bandeja, GC-17).
- `kioskd servir` roda o agente a cada 2 s e a limpeza diária do log; `--sem-agente` desliga o agente.
- Textos pt-BR dos avisos em `textos` (testáveis fora do Windows).
- F0 resolvida por dois caminhos: verificação cruzada local (`--target x86_64-pc-windows-gnu`, com mingw) e o runner `windows-latest` do CI, que roda `tests/windows.rs` em Windows real.

## Antiadulteração: `AGENT_STOPPED` por batimento no banco (2026-10-29)

- Desvio da ADR-004 (que previa `AGENT_STOPPED` na parada do serviço): quem sabe que o serviço parou é o **próximo** agente, não o que parou. O agente grava um batimento no banco a cada 10 s (`agent_heartbeat`, uma linha, migração 0007), o serviço marca a parada normal ao receber o pedido do Windows, e o agente que sobe compara com agora. Intervalo maior que 3 batimentos (30 s) vira `AGENT_STOPPED`: `SUCCESS` se foi parada de propósito (inclui o desligamento do PC), `FAILURE` se o agente sumiu (falha, encerrado à força, falta de energia). O reinício de 5 s depois de uma falha não gera evento.
- Salto de relógio: já coberto pelo E-003 (`registrar_sessao` recusa intervalos negativos ou acima de 600 s, grava `TAMPER_SUSPECTED` e não conta o intervalo).
- Testes: `tests/agente_parada.rs` (9, com tempo simulado e SQLite real) e, no CI Windows, parada de propósito e `taskkill` seguidos de reinício com o intervalo acima do limiar.
- Falta da F4: o batimento da UI (3 batimentos perdidos geram `TAMPER_SUSPECTED`) e o teste negativo de `taskkill` na UI, que dependem da bandeja (F3).

## Aceites

- Todo aceite deste épico exige Windows real ou VM; build no macOS não certifica comportamento.
- Textos da UI em pt-BR; outros idiomas fora do escopo do MVP.

## Encerramento

A preencher.
