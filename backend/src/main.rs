mod auth;
mod config;
mod error;
mod media_analyzer;
mod metadata;
mod models;
mod naming;
mod repository;
mod routes;
mod scanner;
mod security;
mod state;
mod transcoder;

use anyhow::{Context, Result};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::sync::Arc;

use crate::transcoder::Transcoder;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing_appender::rolling;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let bootstrap_config = config::Config::from_env()?;
    std::fs::create_dir_all(&bootstrap_config.log_dir)
        .with_context(|| format!("创建日志目录失败: {}", bootstrap_config.log_dir.display()))?;
    let file_appender = rolling::daily(&bootstrap_config.log_dir, "server.log");
    let (file_writer, _log_guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "movie_rust_backend=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .init();

    let config = bootstrap_config;
    let static_dir = config.static_dir.clone();
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url)
        .await
        .context("连接 PostgreSQL 失败，请检查 DATABASE_URL")?;

    run_startup_schema_tasks(&pool, &config).await?;

    let mut metadata_manager = metadata::provider::MetadataProviderManager::new();

    if let Some(tmdb_api_key) = &config.tmdb_api_key {
        let tmdb_provider = metadata::tmdb::TmdbProvider::new(tmdb_api_key.clone());
        metadata_manager.register_provider(Box::new(tmdb_provider));
        tracing::info!("TMDB 元数据提供者已注册");
    }

    let bind_addr = config.bind_addr()?;
    let config = Arc::new(config);
    let transcoder = Transcoder::new(config.clone());
    let state = AppState {
        pool,
        config,
        metadata_manager: Some(Arc::new(metadata_manager)),
        websocket_sessions: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        transcoder,
    };

    let spa =
        ServeDir::new(&static_dir).not_found_service(ServeFile::new(static_dir.join("index.html")));

    let http_trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
        .on_failure(DefaultOnFailure::new().level(Level::ERROR));

    let app = routes::router(state.clone())
        .fallback_service(spa)
        .layer(http_trace)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("Movie Rust backend listening on http://{}", bind_addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn run_startup_schema_tasks(pool: &sqlx::PgPool, config: &config::Config) -> Result<()> {
    match sqlx::migrate!("./migrations").run(pool).await {
        Ok(_) => {}
        Err(error) => {
            let error_text = error.to_string();
            if error_text.contains("previously applied but has been modified") {
                tracing::warn!(
                    "检测到 sqlx 迁移校验失败（已应用迁移文件被修改），继续执行兼容性补齐 SQL：{}",
                    error_text
                );
            } else {
                return Err(error).context("执行数据库迁移失败");
            }
        }
    }

    ensure_schema_compatibility(pool, config).await?;
    Ok(())
}

async fn ensure_schema_compatibility(pool: &sqlx::PgPool, config: &config::Config) -> Result<()> {
    let compatibility_sql = [
        r#"
        CREATE EXTENSION IF NOT EXISTS pgcrypto
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS system_settings (
            key text PRIMARY KEY,
            value jsonb NOT NULL,
            updated_at timestamptz NOT NULL DEFAULT now()
        )
        "#,
        r#"
        CREATE OR REPLACE FUNCTION update_updated_at_column()
        RETURNS TRIGGER AS $$
        BEGIN
            NEW.updated_at = NOW();
            RETURN NEW;
        END;
        $$ language 'plpgsql'
        "#,
        r#"
        ALTER TABLE users
            ADD COLUMN IF NOT EXISTS policy JSONB DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS configuration JSONB DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS last_login_date TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS last_activity_date TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS easy_password TEXT,
            ADD COLUMN IF NOT EXISTS connect_user_id TEXT,
            ADD COLUMN IF NOT EXISTS connect_user_name TEXT,
            ADD COLUMN IF NOT EXISTS connect_link_type TEXT,
            ADD COLUMN IF NOT EXISTS connect_access_key TEXT,
            ADD COLUMN IF NOT EXISTS image_infos JSONB NOT NULL DEFAULT '[]'::jsonb,
            ADD COLUMN IF NOT EXISTS typed_settings JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS primary_image_path TEXT,
            ADD COLUMN IF NOT EXISTS backdrop_image_path TEXT,
            ADD COLUMN IF NOT EXISTS logo_image_path TEXT,
            ADD COLUMN IF NOT EXISTS date_modified TIMESTAMPTZ NOT NULL DEFAULT now()
        "#,
        r#"
        ALTER TABLE sessions
            ADD COLUMN IF NOT EXISTS expires_at TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS app_name TEXT,
            ADD COLUMN IF NOT EXISTS app_icon_url TEXT,
            ADD COLUMN IF NOT EXISTS remote_end_point TEXT,
            ADD COLUMN IF NOT EXISTS playable_media_types TEXT[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS supported_commands TEXT[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS capabilities JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS now_playing_item_id UUID,
            ADD COLUMN IF NOT EXISTS play_state JSONB NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_sessions_expires_at
            ON sessions(expires_at)
        "#,
        r#"
        ALTER TABLE libraries
            ADD COLUMN IF NOT EXISTS library_options jsonb NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS preferred_metadata_language TEXT,
            ADD COLUMN IF NOT EXISTS metadata_country_code TEXT,
            ADD COLUMN IF NOT EXISTS collection_type_options JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS path_infos JSONB NOT NULL DEFAULT '[]'::jsonb,
            ADD COLUMN IF NOT EXISTS date_modified timestamptz NOT NULL DEFAULT now()
        "#,
        r#"
        UPDATE libraries
        SET library_options = jsonb_build_object(
                'Enabled', true,
                'EnablePhotos', true,
                'EnableRealtimeMonitor', false,
                'EnableAutomaticSeriesGrouping', true,
                'PreferredMetadataLanguage', 'zh',
                'MetadataCountryCode', 'CN',
                'SeasonZeroDisplayName', 'Specials',
                'MetadataSavers', jsonb_build_array('Nfo'),
                'LocalMetadataReaderOrder', jsonb_build_array('Nfo'),
                'PathInfos', jsonb_build_array(jsonb_build_object('Path', path))
            ),
            date_modified = now()
        WHERE library_options = '{}'::jsonb
        "#,
        r#"
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
            ADD COLUMN IF NOT EXISTS audio_codec text,
            ADD COLUMN IF NOT EXISTS original_title text,
            ADD COLUMN IF NOT EXISTS official_rating text,
            ADD COLUMN IF NOT EXISTS community_rating double precision,
            ADD COLUMN IF NOT EXISTS critic_rating double precision,
            ADD COLUMN IF NOT EXISTS custom_rating text,
            ADD COLUMN IF NOT EXISTS home_page_url text,
            ADD COLUMN IF NOT EXISTS budget bigint,
            ADD COLUMN IF NOT EXISTS revenue bigint,
            ADD COLUMN IF NOT EXISTS studios text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS tags text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS taglines text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS production_locations text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS official_rating_description text,
            ADD COLUMN IF NOT EXISTS logo_path text,
            ADD COLUMN IF NOT EXISTS thumb_path text,
            ADD COLUMN IF NOT EXISTS banner_path text,
            ADD COLUMN IF NOT EXISTS art_path text,
            ADD COLUMN IF NOT EXISTS disc_path text,
            ADD COLUMN IF NOT EXISTS image_tags jsonb NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS backdrop_image_tags text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS remote_trailers text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS status text,
            ADD COLUMN IF NOT EXISTS end_date date,
            ADD COLUMN IF NOT EXISTS air_days text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS air_time text,
            ADD COLUMN IF NOT EXISTS bit_rate BIGINT,
            ADD COLUMN IF NOT EXISTS size_bytes BIGINT,
            ADD COLUMN IF NOT EXISTS primary_image_aspect_ratio double precision,
            ADD COLUMN IF NOT EXISTS locked_fields text[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS is_locked boolean NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS is_virtual_item boolean NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS is_place_holder boolean NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS is_shortcut boolean NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS is_hd boolean,
            ADD COLUMN IF NOT EXISTS location_type text,
            ADD COLUMN IF NOT EXISTS path_protocol text,
            ADD COLUMN IF NOT EXISTS display_order text,
            ADD COLUMN IF NOT EXISTS forced_sort_name text,
            ADD COLUMN IF NOT EXISTS presentation_unique_key text,
            ADD COLUMN IF NOT EXISTS external_id text,
            ADD COLUMN IF NOT EXISTS album text,
            ADD COLUMN IF NOT EXISTS album_id uuid,
            ADD COLUMN IF NOT EXISTS series_id uuid,
            ADD COLUMN IF NOT EXISTS season_id uuid,
            ADD COLUMN IF NOT EXISTS display_parent_id uuid,
            ADD COLUMN IF NOT EXISTS preferred_metadata_language text,
            ADD COLUMN IF NOT EXISTS preferred_metadata_country_code text,
            ADD COLUMN IF NOT EXISTS date_last_saved timestamptz,
            ADD COLUMN IF NOT EXISTS date_last_refreshed timestamptz,
            ADD COLUMN IF NOT EXISTS extra_type text,
            ADD COLUMN IF NOT EXISTS sync_status text,
            ADD COLUMN IF NOT EXISTS share_level text,
            ADD COLUMN IF NOT EXISTS external_urls jsonb NOT NULL DEFAULT '[]'::jsonb,
            ADD COLUMN IF NOT EXISTS trailer_urls jsonb NOT NULL DEFAULT '[]'::jsonb,
            ADD COLUMN IF NOT EXISTS provider_metadata jsonb NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS emby_extra jsonb NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        ALTER TABLE user_item_data
            ADD COLUMN IF NOT EXISTS rating DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS played_percentage DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS unplayed_item_count INTEGER,
            ADD COLUMN IF NOT EXISTS likes BOOLEAN,
            ADD COLUMN IF NOT EXISTS audio_stream_index INTEGER,
            ADD COLUMN IF NOT EXISTS subtitle_stream_index INTEGER,
            ADD COLUMN IF NOT EXISTS hide_from_resume BOOLEAN NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS user_data_key TEXT,
            ADD COLUMN IF NOT EXISTS item_display_preferences_id TEXT,
            ADD COLUMN IF NOT EXISTS custom_data JSONB NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS media_streams (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            index INTEGER NOT NULL,
            stream_type TEXT NOT NULL CHECK (stream_type IN ('video', 'audio', 'subtitle', 'data', 'attachment')),
            codec TEXT,
            codec_tag TEXT,
            language TEXT,
            title TEXT,
            is_default BOOLEAN DEFAULT false,
            is_forced BOOLEAN DEFAULT false,
            is_external BOOLEAN DEFAULT false,
            is_hearing_impaired BOOLEAN DEFAULT false,
            profile TEXT,
            width INTEGER,
            height INTEGER,
            channels INTEGER,
            sample_rate INTEGER,
            bit_rate INTEGER,
            bit_depth INTEGER,
            channel_layout TEXT,
            aspect_ratio TEXT,
            average_frame_rate FLOAT,
            real_frame_rate FLOAT,
            is_interlaced BOOLEAN DEFAULT false,
            color_range TEXT,
            color_space TEXT,
            color_transfer TEXT,
            color_primaries TEXT,
            rotation INTEGER,
            hdr10_plus_present_flag BOOLEAN,
            dv_version_major INTEGER,
            dv_version_minor INTEGER,
            dv_profile INTEGER,
            dv_level INTEGER,
            dv_bl_signal_compatibility_id INTEGER,
            comment TEXT,
            time_base TEXT,
            codec_time_base TEXT,
            created_at TIMESTAMPTZ DEFAULT now(),
            updated_at TIMESTAMPTZ DEFAULT now(),
            UNIQUE(media_item_id, index, stream_type)
        )
        "#,
        r#"
        ALTER TABLE media_streams
            ADD COLUMN IF NOT EXISTS attachment_size INTEGER,
            ADD COLUMN IF NOT EXISTS extended_video_sub_type TEXT,
            ADD COLUMN IF NOT EXISTS extended_video_sub_type_description TEXT,
            ADD COLUMN IF NOT EXISTS extended_video_type TEXT,
            ADD COLUMN IF NOT EXISTS is_anamorphic BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_avc BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_external_url TEXT,
            ADD COLUMN IF NOT EXISTS is_text_subtitle_stream BOOLEAN,
            ADD COLUMN IF NOT EXISTS level INTEGER,
            ADD COLUMN IF NOT EXISTS pixel_format TEXT,
            ADD COLUMN IF NOT EXISTS ref_frames INTEGER,
            ADD COLUMN IF NOT EXISTS stream_start_time_ticks BIGINT,
            ADD COLUMN IF NOT EXISTS display_title TEXT,
            ADD COLUMN IF NOT EXISTS path TEXT,
            ADD COLUMN IF NOT EXISTS protocol TEXT,
            ADD COLUMN IF NOT EXISTS delivery_method TEXT,
            ADD COLUMN IF NOT EXISTS delivery_url TEXT,
            ADD COLUMN IF NOT EXISTS supports_external_stream BOOLEAN,
            ADD COLUMN IF NOT EXISTS nal_length_size TEXT,
            ADD COLUMN IF NOT EXISTS video_range TEXT,
            ADD COLUMN IF NOT EXISTS video_range_type TEXT,
            ADD COLUMN IF NOT EXISTS localized_undefined TEXT,
            ADD COLUMN IF NOT EXISTS localized_default TEXT,
            ADD COLUMN IF NOT EXISTS localized_forced TEXT,
            ADD COLUMN IF NOT EXISTS localized_external TEXT,
            ADD COLUMN IF NOT EXISTS emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_media_streams_media_item_id
            ON media_streams(media_item_id)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_media_streams_stream_type
            ON media_streams(stream_type)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_media_streams_language
            ON media_streams(language)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_media_streams_codec
            ON media_streams(codec)
        "#,
        r#"
        DROP TRIGGER IF EXISTS update_media_streams_updated_at ON media_streams
        "#,
        r#"
        CREATE TRIGGER update_media_streams_updated_at
            BEFORE UPDATE ON media_streams
            FOR EACH ROW
            EXECUTE FUNCTION update_updated_at_column()
        "#,
        r#"
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
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_media_chapters_media_item_id
            ON media_chapters(media_item_id)
        "#,
        r#"
        ALTER TABLE media_chapters
            ADD COLUMN IF NOT EXISTS image_date_modified TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        ALTER TABLE users
            ADD COLUMN IF NOT EXISTS primary_image_path TEXT,
            ADD COLUMN IF NOT EXISTS backdrop_image_path TEXT,
            ADD COLUMN IF NOT EXISTS logo_image_path TEXT,
            ADD COLUMN IF NOT EXISTS date_modified TIMESTAMPTZ NOT NULL DEFAULT now()
        "#,
        r#"
        ALTER TABLE media_items
            ADD COLUMN IF NOT EXISTS critic_rating DOUBLE PRECISION
        "#,
        r#"
        ALTER TABLE media_items
            ADD COLUMN IF NOT EXISTS taglines TEXT[] NOT NULL DEFAULT ARRAY[]::text[]
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS persons (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name TEXT NOT NULL,
            sort_name TEXT,
            overview TEXT,
            external_url TEXT,
            provider_ids JSONB DEFAULT '{}'::jsonb,
            premiere_date TIMESTAMPTZ,
            production_year INTEGER,
            primary_image_path TEXT,
            backdrop_image_path TEXT,
            logo_image_path TEXT,
            favorite_count INTEGER DEFAULT 0,
            created_at TIMESTAMPTZ DEFAULT now(),
            updated_at TIMESTAMPTZ DEFAULT now(),
            UNIQUE(name, sort_name)
        )
        "#,
        r#"
        ALTER TABLE persons
            ADD COLUMN IF NOT EXISTS birth_date TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS death_date TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS birth_place TEXT,
            ADD COLUMN IF NOT EXISTS image_tags JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS backdrop_image_tags TEXT[] NOT NULL DEFAULT ARRAY[]::text[],
            ADD COLUMN IF NOT EXISTS external_urls JSONB NOT NULL DEFAULT '[]'::jsonb,
            ADD COLUMN IF NOT EXISTS provider_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS person_roles (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            person_id UUID NOT NULL REFERENCES persons(id) ON DELETE CASCADE,
            media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            role_type TEXT NOT NULL CHECK (role_type IN ('Actor', 'Director', 'Writer', 'Producer', 'Composer', 'Cinematographer', 'Editor', 'Other')),
            role TEXT,
            sort_order INTEGER DEFAULT 0,
            is_featured BOOLEAN DEFAULT false,
            is_leading_role BOOLEAN DEFAULT false,
            is_recurring BOOLEAN DEFAULT false,
            created_at TIMESTAMPTZ DEFAULT now(),
            updated_at TIMESTAMPTZ DEFAULT now(),
            UNIQUE(person_id, media_item_id, role_type, role),
            CONSTRAINT check_role_not_empty CHECK (role IS NULL OR trim(role) != '')
        )
        "#,
        r#"
        ALTER TABLE person_roles
            ADD COLUMN IF NOT EXISTS role_id TEXT,
            ADD COLUMN IF NOT EXISTS person_type TEXT,
            ADD COLUMN IF NOT EXISTS provider_ids JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_persons_name ON persons(name)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_persons_sort_name ON persons(sort_name)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_persons_production_year ON persons(production_year)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_person_roles_person_id ON person_roles(person_id)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_person_roles_media_item_id ON person_roles(media_item_id)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_person_roles_role_type ON person_roles(role_type)
        "#,
        r#"
        DROP TRIGGER IF EXISTS update_persons_updated_at ON persons
        "#,
        r#"
        CREATE TRIGGER update_persons_updated_at
            BEFORE UPDATE ON persons
            FOR EACH ROW
            EXECUTE FUNCTION update_updated_at_column()
        "#,
        r#"
        DROP TRIGGER IF EXISTS update_person_roles_updated_at ON person_roles
        "#,
        r#"
        CREATE TRIGGER update_person_roles_updated_at
            BEFORE UPDATE ON person_roles
            FOR EACH ROW
            EXECUTE FUNCTION update_updated_at_column()
        "#,
        r#"
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
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_id
            ON series_episode_catalog(series_id)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_date
            ON series_episode_catalog(series_id, premiere_date)
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS media_sources (
            id TEXT PRIMARY KEY,
            media_item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            protocol TEXT,
            path TEXT,
            encoder_path TEXT,
            encoder_protocol TEXT,
            media_stream_id TEXT,
            container TEXT,
            size_bytes BIGINT,
            name TEXT,
            is_remote BOOLEAN NOT NULL DEFAULT false,
            has_mixed_protocols BOOLEAN NOT NULL DEFAULT false,
            supports_transcoding BOOLEAN NOT NULL DEFAULT false,
            supports_direct_stream BOOLEAN NOT NULL DEFAULT true,
            supports_direct_play BOOLEAN NOT NULL DEFAULT true,
            is_infinite_stream BOOLEAN NOT NULL DEFAULT false,
            requires_opening BOOLEAN NOT NULL DEFAULT false,
            requires_closing BOOLEAN NOT NULL DEFAULT false,
            requires_looping BOOLEAN NOT NULL DEFAULT false,
            supports_probing BOOLEAN NOT NULL DEFAULT true,
            video_type TEXT,
            iso_type TEXT,
            bitrate INTEGER,
            default_audio_stream_index INTEGER,
            default_subtitle_stream_index INTEGER,
            transcoding_url TEXT,
            transcoding_sub_protocol TEXT,
            transcoding_container TEXT,
            timestamp TEXT,
            live_stream_id TEXT,
            buffer_ms INTEGER,
            required_http_headers JSONB NOT NULL DEFAULT '{}'::jsonb,
            formats TEXT[] NOT NULL DEFAULT ARRAY[]::text[],
            media_attachments JSONB NOT NULL DEFAULT '[]'::jsonb,
            emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_media_sources_media_item_id
            ON media_sources(media_item_id)
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS item_images (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            item_id UUID REFERENCES media_items(id) ON DELETE CASCADE,
            user_id UUID REFERENCES users(id) ON DELETE CASCADE,
            person_id UUID REFERENCES persons(id) ON DELETE CASCADE,
            image_type TEXT NOT NULL,
            image_index INTEGER NOT NULL DEFAULT 0,
            path TEXT,
            url TEXT,
            tag TEXT,
            blur_hash TEXT,
            width INTEGER,
            height INTEGER,
            size_bytes BIGINT,
            date_modified TIMESTAMPTZ NOT NULL DEFAULT now(),
            emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb,
            CHECK (item_id IS NOT NULL OR user_id IS NOT NULL OR person_id IS NOT NULL)
        )
        "#,
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_item_images_item_slot
            ON item_images(item_id, image_type, image_index)
            WHERE item_id IS NOT NULL
        "#,
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_item_images_user_slot
            ON item_images(user_id, image_type, image_index)
            WHERE user_id IS NOT NULL
        "#,
        r#"
        CREATE UNIQUE INDEX IF NOT EXISTS idx_item_images_person_slot
            ON item_images(person_id, image_type, image_index)
            WHERE person_id IS NOT NULL
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS device_registry (
            id TEXT PRIMARY KEY,
            name TEXT,
            app_name TEXT,
            app_version TEXT,
            last_user_id UUID REFERENCES users(id) ON DELETE SET NULL,
            last_user_name TEXT,
            date_last_activity TIMESTAMPTZ,
            capabilities JSONB NOT NULL DEFAULT '{}'::jsonb,
            emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS api_keys (
            access_token TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            app_name TEXT,
            device_id TEXT,
            user_id UUID REFERENCES users(id) ON DELETE CASCADE,
            date_created TIMESTAMPTZ NOT NULL DEFAULT now(),
            date_last_activity TIMESTAMPTZ,
            revoked_at TIMESTAMPTZ,
            emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS activity_log (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name TEXT NOT NULL,
            type TEXT NOT NULL,
            short_overview TEXT,
            overview TEXT,
            severity TEXT NOT NULL DEFAULT 'Info',
            item_id UUID REFERENCES media_items(id) ON DELETE SET NULL,
            user_id UUID REFERENCES users(id) ON DELETE SET NULL,
            date TIMESTAMPTZ NOT NULL DEFAULT now(),
            emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_activity_log_date
            ON activity_log(date DESC)
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS scheduled_tasks (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            category TEXT,
            key TEXT,
            description TEXT,
            state TEXT NOT NULL DEFAULT 'Idle',
            last_execution_result JSONB,
            triggers JSONB NOT NULL DEFAULT '[]'::jsonb,
            current_progress_percentage DOUBLE PRECISION,
            date_last_executed TIMESTAMPTZ,
            date_next_run TIMESTAMPTZ,
            emby_extra JSONB NOT NULL DEFAULT '{}'::jsonb,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS session_play_queue (
            session_id text NOT NULL REFERENCES sessions(access_token) ON DELETE CASCADE,
            item_id uuid NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            playlist_item_id text NOT NULL DEFAULT md5(random()::text || clock_timestamp()::text),
            sort_index integer NOT NULL DEFAULT 0,
            position_ticks bigint,
            is_paused boolean,
            play_state text NOT NULL DEFAULT 'Playing',
            updated_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY (session_id, item_id)
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_session_play_queue_session
            ON session_play_queue(session_id, sort_index)
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS session_commands (
            id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
            session_id text NOT NULL REFERENCES sessions(access_token) ON DELETE CASCADE,
            command text NOT NULL,
            payload jsonb NOT NULL DEFAULT '{}'::jsonb,
            created_at timestamptz NOT NULL DEFAULT now(),
            consumed_at timestamptz
        )
        "#,
        r#"
        ALTER TABLE session_commands
            ADD COLUMN IF NOT EXISTS consumed_at timestamptz
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_session_commands_session
            ON session_commands(session_id, created_at DESC)
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_session_commands_unconsumed
            ON session_commands(session_id, created_at)
            WHERE consumed_at IS NULL
        "#,
        r#"
        CREATE TABLE IF NOT EXISTS display_preferences (
            user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            display_preferences_id text NOT NULL,
            client text NOT NULL DEFAULT 'emby',
            preferences jsonb NOT NULL DEFAULT '{}'::jsonb,
            updated_at timestamptz NOT NULL DEFAULT now(),
            PRIMARY KEY (user_id, display_preferences_id, client)
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_display_preferences_user
            ON display_preferences(user_id, updated_at DESC)
        "#,
        r#"
        INSERT INTO system_settings (key, value)
        SELECT 'startup_wizard_completed', to_jsonb(EXISTS (SELECT 1 FROM users))
        ON CONFLICT (key) DO NOTHING
        "#,
    ];

    for statement in compatibility_sql {
        sqlx::query(statement)
            .execute(pool)
            .await
            .with_context(|| format!("执行兼容性补齐 SQL 失败: {statement}"))?;
    }

    ensure_initial_system_settings(pool, config).await?;

    Ok(())
}

async fn ensure_initial_system_settings(
    pool: &sqlx::PgPool,
    config: &config::Config,
) -> Result<()> {
    let cache_path = config.transcode_dir.to_string_lossy().to_string();
    let seeds = [
        (
            "startup_configuration",
            json!({
                "ServerName": config.server_name,
                "UICulture": config.ui_culture,
                "MetadataCountryCode": config.metadata_country_code,
                "PreferredMetadataLanguage": config.preferred_metadata_language
            }),
        ),
        (
            "startup_remote_access",
            json!({
                "EnableRemoteAccess": config.enable_remote_access,
                "EnableAutomaticPortMapping": config.enable_automatic_port_mapping
            }),
        ),
        (
            "branding_configuration",
            json!({
                "LoginDisclaimer": config.branding_login_disclaimer,
                "CustomCss": config.branding_custom_css,
                "SplashscreenEnabled": config.branding_splashscreen_enabled
            }),
        ),
        (
            "server_configuration",
            json!({
                "ServerName": config.server_name,
                "UICulture": config.ui_culture,
                "MetadataCountryCode": config.metadata_country_code,
                "PreferredMetadataLanguage": config.preferred_metadata_language,
                "EnableRemoteAccess": config.enable_remote_access,
                "EnableUPnP": config.enable_automatic_port_mapping,
                "QuickConnectAvailable": false,
                "CachePath": cache_path,
                "MetadataPath": "metadata",
                "LibraryScanFanoutConcurrency": 0,
                "ParallelImageEncodingLimit": 0
            }),
        ),
        (
            "display_preferences_defaults:vue",
            default_display_preferences_template("vue", &config.ui_culture),
        ),
        (
            "display_preferences_defaults:emby",
            default_display_preferences_template("emby", &config.ui_culture),
        ),
    ];

    for (key, value) in seeds {
        sqlx::query(
            r#"
            INSERT INTO system_settings (key, value, updated_at)
            VALUES ($1, $2, now())
            ON CONFLICT (key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(pool)
        .await
        .with_context(|| format!("初始化系统设置失败: {key}"))?;
    }

    Ok(())
}

fn default_display_preferences_template(client: &str, ui_culture: &str) -> Value {
    json!({
        "Client": client,
        "ViewType": "Poster",
        "SortBy": "SortName",
        "SortOrder": "Ascending",
        "IndexBy": "SortName",
        "RememberIndexing": false,
        "RememberSorting": false,
        "PrimaryImageHeight": 250,
        "PrimaryImageWidth": 166,
        "ScrollDirection": "Horizontal",
        "ShowBackdrop": true,
        "ShowSidebar": true,
        "ShowLocalTrailers": true,
        "ShowMissingEpisodes": false,
        "CustomPrefs": {
            "landing-libraries": "views",
            "skip-details": "false",
            "ui-culture": ui_culture
        }
    })
}
