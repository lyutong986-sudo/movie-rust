use crate::{config::Config, metadata::provider::MetadataProviderManager, transcoder::Transcoder};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub metadata_manager: Option<Arc<MetadataProviderManager>>,
    pub websocket_sessions: Arc<tokio::sync::RwLock<std::collections::HashMap<uuid::Uuid, crate::routes::websocket::WebSocketSession>>>,
    pub websocket_events: broadcast::Sender<String>,
    pub transcoder: Transcoder,
}
