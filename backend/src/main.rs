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
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "movie_rust_backend=debug,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env()?;
    let static_dir = config.static_dir.clone();
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

async fn run_startup_schema_tasks(pool: &sqlx::PgPool) -> Result<()> {
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

    ensure_schema_compatibility(pool).await?;
    Ok(())
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
    ];

    for statement in compatibility_sql {
        sqlx::query(statement)
            .execute(pool)
            .await
            .with_context(|| format!("执行兼容性补齐 SQL 失败: {statement}"))?;
    }

    Ok(())
}
