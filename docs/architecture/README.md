# Arquitetura — kiosk (controle parental doméstico)

> **Estado (2026-10-27)**: especificação original de 2026-09-06, preservada abaixo sem edição. O produto foi definido como controle parental doméstico para Windows, e duas ADRs aceitas em 2026-10-27 revisam partes dela:
> - [ADR-001](../adr/ADR-001-enforcement-ativo-e-visivel.md): Seção I.2 (Regra da Discrepância) e o objetivo de "invisibilidade". O enforcement passa a ser ativo e visível.
> - [ADR-002](../adr/ADR-002-identidade-por-conta-windows.md): Seção II. Entram `child_profile` e `usage_ledger`; alvo somente Windows.
>
> Fluxograma de cada comando (`kioskd` e `kioskctl`), com a integração entre cliente, serviço, banco e log: [fluxos-dos-comandos](fluxos-dos-comandos.md).
>
> Épicos que implementam esta arquitetura: [epics](../epics/README.md). Histórico da revisão: [insight](../insights/2026-10-27-revisao-estado-inicial.md).

---

# 🛡️ Desenho Arquitetural: Plataforma de Controle Crítico Kiosk Engine
**Status:** Especificação Finalizada / Pronto para Desenvolvimento.
**Objetivo Principal:** Criar um motor backend robusto em Rust que ofereça controle absoluto, invisibilidade ao usuário alvo e rastreabilidade forense completa (SIEM Lite).

---

### 🛡️ Seção I: O Princípio Operacional e Defensivo (O Como Agir)

Esta seção define as regras de ouro do sistema para garantir a estabilidade operacional em ambientes críticos. A prioridade é o menor risco, mesmo que isso signifique uma funcionalidade mais restrita.

1.  **Controle Transacional Total:** Toda alteração (`ADD`, `MODIFY`, `REVOKE`) deve passar pelo fluxo completo (Camadas 0 a 4) antes de ser considerada "commitada" na tabela `Active_Rules`. O sucesso é medido pela integridade do registro no `Security_Log` e não apenas pela mudança em si.
2.  **Regra da Discrepância (Menor Risco):** Se o motor (`Task Killer`) detecta uma regra cujo estado físico (o processo está rodando) contradiz seu estado lógico (DB diz que foi revogada), a ação é **passiva**. O Daemon registra um evento `STALE_RULE` no `Security_Log`, associado ao ID daquela regra. Ele não tenta matar o processo, pois isso seria uma intervenção destrutiva sem garantia de sucesso e aumentaria o risco operacional (priorizando estabilidade sobre correção forçada).
3.  **Resiliência do Log:** O `Security_Log` é a fonte primária da verdade forense em caso de falha catastrófica no sistema ou na memória RAM, pois ele armazena os metadados dos comandos externos e internos (`IP`, `Timestamp`, `RejectionReasonDetail`).

### Seção II: O Contrato do Dado (O Que Armazenar)

A base de dados SQLite deve ser estruturada para suportar alta disponibilidade, rastreabilidade histórica e defesa. As tabelas são desenhadas em conjunto com o fluxo transacional.

#### 1. Tabela `App_Metadata`
*   **Função:** Mapeia os processos sob vigilância; otimiza a busca do Daemon.
*   **Campos Chave:**
    *   `app_id (PK)`: UUID único.
    *   `process_name`: Nome exato do `.exe`.
    *   `current_status`: Status de monitoramento (`MONITORED`, `WHITELISTED`, `IGNORE`).

#### 2. Tabela `Active_Rules`
*   **Função:** Contém o estado *vivo* da regra, rastreando seu ciclo de vida (PENDING $\to$ ACTIVE $\to$ REVOKED).
*   **Campos Chave:**
    *   `rule_id (PK)`: UUID único para esta instância da regra.
    *   `app_id (FK)`: O alvo deste controle (`App_Metadata`).
    *   `creation_timestamp`: Quando a regra foi criada/ativada pela primeira vez.
    *   **`current_state`**: Status do ciclo de vida (`ACTIVE`, `PENDING`, `REVOKED`). **(CRÍTICO)**
    *   `daily_limit_seconds`: O limite máximo em segundos (Nulo = Sem Limite).
    *   `time_window_start`: Hora mínima para aplicar a regra (`HH:MM`).
    *   `effective_until_timestamp`: Timestamp de desativação lógica.

#### 3. Tabela `Security_Log`
*   **Função:** O log forense absoluto (Abordagem B). Registra o motivo *exato* de cada tentativa, sucesso ou falha.
*   **Campos Chave:**
    *   `log_id (PK)`: UUID único para cada evento.
    *   `timestamp`: Momento exato do evento/tentativa.
    *   `client_ip`: IP da origem do comando (`100.x.x.x`).
    *   `action_attempted`: A intenção (Ex: `ADD_RULE`, `MODIFY_LIMIT`, `REVOKE`, etc.).
    *   `status`: O resultado da transação (`SUCCESS`, `FAILURE`).
    *   **`rejection_reason_detail`**: Texto livre detalhado com a razão exata do bloqueio. **(CRÍTICO)**

### ⚙️ Seção III: O Fluxo de Processamento (O Como Fazer)

Este é o estado da máquina que deve ser implementado pelo Daemon Rust Core, garantindo atomicidade e defesa em cada transação crítica.

**Workflow Proposto: Recebimento de Comando Crítico (`POST /api/windows/{action}`)**

1.  **[Camada 0: Recepção]**: O Daemon recebe a requisição HTTP completa (incluindo cabeçalhos criptográficos assinados).
2.  **[Camada 1: Defesa Pessoal - Autenticação Criptográfica]:** Verifica se o comando foi assinado corretamente pelo dispositivo legítimo (`proof-of-possession`). **Falha $\to$ Log `AUTH_FAIL` e resposta genérica.**
3.  **[Camada 2: Defesa Pessoal - Autorização Lógica]:** Verifica se a identidade do cliente tem permissão para o comando (ex: só Admin pode revogar). **Falha $\to$ Log `AUTHZ_FAIL` e resposta genérica.**
4.  **[Camada 3: Validação de Parâmetros]:** Valida os parâmetros passados no corpo da requisição (`target`, datas, durações). **Falha $\to$ Log `VALIDATION_FAIL` e resposta genérica.**
5.  **[Camada 4: Execução Atômica (Commit)]:** Somente após passar nas três camadas defensivas anteriores, o Daemon executa a transição de estado na tabela `Active_Rules`. O comando é um único bloco atômico que atualiza status e registra o evento no log de sucesso.
6.  **[Camada 5: Registro & Resposta]:** A operação bem-sucedida é registrada no *Log* com Status `SUCCESS` e uma resposta HTTP genérica (`202 Accepted`) é enviada, sem expor nenhum detalhe interno.

***