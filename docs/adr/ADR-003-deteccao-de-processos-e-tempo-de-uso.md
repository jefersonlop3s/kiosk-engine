# ADR-003 — Detecção de processos e definição de tempo de uso

Status: Aceito em 2026-10-28 pelo dono (Jeferson Lopes). Data: 2026-10-28. Autoridade de aceite: dono do projeto (Jeferson Lopes).
Repositórios/contratos afetados: kiosk; [E-004](../epics/E-004-agente-windows-enforcement.md) F1, [E-003](../epics/E-003-motor-de-regras-e-uso.md) (consumo).

## Contexto

O agente precisa saber, a cada instante, **quais processos cada conta de criança executa** ([ADR-002](ADR-002-identidade-por-conta-windows.md): perfil por SID) para consultar o motor e agir ([ADR-001](ADR-001-enforcement-ativo-e-visivel.md)). O serviço roda como SYSTEM, então enxerga todas as sessões.

Há uma segunda decisão, que o motor de [E-003](../epics/E-003-motor-de-regras-e-uso.md) deixou aberta (GC-09 no [PENDING](../PENDING.md)): **o que conta como "tempo de uso"**. Somar o tempo de cada app como "total do perfil" conta em dobro dois apps simultâneos.

Origem: [E-004](../epics/E-004-agente-windows-enforcement.md), seção "ADR a abrir ao iniciar F1".

## Decisão

Proposta (a validar em Windows real na F0/F1):

1. **Detecção por varredura periódica (polling)** a cada 2 s, com `CreateToolhelp32Snapshot`/`Process32Next` (ou `NtQuerySystemInformation`), lendo para cada processo o **SID do dono** via `OpenProcessToken` + `GetTokenInformation(TokenUser)` e o **nome do executável**. Intervalo configurável; mínimo 1 s.
2. **Tempo de uso do app**: conta enquanto existir ao menos um processo do app na conta da criança, esteja ou não em primeiro plano. Fechar a janela e deixar o processo vivo continua contando.
3. **Tempo de uso total do perfil**: conta enquanto a **sessão** da criança estiver ativa (logada e desbloqueada, via `WTSEnumerateSessions`/`WTSQuerySessionInformation`), medido por tick e guardado numa tabela própria (`session_usage`), **não** pela soma dos apps. Isso resolve o GC-09.
4. **Identificação do app** por nome do executável (`App_Metadata.process_name`, sem diferenciar caixa), como já está no contrato. Caminho completo e assinatura digital não entram no MVP.
5. Processo sem SID mapeado a um perfil (conta dos pais, SYSTEM) não é controlado, como na ADR-002.

Alternativas rejeitadas:
- **Eventos ETW** (`Microsoft-Windows-Kernel-Process`): detecta o início do processo na hora, mas exige sessão ETW privilegiada, trata perda de eventos e é bem mais complexo. Ganha pouco: com tick de 2 s, o pior caso é uma janela de 2 s antes do aviso ou encerramento, aceitável para controle parental.
- **WMI `Win32_ProcessStartTrace`**: simples, mas é entrega de melhor esforço, tem sobrecarga e quebra com o serviço WMI reiniciado.
- **Driver de kernel / callbacks de criação de processo**: bloqueia a execução antes de iniciar, mas exige assinatura de driver e eleva o risco de travar a máquina. Fora de proporção para uso doméstico.
- **Só app em primeiro plano**: criança minimiza o jogo e o tempo para de contar; exige um componente na sessão do usuário só para isso.
- **Total = soma dos apps**: conta em dobro uso simultâneo (GC-09).

## Consequências

- Ganho: implementação simples, testável por um adaptador que devolve um *snapshot*; o motor e o laço do agente não dependem do mecanismo (a mesma interface serve a uma troca futura por ETW).
- Custo: latência de até 1 tick; processo que inicia e termina entre dois ticks passa despercebido (irrelevante para limite de tempo).
- Custo: nova tabela `session_usage` (migração) e mudança no cálculo do total do perfil.
- Risco: renomear o `.exe` burla a identificação por nome. Mitigação futura (hash/assinatura) fica fora do MVP e deve ser registrada como limitação aos pais.
- Reversível: detecção e contagem ficam atrás da interface do agente; trocar o mecanismo não muda o motor.
- Validação: só em Windows real (F0): log de processos de duas contas distintas e teste de que o processo de outra conta não é atribuído à criança.

## Referências e evolução

- Consome: [E-004](../epics/E-004-agente-windows-enforcement.md); fecha o GC-09 do [PENDING](../PENDING.md) quando aceita e implementada.
- Complementa: [ADR-001](ADR-001-enforcement-ativo-e-visivel.md), [ADR-002](ADR-002-identidade-por-conta-windows.md).
