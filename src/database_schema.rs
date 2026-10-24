// sovereign-pair/src/database_schema.rs

use uuid::Uuid;
use chrono::{DateTime, Utc};

/// ===================================================
/// 🛡️ TABELA: AppMetadata
/// Mapeia os processos sob vigilância (o que está sendo monitorado).
/// ===================================================
#[derive(Debug, Clone)]
pub struct AppMetadata {
    // app_id (PK): UUID único. Essencial para referências cruzadas e integridade de chave estrangeira.
    pub app_id: Uuid, 
    // process_name: Nome exato do .exe ou binário que está sendo monitorado.
    pub process_name: String,
    // current_status: Status de monitoramento (MONITORED, WHITELISTED, IGNORE). É o estado lógico da regra.
    #[derive(Debug, PartialEq)]
    pub status: ProcessStatus, 
}

/// Enum para representar os estados lógicos do AppMetadata.
#[derive(Debug, PartialEq, Clone)]
pub enum ProcessStatus {
    Monitored,      // O processo está ativo e sob vigilância ativa (requer check constante)
    Whitelisted,    // O processo foi aprovado; não deve ser alvo de controle por regras específicas.
    Ignored,        // Processo que o sistema decide ignorar intencionalmente.
}

/// ===================================================
/// 🛡️ TABELA: ActiveRules
/// Contém o estado VIVO da regra (O ciclo de vida do controle).
/// Esta é a tabela principal para transações críticas.
/// ===================================================
#[derive(Debug, Clone)]
pub struct ActiveRule {
    // rule_id (PK): UUID único para esta instância da regra. 
    pub rule_id: Uuid,
    // app_id (FK): O alvo deste controle (referência a AppMetadata). Garante que o alvo existe e é válido.
    pub target_app_id: Uuid,
    // creation_timestamp: Quando a regra foi criada/ativada pela primeira vez. Essencial para rastreabilidade histórica.
    pub creation_timestamp: DateTime<Utc>, 
    // current_state: Status do ciclo de vida (PENDING -> ACTIVE -> REVOKED). É o estado *vivo*.
    #[derive(Debug, PartialEq)]
    pub state: RuleState,
    // daily_limit_seconds: Limite máximo em segundos. Se for NULL no DB, usamos Option<u64>. Aqui representamos como u64 com um valor especial (0) para "Sem Limite".
    pub daily_limit_seconds: u64, 
    // time_window_start/end: Controla a janela de aplicação. Usaremos structs dedicadas em outro arquivo.
    // AQUI ESTÃO OS CAMPOS CHAVE PARA O CONTRATO DO DADO:
    pub effective_until_timestamp: DateTime<Utc>, // Timestamp de desativação lógica (quando o controle deve parar).
}

/// Enum para representar os estados do ciclo de vida da regra.
#[derive(Debug, PartialEq, Clone)]
pub enum RuleState {
    Pending,  // Aguardando ativação ou confirmação de configuração.
    Active,   // Regra está em pleno funcionamento e sendo aplicada pelo Daemon Core.
    Revoked,  // A regra foi desativada logicamente (não deve mais ter efeito).
}

/// ===================================================
/// 🛡️ TABELA: SecurityLog
/// O LOG FORENSE ABSOLUTO (Abordagem B). Registra o motivo EXATO de cada tentativa.
/// Esta tabela é otimizada para imutabilidade e busca forense, não para leitura rápida de status operacional.
/// ===================================================
#[derive(Debug, Clone)]
pub struct SecurityEvent {
    // log_id (PK): UUID único para este evento/tentativa. Garante a unicidade do registro.
    pub event_id: Uuid, 
    // timestamp: Momento EXATO do evento/tentativa. A fonte da verdade forense.
    pub timestamp: DateTime<Utc>,
    // client_ip: IP da origem do comando (ex: "100.x.x.x"). Essencial para rastrear o agente atacante ou administrador.
    pub client_ip: String, 
    // action_attempted: A intenção do cliente (Ex: ADD_RULE, MODIFY_LIMIT, REVOKE). O que ele *tentou* fazer?
    pub action_attempted: ActionType, 
    // status: Resultado da transação (SUCCESS ou FAILURE). Foi aceito ou rejeitado pelo Daemon Core.
    #[derive(Debug, PartialEq)]
    pub transaction_status: TransactionStatus,
    // REJECTION_REASON_DETAIL (CRÍTICO): Texto livre detalhado com a razão EXATA do bloqueio/falha. 
    // Este é o campo que armazena os "porquês" forenses e deve ser robusto para textos longos.
    pub rejection_reason_detail: String,
}

/// Enum para as ações críticas registradas (a intenção).
#[derive(Debug, PartialEq)]
pub enum ActionType {
    AddRule, 
    ModifyLimit, 
    Revoke,
    // ... outros comandos como ChangeWhitelistStatus, etc.
}

/// Enum para o resultado da transação (o que realmente aconteceu).
#[derive(Debug, PartialEq)]
pub enum TransactionStatus {
    Success, // A intenção foi executada com sucesso e alterou o estado do sistema corretamente.
    Failure, // O comando falhou por erro interno ou validação de dados.
}

// Nota: As estruturas para a Camada 0 (Recepção) e Camada 1 (Autenticação Criptográfica) serão criadas em arquivos separados (`api_handler.rs` e `crypto_utils.rs`).