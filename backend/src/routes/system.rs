use crate::{
    auth::AuthSession,
    models::{PublicSystemInfo, SystemInfo},
    state::AppState,
};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/System/Info/Public", get(public_info))
        .route("/System/Info", get(system_info))
        .route("/System/Ping", get(ping).post(ping))
}

async fn public_info(State(state): State<AppState>) -> Json<PublicSystemInfo> {
    Json(PublicSystemInfo {
        local_address: format!("http://{}:{}", state.config.host, state.config.port),
        server_name: state.config.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Movie Rust".to_string(),
        operating_system: std::env::consts::OS.to_string(),
        id: state.config.server_id.to_string(),
        startup_wizard_completed: true,
    })
}

async fn system_info(_session: AuthSession, State(state): State<AppState>) -> Json<SystemInfo> {
    Json(SystemInfo {
        local_address: format!("http://{}:{}", state.config.host, state.config.port),
        server_name: state.config.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Movie Rust".to_string(),
        operating_system: std::env::consts::OS.to_string(),
        id: state.config.server_id.to_string(),
        startup_wizard_completed: true,
        can_self_restart: false,
    })
}

async fn ping() -> StatusCode {
    StatusCode::NO_CONTENT
}
