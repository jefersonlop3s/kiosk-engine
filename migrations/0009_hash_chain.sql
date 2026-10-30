-- ADR-012: imutabilidade do log forense por hash encadeado.
-- `prev_hash` = SHA-256(log_id_anterior || prev_hash_anterior) em hexadecimal,
-- calculado pelo código (Rust) no momento da inserção. O primeiro evento tem
-- prev_hash = NULL.

ALTER TABLE security_log ADD COLUMN prev_hash TEXT;

-- Protege contra UPDATE: o log é append-only.
CREATE TRIGGER security_log_write_protect BEFORE UPDATE ON security_log
BEGIN
    SELECT RAISE(ABORT, 'security_log is append-only; UPDATE denied');
END;

-- Protege contra DELETE de eventos recentes (menos de 90 dias).
-- A política de retenção (apagar com mais de 90 dias) passa porque a condição
-- do trigger verifica OLD.timestamp.
CREATE TRIGGER security_log_delete_protect BEFORE DELETE ON security_log
BEGIN
    SELECT RAISE(ABORT, 'security_log: só registros com mais de 90 dias podem ser apagados')
    WHERE OLD.timestamp >= datetime('now', '-90 days');
END;