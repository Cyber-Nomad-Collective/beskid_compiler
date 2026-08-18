-- Durable replacement for the retired C# registry operations tables.  It
-- intentionally contains no SMTP credentials: pckg delegates browser identity
-- to Authelia and has no email-login or email-delivery authority.
CREATE TABLE IF NOT EXISTS pckg_blocked_link_patterns (
    id UUID PRIMARY KEY,
    pattern TEXT NOT NULL CHECK (BTRIM(pattern) <> '' AND LENGTH(pattern) <= 512),
    note TEXT NULL CHECK (note IS NULL OR LENGTH(note) <= 2000),
    created_by_subject TEXT NOT NULL CHECK (length(btrim(created_by_subject)) > 0 AND created_by_subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(created_by_subject) <= 255),
    created_at_utc TIMESTAMPTZ NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS pckg_blocked_link_patterns_lower_pattern_uq
    ON pckg_blocked_link_patterns (LOWER(pattern));

CREATE TABLE IF NOT EXISTS pckg_registry_activity (
    id BIGSERIAL PRIMARY KEY,
    occurred_at_utc TIMESTAMPTZ NOT NULL,
    severity TEXT NOT NULL CHECK (BTRIM(severity) <> '' AND LENGTH(severity) <= 64),
    action TEXT NOT NULL CHECK (BTRIM(action) <> '' AND LENGTH(action) <= 128),
    message TEXT NOT NULL CHECK (LENGTH(message) <= 4000),
    trace_id TEXT NULL CHECK (trace_id IS NULL OR LENGTH(trace_id) <= 256),
    actor_subject TEXT NULL CHECK (actor_subject IS NULL OR (length(btrim(actor_subject)) > 0 AND actor_subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(actor_subject) <= 255)),
    package_name TEXT NULL CHECK (package_name IS NULL OR LENGTH(package_name) <= 256),
    version TEXT NULL CHECK (version IS NULL OR LENGTH(version) <= 128)
);
CREATE INDEX IF NOT EXISTS pckg_registry_activity_recent_idx
    ON pckg_registry_activity (occurred_at_utc DESC, id DESC);

-- A weekly spotlight is an auditable in-app-only operation.  SMTP settings
-- and personal-email management were retired with the Auth Hub cutover; this
-- table preserves operator visibility without recreating a second identity or
-- mail authority.
CREATE TABLE IF NOT EXISTS pckg_weekly_spotlight_runs (
    id UUID PRIMARY KEY,
    ran_by_subject TEXT NOT NULL CHECK (length(btrim(ran_by_subject)) > 0 AND ran_by_subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(ran_by_subject) <= 255),
    ran_at_utc TIMESTAMPTZ NOT NULL,
    activity_count BIGINT NOT NULL CHECK (activity_count >= 0),
    delivery TEXT NOT NULL CHECK (delivery = 'in_app_only')
);
