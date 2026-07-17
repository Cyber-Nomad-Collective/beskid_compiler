CREATE TABLE IF NOT EXISTS pckg_package_community_reviews (
    id UUID PRIMARY KEY,
    package_id UUID NOT NULL REFERENCES pckg_packages(id) ON DELETE CASCADE,
    author_subject TEXT NOT NULL CHECK (author_subject ~ '^github:[0-9]+$'),
    rating SMALLINT NOT NULL CHECK (rating BETWEEN 1 AND 5),
    comment TEXT NOT NULL CHECK (length(trim(comment)) > 0),
    created_at_utc TIMESTAMPTZ NOT NULL,
    updated_at_utc TIMESTAMPTZ NOT NULL,
    UNIQUE (package_id, author_subject)
);
CREATE INDEX IF NOT EXISTS pckg_package_community_reviews_public_idx
    ON pckg_package_community_reviews (package_id, created_at_utc DESC);
