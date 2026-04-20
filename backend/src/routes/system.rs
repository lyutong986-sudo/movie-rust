use crate::{
    auth::AuthSession,
    models::{BrandingConfiguration, EndpointInfo, PublicSystemInfo, SystemInfo},
    repository,
    state::AppState,
};
use axum::{
    extract::State,
    http::{header::CONTENT_TYPE, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/System/Info/Public", get(public_info))
        .route("/system/info/public", get(public_info))
        .route("/System/Info", get(system_info))
        .route("/system/info", get(system_info))
        .route("/System/Endpoint", get(endpoint_info))
        .route("/system/endpoint", get(endpoint_info))
        .route("/System/Ping", get(ping).post(ping))
        .route("/system/ping", get(ping).post(ping))
        .route("/Branding/Configuration", get(branding_configuration))
        .route("/branding/configuration", get(branding_configuration))
        .route("/Branding/Css", get(branding_css))
        .route("/Branding/Css.css", get(branding_css))
        .route("/branding/css", get(branding_css))
        .route("/branding/css.css", get(branding_css))
}

async fn public_info(
    State(state): State<AppState>,
) -> Result<Json<PublicSystemInfo>, crate::error::AppError> {
    let startup_wizard_completed = repository::startup_wizard_completed(&state.pool).await?;

    Ok(Json(PublicSystemInfo {
        local_address: format!("http://{}:{}", state.config.host, state.config.port),
        server_name: state.config.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Movie Rust".to_string(),
        operating_system: std::env::consts::OS.to_string(),
        id: state.config.server_id.to_string(),
        startup_wizard_completed,
    }))
}

async fn system_info(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<SystemInfo>, crate::error::AppError> {
    let startup_wizard_completed = repository::startup_wizard_completed(&state.pool).await?;

    Ok(Json(SystemInfo {
        local_address: format!("http://{}:{}", state.config.host, state.config.port),
        server_name: state.config.server_name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Movie Rust".to_string(),
        operating_system: std::env::consts::OS.to_string(),
        id: state.config.server_id.to_string(),
        startup_wizard_completed,
        can_self_restart: false,
    }))
}

async fn endpoint_info(_session: AuthSession) -> Json<EndpointInfo> {
    Json(EndpointInfo {
        is_local: true,
        is_in_network: true,
    })
}

async fn branding_configuration() -> Json<BrandingConfiguration> {
    Json(BrandingConfiguration {
        login_disclaimer: String::new(),
        custom_css: String::new(),
        splashscreen_enabled: false,
    })
}

async fn branding_css() -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/css; charset=utf-8")], "")
}

async fn ping() -> StatusCode {
    StatusCode::NO_CONTENT
}
