use crate::{
    config::Config, metadata::provider::MetadataProviderManager, transcoder::Transcoder,
    work_limiter::WorkLimiters,
};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub metadata_manager: Option<Arc<MetadataProviderManager>>,
    pub websocket_sessions: Arc<
        tokio::sync::RwLock<
            std::collections::HashMap<uuid::Uuid, crate::routes::websocket::WebSocketSession>,
        >,
    >,
    pub transcoder: Transcoder,
    pub work_limiters: WorkLimiters,
}
