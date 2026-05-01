mod auth;
mod config;
mod error;
mod file_watcher;
pub mod http_client;
mod media_analyzer;
pub mod repo_cache;
mod metadata;
mod models;
mod naming;
pub mod refresh_queue;
mod remote_emby;
mod repository;
mod routes;
mod scanner;
mod security;
mod state;
mod transcoder;
mod webhooks;
mod work_limiter;

use anyhow::{Context, Result};
use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use sqlx::postgres::PgPoolOptions;
use state::AppState;
use std::path::PathBuf;
use std::sync::Arc;

use crate::transcoder::Transcoder;
use crate::work_limiter::{WorkLimiterConfig, WorkLimiters};
use tower_http::{
    cors::{AllowOrigin, Any, CorsLayer},
    services::ServeDir,
    trace::{DefaultMakeSpan, DefaultOnFailure, DefaultOnResponse, TraceLayer},
};
use tracing::Level;
use tracing_appender::rolling;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main(flavor = "multi_thread")]
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
    let state_config_max_conns = config.database_max_connections;
    let pool = PgPoolOptions::new()
        .max_connections(config.database_max_connections)
        .min_connections(config.database_max_connections.min(5))
        .acquire_timeout(std::time::Duration::from_secs(15))
        .idle_timeout(std::time::Duration::from_secs(600))
        .connect(&config.database_url)
        .await
        .context("连接 PostgreSQL 失败，请检查 DATABASE_URL")?;

    run_startup_schema_tasks(&pool).await?;

    let mut metadata_manager = metadata::provider::MetadataProviderManager::new();

    let startup_cfg = repository::startup_configuration(&pool, &config).await.ok();
    let tmdb_api_key = match &config.tmdb_api_key {
        Some(key) => Some(key.clone()),
        None => startup_cfg
            .as_ref()
            .map(|s| s.tmdb_api_key.clone())
            .filter(|k| !k.trim().is_empty()),
    };
    let tmdb_extra_keys = startup_cfg
        .as_ref()
        .map(|s| s.tmdb_api_keys.clone())
        .unwrap_or_default();
    if let Some(tmdb_api_key) = tmdb_api_key {
        let key_count = 1 + tmdb_extra_keys.len();
        let tmdb_provider = metadata::tmdb::TmdbProvider::new_with_multi_keys(
            tmdb_api_key.clone(),
            tmdb_extra_keys,
            &config.preferred_metadata_language,
            &config.metadata_country_code,
        );
        metadata_manager.register_provider(Box::new(tmdb_provider));
        tracing::info!("TMDB 元数据提供者已注册（{key_count} 个 API Key 轮询）");
    } else {
        tracing::warn!("未配置 TMDB API Key（设置 TMDB_API_KEY 环境变量或在系统设置中配置）");
    }

    let bind_addr = config.bind_addr()?;
    let config = Arc::new(config);
    let transcoder = Transcoder::new(config.clone());
    let work_limiters = WorkLimiters::new(WorkLimiterConfig {
        library_scan_limit: 2,
        media_analysis_limit: 8,
        tmdb_metadata_limit: 4,
    });
    let http_client = reqwest::Client::builder()
        .pool_max_idle_per_host(32)
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("failed to build HTTP client");

    let max_conns = state_config_max_conns;
    let scan_db_permits = (max_conns as usize).saturating_sub(8).max(2);
    let (event_tx, _) = tokio::sync::broadcast::channel::<crate::state::ServerEvent>(256);
    let state = AppState {
        pool,
        config,
        metadata_manager: Some(Arc::new(metadata_manager)),
        websocket_sessions: Arc::new(dashmap::DashMap::new()),
        transcoder,
        work_limiters,
        task_tokens: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        http_client,
        scan_db_semaphore: Arc::new(tokio::sync::Semaphore::new(scan_db_permits)),
        event_tx,
    };

    // SPA 入口：
    // - `ServeDir` 负责实际存在于 `frontend/dist` 下的静态资源（`/index.html`、`/assets/*`、
    //   `/favicon.svg`、`/manifest.webmanifest` 等）。
    // - 对于 **Vue Router 客户端路由**（例如 `/settings`、`/library/<id>`、`/queue`、`/wizard`）
    //   等无对应静态文件的路径，使用 `ServeDir::fallback` 返回 `index.html`。
    //   注意：必须用 `fallback`，不能用 `not_found_service`——后者会把回退响应**强制改成 404**，
    //   导致浏览器地址栏、SEO、PWA 安装等仍显示 Not Found，尽管 body 已是 SPA HTML。
    let index_path: Arc<PathBuf> = Arc::new(static_dir.join("index.html"));
    let spa_index_service = {
        let index_path = index_path.clone();
        tower::service_fn(move |_req: axum::http::Request<Body>| {
            let index_path = index_path.clone();
            async move { Ok::<_, std::convert::Infallible>(serve_spa_index(&index_path).await) }
        })
    };
    let spa = ServeDir::new(&static_dir).fallback(spa_index_service);

    let http_trace = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO))
        .on_failure(DefaultOnFailure::new().level(Level::ERROR));

    let api_router = routes::router(state.clone())
        .layer(tower_http::compression::CompressionLayer::new()
            .gzip(true)
            .br(true))
        .layer(http_trace)
        .layer(axum::middleware::from_fn(request_timeout_middleware));

    let app = api_router
        .fallback_service(spa)
        .layer(build_cors_layer(&state.config));

    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("Movie Rust backend listening on http://{}", bind_addr);

    tokio::spawn(routes::scheduled_tasks::run_scheduler(state.clone()));

    let remote_emby_refresh_pool = state.pool.clone();
    tokio::spawn(async move {
        crate::remote_emby::remote_emby_token_refresh_loop(remote_emby_refresh_pool).await;
    });

    if state.config.enable_remote_library_monitor {
        let monitor_state = state.clone();
        tokio::spawn(async move {
            crate::remote_emby::remote_library_monitor_loop(monitor_state).await;
        });
    } else {
        tracing::info!("远端 Emby 库高频轮询已禁用（APP_ENABLE_REMOTE_LIBRARY_MONITOR=false），由计划任务负责增量更新");
    }

    // 远端 Emby 源「按源粒度」的自动增量同步循环：每分钟根据每个源的
    // auto_sync_interval_minutes 配置触发增量同步（与 EnableRealtimeMonitor 无关）。
    let auto_sync_state = state.clone();
    tokio::spawn(async move {
        crate::remote_emby::remote_emby_auto_sync_loop(auto_sync_state).await;
    });

    let watcher_state = state.clone();
    tokio::spawn(async move {
        crate::file_watcher::file_watcher_loop(watcher_state).await;
    });

    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

