-- This migration deliberately contains no query against AspNetUsers, username,
-- normalized username, or email. An operator must provide reviewed mappings
-- from the legacy Identity primary key to a GitHub Auth Hub subject.

CREATE TABLE IF NOT EXISTS pckg_legacy_identity_subject_map (
    legacy_identity_id TEXT PRIMARY KEY,
    auth_hub_subject TEXT NOT NULL,
    approved_by TEXT NOT NULL,
    approved_at_utc TIMESTAMPTZ NOT NULL,
    CONSTRAINT pckg_legacy_identity_subject_map_subject_format
        CHECK (auth_hub_subject ~ '^github:[0-9]+$'),
    CONSTRAINT pckg_legacy_identity_subject_map_approver_nonempty
        CHECK (length(trim(approved_by)) > 0)
);

CREATE TABLE IF NOT EXISTS pckg_legacy_identity_cutover_runs (
    run_id UUID PRIMARY KEY,
    requested_by TEXT NOT NULL,
    started_at_utc TIMESTAMPTZ NOT NULL,
    completed_at_utc TIMESTAMPTZ NULL,
    status TEXT NOT NULL,
    mapped_identity_count BIGINT NOT NULL DEFAULT 0,
    legacy_package_count BIGINT NOT NULL DEFAULT 0,
    imported_package_count BIGINT NOT NULL DEFAULT 0,
    imported_version_count BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT pckg_legacy_identity_cutover_runs_status
        CHECK (status IN ('running', 'rejected_unmapped_identity', 'completed')),
    CONSTRAINT pckg_legacy_identity_cutover_runs_requested_by_nonempty
        CHECK (length(trim(requested_by)) > 0)
);

CREATE TABLE IF NOT EXISTS pckg_legacy_identity_cutover_unmapped_identities (
    run_id UUID NOT NULL REFERENCES pckg_legacy_identity_cutover_runs(run_id) ON DELETE CASCADE,
    legacy_identity_id TEXT NOT NULL,
    package_count BIGINT NOT NULL CHECK (package_count > 0),
    PRIMARY KEY (run_id, legacy_identity_id)
);
