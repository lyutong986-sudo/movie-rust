use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::{
        AddVirtualFolderDto, BaseItemDto, CreateLibraryRequest, LibraryMediaFolderDto,
        LibrarySubFolderDto, MediaPathDto, ScanSummary, UpdateLibraryOptionsDto,
        UpdateMediaPathRequestDto, VirtualFolderInfoDto, VirtualFolderQuery,
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
use serde::{Deserialize, Serialize};
use std::path::{Path as StdPath, PathBuf};
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
        .route("/Library/VirtualFolders/Delete", post(remove_virtual_folder))
        .route("/Library/VirtualFolders/Query", get(virtual_folders))
        .route("/Library/VirtualFolders/Name", post(rename_virtual_folder))
        .route(
            "/Library/VirtualFolders/Paths",
            post(add_media_path).delete(remove_media_path),
        )
        .route("/Library/VirtualFolders/Paths/Delete", post(remove_media_path))
        .route(
            "/Library/VirtualFolders/Paths/Update",
            post(update_media_path),
        )
        .route(
            "/Library/VirtualFolders/LibraryOptions",
            post(update_library_options),
        )
        .route("/Library/Refresh", post(refresh_libraries))
        .route("/Library/Media/Updated", post(library_notify))
        .route("/Library/Movies/Added", post(library_notify))
        .route("/Library/Movies/Updated", post(library_notify))
        .route("/Library/Series/Added", post(library_notify))
        .route("/Library/Series/Updated", post(library_notify))
        .route("/Library/PhysicalPaths", get(physical_paths))
        .route(
            "/Library/SelectableMediaFolders",
            get(selectable_media_folders),
        )
        .route("/Environment/Drives", get(environment_drives))
        .route(
            "/Environment/DefaultDirectoryBrowser",
            get(environment_default_directory_browser),
        )
        .route("/Environment/NetworkDevices", get(environment_network_devices))
        .route("/Environment/NetworkShares", get(environment_network_shares))
        .route(
            "/Environment/DirectoryContents",
            get(environment_directory_contents).post(environment_directory_contents),
        )
        .route(
            "/Environment/ValidatePath",
            get(environment_validate_path).post(environment_validate_path),
        )
        .route("/Environment/ParentPath", get(environment_parent_path))
        .route("/api/admin/scan", post(scan_libraries))
}

fn enqueue_library_scan(state: &AppState, trigger: &str) {
    let pool = state.pool.clone();
    let metadata_manager = state.metadata_manager.clone();
    let config = state.config.clone();
    let work_limiters = state.work_limiters.clone();
    let trigger = trigger.to_string();

    tokio::spawn(async move {
        match scanner::scan_all_libraries(&pool, metadata_manager, &config, work_limiters).await {
            Ok(summary) => {
                tracing::info!(
                    trigger = %trigger,
                    libraries = summary.libraries,
                    scanned_files = summary.scanned_files,
                    imported_items = summary.imported_items,
                    "后台媒体库扫描完成"
                );
            }
            Err(error) => {
                tracing::error!(trigger = %trigger, error = %error, "后台媒体库扫描失败");
            }
        }
    });
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
    Query(query): Query<VirtualFolderQuery>,
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
    let refresh_library = query.refresh_library.unwrap_or(false);
    let library = repository::create_library(
        &state.pool,
        payload.name.trim(),
        payload.collection_type.trim(),
        &paths,
        payload.library_options,
    )
    .await?;
    if refresh_library {
        enqueue_library_scan(&state, "create_library");
    }
    Ok(Json(
        repository::library_to_item_dto(&state.pool, &library, state.config.server_id).await?,
    ))
}

