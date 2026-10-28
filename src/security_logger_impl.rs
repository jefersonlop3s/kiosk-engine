use sqlx::{SqlitePool, FromRow};
use crate::database_schema::*; // Importa os structs do schema definido anteriormente.

/// Estrutura que representa um evento de segurança crítico para registro forense imutável (Abordagem B).
pub struct SecurityEvent { 
     event_id: Uuid,       // PK único para este log - garante unicidade e rastreabilidade.
    timestamp: DateTime<Utc>,  // Momento EXATO do evento/tentativa (não o da escrita no banco!).
        client_ip: String,      // Origem do comando recebido (ex: "10.24.x.y"). Rastreia a fonte maliciosa ou admin legítimo.
    action_attempted: ActionType,  // O que foi TENTADO fazer? ADD_RULE, MODIFY_LIMIT, REVOKE etc.
        status: TransactionStatus,   // Resultado final da transação (SUCCESS/FAILURE).
     rejection_reason_detail: String, // CRÍTICO! Texto livre detalhado com o motivo EXATO do bloqueio/falha forense.

}