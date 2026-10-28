-- E-002 F1: tipo do evento (camadas da API, motor e enforcement da ADR-001).
-- `action_attempted` segue sendo a intenção; `event_type` classifica o desfecho.
ALTER TABLE security_log ADD COLUMN event_type TEXT NOT NULL DEFAULT 'SUCCESS'
    CHECK (event_type IN (
        'SUCCESS', 'AUTH_FAIL', 'AUTHZ_FAIL', 'VALIDATION_FAIL', 'INTERNAL_ERROR',
        'STALE_RULE', 'WARNING_SHOWN', 'APP_TERMINATED', 'SESSION_LOCKED',
        'AGENT_STOPPED', 'TAMPER_SUSPECTED'
    ));
