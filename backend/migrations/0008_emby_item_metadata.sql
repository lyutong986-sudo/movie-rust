ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS original_title text,
    ADD COLUMN IF NOT EXISTS official_rating text,
    ADD COLUMN IF NOT EXISTS community_rating double precision,
    ADD COLUMN IF NOT EXISTS studios text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS tags text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS production_locations text[] NOT NULL DEFAULT ARRAY[]::text[];

