use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::{
        uuid_to_emby_guid, AddVirtualFolderDto, BaseItemDto, CreateLibraryRequest, LibraryMediaFolderDto,
        LibrarySubFolderDto, MediaPathDto, ScanSummary, UpdateLibraryOptionsDto,
        UpdateMediaPathRequestDto, VirtualFolderInfoDto,
        VirtualFolderQuery,
    },
    repository, scanner,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/libraries",
            get(admin_libraries).post(create_library),
        )
        .route("/api/admin/libraries/{library_id}", delete(delete_library))
        .route(
            "/Library/VirtualFolders",
            get(virtual_folders)
                .post(add_virtual_folder)
                .delete(remove_virtual_folder),
        )
        .route("/Library/VirtualFolders/Query", get(virtual_folders))
        .route("/Library/VirtualFolders/Name", post(rename_virtual_folder))
        .route(
            "/Library/VirtualFolders/Paths",
            post(add_media_path).delete(remove_media_path),
        )
        .route(
            "/Library/VirtualFolders/Paths/Update",
            post(update_media_path),
        )
        .route(
            "/Library/VirtualFolders/LibraryOptions",
            post(update_library_options),
        )
        .route("/Library/Refresh", post(refresh_libraries))
        .route("/Library/PhysicalPaths", get(physical_paths))
        .route("/Library/SelectableMediaFolders", get(selectable_media_folders))
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
    let mut paths = if payload.paths.is_empty() {
        vec![payload.path.clone()]
    } else {
        payload.paths.clone()
    };
    if paths.iter().all(|path| path.trim().is_empty()) {
        paths = payload
            .library_options
            .path_infos
            .iter()
            .map(|path| path.path.clone())
            .collect();
    }
    let library = repository::create_library(
        &state.pool,
        payload.name.trim(),
        payload.collection_type.trim(),
        &paths,
        payload.library_options,
    )
    .await?;
    Ok(Json(
        repository::library_to_item_dto(&state.pool, &library, state.config.server_id).await?,
    ))
}

