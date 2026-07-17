-- Durable current state for package review requests. The separate moderation
-- decision log remains an append-only audit history.
CREATE TABLE IF NOT EXISTS pckg_package_review_requests (
    id UUID PRIMARY KEY,
    package_id UUID NOT NULL REFERENCES pckg_packages(id) ON DELETE CASCADE,
    requested_by_subject TEXT NOT NULL CHECK (requested_by_subject ~ '^github:[0-9]+$'),
    reason TEXT NOT NULL CHECK (BTRIM(reason) <> '' AND LENGTH(reason) <= 4000),
    status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'needs_changes', 'rejected')),
    submitted_at_utc TIMESTAMPTZ NOT NULL,
    reviewer_subject TEXT NULL CHECK (reviewer_subject ~ '^github:[0-9]+$'),
    review_notes TEXT NULL CHECK (review_notes IS NULL OR LENGTH(review_notes) <= 4000),
    reviewed_at_utc TIMESTAMPTZ NULL,
    CHECK ((reviewer_subject IS NULL) = (reviewed_at_utc IS NULL))
);

CREATE INDEX IF NOT EXISTS pckg_package_review_requests_queue_idx
    ON pckg_package_review_requests (submitted_at_utc DESC);
CREATE INDEX IF NOT EXISTS pckg_package_review_requests_package_idx
    ON pckg_package_review_requests (package_id, submitted_at_utc DESC);
