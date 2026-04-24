-- ============================================================================
-- 0001_schema.sql
--
-- 本项目**唯一**的 schema 文件：一次描述完整 schema（表、列、索引、约束、触发器）。
--
-- 设计原则：
--   1. 所有 CREATE / ALTER / INDEX 均加 `IF NOT EXISTS`，确保同一脚本
--      可以重复执行而不报错 —— 用来同时支撑"首次建库"和"老库升级"。
--   2. 列名、默认值、约束需和 backend/src/main.rs::ensure_schema_compatibility
--      完全一致；ensure_schema_compatibility 就是本文件在运行时的守护者。
--   3. 以后要加新字段（比如新的 Emby 字段）：
--        - 在这里加 `ADD COLUMN IF NOT EXISTS ...`；
--        - 在 ensure_schema_compatibility 的对应段落里同步加一行。
--      **不要再新建 0002_*.sql 之类的迁移文件**。
--   4. 本文件刻意保留了大量 Emby SDK 中的字段作为"预留列"——即使后端暂时
--      不写入，也先占位，避免后续每加一个 Emby 功能就改 schema。
-- ============================================================================

-- ---------------------------------------------------------------------------
-- 辅助：updated_at 自动维护触发器。persons / media_streams 等表用到。
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ---------------------------------------------------------------------------
-- users：账号 + Emby 用户策略
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS users (
    id                  uuid PRIMARY KEY,
    name                text NOT NULL UNIQUE,
    password_hash       text NOT NULL,
    easy_password_hash  text,                                       -- Emby EasyPassword (PIN)
    is_admin            boolean NOT NULL DEFAULT false,
    is_hidden           boolean NOT NULL DEFAULT false,
    is_disabled         boolean NOT NULL DEFAULT false,
    policy              jsonb NOT NULL DEFAULT '{}'::jsonb,
    configuration       jsonb NOT NULL DEFAULT '{}'::jsonb,
    primary_image_path  text,
    backdrop_image_path text,
    logo_image_path     text,
    date_modified       timestamptz NOT NULL DEFAULT now(),
    created_at          timestamptz NOT NULL DEFAULT now()
);

-- 老库兼容：如果 users 早于 0001 建立，这里用 ADD COLUMN IF NOT EXISTS 补齐。
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS easy_password_hash  text,
    ADD COLUMN IF NOT EXISTS is_hidden           boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS is_disabled         boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS policy              jsonb   NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS configuration       jsonb   NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS primary_image_path  text,
    ADD COLUMN IF NOT EXISTS backdrop_image_path text,
    ADD COLUMN IF NOT EXISTS logo_image_path     text,
    ADD COLUMN IF NOT EXISTS date_modified       timestamptz NOT NULL DEFAULT now();

-- ---------------------------------------------------------------------------
-- sessions：登录会话 / 访问令牌
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS sessions (
    access_token        text PRIMARY KEY,
    user_id             uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id           text,
    device_name         text,
    client              text,
    application_version text,
    session_type        text NOT NULL DEFAULT 'Interactive',
    created_at          timestamptz NOT NULL DEFAULT now(),
    last_activity_at    timestamptz NOT NULL DEFAULT now(),
    expires_at          timestamptz
);

ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS session_type text NOT NULL DEFAULT 'Interactive',
    ADD COLUMN IF NOT EXISTS expires_at   timestamptz;

CREATE INDEX IF NOT EXISTS idx_sessions_user         ON sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at   ON sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_sessions_session_type ON sessions(session_type);

-- ---------------------------------------------------------------------------
-- libraries：媒体库（对应 Emby "VirtualFolder"）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS libraries (
    id              uuid PRIMARY KEY,
    name            text NOT NULL UNIQUE,
    collection_type text NOT NULL DEFAULT 'movies',
    path            text NOT NULL,
    library_options jsonb NOT NULL DEFAULT '{}'::jsonb,
    date_modified   timestamptz NOT NULL DEFAULT now(),
    created_at      timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE libraries
    ADD COLUMN IF NOT EXISTS library_options jsonb       NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS date_modified   timestamptz NOT NULL DEFAULT now();

-- 小写名字唯一索引：避免"Movies" / "movies" 重复建库。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = current_schema()
          AND indexname  = 'idx_libraries_name_unique'
    ) AND NOT EXISTS (
        SELECT 1 FROM libraries GROUP BY lower(name) HAVING COUNT(*) > 1
    ) THEN
        CREATE UNIQUE INDEX idx_libraries_name_unique ON libraries (lower(name));
    END IF;
