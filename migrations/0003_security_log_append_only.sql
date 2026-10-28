-- E-002 F2: o log forense só aceita INSERT. UPDATE e DELETE abortam.
-- Não impede quem tem acesso ao arquivo (DROP TRIGGER/TABLE, edição fora do SQLite);
-- a retenção (F3) precisará de um caminho próprio e explícito.
CREATE TRIGGER security_log_no_update
BEFORE UPDATE ON security_log
BEGIN
    SELECT RAISE(ABORT, 'security_log é append-only: UPDATE recusado');
END;

CREATE TRIGGER security_log_no_delete
BEFORE DELETE ON security_log
BEGIN
    SELECT RAISE(ABORT, 'security_log é append-only: DELETE recusado');
END;
