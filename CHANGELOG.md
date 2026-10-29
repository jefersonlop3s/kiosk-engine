# Changelog

Mudanças do projeto kiosk. Formato baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/).

## [Não lançado]

### Adicionado
- **Rastro das tentativas de comando sem privilégio** (ADR-011): quando uma conta sem elevação (a da criança, por exemplo) tenta um comando administrativo do `kioskd`, ele recusa e avisa o serviço por um named pipe local (`kiosk-audit`); o serviço, que é SYSTEM, grava um `AUTHZ_FAIL` no log forense, ligado ao perfil da criança. O chamador é lido do token da conexão, nunca da mensagem; no máximo 5 avisos por minuto por conta. Administrador que esqueceu de elevar também é registrado, marcado como tal. `kioskctl log --profile` mostra a tentativa (sem o SID).
- Todo comando do `kioskd` que mexe nos dados (`pairing-code`, `devices`, `revoke`, `profile *`, `app *`, `rule *`, `install`, `uninstall`) confere a elevação antes de abrir o banco e explica a recusa, no lugar do erro cru do SQLite (`unable to open database file (code: 14)`). A mensagem diz se a tentativa foi ou não registrada.

- **`kioskd version` e `kioskctl version`** (também `--version`, `-V` e, em português, `versao`): versão, commit, data, alvo e perfil da compilação. A primeira linha tem formato fixo (`kioskd 0.0.6 (a1b2c3d)`); o resto sai no idioma de quem lê.
- `kioskd status` mostra a versão deste executável e a do executável instalado (lida de `kioskd version` do que o serviço roda), e `kioskd install` diz a versão instalada. O log do serviço também grava o commit na linha de início.

### Alterado
- Em `kioskctl log`, o detalhe de `AUTHZ_FAIL` continua omitido, exceto o da tentativa sem privilégio.

### Corrigido
- O serviço registrava `ImpersonateNamedPipeClient ... (os error 1368)` no `kioskd.log` quando algo conectava ao pipe `kiosk-audit` e fechava sem enviar nada (visto no PC real, com `Test-Path \\.\pipe\kiosk-audit`). Uma sondagem assim não é erro e agora é descartada em silêncio; uma linha vazia enviada de verdade continua virando `VALIDATION_FAIL`.

## [0.0.6] - 2026-10-29

Ainda **pré-lançamento**. Corrige o que o teste da 0.0.5 no PC real mostrou: opção digitada errada deixa de ser ignorada em silêncio, e o `kioskd` avisa quando uma regra já nasce com o limite menor ou igual ao uso de hoje. A ADR-011 (rastro das tentativas de comando sem privilégio) foi aceita e entra numa próxima versão. Serviço e `kioskctl` da 0.0.5 continuam compatíveis.

### Corrigido
- `kioskd` e `kioskctl` ignoravam em silêncio opção desconhecida, opção sem valor e argumento sobrando: um `--activete` digitado errado criava uma regra pendente sem avisar. Agora recusam, com sugestão quando há uma opção parecida (`você quis dizer --activate?`), antes de abrir o banco ou gravar. `--help` e `-h` mostram o uso.
- A duração nas listagens do `kioskd` (`0h03`) passa a `3min 00s`, o mesmo formato do `kioskctl`.

### Adicionado
- `kioskd rule create`, `rule activate`, `rule limit` e `profile limit` avisam quando a regra ativa (ou o limite total) já nasce com o limite menor ou igual ao uso de hoje: o app seria fechado, ou a sessão bloqueada, assim que abrisse. Visto no PC real: o limite de 3 min num app que já tinha 11 min de uso hoje parecia "regra que não revoga".

### Documentação
- ADR-011 proposta: rastro das tentativas de comando administrativo sem privilégio (GC-21). O comando avisa o serviço por named pipe local, o serviço descobre o chamador pelo token e grava o evento no banco; todo comando que abre o banco passa a conferir a elevação.
- Registro do teste da 0.0.5 no PC real do dono e do GC-21: a recusa de `kioskd pairing-code` por conta sem privilégio não fica no log forense, porque o banco só abre para administradores.

## [0.0.5] - 2026-10-29