async fn delete_library(
    session: AuthSession,
    State(state): State<AppState>,
    Path(library_id): Path<Uuid>,
    Query(query): Query<VirtualFolderQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    repository::delete_library(&state.pool, library_id).await?;
    if query.refresh_library.unwrap_or(false) {
        enqueue_library_scan(&state, "delete_library");
    }
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
    let refresh_library = query.refresh_library.unwrap_or(false);
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
    if refresh_library {
        enqueue_library_scan(&state, "add_virtual_folder");
    }
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
    let refresh_library = query.refresh_library.unwrap_or(false);
    repository::delete_library_by_name(&state.pool, name).await?;
    if refresh_library {
        enqueue_library_scan(&state, "remove_virtual_folder");
    }
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
    repository::update_library_options(&state.pool, payload.id, payload.library_options).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_media_path(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<VirtualFolderQuery>,
    Json(payload): Json<MediaPathDto>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let path = payload
        .path_info
        .map(|info| info.path)
        .or(payload.path)
        .ok_or_else(|| AppError::BadRequest("缺少媒体路径".to_string()))?;
    repository::add_library_path(&state.pool, &payload.name, &path).await?;
    if query.refresh_library.unwrap_or(false) {
        enqueue_library_scan(&state, "add_media_path");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn update_media_path(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<VirtualFolderQuery>,
    Json(payload): Json<UpdateMediaPathRequestDto>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    repository::update_library_path(&state.pool, &payload.name, payload.path_info).await?;
    if query.refresh_library.unwrap_or(false) {
        enqueue_library_scan(&state, "update_media_path");
    }
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
    let refresh_library = query.refresh_library.unwrap_or(false);
    repository::remove_library_path(&state.pool, name, path).await?;
    if refresh_library {
        enqueue_library_scan(&state, "remove_media_path");
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn scan_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<ScanSummary>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(
        scanner::scan_all_libraries(
            &state.pool,
            state.metadata_manager.clone(),
            &state.config,
            state.work_limiters.clone(),
        )
        .await?,
    ))
}

async fn refresh_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    enqueue_library_scan(&state, "refresh_libraries");
    Ok(StatusCode::NO_CONTENT)
}

async fn library_notify(
    session: AuthSession,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    Ok(StatusCode::NO_CONTENT)
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
            id: library.id.to_string(),
            guid: library.id.to_string(),
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

#[derive(Debug, Deserialize)]
struct DirectoryContentsQuery {
    #[serde(default, rename = "Path", alias = "path")]
    path: Option<String>,
    #[serde(default, rename = "IncludeFiles", alias = "includeFiles")]
    include_files: Option<bool>,
    #[serde(default, rename = "IncludeDirectories", alias = "includeDirectories")]
    include_directories: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ParentPathQuery {
    #[serde(default, rename = "Path", alias = "path")]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ValidatePathQuery {
    #[serde(default, rename = "Path", alias = "path")]
    path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct FileSystemEntryInfo {
    name: String,
    path: String,
    #[serde(rename = "Type")]
    entry_type: String,
}

async fn environment_drives(
    session: AuthSession,
) -> Result<Json<Vec<FileSystemEntryInfo>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(list_root_entries()))
}

async fn environment_directory_contents(
    session: AuthSession,
    Query(query): Query<DirectoryContentsQuery>,
) -> Result<Json<Vec<FileSystemEntryInfo>>, AppError> {
    auth::require_admin(&session)?;
    let path = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 Path 参数".to_string()))?;
    let include_files = query.include_files.unwrap_or(false);
    let include_directories = query.include_directories.unwrap_or(true);

    let mut entries = Vec::new();
    let dir = PathBuf::from(path);
    if !dir.is_dir() {
        return Err(AppError::NotFound("目录不存在".to_string()));
    }

    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else {
            continue;
        };
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let is_dir = file_type.is_dir();
        let is_file = file_type.is_file();
        if (is_dir && !include_directories) || (is_file && !include_files) || (!is_dir && !is_file)
        {
            continue;
        }

        entries.push(FileSystemEntryInfo {
            name: entry.file_name().to_string_lossy().to_string(),
            path: entry.path().to_string_lossy().to_string(),
            entry_type: if is_dir { "Directory" } else { "File" }.to_string(),
        });
    }

    entries.sort_by(|left, right| {
        left.entry_type
            .cmp(&right.entry_type)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(Json(entries))
}

async fn environment_parent_path(
    session: AuthSession,
    Query(query): Query<ParentPathQuery>,
) -> Result<Json<String>, AppError> {
    auth::require_admin(&session)?;
    let path = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 Path 参数".to_string()))?;
    let parent = StdPath::new(path)
        .parent()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok(Json(parent))
}

async fn environment_default_directory_browser(
    session: AuthSession,
) -> Result<Json<Vec<FileSystemEntryInfo>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(list_root_entries()))
}

async fn environment_network_devices(
    session: AuthSession,
) -> Result<Json<Vec<FileSystemEntryInfo>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(Vec::new()))
}

async fn environment_network_shares(
    session: AuthSession,
) -> Result<Json<Vec<FileSystemEntryInfo>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(Vec::new()))
}

async fn environment_validate_path(
    session: AuthSession,
    Query(query): Query<ValidatePathQuery>,
) -> Result<Json<bool>, AppError> {
    auth::require_admin(&session)?;
    let path = query.path.unwrap_or_default();
    let is_valid = !path.trim().is_empty() && StdPath::new(path.trim()).exists();
    Ok(Json(is_valid))
}

fn list_root_entries() -> Vec<FileSystemEntryInfo> {
    #[cfg(windows)]
    {
        let mut entries = Vec::new();
        for letter in b'A'..=b'Z' {
            let path = format!("{}:\\", letter as char);
            if StdPath::new(&path).is_dir() {
                entries.push(FileSystemEntryInfo {
                    name: path.clone(),
                    path,
                    entry_type: "Directory".to_string(),
                });
            }
        }
        entries
    }

    #[cfg(not(windows))]
    {
        vec![FileSystemEntryInfo {
            name: "/".to_string(),
            path: "/".to_string(),
            entry_type: "Directory".to_string(),
        }]
    }
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
