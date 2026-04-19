use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::{BaseItemDto, CreateLibraryRequest, ScanSummary},
    repository, scanner,
    state::AppState,
};
use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/libraries",
            get(admin_libraries).post(create_library),
        )
        .route("/api/admin/scan", post(scan_libraries))
}

async fn admin_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    auth::require_admin(&session)?;
    let libraries = repository::list_libraries(&state.pool).await?;
    let mut items = Vec::with_capacity(libraries.len());

    for library in libraries {
        items.push(
            repository::library_to_item_dto(&state.pool, &library, state.config.server_id).await?,
        );
    }

    Ok(Json(items))
}

async fn create_library(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<CreateLibraryRequest>,
) -> Result<Json<BaseItemDto>, AppError> {
    auth::require_admin(&session)?;
    let library = repository::create_library(
        &state.pool,
        payload.name.trim(),
        payload.collection_type.trim(),
        payload.path.trim(),
    )
    .await?;
    Ok(Json(
        repository::library_to_item_dto(&state.pool, &library, state.config.server_id).await?,
    ))
}

async fn scan_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<ScanSummary>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(scanner::scan_all_libraries(&state.pool).await?))
}
