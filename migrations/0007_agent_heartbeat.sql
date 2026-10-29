-- E-004 F4: o agente marca que está vivo (a cada 10 s) e, numa parada normal, que parou de
-- propósito. Ao subir, o próximo agente compara com agora: um intervalo longo sem batimento é um
-- período sem enforcement, e vira `AGENT_STOPPED` no log forense (parada normal ou não).
-- Uma linha só (id = 1); o log forense não guarda batimentos, para não crescer a cada 10 s.
CREATE TABLE agent_heartbeat (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    beat_at    TEXT NOT NULL,
    clean_stop INTEGER NOT NULL DEFAULT 0 CHECK (clean_stop IN (0, 1))
) STRICT;
