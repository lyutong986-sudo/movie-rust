use crate::{
    config::Config, metadata::provider::MetadataProviderManager, transcoder::Transcoder,
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
    pub websocket_sessions: Arc<DashMap<uuid::Uuid, crate::routes::websocket::WebSocketSession>>,
    pub transcoder: Transcoder,
    pub work_limiters: WorkLimiters,
    pub task_tokens: Arc<tokio::sync::RwLock<std::collections::HashMap<String, CancellationToken>>>,
    /// Shared HTTP client for all outbound requests (connection pooling)
    pub http_client: reqwest::Client,
    /// Limits how many DB connections the scanner/sync background tasks can hold
    /// simultaneously, reserving the rest for API requests.
    pub scan_db_semaphore: Arc<Semaphore>,
    /// Broadcast channel for server events (WebSocket push to clients)
    pub event_tx: tokio::sync::broadcast::Sender<ServerEvent>,
}
