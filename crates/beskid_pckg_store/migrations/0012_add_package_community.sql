-- Links a package to its NodeBB discussion category. The registry stores only
-- the category identity; topics, posts and moderation stay in NodeBB.
ALTER TABLE pckg_packages
    ADD COLUMN IF NOT EXISTS community_category_id INTEGER NULL,
    ADD COLUMN IF NOT EXISTS community_category_slug TEXT NULL;
