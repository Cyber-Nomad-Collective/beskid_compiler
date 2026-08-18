-- pckg automation keys are owned by stable Authelia subjects (the
-- `Remote-User` claim, or a carried-over `github:<numeric-id>`).  A raw token
-- is intentionally never stored: token_sha256 is the only credential material
-- retained by PostgreSQL.

CREATE TABLE IF NOT EXISTS pckg_api_keys (
    id UUID PRIMARY KEY,
    subject TEXT NOT NULL CHECK (length(btrim(subject)) > 0 AND subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(subject) <= 255),
    label TEXT NOT NULL CHECK (BTRIM(label) <> ''),
    token_sha256 TEXT NOT NULL UNIQUE CHECK (token_sha256 ~ '^[0-9a-f]{64}$'),
    scopes TEXT[] NOT NULL CHECK (cardinality(scopes) > 0),
    created_at_utc TIMESTAMPTZ NOT NULL,
    revoked_at_utc TIMESTAMPTZ NULL
);
CREATE INDEX IF NOT EXISTS pckg_api_keys_subject_created_idx
    ON pckg_api_keys (subject, created_at_utc DESC);