Ainda **pré-lançamento**. Idiomas (en-US, pt-BR, es-ES), comandos em inglês com o português equivalente, pasta do kiosk no PATH e exigência de administrador para gerar o código de pareamento. **Atualize o `kioskd` e o `kioskctl` juntos**: o corpo do pareamento mudou e um `kioskctl` 0.0.4 não pareia com um serviço 0.0.5. Dispositivos já pareados continuam funcionando. O log gravado pela 0.0.4 mantém o texto em português.

### Adicionado
- Mensagens em **en-US, pt-BR e es-ES**, no idioma de quem as lê (ADR-010): `kioskd` e `kioskctl` por `KIOSK_LANG` ou pelo idioma do Windows; serviço no idioma de quem o instalou; avisos à criança no idioma da conta dela. Cada mensagem exige os três idiomas, ou o build falha.
- `kioskctl log` mostra o detalhe dos eventos traduzido; o log forense passa a guardar um código (JSON) para o enforcement. Linhas antigas aparecem como foram gravadas.
- Dias da semana aceitos também em espanhol (`lun,mar,mie,jue,vie,sab,dom`); `uninstall --purge-data` pede `DELETE`, `APAGAR` ou `BORRAR`, conforme o idioma.

### Alterado
- Códigos de motivo no log e no protocolo da UI em inglês (`APP_DAILY_LIMIT`, `TOTAL_DAILY_LIMIT`, `APP_BLOCKED`, `APP_OUTSIDE_WINDOW`, `OUTSIDE_WINDOW`).
- Detalhes das falhas da API (autenticação e validação), gravados só no banco local, em inglês.
- Comandos e opções do `kioskd` e do `kioskctl` em inglês, que é a convenção (`kioskd status`, `kioskd install --listen`, `kioskd pairing-code`, `kioskctl pair --server` etc.). **Os nomes em português continuam valendo, sem prazo**, e podem ser misturados. Lista em `docs/runbooks/comandos-em-portugues.md`, que também vai no pacote do release (ADR-010).
- Dias da semana aceitam `mon,tue,...`, além de `seg,ter,...` e de `0` a `6`.
- `kioskd install` registra o serviço como `kioskd.exe service`. O registro da 0.0.4 (`servico`) segue funcionando; reinstalar atualiza.
- **Quebra**: o corpo de `POST /api/pair` passa a `code`, `public_key` e `name`, porque a API fala só inglês. Um `kioskctl` 0.0.4 não pareia com um serviço novo; atualize os dois. Dispositivos já pareados não são afetados.

### Adicionado
- `kioskd instalar` põe `C:\Program Files\kiosk` no PATH do sistema, sem duplicar e preservando as outras entradas, e avisa o Windows. Em janelas novas, `kioskd` e `kioskctl` funcionam de qualquer pasta. `kioskd desinstalar` tira a entrada; `kioskd estado` mostra se ela está lá.

### Segurança
- `kioskd codigo-pareamento` no Windows exige administrador elevado. Sem isso, recusa, grava `AUTHZ_FAIL` no log forense e não cria código. Antes, sem o `kioskd instalar` ou com `KIOSK_DB` em outro lugar, uma conta padrão conseguia gerar código e parear um cliente.

### Documentação
- E-012 "Internacionalização" e ADR-010 propostos: en-US, pt-BR e es-ES nas interfaces, no serviço e nas respostas; comandos e opções da CLI em inglês, com os nomes em português aceitos até a 0.1.0.
- `README.md` novo: o que é o kiosk, como o cliente e o serviço funcionam, conceitos, plataformas e dois fluxogramas para entender o projeto num olhar.
- ROADMAP com a maturidade de cada épico em porcentagem (critério: média das fases, concluída 100%, parcial 50%).
- Responsáveis são os administradores: todos gerem todas as contas monitoradas, no Windows, no Linux e no macOS (decisão do dono).
- Insight do responsável temporário (delegação por até 5 dias a quem não é administrador), sem compromisso.
- E-008 "Cliente gráfico dos pais" proposto (Windows, Linux e macOS na Tailscale, usando o cliente do `kioskctl`; começa pela listagem remota na API).
- E-009, E-010 e E-011 (serviço em Linux, macOS e Android) registrados como congelados, até o `kioskd` no Windows e o cliente gráfico amadurecerem; cada um revisa a ADR-002 e traz o próprio CI/CD.

## [0.0.4] - 2026-10-29

