use std::{env, net::SocketAddr, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: u32,
    pub host: String,
    pub port: u16,
    pub server_name: String,
    pub server_id: Uuid,
    pub ui_culture: String,
    pub metadata_country_code: String,
    pub preferred_metadata_language: String,
    pub enable_remote_access: bool,
    pub enable_automatic_port_mapping: bool,
    pub public_url: Option<String>,
    pub branding_login_disclaimer: String,
    pub branding_custom_css: String,
    pub branding_splashscreen_enabled: bool,
    pub static_dir: PathBuf,
    pub log_dir: PathBuf,
    /// PB42-IC：远端 / 上游图片代理的本地磁盘缓存目录。
    ///
    /// 命中后续 `/Items/{id}/Images/...` 请求时不再回源到 upstream Emby，可大幅
    /// 削减上游 QPS（Hills 客户端打开主页瞬间会发 100+ 张图请求），同时给
    /// 304 / If-None-Match 留个稳定 ETag 来源。
    /// 文件按 cache_key（URL+变换参数 SHA256）落盘，旁边一份 `.ct` 文本记录
    /// content-type；超过 `image_cache_ttl_secs` 的旧文件由后台 worker 周期清理。
    pub image_cache_dir: PathBuf,
    pub image_cache_ttl_secs: u64,
    pub tmdb_api_key: Option<String>,
    pub api_key: Option<String>,
    pub ffmpeg_path: String,
    pub transcode_dir: PathBuf,
    pub transcode_threads: u32,
    pub enable_transcoding: bool,
    pub max_transcode_sessions: u32,
    pub allowed_origins: Vec<String>,
    /// 是否启用独立的远端 Emby 库高频轮询循环（5 分钟）。
    /// 关闭后远端库的增量更新将完全交由计划任务"媒体库扫描"承担。
    pub enable_remote_library_monitor: bool,
    /// PB49 (S2)：远端 sync 完成后是否再触发一次本地 scanner 兜底扫描。
    ///
    /// 默认 `true`：兼容旧行为——`incremental_update_library` 在 source sync 成功后
    /// 总是再调一次 `scan_single_library_with_db_semaphore`，保证：
    ///   1. 用户手动加进 library 的「物理路径」上的新增文件能被扫到；
    ///   2. 用户手工删 / 改了某个 STRM 文件能被识别（PB49 S1 短路过滤会自动放行）；
    ///   3. NFO / 海报等 sidecar 资产被新增 / 修改也能反映到 DB。
    ///
    /// 设为 `false`：纯远端镜像场景下的极致优化——跳过本地兜底扫，所有变更完全由
    /// 远端 sync 路径决定。**仅在 library 没有任何本地物理路径、且不会被 file
    /// watcher 触发再扫的「pure remote」场景启用**。混合库（既有远端源又有本地
    /// 物理路径）仍然会执行本地扫描，因为 S2 关闭只跳过「远端 sync 之后那一次
    /// 串联的本地扫描」，独立按钮和 file watcher 触发的扫描不受影响。
    ///
    /// 通过环境变量 `APP_AUTO_LOCAL_SCAN_AFTER_REMOTE_SYNC=false` 关闭。
    pub auto_local_scan_after_remote_sync: bool,
    /// PB49 (Cap)：同时允许跑的远端 Emby sync 任务数上限。
    ///
    /// 用户配置了多个远端 source 时，定时调度器或自动增量轮询可能在同一分钟
    /// 触发 N 个源同时同步——每个源会瞬间占用 ~12 个 PG 连接（主循环 8 +
    /// detail spawn 4），加上日常 API 流量，N≥5 时容易把 PG 池打满
    /// （默认 100 个连接，主循环占用其中 ~70+，剩余 ~30 留给 API）。
    ///
    /// 全局 semaphore 在 per-source mutex 之后获取（acquire `await`），
    /// 等待期间会推一个 `WaitingForGlobalSlot` phase 让前端 UI 看到「队列中」
    /// 而非「卡死」。per-source mutex 仍保证「同源不能并发」，所以本配置
    /// 只影响**不同 source 之间**的并发度。
    ///
    /// - 默认 `2`：同时只跑 2 个源，对常见 50~100 PG 连接配置安全
    /// - 增大：PG 池要相应调大（建议 `database_max_connections >=
    ///   remote_sync_global_concurrency * 15 + 30`）
    /// - 设为 `0`：表示不限制（不推荐）
    ///
    /// 通过环境变量 `APP_REMOTE_SYNC_GLOBAL_CONCURRENCY` 调整。
    pub remote_sync_global_concurrency: usize,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let server_name = env::var("APP_SERVER_NAME").unwrap_or_else(|_| "Movie Rust".to_string());
        let server_id = env::var("APP_SERVER_ID")
            .ok()
            .and_then(|value| Uuid::parse_str(&value).ok())
            .unwrap_or_else(|| Uuid::new_v5(&Uuid::NAMESPACE_DNS, server_name.as_bytes()));

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://movie:movie@localhost:5432/movie_rust".to_string()),
            // PB49 (Cap)：默认从 50 提升到 100。
            //
            // 单个远端 source sync 高峰占用 ~12 连接（主循环 8 + detail spawn 4）。
            // 默认 `remote_sync_global_concurrency=2` 意味着两个源并发时占用 ~24，
            // 加上日常 API / WebSocket 流量后剩余给 scanner / playback ~70+，安全。
            //
            // PG 服务器侧的 `max_connections` 必须 ≥ 此值；PG 默认 100，单实例
            // 跑这一个项目刚好够用，多实例 / 共享 DB 场景需要 PG 那边先扩容。
            database_max_connections: env::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(100),
            host: env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("APP_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8096),
            server_name,
            server_id,
            ui_culture: env::var("APP_UI_CULTURE").unwrap_or_else(|_| "zh-CN".to_string()),
            metadata_country_code: env::var("APP_METADATA_COUNTRY_CODE")
                .unwrap_or_else(|_| "CN".to_string()),
            preferred_metadata_language: env::var("APP_PREFERRED_METADATA_LANGUAGE")
                .unwrap_or_else(|_| "zh".to_string()),
            enable_remote_access: env::var("APP_ENABLE_REMOTE_ACCESS")
                .ok()
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            enable_automatic_port_mapping: env::var("APP_ENABLE_AUTOMATIC_PORT_MAPPING")
                .ok()
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            public_url: env::var("APP_PUBLIC_URL")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            branding_login_disclaimer: env::var("APP_LOGIN_DISCLAIMER").unwrap_or_default(),
            branding_custom_css: env::var("APP_BRANDING_CSS").unwrap_or_default(),
            branding_splashscreen_enabled: env::var("APP_SPLASHSCREEN_ENABLED")
                .ok()
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(false),
            static_dir: env::var("APP_STATIC_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("frontend/dist")),
            log_dir: env::var("LOG_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("logs")),
            image_cache_dir: env::var("APP_IMAGE_CACHE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("image_cache")),
            image_cache_ttl_secs: env::var("APP_IMAGE_CACHE_TTL_SECS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(7 * 24 * 3600),
            tmdb_api_key: env::var("TMDB_API_KEY").ok(),
            api_key: env::var("EMBY_API_KEY").ok(),
            ffmpeg_path: env::var("FFMPEG_PATH").unwrap_or_else(|_| "ffmpeg".to_string()),
            transcode_dir: env::var("TRANSCODE_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("transcodes")),
            transcode_threads: env::var("TRANSCODE_THREADS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(0), // 0 表示自动检测
            enable_transcoding: env::var("ENABLE_TRANSCODING")
                .ok()
                .map(|value| value.to_lowercase() == "true")
                .unwrap_or(false),
            max_transcode_sessions: env::var("MAX_TRANSCODE_SESSIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(4),
            allowed_origins: env::var("APP_ALLOWED_ORIGINS")
                .ok()
                .map(|v| {
                    v.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            enable_remote_library_monitor: env::var("APP_ENABLE_REMOTE_LIBRARY_MONITOR")
                .ok()
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            auto_local_scan_after_remote_sync: env::var("APP_AUTO_LOCAL_SCAN_AFTER_REMOTE_SYNC")
                .ok()
                .map(|value| value.eq_ignore_ascii_case("true"))
                .unwrap_or(true),
            remote_sync_global_concurrency: env::var("APP_REMOTE_SYNC_GLOBAL_CONCURRENCY")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(2),
        })
    }

    pub fn bind_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(format!("{}:{}", self.host, self.port).parse()?)
    }
}
