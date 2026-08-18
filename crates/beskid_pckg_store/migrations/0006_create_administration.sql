-- Registry administration is deliberately opt-in. Roles are projected from
-- Authelia groups by the HTTP adapter, so no role table is persisted here.
-- Deployment operators configure the admin/moderator groups on the Authelia
-- side; the registry only persists publisher verification, per-resource
-- moderation grants and the package-review audit log.

CREATE TABLE IF NOT EXISTS pckg_publisher_verifications (
    subject TEXT PRIMARY KEY CHECK (length(btrim(subject)) > 0 AND subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(subject) <= 255),
    is_verified BOOLEAN NOT NULL,
    reviewed_by_subject TEXT NOT NULL CHECK (length(btrim(reviewed_by_subject)) > 0 AND reviewed_by_subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(reviewed_by_subject) <= 255),
    reviewed_at_utc TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS pckg_resource_permissions (
    subject TEXT NOT NULL CHECK (length(btrim(subject)) > 0 AND subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(subject) <= 255),
    resource_kind TEXT NOT NULL CHECK (resource_kind = 'package'),
    resource_id TEXT NOT NULL CHECK (BTRIM(resource_id) <> ''),
    capability TEXT NOT NULL CHECK (capability = 'moderate'),
    granted_by_subject TEXT NOT NULL CHECK (length(btrim(granted_by_subject)) > 0 AND granted_by_subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(granted_by_subject) <= 255),
    granted_at_utc TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (subject, resource_kind, resource_id, capability)
);
CREATE INDEX IF NOT EXISTS pckg_resource_permissions_resource_idx
    ON pckg_resource_permissions (resource_kind, resource_id);

CREATE TABLE IF NOT EXISTS pckg_package_review_decisions (
    id BIGSERIAL PRIMARY KEY,
    package_id UUID NOT NULL REFERENCES pckg_packages(id) ON DELETE RESTRICT,
    version TEXT NULL,
    decision TEXT NOT NULL CHECK (decision IN ('approved', 'rejected', 'yanked', 'unyanked')),
    reason TEXT NOT NULL DEFAULT '',
    decided_by_subject TEXT NOT NULL CHECK (length(btrim(decided_by_subject)) > 0 AND decided_by_subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(decided_by_subject) <= 255),
    decided_at_utc TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS pckg_package_review_decisions_package_idx
    ON pckg_package_review_decisions (package_id, decided_at_utc DESC);
