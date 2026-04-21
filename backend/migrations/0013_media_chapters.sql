CREATE TABLE IF NOT EXISTS media_chapters (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    chapter_index INTEGER NOT NULL,
    start_position_ticks BIGINT NOT NULL,
    name TEXT,
    marker_type TEXT,
    image_path TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (media_item_id, chapter_index)
);

CREATE INDEX IF NOT EXISTS idx_media_chapters_media_item_id
    ON media_chapters(media_item_id);
