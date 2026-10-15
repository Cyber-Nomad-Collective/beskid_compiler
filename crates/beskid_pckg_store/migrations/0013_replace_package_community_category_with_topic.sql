-- Each public package owns one discussion thread (NodeBB topic) instead of a
-- category. The category link columns from 0012 are dropped; nothing was
-- configured against them in production.
ALTER TABLE pckg_packages
    DROP COLUMN IF EXISTS community_category_id,
    DROP COLUMN IF EXISTS community_category_slug,
    ADD COLUMN IF NOT EXISTS community_topic_id INTEGER NULL,
    ADD COLUMN IF NOT EXISTS community_topic_slug TEXT NULL;
