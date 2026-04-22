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
use axum::routing::get;
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::sync::Arc;

use crate::transcoder::Transcoder;
use tower_http::{
    cors::CorsLayer,
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
        .with(log_filter())
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(file_writer),
        )
        .init();

    let config = bootstrap_config;
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url)
        .await
        .context("连接 PostgreSQL 失败，请检查 DATABASE_URL")?;

    run_startup_schema_tasks(&pool).await?;

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

    let http_trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
        .on_failure(DefaultOnFailure::new().level(Level::ERROR));

    let app = routes::router(state.clone())
        .route("/health", get(|| async { "ok" }))
        .layer(http_trace)
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("Movie Rust backend listening on http://{}", bind_addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn run_startup_schema_tasks(pool: &sqlx::PgPool) -> Result<()> {
    match sqlx::migrate!("./migrations").run(pool).await {
        Ok(_) => {}
        Err(error) => {
            let error_text = error.to_string();
            if error_text.contains("previously applied but has been modified") {
                tracing::debug!(
                    "检测到 sqlx 迁移校验失败（已应用迁移文件被修改），继续执行兼容性补齐 SQL：{}",
                    error_text
                );
            } else {
                return Err(error).context("执行数据库迁移失败");
            }
        }
    }

    ensure_schema_compatibility(pool).await?;
    Ok(())
}

fn log_filter() -> EnvFilter {
    let base = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "movie_rust_backend=debug,tower_http=info,sqlx=warn".into());

    [
        "sqlx=warn",
        "sqlx::query=warn",
        "sqlx::postgres::notice=warn",
        "sqlx::migrate=warn",
    ]
    .into_iter()
    .fold(base, |filter, directive| {
        filter.add_directive(
            directive
                .parse()
                .expect("内置日志过滤规则必须有效"),
        )
    })
}

async fn ensure_schema_compatibility(pool: &sqlx::PgPool) -> Result<()> {
    let compatibility_sql = [
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
            ADD COLUMN IF NOT EXISTS stream_start_time_ticks BIGINT
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
        CREATE TABLE IF NOT EXISTS collection_items (
            collection_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            item_id UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            display_order INTEGER NOT NULL DEFAULT 0,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (collection_id, item_id)
        )
        "#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_collection_items_item
            ON collection_items(item_id)
        "#,
    ];

    for statement in compatibility_sql {
        sqlx::query(statement)
            .execute(pool)
            .await
            .with_context(|| format!("执行兼容性补齐 SQL 失败: {statement}"))?;
    }

    Ok(())
}
