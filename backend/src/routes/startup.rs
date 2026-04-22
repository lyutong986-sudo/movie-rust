use crate::{
    error::AppError,
    models::{StartupConfiguration, StartupRemoteAccessRequest, StartupUserRequest, UserDto},
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
    State(state): State<AppState>,
) -> Result<Json<StartupConfiguration>, AppError> {
    Ok(Json(
        repository::startup_configuration(&state.pool, &state.config).await?,
    ))
}

async fn update_configuration(
    State(state): State<AppState>,
    Json(payload): Json<StartupConfiguration>,
) -> Result<StatusCode, AppError> {
    repository::update_startup_configuration(&state.pool, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn first_user(State(state): State<AppState>) -> Result<Json<Option<UserDto>>, AppError> {
    let users = repository::list_users(&state.pool, false).await?;
    let user = match users.first() {
        Some(user) => Some(
            repository::user_to_dto_with_context(&state.pool, user, state.config.server_id)
                .await?,
        ),
        None => None,
    };
    Ok(Json(user))
}

async fn create_first_user(
    State(state): State<AppState>,
    Json(payload): Json<StartupUserRequest>,
) -> Result<Json<UserDto>, AppError> {
    let user =
        repository::create_initial_admin(&state.pool, &payload.name, &payload.password).await?;
    Ok(Json(
        repository::user_to_dto_with_context(&state.pool, &user, state.config.server_id).await?,
    ))
}

async fn remote_access(
    State(state): State<AppState>,
    Json(payload): Json<StartupRemoteAccessRequest>,
) -> Result<StatusCode, AppError> {
    repository::update_remote_access(&state.pool, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_remote_access(
    State(state): State<AppState>,
) -> Result<Json<StartupRemoteAccessRequest>, AppError> {
    Ok(Json(
        repository::startup_remote_access(&state.pool, &state.config).await?,
    ))
}

async fn complete(State(state): State<AppState>) -> Result<StatusCode, AppError> {
    repository::complete_startup_wizard(&state.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}
