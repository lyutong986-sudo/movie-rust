-- 第七轮：让媒体入库完全幂等，避免扫描被单条冲突打断。
--
-- 背景：
-- * 历史上 media_items 缺列（original_title 等）、persons / media_streams / media_chapters
--   在再次扫描 / 重复探测时会命中 UNIQUE 或 pkey 冲突，导致扫描中途失败、库不全；
-- * 这里统一补齐列 + 索引，让 ON CONFLICT 在上层可以稳定兜底。

-- 1. media_items：把 Emby 必需列在这里再补齐一遍，确保老库也能升上来。
ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS original_title text,
    ADD COLUMN IF NOT EXISTS official_rating text,
    ADD COLUMN IF NOT EXISTS community_rating double precision,
    ADD COLUMN IF NOT EXISTS critic_rating double precision,
    ADD COLUMN IF NOT EXISTS series_name text,
    ADD COLUMN IF NOT EXISTS season_name text,
    ADD COLUMN IF NOT EXISTS index_number integer,
    ADD COLUMN IF NOT EXISTS index_number_end integer,
    ADD COLUMN IF NOT EXISTS parent_index_number integer,
    ADD COLUMN IF NOT EXISTS provider_ids jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS genres text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS studios text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS tags text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS production_locations text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS width integer,
    ADD COLUMN IF NOT EXISTS height integer,
    ADD COLUMN IF NOT EXISTS bit_rate bigint,
    ADD COLUMN IF NOT EXISTS video_codec text,
    ADD COLUMN IF NOT EXISTS audio_codec text,
    ADD COLUMN IF NOT EXISTS logo_path text,
    ADD COLUMN IF NOT EXISTS thumb_path text,
    ADD COLUMN IF NOT EXISTS remote_trailers text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS status text,
    ADD COLUMN IF NOT EXISTS end_date date,
    ADD COLUMN IF NOT EXISTS air_days text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS air_time text;

-- 2. media_streams：老版本可能没有 UNIQUE 约束；没有的话现在就补上，
--    这样上层 `ON CONFLICT (media_item_id, index, stream_type)` 才能命中。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = current_schema()
          AND tablename = 'media_streams'
          AND indexname = 'media_streams_media_item_id_index_stream_type_key'
    ) THEN
        -- 先清掉潜在的重复，避免建唯一索引失败。
        DELETE FROM media_streams ms
        USING (
            SELECT ctid, row_number() OVER (
                PARTITION BY media_item_id, index, stream_type
                ORDER BY created_at ASC, ctid
            ) AS rn
            FROM media_streams
        ) dups
        WHERE ms.ctid = dups.ctid AND dups.rn > 1;

        BEGIN
            ALTER TABLE media_streams
                ADD CONSTRAINT media_streams_media_item_id_index_stream_type_key
                UNIQUE (media_item_id, index, stream_type);
        EXCEPTION WHEN duplicate_object THEN
            NULL;
        END;
    END IF;
END
$$;

-- 3. media_chapters：同理，确保 (media_item_id, chapter_index) 有 UNIQUE。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = current_schema()
          AND tablename = 'media_chapters'
          AND indexname = 'media_chapters_media_item_id_chapter_index_key'
    ) THEN
        DELETE FROM media_chapters mc
        USING (
            SELECT ctid, row_number() OVER (
                PARTITION BY media_item_id, chapter_index
                ORDER BY created_at ASC, ctid
            ) AS rn
            FROM media_chapters
        ) dups
        WHERE mc.ctid = dups.ctid AND dups.rn > 1;

        BEGIN
            ALTER TABLE media_chapters
                ADD CONSTRAINT media_chapters_media_item_id_chapter_index_key
                UNIQUE (media_item_id, chapter_index);
        EXCEPTION WHEN duplicate_object THEN
            NULL;
        END;
    END IF;
END
$$;
