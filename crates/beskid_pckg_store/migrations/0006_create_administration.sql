-- Registry administration is deliberately opt-in. There is no seeded
-- SuperAdmin: deployment operators must explicitly grant the first role after
-- authenticating its stable GitHub subject out of band.

CREATE TABLE IF NOT EXISTS pckg_admin_roles (
    subject TEXT NOT NULL CHECK (subject ~ '^github:[0-9]+$'),
    role TEXT NOT NULL CHECK (role IN ('moderator', 'superadmin')),
    granted_by_subject TEXT NOT NULL CHECK (granted_by_subject ~ '^github:[0-9]+$'),
    granted_at_utc TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (subject, role)
);

CREATE TABLE IF NOT EXISTS pckg_publisher_verifications (
    subject TEXT PRIMARY KEY CHECK (subject ~ '^github:[0-9]+$'),
    is_verified BOOLEAN NOT NULL,
    reviewed_by_subject TEXT NOT NULL CHECK (reviewed_by_subject ~ '^github:[0-9]+$'),
    reviewed_at_utc TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS pckg_resource_permissions (
    subject TEXT NOT NULL CHECK (subject ~ '^github:[0-9]+$'),
    resource_kind TEXT NOT NULL CHECK (resource_kind IN ('package', 'board')),
    resource_id TEXT NOT NULL CHECK (BTRIM(resource_id) <> ''),
    capability TEXT NOT NULL CHECK (capability = 'moderate'),
    granted_by_subject TEXT NOT NULL CHECK (granted_by_subject ~ '^github:[0-9]+$'),
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
    decided_by_subject TEXT NOT NULL CHECK (decided_by_subject ~ '^github:[0-9]+$'),
    decided_at_utc TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS pckg_package_review_decisions_package_idx
    ON pckg_package_review_decisions (package_id, decided_at_utc DESC);
