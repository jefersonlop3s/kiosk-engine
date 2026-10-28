-- Contrato do dado: arquitetura Seção II + ADR-002. Tempos em UTC (RFC 3339);
-- janelas de horário em hora local da máquina, no formato HH:MM.

CREATE TABLE app_metadata (
    app_id          TEXT PRIMARY KEY NOT NULL,
    process_name    TEXT NOT NULL UNIQUE COLLATE NOCASE,
    current_status  TEXT NOT NULL DEFAULT 'MONITORED'
                    CHECK (current_status IN ('MONITORED', 'WHITELISTED', 'IGNORE')),
    last_seen       TEXT
) STRICT;

CREATE TABLE child_profile (
    profile_id                 TEXT PRIMARY KEY NOT NULL,
    windows_sid                TEXT NOT NULL UNIQUE,
    display_name               TEXT NOT NULL,
    daily_total_limit_seconds  INTEGER CHECK (daily_total_limit_seconds IS NULL OR daily_total_limit_seconds >= 0)
) STRICT;

-- Janela de uso permitido por dia da semana (0 = domingo ... 6 = sábado).
CREATE TABLE profile_time_window (
    profile_id    TEXT NOT NULL REFERENCES child_profile(profile_id) ON DELETE CASCADE,
    weekday       INTEGER NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    window_start  TEXT NOT NULL,
    window_end    TEXT NOT NULL CHECK (window_start < window_end),
    PRIMARY KEY (profile_id, weekday)
) STRICT;

CREATE TABLE active_rules (
    rule_id                    TEXT PRIMARY KEY NOT NULL,
    profile_id                 TEXT NOT NULL REFERENCES child_profile(profile_id),
    app_id                     TEXT NOT NULL REFERENCES app_metadata(app_id),
    creation_timestamp         TEXT NOT NULL,
    current_state              TEXT NOT NULL DEFAULT 'PENDING'
                               CHECK (current_state IN ('PENDING', 'ACTIVE', 'REVOKED')),
    daily_limit_seconds        INTEGER CHECK (daily_limit_seconds IS NULL OR daily_limit_seconds >= 0),
    time_window_start          TEXT,
    time_window_end            TEXT,
    effective_until_timestamp  TEXT,
    CHECK ((time_window_start IS NULL) = (time_window_end IS NULL))
) STRICT;

CREATE INDEX idx_active_rules_profile_app ON active_rules (profile_id, app_id);

-- Consumo acumulado por criança × app × dia local (YYYY-MM-DD).
CREATE TABLE usage_ledger (
    profile_id  TEXT NOT NULL REFERENCES child_profile(profile_id),
    app_id      TEXT NOT NULL REFERENCES app_metadata(app_id),
    usage_date  TEXT NOT NULL,
    seconds     INTEGER NOT NULL DEFAULT 0 CHECK (seconds >= 0),
    PRIMARY KEY (profile_id, app_id, usage_date)
) STRICT;

CREATE TABLE security_log (
    log_id                   TEXT PRIMARY KEY NOT NULL,
    timestamp                TEXT NOT NULL,
    client_ip                TEXT NOT NULL,
    action_attempted         TEXT NOT NULL,
    transaction_status       TEXT NOT NULL CHECK (transaction_status IN ('SUCCESS', 'FAILURE')),
    rejection_reason_detail  TEXT NOT NULL
) STRICT;

CREATE INDEX idx_security_log_timestamp ON security_log (timestamp);