END
$$;

-- ---------------------------------------------------------------------------
-- media_items：核心媒体条目表。对应 Emby BaseItemDto。
--   - 已有字段：项目正在使用的。
--   - 预留字段：Emby SDK BaseItemDto 中常见但后端还没写入的。先占位，
--     避免以后每加一个 Emby 功能再改 schema。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS media_items (
    id                       uuid PRIMARY KEY,
    library_id               uuid NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    parent_id                uuid REFERENCES media_items(id) ON DELETE CASCADE,
    name                     text NOT NULL,
    original_title           text,
    sort_name                text NOT NULL,
    forced_sort_name         text,
    item_type                text NOT NULL DEFAULT 'Movie',
    media_type               text NOT NULL DEFAULT 'Video',
    path                     text NOT NULL,
    container                text,
    overview                 text,
    taglines                 text[] NOT NULL DEFAULT ARRAY[]::text[],
    locked_fields            text[] NOT NULL DEFAULT ARRAY[]::text[],
    lock_data                boolean NOT NULL DEFAULT false,

    -- 评分 / 分级
    official_rating          text,
    custom_rating            text,
    community_rating         double precision,
    critic_rating            double precision,

    -- 时长 / 日期
    runtime_ticks            bigint,
    premiere_date            date,
    start_date               date,
    end_date                 date,
    date_created             timestamptz NOT NULL DEFAULT now(),
    date_modified            timestamptz NOT NULL DEFAULT now(),
    date_last_saved          timestamptz,
    date_last_media_added    timestamptz,
    production_year          integer,

    -- 系列字段
    status                   text,
    air_days                 text[] NOT NULL DEFAULT ARRAY[]::text[],
    air_time                 text,
    series_name              text,
    series_id                uuid,
    season_name              text,
    season_id                uuid,
    index_number             integer,
    index_number_end         integer,
    parent_index_number      integer,
    sort_index_number        integer,
    sort_parent_index_number integer,
    display_order            text,

    -- 外部元数据
    provider_ids             jsonb NOT NULL DEFAULT '{}'::jsonb,
    external_urls            jsonb NOT NULL DEFAULT '[]'::jsonb,
    genres                   text[] NOT NULL DEFAULT ARRAY[]::text[],
    studios                  text[] NOT NULL DEFAULT ARRAY[]::text[],
    tags                     text[] NOT NULL DEFAULT ARRAY[]::text[],
    production_locations     text[] NOT NULL DEFAULT ARRAY[]::text[],
    remote_trailers          text[] NOT NULL DEFAULT ARRAY[]::text[],

    -- 媒体技术参数
    width                    integer,
    height                   integer,
    bit_rate                 bigint,
    size                     bigint,
    file_name                text,
    video_codec              text,
    audio_codec              text,

    -- 图片路径（本地绝对路径）
    image_primary_path       text,
    backdrop_path            text,
    backdrop_paths           text[] NOT NULL DEFAULT ARRAY[]::text[],
    logo_path                text,
    thumb_path               text,
    art_path                 text,
    banner_path              text,
    disc_path                text,
    box_path                 text,
    menu_path                text,

    -- 图片 Tag / 聚合信息（Emby 响应需要）
    image_tags               jsonb NOT NULL DEFAULT '{}'::jsonb,
    backdrop_image_tags      text[] NOT NULL DEFAULT ARRAY[]::text[],
    primary_image_tag        text,
    primary_image_item_id    uuid,
    primary_image_aspect_ratio double precision,
    parent_logo_item_id      uuid,
    parent_logo_image_tag    text,
    parent_backdrop_item_id  uuid,
    parent_backdrop_image_tags text[] NOT NULL DEFAULT ARRAY[]::text[],
    parent_thumb_item_id     uuid,
    parent_thumb_image_tag   text,
    series_primary_image_tag text,
    series_studio            text,

    -- 计数/聚合（Emby 用来表现文件夹汇总）
    child_count              integer,
    recursive_item_count     bigint,
    season_count             integer,
    series_count             integer,
    movie_count              integer,
    special_feature_count    integer,
    local_trailer_count      integer NOT NULL DEFAULT 0,
    part_count               integer NOT NULL DEFAULT 0,

    -- 布尔标记（主要面向 Emby filter）
    is_movie                 boolean,
    is_series                boolean,
    is_folder                boolean,
    is_hd                    boolean,
    is_3d                    boolean,
    disabled                 boolean NOT NULL DEFAULT false,
    can_delete               boolean NOT NULL DEFAULT true,
    can_download             boolean NOT NULL DEFAULT true,
    supports_sync            boolean NOT NULL DEFAULT false,
    supports_resume          boolean NOT NULL DEFAULT true,

    -- 杂项（Emby 客户端可能读取）
    etag                     text,
    presentation_unique_key  text,
    collection_type          text,
    location_type            text,
    extra_type               text,

    UNIQUE (library_id, path)
);

