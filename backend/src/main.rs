mod auth;
mod config;
mod error;
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
use tower_http::{cors::CorsLayer, trace::TraceLayer};
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
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .connect(&config.database_url)
        .await
        .context("连接 PostgreSQL 失败，请检查 DATABASE_URL")?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .context("执行数据库迁移失败")?;

    repository::ensure_default_admin(&pool, &config).await?;

    let bind_addr = config.bind_addr()?;
    let state = AppState {
        pool,
        config: Arc::new(config),
    };

    let app = routes::router(state.clone())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("Movie Rust backend listening on http://{}", bind_addr);

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}
