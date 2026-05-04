use crate::{
    config::Config, metadata::provider::MetadataProviderManager,
    metadata::translator::TranslatorService, transcoder::Transcoder,
    work_limiter::WorkLimiters,
};
use dashmap::DashMap;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Emby-compatible server-side events broadcast to WebSocket clients.
#[derive(Debug, Clone)]
pub enum ServerEvent {
    /// User data changed (played, favorite, progress, etc.)
    UserDataChanged {
        user_id: Uuid,
        user_data_list: Vec<serde_json::Value>,
    },
    /// Library items added/updated/removed
    LibraryChanged {
        items_added: Vec<String>,
        items_updated: Vec<String>,
        items_removed: Vec<String>,
    },
    /// Sessions changed (playback start/stop/progress)
    SessionsChanged,
}

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub metadata_manager: Option<Arc<MetadataProviderManager>>,
    /// PB52：翻译兜底服务。可选——前端关闭翻译时构造一个 default-disabled 的实例
    /// 而不是 None，避免其它模块每次都要 `if let Some(translator)` 判空。
    pub translator: Option<Arc<TranslatorService>>,
    pub websocket_sessions: Arc<DashMap<uuid::Uuid, crate::routes::websocket::WebSocketSession>>,
    pub transcoder: Transcoder,
    pub work_limiters: WorkLimiters,
    pub task_tokens: Arc<tokio::sync::RwLock<std::collections::HashMap<String, CancellationToken>>>,
    /// Shared HTTP client for all outbound requests (connection pooling)
    pub http_client: reqwest::Client,
    /// Limits how many DB connections the scanner/sync background tasks can hold
    /// simultaneously, reserving the rest for API requests.
    pub scan_db_semaphore: Arc<Semaphore>,
    /// PB49 (Cap)：远端 Emby sync 全局并发上限。
    ///
    /// 与 `SOURCE_SYNC_LOCKS`（per-source mutex）正交：
    ///   - per-source mutex 保证「同一 source 不能并发」，重复触发立即返回 BUSY
    ///   - 本 semaphore 保证「不同 source 加起来不超过 N 个并发」，N 个之外的
    ///     源会在 acquire().await 上排队，UI 上看到的 phase = `WaitingForGlobalSlot`
    ///
    /// 由 `Config.remote_sync_global_concurrency` 决定容量；`None` 表示不限制
    /// （等价于 `Semaphore::new(usize::MAX)`，仍提供句柄一致性，避免分支复杂化）。
    pub remote_sync_global_semaphore: Arc<Semaphore>,
    /// Broadcast channel for server events (WebSocket push to clients)
    pub event_tx: tokio::sync::broadcast::Sender<ServerEvent>,
}