-- 老库升级：把前面 SDK 预留字段加上。新库此处全部 no-op。
ALTER TABLE media_items
    ADD COLUMN IF NOT EXISTS original_title             text,
    ADD COLUMN IF NOT EXISTS forced_sort_name           text,
    ADD COLUMN IF NOT EXISTS taglines                   text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS locked_fields              text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS lock_data                  boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS official_rating            text,
    ADD COLUMN IF NOT EXISTS custom_rating              text,
    ADD COLUMN IF NOT EXISTS community_rating           double precision,
    ADD COLUMN IF NOT EXISTS critic_rating              double precision,
    ADD COLUMN IF NOT EXISTS start_date                 date,
    ADD COLUMN IF NOT EXISTS end_date                   date,
    ADD COLUMN IF NOT EXISTS date_last_saved            timestamptz,
    ADD COLUMN IF NOT EXISTS date_last_media_added      timestamptz,
    ADD COLUMN IF NOT EXISTS status                     text,
    ADD COLUMN IF NOT EXISTS air_days                   text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS air_time                   text,
    ADD COLUMN IF NOT EXISTS series_name                text,
    ADD COLUMN IF NOT EXISTS series_id                  uuid,
    ADD COLUMN IF NOT EXISTS season_name                text,
    ADD COLUMN IF NOT EXISTS season_id                  uuid,
    ADD COLUMN IF NOT EXISTS index_number               integer,
    ADD COLUMN IF NOT EXISTS index_number_end           integer,
    ADD COLUMN IF NOT EXISTS parent_index_number        integer,
    ADD COLUMN IF NOT EXISTS sort_index_number          integer,
    ADD COLUMN IF NOT EXISTS sort_parent_index_number   integer,
    ADD COLUMN IF NOT EXISTS display_order              text,
    ADD COLUMN IF NOT EXISTS provider_ids               jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS external_urls              jsonb NOT NULL DEFAULT '[]'::jsonb,
    ADD COLUMN IF NOT EXISTS genres                     text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS studios                    text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS tags                       text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS production_locations       text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS remote_trailers            text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS width                      integer,
    ADD COLUMN IF NOT EXISTS height                     integer,
    ADD COLUMN IF NOT EXISTS bit_rate                   bigint,
    ADD COLUMN IF NOT EXISTS size                       bigint,
    ADD COLUMN IF NOT EXISTS file_name                  text,
    ADD COLUMN IF NOT EXISTS video_codec                text,
    ADD COLUMN IF NOT EXISTS audio_codec                text,
    ADD COLUMN IF NOT EXISTS logo_path                  text,
    ADD COLUMN IF NOT EXISTS thumb_path                 text,
    ADD COLUMN IF NOT EXISTS art_path                   text,
    ADD COLUMN IF NOT EXISTS banner_path                text,
    ADD COLUMN IF NOT EXISTS disc_path                  text,
    ADD COLUMN IF NOT EXISTS backdrop_paths             text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS box_path                   text,
    ADD COLUMN IF NOT EXISTS menu_path                  text,
    ADD COLUMN IF NOT EXISTS image_tags                 jsonb NOT NULL DEFAULT '{}'::jsonb,
    ADD COLUMN IF NOT EXISTS backdrop_image_tags        text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS primary_image_tag          text,
    ADD COLUMN IF NOT EXISTS primary_image_item_id      uuid,
    ADD COLUMN IF NOT EXISTS primary_image_aspect_ratio double precision,
    ADD COLUMN IF NOT EXISTS parent_logo_item_id        uuid,
    ADD COLUMN IF NOT EXISTS parent_logo_image_tag      text,
    ADD COLUMN IF NOT EXISTS parent_backdrop_item_id    uuid,
    ADD COLUMN IF NOT EXISTS parent_backdrop_image_tags text[] NOT NULL DEFAULT ARRAY[]::text[],
    ADD COLUMN IF NOT EXISTS parent_thumb_item_id       uuid,
    ADD COLUMN IF NOT EXISTS parent_thumb_image_tag     text,
    ADD COLUMN IF NOT EXISTS series_primary_image_tag   text,
    ADD COLUMN IF NOT EXISTS series_studio              text,
    ADD COLUMN IF NOT EXISTS child_count                integer,
    ADD COLUMN IF NOT EXISTS recursive_item_count       bigint,
    ADD COLUMN IF NOT EXISTS season_count               integer,
    ADD COLUMN IF NOT EXISTS series_count               integer,
    ADD COLUMN IF NOT EXISTS movie_count                integer,
    ADD COLUMN IF NOT EXISTS special_feature_count      integer,
    ADD COLUMN IF NOT EXISTS local_trailer_count        integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS part_count                 integer NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS is_movie                   boolean,
    ADD COLUMN IF NOT EXISTS is_series                  boolean,
    ADD COLUMN IF NOT EXISTS is_folder                  boolean,
    ADD COLUMN IF NOT EXISTS is_hd                      boolean,
    ADD COLUMN IF NOT EXISTS is_3d                      boolean,
    ADD COLUMN IF NOT EXISTS disabled                   boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS can_delete                 boolean NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS can_download               boolean NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS supports_sync              boolean NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS supports_resume            boolean NOT NULL DEFAULT true,
    ADD COLUMN IF NOT EXISTS etag                       text,
    ADD COLUMN IF NOT EXISTS presentation_unique_key    text,
    ADD COLUMN IF NOT EXISTS collection_type            text,
    ADD COLUMN IF NOT EXISTS location_type              text,
    ADD COLUMN IF NOT EXISTS extra_type                 text;

