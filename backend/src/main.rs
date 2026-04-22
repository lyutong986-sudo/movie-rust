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
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("执行数据库迁移失败")?;
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
