ALTER TABLE users
    ADD COLUMN IF NOT EXISTS primary_image_path TEXT,
    ADD COLUMN IF NOT EXISTS backdrop_image_path TEXT,
    ADD COLUMN IF NOT EXISTS logo_image_path TEXT,
    ADD COLUMN IF NOT EXISTS date_modified TIMESTAMPTZ NOT NULL DEFAULT now();