CREATE INDEX IF NOT EXISTS idx_media_items_library   ON media_items(library_id);
CREATE INDEX IF NOT EXISTS idx_media_items_parent    ON media_items(parent_id);
CREATE INDEX IF NOT EXISTS idx_media_items_type      ON media_items(item_type);
CREATE INDEX IF NOT EXISTS idx_media_items_sort      ON media_items(sort_name);
CREATE INDEX IF NOT EXISTS idx_media_items_series    ON media_items(series_id);
CREATE INDEX IF NOT EXISTS idx_media_items_premiere  ON media_items(premiere_date);

-- ---------------------------------------------------------------------------
-- user_item_data：每个用户对每个条目的播放/收藏状态（Emby UserItemDataDto）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS user_item_data (
    user_id                 uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id                 uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playback_position_ticks bigint NOT NULL DEFAULT 0,
    play_count              integer NOT NULL DEFAULT 0,
    is_favorite             boolean NOT NULL DEFAULT false,
    is_played               boolean NOT NULL DEFAULT false,
    last_played_date        timestamptz,
    -- Emby SDK 预留：
    rating                  double precision,
    played_percentage       double precision,
    unplayed_item_count     integer,
    likes                   boolean,
    updated_at              timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, item_id)
);

ALTER TABLE user_item_data
    ADD COLUMN IF NOT EXISTS rating              double precision,
    ADD COLUMN IF NOT EXISTS played_percentage   double precision,
    ADD COLUMN IF NOT EXISTS unplayed_item_count integer,
    ADD COLUMN IF NOT EXISTS likes               boolean;

