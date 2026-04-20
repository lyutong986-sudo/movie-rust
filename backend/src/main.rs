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

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::sync::Arc;
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
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

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("执行数据库迁移失败")?;

    // 创建元数据提供者管理器
    let mut metadata_manager = metadata::provider::MetadataProviderManager::new();
    
    // 如果配置了TMDB API密钥，添加TMDB提供者
    if let Some(tmdb_api_key) = &config.tmdb_api_key {
        let tmdb_provider = metadata::tmdb::TmdbProvider::new(tmdb_api_key.clone());
        metadata_manager.register_provider(Box::new(tmdb_provider));
        tracing::info!("TMDB元数据提供者已注册");
    }

    let bind_addr = config.bind_addr()?;
    let state = AppState {
        pool,
        config: Arc::new(config),
        metadata_manager: Some(Arc::new(metadata_manager)),
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
