use crate::{
    auth::{self, OptionalAuthSession},
    error::AppError,
    models::{PublicUserDto, StartupConfiguration, StartupRemoteAccessRequest, StartupUserRequest},
    repository,
    state::AppState,
};
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/Startup/Configuration",
            get(configuration).post(update_configuration),
        )
        .route("/Startup/User", get(first_user).post(create_first_user))
        .route("/Startup/FirstUser", get(first_user))
        .route(
            "/Startup/RemoteAccess",
            get(get_remote_access).post(remote_access),
        )
        .route("/Startup/Complete", axum::routing::post(complete))
}

async fn configuration(
    session: OptionalAuthSession,
    State(state): State<AppState>,
) -> Result<Json<StartupConfiguration>, AppError> {
    ensure_startup_access(&state, session.0.as_ref()).await?;
    Ok(Json(
        repository::startup_configuration(&state.pool, &state.config).await?,
    ))
}

async fn update_configuration(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Json(payload): Json<StartupConfiguration>,
) -> Result<StatusCode, AppError> {
    ensure_startup_access(&state, session.0.as_ref()).await?;
    repository::update_startup_configuration(&state.pool, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn first_user(
    session: OptionalAuthSession,
    State(state): State<AppState>,
) -> Result<Json<Option<PublicUserDto>>, AppError> {
    ensure_startup_access(&state, session.0.as_ref()).await?;
    let user = sqlx::query_as::<_, crate::models::DbUser>(
        "SELECT id, name, password_hash, is_admin, is_hidden, is_disabled, policy, \
         configuration, primary_image_path, backdrop_image_path, logo_image_path, date_modified, \
         easy_password_hash, created_at, legacy_password_format, legacy_password_hash \
         FROM users ORDER BY name LIMIT 1",
    )
    .fetch_optional(&state.pool)
    .await?;
    let dto = user.map(|u| repository::user_to_public_dto(&u, state.config.server_id));
    Ok(Json(dto))
}

async fn create_first_user(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Json(payload): Json<StartupUserRequest>,
) -> Result<Json<PublicUserDto>, AppError> {
    ensure_startup_wizard_open(&state, session.0.as_ref()).await?;
    let user =
        repository::create_initial_admin(&state.pool, &payload.name, &payload.password).await?;
    Ok(Json(repository::user_to_public_dto(
        &user,
        state.config.server_id,
    )))
}

async fn remote_access(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Json(payload): Json<StartupRemoteAccessRequest>,
) -> Result<StatusCode, AppError> {
    ensure_startup_access(&state, session.0.as_ref()).await?;
    repository::update_remote_access(&state.pool, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_remote_access(
    session: OptionalAuthSession,
    State(state): State<AppState>,
) -> Result<Json<StartupRemoteAccessRequest>, AppError> {
    ensure_startup_access(&state, session.0.as_ref()).await?;
    Ok(Json(
        repository::startup_remote_access(&state.pool, &state.config).await?,
    ))
}

async fn complete(
    session: OptionalAuthSession,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    ensure_startup_access(&state, session.0.as_ref()).await?;
    repository::complete_startup_wizard(&state.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ensure_startup_access(
    state: &AppState,
    session: Option<&auth::AuthSession>,
) -> Result<(), AppError> {
    if startup_wizard_open(state).await? {
        return Ok(());
    }

    let session = session.ok_or(AppError::Unauthorized)?;
    auth::require_admin(session)
}

async fn ensure_startup_wizard_open(
    state: &AppState,
    session: Option<&auth::AuthSession>,
) -> Result<(), AppError> {
    if startup_wizard_open(state).await? {
        return Ok(());
    }

    if let Some(session) = session {
        auth::require_admin(session)?;
    }
    Err(AppError::Forbidden)
}

async fn startup_wizard_open(state: &AppState) -> Result<bool, AppError> {
    let completed = repository::startup_wizard_completed(&state.pool).await?;
    let user_count = repository::user_count(&state.pool).await?;
    Ok(!completed && user_count == 0)
}
