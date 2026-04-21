CREATE TABLE IF NOT EXISTS series_episode_catalog (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    series_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    external_series_id TEXT NOT NULL,
    external_season_id TEXT,
    external_episode_id TEXT,
    season_number INTEGER NOT NULL,
    episode_number INTEGER NOT NULL,
    episode_number_end INTEGER,
    name TEXT NOT NULL,
    overview TEXT,
    premiere_date DATE,
    image_path TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (series_id, provider, season_number, episode_number)
);

CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_id
    ON series_episode_catalog(series_id);

CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_date
    ON series_episode_catalog(series_id, premiere_date);
