use crate::{config::Config, metadata::provider::MetadataProviderManager};
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub metadata_manager: Option<Arc<MetadataProviderManager>>,
}