-- ---------------------------------------------------------------------------
-- playback_events：播放事件流水
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS playback_events (
    id                   uuid PRIMARY KEY,
    user_id              uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    item_id              uuid REFERENCES media_items(id) ON DELETE SET NULL,
    session_id           text,
    event_type           text NOT NULL,
    position_ticks       bigint,
    is_paused            boolean,
    played_to_completion boolean,
    created_at           timestamptz NOT NULL DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- media_streams：视频/音频/字幕等轨道信息。对应 Emby MediaStream。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS media_streams (
    id                                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    media_item_id                      uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    index                              integer NOT NULL,
    stream_type                        text NOT NULL
        CHECK (stream_type IN ('video','audio','subtitle','data','attachment')),
    codec                              text,
    codec_tag                          text,
    language                           text,
    title                              text,
    is_default                         boolean DEFAULT false,
    is_forced                          boolean DEFAULT false,
    is_external                        boolean DEFAULT false,
    is_hearing_impaired                boolean DEFAULT false,
    profile                            text,
    width                              integer,
    height                             integer,
    channels                           integer,
    sample_rate                        integer,
    bit_rate                           integer,
    bit_depth                          integer,
    channel_layout                     text,
    aspect_ratio                       text,
    average_frame_rate                 double precision,
    real_frame_rate                    double precision,
    is_interlaced                      boolean DEFAULT false,
    color_range                        text,
    color_space                        text,
    color_transfer                     text,
    color_primaries                    text,
    rotation                           integer,
    hdr10_plus_present_flag            boolean,
    dv_version_major                   integer,
    dv_version_minor                   integer,
    dv_profile                         integer,
    dv_level                           integer,
    dv_bl_signal_compatibility_id      integer,
    comment                            text,
    time_base                          text,
    codec_time_base                    text,
    attachment_size                    integer,
    extended_video_sub_type            text,
    extended_video_sub_type_description text,
    extended_video_type                text,
    is_anamorphic                      boolean,
    is_avc                             boolean,
    is_external_url                    text,
    is_text_subtitle_stream            boolean,
    level                              integer,
    pixel_format                       text,
    ref_frames                         integer,
    stream_start_time_ticks            bigint,
    -- Emby SDK 预留字段：
    mime_type                          text,
    subtitle_location_type             text,
    is_closed_captions                 boolean,
    nal_length_size                    text,
    video_range                        text,
    delivery_method                    text,
    delivery_url                       text,
    extradata                          text,
    created_at                         timestamptz NOT NULL DEFAULT now(),
    updated_at                         timestamptz NOT NULL DEFAULT now(),
    UNIQUE (media_item_id, index, stream_type)
);

ALTER TABLE media_streams
    ADD COLUMN IF NOT EXISTS attachment_size                     integer,
    ADD COLUMN IF NOT EXISTS extended_video_sub_type             text,
    ADD COLUMN IF NOT EXISTS extended_video_sub_type_description text,
    ADD COLUMN IF NOT EXISTS extended_video_type                 text,
    ADD COLUMN IF NOT EXISTS is_anamorphic                       boolean,
    ADD COLUMN IF NOT EXISTS is_avc                              boolean,
    ADD COLUMN IF NOT EXISTS is_external_url                     text,
    ADD COLUMN IF NOT EXISTS is_text_subtitle_stream             boolean,
    ADD COLUMN IF NOT EXISTS level                               integer,
    ADD COLUMN IF NOT EXISTS pixel_format                        text,
    ADD COLUMN IF NOT EXISTS ref_frames                          integer,
    ADD COLUMN IF NOT EXISTS stream_start_time_ticks             bigint,
    ADD COLUMN IF NOT EXISTS mime_type                           text,
    ADD COLUMN IF NOT EXISTS subtitle_location_type              text,
    ADD COLUMN IF NOT EXISTS is_closed_captions                  boolean,
    ADD COLUMN IF NOT EXISTS nal_length_size                     text,
    ADD COLUMN IF NOT EXISTS video_range                         text,
    ADD COLUMN IF NOT EXISTS delivery_method                     text,
    ADD COLUMN IF NOT EXISTS delivery_url                        text,
    ADD COLUMN IF NOT EXISTS extradata                           text;

CREATE INDEX IF NOT EXISTS idx_media_streams_media_item_id ON media_streams(media_item_id);
CREATE INDEX IF NOT EXISTS idx_media_streams_stream_type   ON media_streams(stream_type);
CREATE INDEX IF NOT EXISTS idx_media_streams_language      ON media_streams(language);
CREATE INDEX IF NOT EXISTS idx_media_streams_codec         ON media_streams(codec);

DROP TRIGGER IF EXISTS update_media_streams_updated_at ON media_streams;
CREATE TRIGGER update_media_streams_updated_at
    BEFORE UPDATE ON media_streams
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- 老库里若缺 UNIQUE 约束，这里兜一刀（先清重复，再建）。
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = current_schema()
          AND tablename  = 'media_streams'
          AND indexname  = 'media_streams_media_item_id_index_stream_type_key'
    ) THEN
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
        EXCEPTION WHEN duplicate_object THEN NULL;
        END;
    END IF;