Ainda **pré-lançamento**. O `kioskd` passa a rodar como serviço do Windows (ADR-008, etapa 1), sem depender de uma janela aberta. O reinício do PC com o serviço instalado ainda não foi validado em máquina real (GC-16).

### Adicionado
- `kioskd instalar --escutar ip:porta[,...]`: instala o kiosk como serviço do Windows (ADR-008, etapa 1). Copia os executáveis para `C:\Program Files\kiosk`, deixa `C:\ProgramData\kiosk` só para SYSTEM e Administradores, registra o serviço `kiosk` (conta SYSTEM, início automático, reinício em falha; conta padrão não consegue parar) e o inicia. Rodar de novo atualiza.
- `kioskd desinstalar [--apagar-dados]`: remove o serviço e os executáveis; os dados só são apagados depois de digitar `APAGAR`.
- `kioskd estado`: estado, pid, início, conta, endereço de escuta e log do serviço.
- `kioskd servico`: modo serviço chamado pelo Windows, com o log em `C:\ProgramData\kiosk\kioskd.log`.

### Alterado
- Com o serviço instalado, os comandos de cadastro do `kioskd` precisam de PowerShell como Administrador, porque a pasta do banco fica protegida.

### Documentação
- ADR-007 a ADR-009 aceitas pelo dono; E-007 em andamento.
- Runbook: instalação como serviço (seção 4.1) e modo console para testes (4.2); fluxogramas dos quatro comandos novos.
- ADR-007 a ADR-009 redigidas para o E-007 (assistente no terminal, autoinstalação e depois `.msi`, lista de programas pelo menu Iniciar e processos).
- E-007 "Onboarding e instalador" proposto (assistente de primeira configuração, serviço do Windows e instalador); índice de épicos e roadmap atualizados.
- Resultados dos testes no Windows real: avisos, limite do app e bloqueio de sessão validados; logoff não ocorreu (GC-20).

## [0.0.3] - 2026-10-28

Ainda **pré-lançamento**. Corrige armadilhas encontradas no primeiro teste no Windows.

### Adicionado
- Fluxograma Mermaid de cada comando de `kioskd` e `kioskctl` em `docs/architecture/fluxos-dos-comandos.md`, regra no `AGENTS.md` e teste que reprova comando sem fluxograma.
- `kioskd regra criar|ativar|revogar|limite|listar`: regras administradas no próprio PC, sem parear dispositivo, com a mesma validação e a mesma auditoria da API (`--ativar` cria já ativa).

### Corrigido
- Sem `KIOSK_DB`, o `kioskd` criava em silêncio um banco novo na pasta atual, e cadastros feitos em outra janela pareciam sumir. No Windows o padrão passa a ser `C:\ProgramData\kiosk\kiosk.db`, e todo comando informa o banco em uso.
- Pareamento recusado agora explica o código esperado (8 caracteres, 5 minutos, uso único, gerado por `kioskd codigo-pareamento` no mesmo banco).
- Runbook: identificação da conta da criança (nome × SID × ID do perfil, `Get-LocalUser`, conta Microsoft), escuta em `127.0.0.1` para parear no próprio PC e o banco padrão.

## [0.0.2] - 2026-10-28

Ainda **pré-lançamento**, com as mesmas limitações da 0.0.1. Torna possível o primeiro uso real no Windows: antes não havia como cadastrar a criança nem os apps.

### Adicionado
- `kioskd perfil criar|listar|limite|janela`: cadastro do perfil pela conta Windows (o SID é obtido com `LookupAccountNameW`; grupos são recusados), limite diário total e janelas por dia da semana.
- `kioskd app criar|listar`: cadastro dos programas controlados, isentos ou ignorados.
- `kioskd` cria a pasta do banco se ela não existir.

### Corrigido
- Runbook reescrito para o PowerShell no Windows: a 0.0.1 trazia comandos em sintaxe de bash (`VAR=valor comando`), que não funcionam no Windows, e um `KIOSK_LISTEN` sem porta.
- `KIOSK_LISTEN` sem porta agora explica o formato (`falta a porta ... use ip:porta`).

## [0.0.1] - 2026-10-28

Primeira versão, **pré-lançamento**. O motor, o log forense, a API e o CLI estão completos e testados (Linux e Windows no CI); o agente Windows encerra apps em Windows real, mas bloqueio de sessão, logoff e avisos ainda não foram validados em sessão interativa com conta de criança. Não use como única proteção.

