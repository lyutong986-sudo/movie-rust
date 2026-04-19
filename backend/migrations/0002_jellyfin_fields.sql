ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS series_name text,
    ADD COLUMN IF NOT EXISTS season_name text,
    ADD COLUMN IF NOT EXISTS index_number integer,
    ADD COLUMN IF NOT EXISTS index_number_end integer,
    ADD COLUMN IF NOT EXISTS parent_index_number integer,
    ADD COLUMN IF NOT EXISTS provider_ids jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS genres text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS width integer,
    ADD COLUMN IF NOT EXISTS height integer,
    ADD COLUMN IF NOT EXISTS video_codec text,
    ADD COLUMN IF NOT EXISTS audio_codec text;
