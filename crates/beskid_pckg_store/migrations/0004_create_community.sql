-- Community data is independent of the retired ASP.NET Identity schema.
-- Every human identity is an Auth Hub GitHub subject; no username/email/id
-- bridge is permitted in this schema.

CREATE TABLE IF NOT EXISTS pckg_community_profiles (
    subject TEXT PRIMARY KEY CHECK (subject ~ '^github:[0-9]+$'),
    display_name TEXT NOT NULL CHECK (BTRIM(display_name) <> ''),
    bio TEXT NOT NULL DEFAULT '',
    social_links JSONB NOT NULL DEFAULT '[]'::JSONB,
    is_publisher_verified BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at_utc TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS pckg_community_boards (
    id TEXT PRIMARY KEY CHECK (BTRIM(id) <> ''),
    title TEXT NOT NULL CHECK (BTRIM(title) <> ''),
    locked BOOLEAN NOT NULL DEFAULT FALSE,
    created_at_utc TIMESTAMPTZ NOT NULL,
    updated_at_utc TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS pckg_community_posts (
    id BIGSERIAL PRIMARY KEY,
    board_id TEXT NOT NULL REFERENCES pckg_community_boards(id) ON DELETE RESTRICT,
    author_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE RESTRICT,
    title TEXT NOT NULL CHECK (BTRIM(title) <> ''),
    content TEXT NOT NULL,
    score INTEGER NOT NULL DEFAULT 0,
    created_at_utc TIMESTAMPTZ NOT NULL,
    updated_at_utc TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS pckg_community_posts_board_created_idx
    ON pckg_community_posts (board_id, created_at_utc DESC);

CREATE TABLE IF NOT EXISTS pckg_community_comments (
    id BIGSERIAL PRIMARY KEY,
    post_id BIGINT NOT NULL REFERENCES pckg_community_posts(id) ON DELETE CASCADE,
    author_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE RESTRICT,
    content TEXT NOT NULL CHECK (BTRIM(content) <> ''),
    parent_comment_id BIGINT NULL REFERENCES pckg_community_comments(id) ON DELETE CASCADE,
    score INTEGER NOT NULL DEFAULT 0,
    created_at_utc TIMESTAMPTZ NOT NULL,
    updated_at_utc TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS pckg_community_comments_post_created_idx
    ON pckg_community_comments (post_id, created_at_utc ASC);

CREATE TABLE IF NOT EXISTS pckg_community_post_votes (
    post_id BIGINT NOT NULL REFERENCES pckg_community_posts(id) ON DELETE CASCADE,
    voter_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE CASCADE,
    value SMALLINT NOT NULL CHECK (value IN (-1, 1)),
    updated_at_utc TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (post_id, voter_subject),
    UNIQUE (post_id, voter_subject)
);

CREATE TABLE IF NOT EXISTS pckg_community_comment_votes (
    comment_id BIGINT NOT NULL REFERENCES pckg_community_comments(id) ON DELETE CASCADE,
    voter_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE CASCADE,
    value SMALLINT NOT NULL CHECK (value IN (-1, 1)),
    updated_at_utc TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (comment_id, voter_subject),
    UNIQUE (comment_id, voter_subject)
);

CREATE TABLE IF NOT EXISTS pckg_community_publisher_follows (
    follower_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE CASCADE,
    publisher_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE CASCADE,
    created_at_utc TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (follower_subject, publisher_subject),
    CHECK (follower_subject <> publisher_subject)
);

CREATE TABLE IF NOT EXISTS pckg_community_package_follows (
    follower_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE CASCADE,
    package_id UUID NOT NULL REFERENCES pckg_packages(id) ON DELETE CASCADE,
    created_at_utc TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (follower_subject, package_id)
);

CREATE TABLE IF NOT EXISTS pckg_community_notification_preferences (
    subject TEXT PRIMARY KEY REFERENCES pckg_community_profiles(subject) ON DELETE CASCADE,
    mention_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    reply_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    followed_publisher_post_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    moderation_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at_utc TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS pckg_community_notifications (
    id BIGSERIAL PRIMARY KEY,
    recipient_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE CASCADE,
    scope TEXT NOT NULL CHECK (scope IN ('mention', 'reply', 'followed_publisher_post', 'moderation')),
    actor_subject TEXT NOT NULL REFERENCES pckg_community_profiles(subject) ON DELETE RESTRICT,
    post_id BIGINT NULL REFERENCES pckg_community_posts(id) ON DELETE CASCADE,
    comment_id BIGINT NULL REFERENCES pckg_community_comments(id) ON DELETE CASCADE,
    created_at_utc TIMESTAMPTZ NOT NULL,
    read_at_utc TIMESTAMPTZ NULL
);
CREATE INDEX IF NOT EXISTS pckg_community_notifications_recipient_unread_idx
    ON pckg_community_notifications (recipient_subject, created_at_utc DESC)
    WHERE read_at_utc IS NULL;
