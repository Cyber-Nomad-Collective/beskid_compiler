-- Typed community-delivery preferences and the self-addressed system check.
ALTER TABLE pckg_community_notification_preferences
    ADD COLUMN IF NOT EXISTS system_enabled BOOLEAN NOT NULL DEFAULT TRUE;

ALTER TABLE pckg_community_notifications
    DROP CONSTRAINT IF EXISTS pckg_community_notifications_scope_check;
ALTER TABLE pckg_community_notifications
    ADD CONSTRAINT pckg_community_notifications_scope_check
    CHECK (scope IN ('system', 'mention', 'reply', 'followed_publisher_post', 'moderation'));