async fn request_timeout_middleware(
    req: axum::http::Request<Body>,
    next: axum::middleware::Next,
) -> Response {
    const TIMEOUT_SECS: u64 = 300;
    match tokio::time::timeout(
        std::time::Duration::from_secs(TIMEOUT_SECS),
        next.run(req),
    )
    .await
    {
        Ok(response) => response,
        Err(_elapsed) => {
            tracing::warn!("请求处理超时（{TIMEOUT_SECS}s），返回 503");
            (
                StatusCode::SERVICE_UNAVAILABLE,
                [(header::CONTENT_TYPE, "application/json")],
                r#"{"error":"Service Unavailable","message":"请求超时，服务器负载过高，请稍后重试"}"#,
            )
                .into_response()
        }
    }
}

/// 读取 `index.html` 并作为 `200 OK text/html; charset=utf-8` 返回。
/// 使用进程内缓存：首次读入后缓存到内存，后续请求零磁盘 IO。
/// 若需热更新前端产物，重启后端即可刷新缓存。
async fn serve_spa_index(index_path: &std::path::Path) -> Response {
    use std::sync::OnceLock;
    static CACHED_INDEX: OnceLock<bytes::Bytes> = OnceLock::new();

    if let Some(cached) = CACHED_INDEX.get() {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(header::CACHE_CONTROL, "no-cache")
            .body(Body::from(cached.clone()))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    }

    match tokio::fs::read(index_path).await {
        Ok(bytes) => {
            let b = bytes::Bytes::from(bytes);
            let _ = CACHED_INDEX.set(b.clone());
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header(header::CACHE_CONTROL, "no-cache")
                .body(Body::from(b))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        Err(err) => {
            tracing::error!(
                path = %index_path.display(),
                error = %err,
                "读取 SPA 入口文件失败（请确认 frontend/dist/index.html 已构建并放置正确位置）"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                format!(
                    "SPA 入口 {} 未找到：请先在项目根目录执行 `cd frontend && npm run build`，\
                     或将构建产物拷贝到 APP_STATIC_DIR 指定的目录。",
                    index_path.display()
                ),
            )
                .into_response()
        }
    }
}

async fn run_startup_schema_tasks(pool: &sqlx::PgPool) -> Result<()> {
    // 项目只保留一个 migration：`0001_schema.sql`，它描述完整 schema 并对所有
    // 对象使用 `IF NOT EXISTS`。之后任何新字段都通过
    // `0001_schema.sql` + `ensure_schema_compatibility` 原地补齐，**不再新增**
    // migration 文件。
    match sqlx::migrate!("./migrations").run(pool).await {
        Ok(_) => {}
        Err(error) => {
            let error_text = error.to_string();
            if error_text.contains("previously applied but has been modified") {
                tracing::warn!(
                    "检测到 sqlx 迁移校验失败（0001_schema.sql 被修改），继续执行运行时 schema 守护：{}",
                    error_text
                );
            } else {
                return Err(error).context("执行数据库迁移失败");
            }
        }
    }

    ensure_schema_compatibility(pool).await?;

    // PB24：启动时一次性清理孤儿远端虚拟路径——历史上 PB23 修复之前删除过的远端
    // 源在 libraries 表里残留的 `__remote_view_<source_id>_*` 独立库 / merge 库
    // PathInfos entry。仅删那些 source_id 已不存在的孤儿；现存远端源的虚拟路径不动。
    // 幂等、纯 SQL，几个 ms 就能跑完，对启动时间几乎无影响；失败仅 warn 不阻塞启动。
    match repository::cleanup_orphan_remote_view_paths(pool).await {
        Ok((deleted, updated, orphan_ids)) if deleted > 0 || updated > 0 => {
            tracing::info!(
                deleted_libraries = deleted,
                updated_libraries = updated,
                orphan_source_ids = orphan_ids,
                "启动清理：发现并清掉历史孤儿远端虚拟路径"
            );
        }
        Ok(_) => {}
        Err(error) => {
            tracing::warn!(error = %error, "启动清理孤儿远端虚拟路径失败（不阻塞启动）");
        }
    }

    Ok(())
}

/// 运行时 schema 守护者。
///
/// 作用：
/// * 老库（还没跑最新 `0001_schema.sql`）在这里被自动补上缺列、缺索引；
/// * 新库跑完 `0001_schema.sql` 后这里的语句全部是 no-op。
///
/// 使用约定：
/// **不要在这里做业务逻辑**，只做 `ADD COLUMN IF NOT EXISTS` / `CREATE INDEX
/// IF NOT EXISTS` / `CREATE TABLE IF NOT EXISTS` 这类幂等 DDL，和
/// `0001_schema.sql` 一一对齐。加新字段时：
///   1. 先在 `0001_schema.sql` 里加一行；
///   2. 再在这里加同样的 `ADD COLUMN IF NOT EXISTS`。
async fn ensure_schema_compatibility(pool: &sqlx::PgPool) -> Result<()> {
    let compatibility_sql = [
        // -------------------------------------------------------------------
        // users：核心账号 + Emby 用户策略 + EasyPassword。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE users
            ADD COLUMN IF NOT EXISTS easy_password_hash      TEXT,
            ADD COLUMN IF NOT EXISTS is_hidden               BOOLEAN NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS is_disabled             BOOLEAN NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS policy                  JSONB   NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS configuration           JSONB   NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS primary_image_path      TEXT,
            ADD COLUMN IF NOT EXISTS backdrop_image_path     TEXT,
            ADD COLUMN IF NOT EXISTS logo_image_path         TEXT,
            ADD COLUMN IF NOT EXISTS date_modified           TIMESTAMPTZ NOT NULL DEFAULT now(),
            ADD COLUMN IF NOT EXISTS legacy_password_format  TEXT,
            ADD COLUMN IF NOT EXISTS legacy_password_hash    TEXT
        "#,
        // -------------------------------------------------------------------
        // sessions：会话令牌 + session_type + expires_at。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE sessions
            ADD COLUMN IF NOT EXISTS session_type   TEXT NOT NULL DEFAULT 'Interactive',
            ADD COLUMN IF NOT EXISTS expires_at     TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS remote_address TEXT
        "#,
        r#"CREATE INDEX IF NOT EXISTS idx_sessions_user         ON sessions(user_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_sessions_expires_at   ON sessions(expires_at)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_sessions_session_type ON sessions(session_type)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_sessions_last_activity ON sessions(last_activity_at DESC)"#,
        // -------------------------------------------------------------------
        // libraries：库选项 JSON + 修改时间。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE libraries
            ADD COLUMN IF NOT EXISTS library_options    JSONB       NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS date_modified      TIMESTAMPTZ NOT NULL DEFAULT now(),
            ADD COLUMN IF NOT EXISTS primary_image_path TEXT,
            ADD COLUMN IF NOT EXISTS primary_image_tag  TEXT
        "#,
        r#"
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
        $$
        "#,
        // -------------------------------------------------------------------
        // remote_emby_sources：外部 Emby 中转源（账号密码登录 + 伪装 UA）。
        // -------------------------------------------------------------------
        r#"
        CREATE TABLE IF NOT EXISTS remote_emby_sources (
            id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name               TEXT NOT NULL,
            server_url         TEXT NOT NULL,
            username           TEXT NOT NULL,
            password           TEXT NOT NULL,
            spoofed_user_agent TEXT NOT NULL,
            target_library_id  UUID NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
            display_mode       TEXT NOT NULL DEFAULT 'separate',
            remote_view_ids    TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            remote_views       JSONB NOT NULL DEFAULT '[]'::jsonb,
            enabled            BOOLEAN NOT NULL DEFAULT true,
            remote_user_id     TEXT,
            access_token       TEXT,
            source_secret      UUID NOT NULL DEFAULT gen_random_uuid(),
            last_sync_at       TIMESTAMPTZ,
            last_sync_error    TEXT,
            created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS display_mode TEXT NOT NULL DEFAULT 'separate'
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS remote_view_ids TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[]
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS remote_views JSONB NOT NULL DEFAULT '[]'::jsonb
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS strm_output_path TEXT
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS sync_metadata BOOLEAN NOT NULL DEFAULT true
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS sync_subtitles BOOLEAN NOT NULL DEFAULT true
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS token_refresh_interval_secs INTEGER NOT NULL DEFAULT 3600
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS last_token_refresh_at TIMESTAMPTZ
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS view_library_map JSONB NOT NULL DEFAULT '{}'::jsonb
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS proxy_mode TEXT NOT NULL DEFAULT 'proxy'
        "#,
        r#"
        ALTER TABLE remote_emby_sources
            ADD COLUMN IF NOT EXISTS auto_sync_interval_minutes INTEGER NOT NULL DEFAULT 0
        "#,
        r#"CREATE UNIQUE INDEX IF NOT EXISTS idx_remote_emby_sources_name_unique ON remote_emby_sources (lower(name))"#,
        r#"CREATE INDEX IF NOT EXISTS idx_remote_emby_sources_library ON remote_emby_sources(target_library_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_remote_emby_sources_enabled ON remote_emby_sources(enabled)"#,
        // -------------------------------------------------------------------
        // media_items：核心媒体表（对齐 BaseItemDto 全量预留列）。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE media_items
            ADD COLUMN IF NOT EXISTS original_title             TEXT,
            ADD COLUMN IF NOT EXISTS forced_sort_name           TEXT,
            ADD COLUMN IF NOT EXISTS taglines                   TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS locked_fields              TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS lock_data                  BOOLEAN NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS official_rating            TEXT,
            ADD COLUMN IF NOT EXISTS parental_rating_value      INTEGER,
            ADD COLUMN IF NOT EXISTS custom_rating              TEXT,
            ADD COLUMN IF NOT EXISTS community_rating           DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS critic_rating              DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS start_date                 DATE,
            ADD COLUMN IF NOT EXISTS end_date                   DATE,
            ADD COLUMN IF NOT EXISTS date_last_saved            TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS date_last_media_added      TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS status                     TEXT,
            ADD COLUMN IF NOT EXISTS air_days                   TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS air_time                   TEXT,
            ADD COLUMN IF NOT EXISTS series_name                TEXT,
            ADD COLUMN IF NOT EXISTS series_id                  UUID,
            ADD COLUMN IF NOT EXISTS season_name                TEXT,
            ADD COLUMN IF NOT EXISTS season_id                  UUID,
            ADD COLUMN IF NOT EXISTS index_number               INTEGER,
            ADD COLUMN IF NOT EXISTS index_number_end           INTEGER,
            ADD COLUMN IF NOT EXISTS parent_index_number        INTEGER,
            ADD COLUMN IF NOT EXISTS sort_index_number          INTEGER,
            ADD COLUMN IF NOT EXISTS sort_parent_index_number   INTEGER,
            ADD COLUMN IF NOT EXISTS display_order              TEXT,
            ADD COLUMN IF NOT EXISTS provider_ids               JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS external_urls              JSONB NOT NULL DEFAULT '[]'::jsonb,
            ADD COLUMN IF NOT EXISTS genres                     TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS studios                    TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS tags                       TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS production_locations       TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS remote_trailers            TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS width                      INTEGER,
            ADD COLUMN IF NOT EXISTS height                     INTEGER,
            ADD COLUMN IF NOT EXISTS bit_rate                   BIGINT,
            ADD COLUMN IF NOT EXISTS size                       BIGINT,
            ADD COLUMN IF NOT EXISTS file_name                  TEXT,
            ADD COLUMN IF NOT EXISTS video_codec                TEXT,
            ADD COLUMN IF NOT EXISTS audio_codec                TEXT,
            ADD COLUMN IF NOT EXISTS logo_path                  TEXT,
            ADD COLUMN IF NOT EXISTS thumb_path                 TEXT,
            ADD COLUMN IF NOT EXISTS art_path                   TEXT,
            ADD COLUMN IF NOT EXISTS banner_path                TEXT,
            ADD COLUMN IF NOT EXISTS disc_path                  TEXT,
            ADD COLUMN IF NOT EXISTS backdrop_paths           TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS box_path                   TEXT,
            ADD COLUMN IF NOT EXISTS menu_path                  TEXT,
            ADD COLUMN IF NOT EXISTS image_tags                 JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS backdrop_image_tags        TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS primary_image_tag          TEXT,
            ADD COLUMN IF NOT EXISTS primary_image_item_id      UUID,
            ADD COLUMN IF NOT EXISTS primary_image_aspect_ratio DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS parent_logo_item_id        UUID,
            ADD COLUMN IF NOT EXISTS parent_logo_image_tag      TEXT,
            ADD COLUMN IF NOT EXISTS parent_backdrop_item_id    UUID,
            ADD COLUMN IF NOT EXISTS parent_backdrop_image_tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
            ADD COLUMN IF NOT EXISTS parent_thumb_item_id       UUID,
            ADD COLUMN IF NOT EXISTS parent_thumb_image_tag     TEXT,
            ADD COLUMN IF NOT EXISTS series_primary_image_tag   TEXT,
            ADD COLUMN IF NOT EXISTS series_studio              TEXT,
            ADD COLUMN IF NOT EXISTS image_blur_hashes          JSONB NOT NULL DEFAULT '{}'::jsonb,
            ADD COLUMN IF NOT EXISTS child_count                INTEGER,
            ADD COLUMN IF NOT EXISTS recursive_item_count       BIGINT,
            ADD COLUMN IF NOT EXISTS season_count               INTEGER,
            ADD COLUMN IF NOT EXISTS series_count               INTEGER,
            ADD COLUMN IF NOT EXISTS movie_count                INTEGER,
            ADD COLUMN IF NOT EXISTS special_feature_count      INTEGER,
            ADD COLUMN IF NOT EXISTS local_trailer_count        INTEGER NOT NULL DEFAULT 0,
            ADD COLUMN IF NOT EXISTS part_count                 INTEGER NOT NULL DEFAULT 0,
            ADD COLUMN IF NOT EXISTS is_movie                   BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_series                  BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_folder                  BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_hd                      BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_3d                      BOOLEAN,
            ADD COLUMN IF NOT EXISTS disabled                   BOOLEAN NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS can_delete                 BOOLEAN NOT NULL DEFAULT true,
            ADD COLUMN IF NOT EXISTS can_download               BOOLEAN NOT NULL DEFAULT true,
            ADD COLUMN IF NOT EXISTS supports_sync              BOOLEAN NOT NULL DEFAULT false,
            ADD COLUMN IF NOT EXISTS supports_resume            BOOLEAN NOT NULL DEFAULT true,
            ADD COLUMN IF NOT EXISTS etag                       TEXT,
            ADD COLUMN IF NOT EXISTS presentation_unique_key    TEXT,
            ADD COLUMN IF NOT EXISTS collection_type            TEXT,
            ADD COLUMN IF NOT EXISTS location_type              TEXT,
            ADD COLUMN IF NOT EXISTS extra_type                 TEXT
        "#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_library  ON media_items(library_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_parent   ON media_items(parent_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_type     ON media_items(item_type)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_sort     ON media_items(sort_name)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_series   ON media_items(series_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_premiere ON media_items(premiere_date)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_date_created ON media_items(date_created DESC)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_community_rating ON media_items(community_rating DESC NULLS LAST)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_type_sort ON media_items(item_type, sort_name)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_type_date ON media_items(item_type, date_created DESC)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_type_series ON media_items(item_type, series_id) WHERE series_id IS NOT NULL"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_genres_gin ON media_items USING gin (genres)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_episode_nextup ON media_items(series_id, parent_index_number, index_number) WHERE item_type = 'Episode'"#,
        // Backfill series_id for Seasons (parent is Series)
        r#"
        UPDATE media_items season
        SET series_id = season.parent_id
        WHERE season.item_type = 'Season'
          AND season.series_id IS NULL
          AND season.parent_id IS NOT NULL
          AND EXISTS (
              SELECT 1 FROM media_items series
              WHERE series.id = season.parent_id AND series.item_type = 'Series'
          )
        "#,
        // Backfill series_id for Episodes (parent is Season → grandparent is Series)
        r#"
        UPDATE media_items ep
        SET series_id = parent_season.parent_id
        FROM media_items parent_season
        WHERE ep.item_type = 'Episode'
          AND ep.series_id IS NULL
          AND ep.parent_id IS NOT NULL
          AND parent_season.id = ep.parent_id
          AND parent_season.item_type = 'Season'
          AND parent_season.parent_id IS NOT NULL
        "#,
        // pg_trgm 全文搜索加速
        r#"DO $$ BEGIN CREATE EXTENSION IF NOT EXISTS pg_trgm; EXCEPTION WHEN OTHERS THEN NULL; END $$"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_name_trgm ON media_items USING gin (name gin_trgm_ops)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_sort_trgm ON media_items USING gin (sort_name gin_trgm_ops)"#,
        // -------------------------------------------------------------------
        // user_item_data：Emby UserItemDataDto 预留列。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE user_item_data
            ADD COLUMN IF NOT EXISTS rating              DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS played_percentage   DOUBLE PRECISION,
            ADD COLUMN IF NOT EXISTS unplayed_item_count INTEGER,
            ADD COLUMN IF NOT EXISTS likes               BOOLEAN
        "#,
        // -------------------------------------------------------------------
        // media_streams：Emby MediaStream 扩展字段 + UNIQUE 兜底。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE media_streams
            ADD COLUMN IF NOT EXISTS attachment_size                     INTEGER,
            ADD COLUMN IF NOT EXISTS extended_video_sub_type             TEXT,
            ADD COLUMN IF NOT EXISTS extended_video_sub_type_description TEXT,
            ADD COLUMN IF NOT EXISTS extended_video_type                 TEXT,
            ADD COLUMN IF NOT EXISTS is_anamorphic                       BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_avc                              BOOLEAN,
            ADD COLUMN IF NOT EXISTS is_external_url                     TEXT,
            ADD COLUMN IF NOT EXISTS is_text_subtitle_stream             BOOLEAN,
            ADD COLUMN IF NOT EXISTS level                               INTEGER,
            ADD COLUMN IF NOT EXISTS pixel_format                        TEXT,
            ADD COLUMN IF NOT EXISTS ref_frames                          INTEGER,
            ADD COLUMN IF NOT EXISTS stream_start_time_ticks             BIGINT,
            ADD COLUMN IF NOT EXISTS mime_type                           TEXT,
            ADD COLUMN IF NOT EXISTS subtitle_location_type              TEXT,
            ADD COLUMN IF NOT EXISTS is_closed_captions                  BOOLEAN,
            ADD COLUMN IF NOT EXISTS nal_length_size                     TEXT,
            ADD COLUMN IF NOT EXISTS video_range                         TEXT,
            ADD COLUMN IF NOT EXISTS delivery_method                     TEXT,
            ADD COLUMN IF NOT EXISTS delivery_url                        TEXT,
            ADD COLUMN IF NOT EXISTS extradata                           TEXT
        "#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_streams_media_item_id ON media_streams(media_item_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_streams_stream_type   ON media_streams(stream_type)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_streams_language      ON media_streams(language)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_streams_codec         ON media_streams(codec)"#,
        r#"
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
        $$
        "#,
        // -------------------------------------------------------------------
        // media_chapters：表本身 + 章节图片字段 + UNIQUE 兜底。
        // -------------------------------------------------------------------
        r#"
        CREATE TABLE IF NOT EXISTS media_chapters (
            id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            media_item_id        UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            chapter_index        INTEGER NOT NULL,
            start_position_ticks BIGINT NOT NULL,
            name                 TEXT,
            marker_type          TEXT,
            image_path           TEXT,
            created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (media_item_id, chapter_index)
        )
        "#,
        r#"
        ALTER TABLE media_chapters
            ADD COLUMN IF NOT EXISTS image_tag           TEXT,
            ADD COLUMN IF NOT EXISTS image_date_modified TIMESTAMPTZ
        "#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_chapters_media_item_id ON media_chapters(media_item_id)"#,
        r#"
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
        $$
        "#,
        // -------------------------------------------------------------------
        // series_episode_catalog：TMDB / TVDB 补齐的分集"应当存在"目录。
        // -------------------------------------------------------------------
        r#"
        CREATE TABLE IF NOT EXISTS series_episode_catalog (
            id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            series_id           UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            provider            TEXT NOT NULL,
            external_series_id  TEXT NOT NULL,
            external_season_id  TEXT,
            external_episode_id TEXT,
            season_number       INTEGER NOT NULL,
            episode_number      INTEGER NOT NULL,
            episode_number_end  INTEGER,
            name                TEXT NOT NULL,
            overview            TEXT,
            premiere_date       DATE,
            image_path          TEXT,
            created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (series_id, provider, season_number, episode_number)
        )
        "#,
        r#"CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_id   ON series_episode_catalog(series_id)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_series_episode_catalog_series_date ON series_episode_catalog(series_id, premiere_date)"#,
        // -------------------------------------------------------------------
        // persons：人物简介/出生地/外链 — 与 0001_schema.sql 保持同源。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE persons
            ADD COLUMN IF NOT EXISTS death_date         TIMESTAMPTZ,
            ADD COLUMN IF NOT EXISTS place_of_birth     TEXT,
            ADD COLUMN IF NOT EXISTS homepage_url       TEXT,
            ADD COLUMN IF NOT EXISTS metadata_synced_at TIMESTAMPTZ
        "#,
        // -------------------------------------------------------------------
        // session_play_queue：播放状态扩展列。
        // -------------------------------------------------------------------
        r#"
        ALTER TABLE session_play_queue
            ADD COLUMN IF NOT EXISTS audio_stream_index    integer,
            ADD COLUMN IF NOT EXISTS subtitle_stream_index integer,
            ADD COLUMN IF NOT EXISTS play_method           text,
            ADD COLUMN IF NOT EXISTS media_source_id       text,
            ADD COLUMN IF NOT EXISTS volume_level          integer,
            ADD COLUMN IF NOT EXISTS repeat_mode           text,
            ADD COLUMN IF NOT EXISTS playback_rate         double precision
        "#,
        // -------------------------------------------------------------------
        // session_commands：老库只有 0018 建表但没 consumed_at。
        // -------------------------------------------------------------------
        r#"ALTER TABLE session_commands ADD COLUMN IF NOT EXISTS consumed_at TIMESTAMPTZ"#,
        r#"
        CREATE INDEX IF NOT EXISTS idx_session_commands_unconsumed
            ON session_commands(session_id, created_at)
            WHERE consumed_at IS NULL
        "#,
        // -------------------------------------------------------------------
        // playlists / playlist_items：用户自定义播放列表。
        // -------------------------------------------------------------------
        r#"
        CREATE TABLE IF NOT EXISTS playlists (
            id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            name       TEXT NOT NULL,
            media_type TEXT NOT NULL DEFAULT 'Video',
            overview   TEXT,
            image_primary_path TEXT,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )
        "#,
        r#"CREATE INDEX IF NOT EXISTS idx_playlists_user_id ON playlists(user_id, updated_at DESC)"#,
        r#"
        CREATE TABLE IF NOT EXISTS playlist_items (
            id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            playlist_id      UUID NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
            media_item_id    UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            playlist_item_id TEXT NOT NULL DEFAULT md5(random()::text || clock_timestamp()::text),
            sort_index       INTEGER NOT NULL DEFAULT 0,
            created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
            UNIQUE (playlist_id, playlist_item_id)
        )
        "#,
        r#"CREATE INDEX IF NOT EXISTS idx_playlist_items_playlist    ON playlist_items(playlist_id, sort_index)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_playlist_items_media_item  ON playlist_items(media_item_id)"#,
        // trickplay
        r#"CREATE TABLE IF NOT EXISTS trickplay_info (
            item_id        UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            width          INTEGER NOT NULL,
            height         INTEGER NOT NULL,
            tile_width     INTEGER NOT NULL DEFAULT 10,
            tile_height    INTEGER NOT NULL DEFAULT 10,
            thumb_count    INTEGER NOT NULL DEFAULT 0,
            interval_ms    INTEGER NOT NULL DEFAULT 10000,
            bandwidth      INTEGER NOT NULL DEFAULT 0,
            created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
            PRIMARY KEY (item_id, width)
        )"#,
        r#"CREATE TABLE IF NOT EXISTS trickplay_tiles (
            item_id    UUID NOT NULL,
            width      INTEGER NOT NULL,
            tile_index INTEGER NOT NULL,
            data       BYTEA NOT NULL,
            PRIMARY KEY (item_id, width, tile_index),
            FOREIGN KEY (item_id, width) REFERENCES trickplay_info(item_id, width) ON DELETE CASCADE
        )"#,
        // media_segments
        r#"CREATE TABLE IF NOT EXISTS media_segments (
            id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            item_id         UUID NOT NULL REFERENCES media_items(id) ON DELETE CASCADE,
            segment_type    TEXT NOT NULL,
            start_ticks     BIGINT NOT NULL,
            end_ticks       BIGINT NOT NULL,
            created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
        )"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_segments_item ON media_segments(item_id, segment_type)"#,
        // webhooks: 出向 webhook 配置（emby Webhooks 插件协议）
        r#"CREATE TABLE IF NOT EXISTS webhooks (
            id                UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name              TEXT NOT NULL,
            url               TEXT NOT NULL,
            enabled           BOOLEAN NOT NULL DEFAULT true,
            events            TEXT[] NOT NULL DEFAULT '{}',
            content_type      TEXT NOT NULL DEFAULT 'application/json',
            secret            TEXT,
            headers_json      JSONB NOT NULL DEFAULT '{}'::jsonb,
            created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
            last_status       INTEGER,
            last_error        TEXT,
            last_triggered_at TIMESTAMPTZ
        )"#,
        r#"CREATE INDEX IF NOT EXISTS idx_webhooks_enabled ON webhooks(enabled) WHERE enabled"#,
        // playback_events 性能索引
        r#"CREATE INDEX IF NOT EXISTS idx_playback_events_user_created ON playback_events(user_id, created_at DESC)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_playback_events_item ON playback_events(item_id) WHERE item_id IS NOT NULL"#,
        // studios/tags GIN 索引（对 aggregate_array_values 全表扫描优化）
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_studios_gin ON media_items USING gin (studios)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_media_items_tags_gin ON media_items USING gin (tags)"#,
    ];

    for statement in compatibility_sql {
        if let Err(error) = sqlx::query(statement).execute(pool).await {
            tracing::error!("Schema 兼容性补齐失败: {error}");
        }
    }

    Ok(())
}

fn build_cors_layer(config: &config::Config) -> CorsLayer {
    use axum::http::{header, Method};
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
        Method::HEAD,
    ];
    let headers = [
        header::AUTHORIZATION,
        header::CONTENT_TYPE,
        header::ACCEPT,
        header::ORIGIN,
        header::HeaderName::from_static("x-emby-token"),
        header::HeaderName::from_static("x-mediabrowser-token"),
        header::HeaderName::from_static("x-emby-authorization"),
    ];

    if config.allowed_origins.is_empty() {
        tracing::info!("CORS: APP_ALLOWED_ORIGINS 未设置，允许所有来源（开发模式）");
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(methods)
            .allow_headers(Any)
    } else {
        let origins: Vec<_> = config
            .allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        tracing::info!("CORS: 已配置 {} 个允许来源", origins.len());
        CorsLayer::new()
            .allow_origin(AllowOrigin::list(origins))
            .allow_methods(methods)
            .allow_headers(headers)
            .allow_credentials(true)
    }
}
