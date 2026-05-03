use axum::{
    extract::{Query, State},
    http::HeaderMap,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::{
    auth,
    error::AppError,
    models::{uuid_to_emby_guid, ConnectAuthenticationExchangeResult},
    repository,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct ConnectExchangeQuery {
    #[serde(alias = "ConnectUserId", alias = "connectUserId")]
    connect_user_id: Option<String>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Connect/Exchange", get(connect_exchange))
        .route("/connect/exchange", get(connect_exchange))
}

async fn connect_exchange(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ConnectExchangeQuery>,
) -> Result<Json<ConnectAuthenticationExchangeResult>, AppError> {
    let connect_user_id = query
        .connect_user_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 ConnectUserId".to_string()))?;

    let access_key = auth::extract_token(&headers, None).ok_or(AppError::Unauthorized)?;

    let Some((local_user_id, payload)) =
        repository::find_user_by_connect_user_id(&state.pool, connect_user_id).await?
    else {
        return Err(AppError::Unauthorized);
    };

    if !repository::connect_exchange_access_key_allowed(&payload, &access_key) {
        return Err(AppError::Unauthorized);
    }

    let user = repository::get_user_by_id(&state.pool, local_user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let device_id = auth::client_value(&headers, "DeviceId");
    let device_name = auth::client_value(&headers, "Device");
    let client = auth::client_value(&headers, "Client");
    let application_version = auth::client_value(&headers, "Version");

    auth::ensure_login_policy(&state, &headers, &user, device_id.as_deref()).await?;

    let session = repository::create_session(
        &state.pool,
        local_user_id,
        device_id,
        device_name,
        client,
        application_version,
        auth::infer_client_ip(&headers),
        None,
    )
    .await?;

    Ok(Json(ConnectAuthenticationExchangeResult {
        local_user_id: uuid_to_emby_guid(&local_user_id),
        access_token: session.access_token,
    }))
}
