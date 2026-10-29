# ADR-009 — Origem da lista de programas do assistente

Status: Aceito em 2026-10-29 pelo dono (Jeferson Lopes). Data: 2026-10-29. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-007](../epics/E-007-onboarding-e-instalador.md) F1.

## Contexto

O agente identifica um programa pelo nome do `.exe` ([ADR-003](ADR-003-deteccao-de-processos-e-tempo-de-uso.md)). Os pais conhecem o programa pelo nome que aparece no menu Iniciar ("Roblox", "Minecraft"), não pelo executável. O assistente do [E-007](../epics/E-007-onboarding-e-instalador.md) precisa mostrar uma lista que os pais reconheçam e gravar o nome do `.exe` correto. Os dados são de crianças: coletar o mínimo ([E-002](../epics/E-002-log-forense.md)).

## Decisão

Proposta:

1. **Atalhos do menu Iniciar**, de todos os usuários e do perfil de cada criança escolhida: cada `.lnk` dá o nome amigável e o executável de destino. É a lista principal.
2. **Processos em execução no momento** nas sessões das crianças escolhidas, para cobrir o que não tem atalho (inclusive apps da Microsoft Store, que não deixam `.lnk` comum).
3. **Entrada manual** do nome do `.exe` como último recurso.
4. A lista mostra nome amigável e executável, marca o que já está cadastrado e **não é guardada**: só o que os pais escolherem vira registro em `app_metadata`.

Alternativas rejeitadas:
- **Chaves de desinstalação do registro**: têm o nome do produto, mas não o executável que roda de fato (um jogo instala vários `.exe`, e o lançador costuma ser outro).
- **Histórico de processos vistos pelo agente** (gravar tudo o que a criança abre para sugerir depois): útil, mas coleta uso que nenhuma regra pediu, contra o princípio de coletar o mínimo. Só com decisão explícita do dono e ADR própria.
- **Enumerar pacotes AppX/MSIX**: necessário para cobrir bem a Microsoft Store, mas é API mais complexa; fica para quando a lista de processos não bastar.

## Consequências

- Ganho: os pais escolhem pelo nome que conhecem; o `.exe` gravado é o que o agente vê.
- Custo: resolver `.lnk` exige a API de shell (COM `IShellLink`) no Windows.
- Risco: um atalho pode apontar para um lançador e não para o jogo; o assistente deve avisar que o nome vale para o `.exe` e mostrar os processos em execução para conferir.
- Reversível: novas fontes entram na lista sem mudar o banco.
- Validação: Windows real com programas instalados para todos e só para a criança; teste de que nada fora das escolhas é gravado.

## Referências e evolução

- Consome: [E-007](../epics/E-007-onboarding-e-instalador.md) F1.
- Relaciona-se com: [ADR-003](ADR-003-deteccao-de-processos-e-tempo-de-uso.md) (identificação pelo `.exe`), [ADR-007](ADR-007-forma-do-assistente-de-configuracao.md).
