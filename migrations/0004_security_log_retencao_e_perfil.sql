-- E-002 F3: retenção de 90 dias (decisão do dono, 2026-10-28) e consulta por perfil.
--
-- `profile_id` é opcional e SEM chave estrangeira de propósito: o evento de falha de uma
-- operação sobre um perfil inexistente precisa ser gravado mesmo assim.
-- Timestamps do log: RFC 3339 UTC com microssegundos e sufixo Z (largura fixa, ordenável como texto).
ALTER TABLE security_log ADD COLUMN profile_id TEXT;
CREATE INDEX idx_security_log_profile_timestamp ON security_log (profile_id, timestamp);

-- O DELETE deixa de ser proibido por completo: só eventos com mais de 90 dias podem sair.
-- Mudar o prazo exige nova migração (o prazo vive aqui).
DROP TRIGGER security_log_no_delete;
CREATE TRIGGER security_log_retention_only_delete
BEFORE DELETE ON security_log
WHEN OLD.timestamp >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', '-90 days')
BEGIN
    SELECT RAISE(ABORT, 'security_log é append-only: só eventos com mais de 90 dias podem ser removidos');
END;
