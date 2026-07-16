-- pckg automation keys are owned by stable Auth Hub GitHub subjects.  A raw
-- token is intentionally never stored: token_sha256 is the only credential
-- material retained by PostgreSQL.

CREATE TABLE IF NOT EXISTS pckg_api_keys (
    id UUID PRIMARY KEY,
    subject TEXT NOT NULL CHECK (subject ~ '^github:[0-9]+$'),
    label TEXT NOT NULL CHECK (BTRIM(label) <> ''),
    token_sha256 TEXT NOT NULL UNIQUE CHECK (token_sha256 ~ '^[0-9a-f]{64}$'),
    scopes TEXT[] NOT NULL CHECK (cardinality(scopes) > 0),
    created_at_utc TIMESTAMPTZ NOT NULL,
    revoked_at_utc TIMESTAMPTZ NULL
);
CREATE INDEX IF NOT EXISTS pckg_api_keys_subject_created_idx
    ON pckg_api_keys (subject, created_at_utc DESC);
