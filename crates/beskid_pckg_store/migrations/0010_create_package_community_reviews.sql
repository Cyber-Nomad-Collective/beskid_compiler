-- Package-scoped reviews (rating + comment). Forum-style community is owned
-- by NodeBB; the registry only persists simple package ratings keyed by the
-- authenticated Authelia subject.
CREATE TABLE IF NOT EXISTS pckg_package_community_reviews (
    id UUID PRIMARY KEY,
    package_id UUID NOT NULL REFERENCES pckg_packages(id) ON DELETE CASCADE,
    author_subject TEXT NOT NULL CHECK (length(btrim(author_subject)) > 0 AND author_subject ~ '^[A-Za-z0-9._:@/-]+$' AND length(author_subject) <= 255),
    rating SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment TEXT NOT NULL CHECK (length(trim(comment)) > 0),
    created_at_utc TIMESTAMPTZ NOT NULL,
    updated_at_utc TIMESTAMPTZ NOT NULL,
    UNIQUE (package_id, author_subject)
);
CREATE INDEX IF NOT EXISTS pckg_package_community_reviews_public_idx
    ON pckg_package_community_reviews (package_id, created_at_utc DESC);
