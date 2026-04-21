ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS status text,
    ADD COLUMN IF NOT EXISTS end_date date,
    ADD COLUMN IF NOT EXISTS air_days text[] NOT NULL DEFAULT '{}',
    ADD COLUMN IF NOT EXISTS air_time text;
