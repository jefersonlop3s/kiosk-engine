-- ADR-006: dispositivos pareados (chave pública Ed25519), anti-replay por nonce e códigos de
-- pareamento de uso único. A chave privada nunca chega ao serviço; o código é guardado só como hash.
CREATE TABLE paired_device (
    device_id     TEXT PRIMARY KEY NOT NULL,
    public_key    BLOB NOT NULL UNIQUE CHECK (length(public_key) = 32),
    display_name  TEXT NOT NULL,
    role          TEXT NOT NULL DEFAULT 'ADMIN' CHECK (role IN ('ADMIN')),
    created_at    TEXT NOT NULL,
    revoked_at    TEXT
) STRICT;

-- Nonces vistos por dispositivo dentro da janela anti-replay; limpos a cada autenticação.
CREATE TABLE seen_nonce (
    device_id  TEXT NOT NULL,
    nonce      TEXT NOT NULL,
    seen_at    TEXT NOT NULL,
    PRIMARY KEY (device_id, nonce)
) STRICT;
CREATE INDEX idx_seen_nonce_seen_at ON seen_nonce (seen_at);

CREATE TABLE pairing_code (
    code_hash   TEXT PRIMARY KEY NOT NULL,
    expires_at  TEXT NOT NULL,
    used_at     TEXT
) STRICT;