END
$$;

-- ---------------------------------------------------------------------------
-- media_chapters：章节标记
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS media_chapters (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    media_item_id        uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    chapter_index        integer NOT NULL,
    start_position_ticks bigint NOT NULL,
    name                 text,
    marker_type          text,
    image_path           text,
    image_tag            text,
    image_date_modified  timestamptz,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    UNIQUE (media_item_id, chapter_index)
);

ALTER TABLE media_chapters
    ADD COLUMN IF NOT EXISTS image_tag           text,
    ADD COLUMN IF NOT EXISTS image_date_modified timestamptz;

CREATE INDEX IF NOT EXISTS idx_media_chapters_media_item_id ON media_chapters(media_item_id);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_indexes
        WHERE schemaname = current_schema()
          AND tablename  = 'media_chapters'
          AND indexname  = 'media_chapters_media_item_id_chapter_index_key'
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
        EXCEPTION WHEN duplicate_object THEN NULL;
        END;
    END IF;
END
$$;

-- ---------------------------------------------------------------------------
-- persons / person_roles：演职员表。对应 Emby BaseItemPerson。
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS persons (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    name                text NOT NULL,
    sort_name           text,
    overview            text,
    external_url        text,
    provider_ids        jsonb DEFAULT '{}'::jsonb,
    premiere_date       timestamptz,
    production_year     integer,
    primary_image_path  text,
    backdrop_image_path text,
    logo_image_path     text,
    favorite_count      integer DEFAULT 0,
    created_at          timestamptz DEFAULT now(),
    updated_at          timestamptz DEFAULT now(),
    UNIQUE (name, sort_name)
);

CREATE INDEX IF NOT EXISTS idx_persons_name            ON persons(name);
CREATE INDEX IF NOT EXISTS idx_persons_sort_name       ON persons(sort_name);
CREATE INDEX IF NOT EXISTS idx_persons_production_year ON persons(production_year);

DROP TRIGGER IF EXISTS update_persons_updated_at ON persons;
CREATE TRIGGER update_persons_updated_at
    BEFORE UPDATE ON persons
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TABLE IF NOT EXISTS person_roles (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    person_id        uuid NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
    media_item_id    uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    role_type        text NOT NULL
        CHECK (role_type IN ('Actor','Director','Writer','Producer','Composer','Cinematographer','Editor','Other')),
    role             text,
    sort_order       integer DEFAULT 0,
    is_featured      boolean DEFAULT false,
    is_leading_role  boolean DEFAULT false,
    is_recurring     boolean DEFAULT false,
    created_at       timestamptz DEFAULT now(),
    updated_at       timestamptz DEFAULT now(),
    UNIQUE (person_id, media_item_id, role_type, role),
    CONSTRAINT check_role_not_empty CHECK (role IS NULL OR trim(role) != '')
);

CREATE INDEX IF NOT EXISTS idx_person_roles_person_id     ON person_roles(person_id);
CREATE INDEX IF NOT EXISTS idx_person_roles_media_item_id ON person_roles(media_item_id);
CREATE INDEX IF NOT EXISTS idx_person_roles_role_type     ON person_roles(role_type);