Binários: `kioskd` (serviço) e `kioskctl` (CLI dos pais); uso em `api-e-cli.md`. Ver as pendências abertas em `docs/PENDING.md`. Licença: GPL-2.0-only (`LICENSE`).

### Documentação
- Estrutura documental do método sovereign-harness em `docs/`, com índice, PENDING, ROADMAP e CONCLUSION.
- `AGENTS.md` como fonte única de instruções para agentes; `CLAUDE.md` passa a ser symlink para ele.
- Arquitetura movida para `docs/architecture/README.md`; `ARCHITECTURE.md` vira stub.
- Repositório publicado em github.com/jefersonlop3s/kiosk-engine; fluxo de trabalho por Pull Request e proibição de dados reais de crianças no Git descritos no AGENTS.md.
- ADR-003 a ADR-006 propostas (detecção de processos, IPC serviço↔UI, rede da API, assinatura e pareamento); as quatro aceitas pelo dono em 2026-10-28.
- ADR-001 (enforcement ativo e visível) e ADR-002 (perfil por conta Windows, somente Windows) propostas e aceitas em 2026-10-27.

### Adicionado
- Migração inicial com o contrato do dado (ADR-002), structs alinhadas e CRUD transacional com testes em SQLite real (E-001 F2–F3).
- CI no GitHub: `cargo fmt --check`, `clippy -D warnings` e `cargo test` em Ubuntu e Windows, em push de qualquer branch (verde em 2026-10-28).
- Log forense: tipo de evento (migração 0002) e gravação do evento na mesma transação de cada escrita; falhas registram a razão em evento próprio (E-002 F1).
- `Security_Log` append-only: triggers recusam UPDATE e DELETE (migração 0003, E-002 F2).
- Retenção de 90 dias do log forense imposta por trigger, `purge_expired_events` e consulta de eventos por período e por perfil (migração 0004, E-002 F3).
- Motor de decisão puro (`decidir`) e contabilidade de uso por dia local, com guarda de relógio (`TAMPER_SUSPECTED`) (E-003 F1–F3).
- Núcleo do agente independente de SO (`Agente`, interface `Sistema`): avisos de 5 e 1 min, encerramento de apps e bloqueio de sessão com log forense, e leituras do banco para o motor (E-004, parte SO-independente).
- Tempo total do perfil pela sessão ativa (`session_usage`, ADR-003) e política de lock seguido de logoff após 2 desbloqueios.
- Assinatura Ed25519 de requisições, anti-replay por nonce, pareamento presencial por código de uso único e revogação de dispositivos (ADR-006, E-005 F2).
- Protocolo serviço ↔ UI com decodificação defensiva, limite de taxa e monitor de batimento (ADR-004, parte independente de plataforma).
- API de comando `POST /api/windows/{ação}` com as Camadas 0 a 5 (criar, ativar e revogar regras), respostas genéricas e escuta restrita a loopback e Tailscale (ADR-005, E-005 F1/F3).
- Serviço `kioskd` (servir, código de pareamento, dispositivos, revogar), CLI dos pais `kioskctl` (parear e criar, ativar e revogar regras), endpoint `POST /api/pair` e runbook de operação (E-005 F3).
- `modify_limit` e consultas de uso e de log pela API e pelo CLI (`regra limite`, `uso`, `log`).
- Adaptador Win32 do agente (processos com SID, sessões, encerrar, bloquear, logoff, aviso) e agente rodando dentro do `kioskd servir` no Windows; textos pt-BR dos avisos.
- Harness Sovereign: guard do agente, gates de git (pre-commit/pre-push) e contrato de validação em `.harness.toml` (E-001 F4).

### Corrigido
- `build.rs` recompila o crate quando `migrations/` muda; antes, uma migração nova podia ser ignorada pelo binário.
- Manifesto na raiz com dependências resolvíveis; `cargo check` passa (E-001 F1).

### Chore
- Trabalho em andamento em `src/` (`db_manager.rs`, `security_logger_impl.rs`, `Cargo.toml`) versionado como está; ainda não compila (GC-01 a GC-07).
- Épicos E-001 a E-006 e roadmap de sprints.
- Insights: revisão do estado inicial e crítica ao relatório de hardening Windows (não adotado).
