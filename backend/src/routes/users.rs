use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::{AuthenticateByNameRequest, AuthenticationResult, UserDto},
    repository, security,
    state::AppState,
};
use axum::{
    extract::{Path, State},
    http::HeaderMap,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Users/Public", get(public_users))
        .route("/Users", get(users))
        .route("/Users/AuthenticateByName", post(authenticate_by_name))
        .route("/Users/Me", get(me))
        .route("/Users/{user_id}", get(user_by_id))
}

async fn public_users(State(state): State<AppState>) -> Result<Json<Vec<UserDto>>, AppError> {
    let users = repository::list_users(&state.pool, true).await?;
    Ok(Json(
        users
            .iter()
            .map(|user| repository::user_to_dto(user, state.config.server_id))
            .collect(),
    ))
}

async fn users(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserDto>>, AppError> {
    let users = repository::list_users(&state.pool, false).await?;
    Ok(Json(
        users
            .iter()
            .map(|user| repository::user_to_dto(user, state.config.server_id))
            .collect(),
    ))
}

async fn user_by_id(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserDto>, AppError> {
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    Ok(Json(repository::user_to_dto(&user, state.config.server_id)))
}

async fn me(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<UserDto>, AppError> {
    let user = repository::get_user_by_id(&state.pool, session.user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized)?;
    Ok(Json(repository::user_to_dto(&user, state.config.server_id)))
}

async fn authenticate_by_name(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<AuthenticateByNameRequest>,
) -> Result<Json<AuthenticationResult>, AppError> {
    let username = payload
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少用户名".to_string()))?;
    let password = payload
        .pw
        .as_deref()
        .or(payload.password.as_deref())
        .unwrap_or_default();

    let user = repository::get_user_by_name(&state.pool, username)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.is_disabled || !security::verify_password(&user.password_hash, password) {
        return Err(AppError::Unauthorized);
    }

    let device_id = payload
        .device_id
        .or_else(|| auth::client_value(&headers, "DeviceId"));
    let device_name = payload
        .device_name
        .or_else(|| auth::client_value(&headers, "Device"));
    let client = payload
        .client
        .or_else(|| auth::client_value(&headers, "Client"));
    let application_version = auth::client_value(&headers, "Version");

    let session = repository::create_session(
        &state.pool,
        user.id,
        device_id,
        device_name,
        client,
        application_version,
    )
    .await?;

    Ok(Json(AuthenticationResult {
        user: repository::user_to_dto(&user, state.config.server_id),
        session_info: repository::session_to_dto(&session),
        access_token: session.access_token,
        server_id: state.config.server_id.to_string(),
    }))
}
