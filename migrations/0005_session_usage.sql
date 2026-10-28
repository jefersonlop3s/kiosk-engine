-- ADR-003: o tempo total do perfil é o tempo de SESSÃO ATIVA (logada e desbloqueada),
-- medido por tick, e não a soma dos apps (que contaria em dobro o uso simultâneo).
-- Dia local no formato YYYY-MM-DD, como em usage_ledger.
CREATE TABLE session_usage (
    profile_id  TEXT NOT NULL REFERENCES child_profile(profile_id),
    usage_date  TEXT NOT NULL,
    seconds     INTEGER NOT NULL DEFAULT 0 CHECK (seconds >= 0),
    PRIMARY KEY (profile_id, usage_date)
) STRICT;
