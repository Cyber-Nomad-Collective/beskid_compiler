CREATE TABLE IF NOT EXISTS pckg_packages (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    owner_subject TEXT NOT NULL,
    is_public BOOLEAN NOT NULL DEFAULT TRUE,
    created_at_utc TIMESTAMPTZ NOT NULL,
    updated_at_utc TIMESTAMPTZ NOT NULL,
    CONSTRAINT pckg_packages_name_key UNIQUE (name)
);

CREATE INDEX IF NOT EXISTS pckg_packages_owner_subject_idx
    ON pckg_packages (owner_subject);

CREATE TABLE IF NOT EXISTS pckg_package_versions (
    id UUID PRIMARY KEY,
    package_id UUID NOT NULL REFERENCES pckg_packages(id) ON DELETE RESTRICT,
    version TEXT NOT NULL,
    checksum_sha256 TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes >= 0),
    is_yanked BOOLEAN NOT NULL DEFAULT FALSE,
    published_at_utc TIMESTAMPTZ NOT NULL,
    yanked_at_utc TIMESTAMPTZ NULL,
    CONSTRAINT pckg_package_versions_package_version_key UNIQUE (package_id, version),
    CONSTRAINT pckg_package_versions_checksum_sha256_format CHECK (checksum_sha256 ~ '^[0-9a-f]{64}$')
);

CREATE INDEX IF NOT EXISTS pckg_package_versions_available_idx
    ON pckg_package_versions (package_id, published_at_utc DESC)
    WHERE is_yanked = FALSE;
