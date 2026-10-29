# ADR-007 — Forma do assistente de primeira configuração

Status: Proposto. Data: 2026-10-29. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-007](../epics/E-007-onboarding-e-instalador.md) F1.

## Contexto

O [E-007](../epics/E-007-onboarding-e-instalador.md) quer que os pais configurem o kiosk de uma vez, sem seguir o [runbook](../runbooks/api-e-cli.md) comando a comando. O assistente precisa: listar as contas locais e deixar escolher as crianças, listar programas e deixar escolher os controlados, criar regras iniciais, configurar a escuta e mostrar o código de pareamento. Nos testes do dono (2026-10-28 e 2026-10-29) os tropeços foram de operação (variáveis por janela, IDs copiados à mão, comando no executável errado), não de lógica: toda a lógica já existe em `kioskd perfil|app|regra`.

## Decisão

Proposta:

1. **F1: assistente interativo no terminal**, `kioskd configurar`, em pt-BR, rodando na conta de administrador. Perguntas numeradas, listas para escolher por número, confirmação antes de gravar e um resumo no fim.
2. **Modo não interativo** no mesmo comando: `kioskd configurar --respostas arquivo.toml`, com as mesmas escolhas em arquivo. Serve para testar o assistente no CI do Windows e para reinstalar com a mesma configuração.
3. **Idempotente**: rodar de novo mostra o que já existe e só acrescenta ou altera o que for escolhido; nunca duplica perfil, app ou regra.
4. O assistente **só orquestra** as operações que já existem (as mesmas do `kioskd`), com a mesma validação e o mesmo log forense; não cria caminho de escrita novo.
5. Uma interface gráfica fica para depois, junto da UI de bandeja da [ADR-004](ADR-004-comunicacao-servico-ui-por-named-pipe.md) (mesma tecnologia), com ADR própria.

Alternativas rejeitadas:
- **Interface gráfica já na F1** (Win32, WinUI, egui, Tauri): mais amigável, mas exige escolher e manter uma pilha de UI antes de existir a bandeja, e não é testável no CI sem sessão interativa.
- **Página web local** (servidor em loopback e navegador): exige outro servidor, proteção contra CSRF e sessão de navegador numa máquina onde a criança também tem conta; aumenta a superfície sem necessidade.
- **Só documentação**: é o estado atual, e os testes mostraram que não basta.

## Consequências

- Ganho: reaproveita todo o código e os testes existentes; o modo não interativo dá teste de ponta a ponta no CI.
- Custo: terminal é menos amigável para pais leigos; a interface gráfica continua sendo dívida.
- Risco: o assistente roda com privilégio de administrador; qualquer erro precisa ser fail-loud e registrado.
- Reversível: a interface gráfica pode chamar as mesmas operações depois.
- Validação: Windows real com conta de criança; CI com o modo não interativo; fluxograma no padrão do [AGENTS.md](../../AGENTS.md#fluxogramas-dos-comandos).

## Referências e evolução

- Consome: [E-007](../epics/E-007-onboarding-e-instalador.md) F1.
- Relaciona-se com: [ADR-004](ADR-004-comunicacao-servico-ui-por-named-pipe.md) (UI de bandeja), [ADR-008](ADR-008-formato-do-instalador.md), [ADR-009](ADR-009-origem-da-lista-de-programas.md).