DROP TRIGGER IF EXISTS update_person_roles_updated_at ON person_roles;
CREATE TRIGGER update_person_roles_updated_at
    BEFORE UPDATE ON person_roles
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- ---------------------------------------------------------------------------
-- system_settings：后端运行期配置（key-value）
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS system_settings (
    key        text PRIMARY KEY,
    value      jsonb NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- 新库：初次建表时，如已有用户则认为启动向导完成，否则待引导。
INSERT INTO system_settings (key, value)
SELECT 'startup_wizard_completed', to_jsonb(EXISTS (SELECT 1 FROM users))
ON CONFLICT (key) DO NOTHING;

-- ---------------------------------------------------------------------------
-- series_episode_catalog：TMDB / TVDB 等抓取的"应当存在"的分集目录
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS series_episode_catalog (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    series_id           uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    provider            text NOT NULL,
    external_series_id  text NOT NULL,
    external_season_id  text,
    external_episode_id text,
    season_number       integer NOT NULL,
    episode_number      integer NOT NULL,
    episode_number_end  integer,
    name                text NOT NULL,
    overview            text,
    premiere_date       date,
    image_path          text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    UNIQUE (series_id, provider, season_number, episode_number)
);

CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_id
    ON series_episode_catalog(series_id);
CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_date
    ON series_episode_catalog(series_id, premiere_date);

-- ---------------------------------------------------------------------------
-- session_play_queue / session_commands：远程控制 & 当前播放队列
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS session_play_queue (
    session_id       text NOT NULL REFERENCES sessions(access_token) ON DELETE CASCADE,
    item_id          uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playlist_item_id text NOT NULL DEFAULT md5(random()::text || clock_timestamp()::text),
    sort_index       integer NOT NULL DEFAULT 0,
    position_ticks   bigint,
    is_paused        boolean,
    play_state       text NOT NULL DEFAULT 'Playing',
    updated_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (session_id, item_id)
);

CREATE INDEX IF NOT EXISTS idx_session_play_queue_session
    ON session_play_queue(session_id, sort_index);

CREATE TABLE IF NOT EXISTS session_commands (
    id          uuid PRIMARY KEY,
    session_id  text NOT NULL REFERENCES sessions(access_token) ON DELETE CASCADE,
    command     text NOT NULL,
    payload     jsonb NOT NULL DEFAULT '{}'::jsonb,
    consumed_at timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now()
);

ALTER TABLE session_commands
    ADD COLUMN IF NOT EXISTS consumed_at timestamptz;

CREATE INDEX IF NOT EXISTS idx_session_commands_session
    ON session_commands(session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_session_commands_unconsumed
    ON session_commands(session_id, created_at)
    WHERE consumed_at IS NULL;

-- ---------------------------------------------------------------------------
-- display_preferences：每用户显示偏好
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS display_preferences (
    user_id                  uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    display_preferences_id   text NOT NULL,
    client                   text NOT NULL DEFAULT 'emby',
    preferences              jsonb NOT NULL DEFAULT '{}'::jsonb,
    updated_at               timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, display_preferences_id, client)
);

CREATE INDEX IF NOT EXISTS idx_display_preferences_user
    ON display_preferences(user_id, updated_at DESC);

-- ---------------------------------------------------------------------------
-- playlists / playlist_items：用户维度的自定义播放列表
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS playlists (
    id         uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name       text NOT NULL,
    media_type text NOT NULL DEFAULT 'Video',
    overview   text,
    image_primary_path text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_playlists_user_id
    ON playlists(user_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS playlist_items (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    playlist_id      uuid NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
    media_item_id    uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
    playlist_item_id text NOT NULL DEFAULT md5(random()::text || clock_timestamp()::text),
    sort_index       integer NOT NULL DEFAULT 0,
    created_at       timestamptz NOT NULL DEFAULT now(),
    UNIQUE (playlist_id, playlist_item_id)
);

CREATE INDEX IF NOT EXISTS idx_playlist_items_playlist
    ON playlist_items(playlist_id, sort_index);
CREATE INDEX IF NOT EXISTS idx_playlist_items_media_item
    ON playlist_items(media_item_id);