async fn delete_library(
    session: AuthSession,
    State(state): State<AppState>,
    Path(library_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    repository::delete_library(&state.pool, library_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn virtual_folders(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<VirtualFolderInfoDto>>, AppError> {
    auth::require_admin(&session)?;
    let libraries = repository::list_libraries(&state.pool).await?;
    Ok(Json(
        libraries
            .iter()
            .map(repository::library_to_virtual_folder_dto)
            .collect(),
    ))
}

async fn add_virtual_folder(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<VirtualFolderQuery>,
    body: Option<Json<AddVirtualFolderDto>>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;

    let name = query
        .name
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("缺少媒体库名称".to_string()))?;
    let collection_type = query.collection_type.as_deref().unwrap_or("movies");
    let mut paths = split_query_paths(query.paths.as_deref().or(query.path.as_deref()));
    let _refresh_library = query.refresh_library.unwrap_or(false);
    let options = body
        .and_then(|Json(body)| body.library_options)
        .unwrap_or_default();
    if paths.is_empty() {
        paths = options
            .path_infos
            .iter()
            .map(|path| path.path.clone())
            .collect();
    }

    repository::create_library(&state.pool, name, collection_type, &paths, options).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_virtual_folder(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<VirtualFolderQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let name = query
        .name
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("缺少媒体库名称".to_string()))?;
    let _refresh_library = query.refresh_library.unwrap_or(false);
    repository::delete_library_by_name(&state.pool, name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn rename_virtual_folder(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<VirtualFolderQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let name = query
        .name
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("缺少媒体库名称".to_string()))?;
    let new_name = query
        .new_name
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("缺少新的媒体库名称".to_string()))?;
    let _refresh_library = query.refresh_library.unwrap_or(false);
    repository::rename_library(&state.pool, name, new_name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_library_options(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<UpdateLibraryOptionsDto>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let library_id = crate::models::emby_id_to_uuid(&payload.id)
        .map_err(|_| AppError::BadRequest(format!("无效的媒体库ID格式: {}", payload.id)))?;
    repository::update_library_options(&state.pool, library_id, payload.library_options).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_media_path(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<MediaPathDto>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let path = payload
        .path_info
        .map(|info| info.path)
        .or(payload.path)
        .ok_or_else(|| AppError::BadRequest("缺少媒体路径".to_string()))?;
    repository::add_library_path(&state.pool, &payload.name, &path).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_media_path(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<UpdateMediaPathRequestDto>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    repository::update_library_path(&state.pool, &payload.name, payload.path_info).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_media_path(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<VirtualFolderQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let name = query
        .name
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("缺少媒体库名称".to_string()))?;
    let path = query
        .path
        .as_deref()
        .ok_or_else(|| AppError::BadRequest("缺少媒体路径".to_string()))?;
    let _refresh_library = query.refresh_library.unwrap_or(false);
    repository::remove_library_path(&state.pool, name, path).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn scan_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<ScanSummary>, AppError> {
    auth::require_admin(&session)?;
    let libraries = repository::list_libraries(&state.pool).await?;
    let library_ids = libraries
        .iter()
        .map(|library| uuid_to_emby_guid(&library.id))
        .collect::<Vec<_>>();
    broadcast_library_refresh_started(&state, &library_ids);
    let result = scanner::scan_all_libraries(&state.pool, state.metadata_manager.as_deref()).await?;
    broadcast_library_refresh_finished(&state, library_ids);
    Ok(Json(result))
}

async fn refresh_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let libraries = repository::list_libraries(&state.pool).await?;
    let library_ids = libraries
        .iter()
        .map(|library| uuid_to_emby_guid(&library.id))
        .collect::<Vec<_>>();
    broadcast_library_refresh_started(&state, &library_ids);
    let _ = scanner::scan_all_libraries(&state.pool, state.metadata_manager.as_deref()).await?;
    broadcast_library_refresh_finished(&state, library_ids);
    Ok(StatusCode::NO_CONTENT)
}

fn broadcast_library_refresh_started(state: &AppState, library_ids: &[String]) {
    for library_id in library_ids {
        crate::routes::websocket::broadcast_message(
            state,
            "RefreshProgress",
            serde_json::json!({
                "ItemId": library_id,
                "Progress": 0
            }),
        );
    }
}

fn broadcast_library_refresh_finished(state: &AppState, library_ids: Vec<String>) {
    for library_id in &library_ids {
        crate::routes::websocket::broadcast_message(
            state,
            "RefreshProgress",
            serde_json::json!({
                "ItemId": library_id,
                "Progress": 100
            }),
        );
    }
    crate::routes::websocket::broadcast_message(
        state,
        "LibraryChanged",
        serde_json::json!({
            "ItemsAdded": [],
            "ItemsRemoved": [],
            "ItemsUpdated": library_ids
        }),
    );
}

async fn physical_paths(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<String>>, AppError> {
    auth::require_admin(&session)?;
    let libraries = repository::list_libraries(&state.pool).await?;
    let mut paths: Vec<String> = libraries
        .iter()
        .flat_map(|library| {
            let mut values = vec![library.path.clone()];
            values.extend(
                repository::library_to_virtual_folder_dto(library)
                    .locations
                    .into_iter(),
            );
            values
        })
        .collect();
    paths.sort();
    paths.dedup();
    Ok(Json(paths))
}

async fn selectable_media_folders(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<LibraryMediaFolderDto>>, AppError> {
    auth::require_admin(&session)?;
    let libraries = repository::list_libraries(&state.pool).await?;
    let items = libraries
        .iter()
        .map(|library| LibraryMediaFolderDto {
            name: library.name.clone(),
            id: uuid_to_emby_guid(&library.id),
            guid: uuid_to_emby_guid(&library.id),
            sub_folders: repository::library_to_virtual_folder_dto(library)
                .locations
                .into_iter()
                .enumerate()
                .map(|(index, path)| LibrarySubFolderDto {
                    name: format!("{}-{}", library.name, index + 1),
                    id: format!("{}:{index}", library.id),
                    path,
                    is_user_access_configurable: true,
                })
                .collect(),
            is_user_access_configurable: true,
        })
        .collect();
    Ok(Json(items))
}

fn split_query_paths(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
