use crate::{
    auth::AuthSession,
    error::AppError,
    repository,
    state::AppState,
};
use axum::{
    extract::{Form, Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path as FsPath, PathBuf},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/Environment/DefaultDirectoryBrowser",
            get(default_directory_browser),
        )
        .route("/Environment/Drives", get(environment_drives))
        .route("/Environment/DirectoryContents", get(directory_contents))
        .route("/Environment/ParentPath", get(parent_path))
        .route("/Environment/NetworkShares", get(network_shares))
        .route("/Environment/NetworkDevices", get(network_devices))
        .route("/Environment/ValidatePath", post(validate_path))
        .route("/Devices", get(devices))
        .route("/Devices/{id}", delete(delete_device))
        .route("/Devices/{id}/Delete", post(delete_device))
        .route("/Devices/CameraUploads", get(camera_uploads))
        .route("/Channels", get(channels))
        .route("/ScheduledTasks", get(scheduled_tasks))
        .route("/ScheduledTasks/{id}/Triggers", post(update_task_triggers))
        .route("/ScheduledTasks/Running/{id}", post(start_task).delete(stop_task))
        .route("/ScheduledTasks/Running/{id}/Delete", post(stop_task))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DirectoryQuery {
    #[serde(default, alias = "path")]
    path: Option<String>,
    #[serde(default, alias = "includeFiles")]
    include_files: Option<bool>,
    #[serde(default, alias = "includeDirectories")]
    include_directories: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PathQuery {
    #[serde(default, alias = "path")]
    path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ValidatePathRequest {
    path: String,
    #[serde(default)]
    validate_writeable: Option<bool>,
}

async fn default_directory_browser(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Json<Value> {
    Json(json!({
        "Path": state.config.static_dir.to_string_lossy().to_string()
    }))
}

async fn environment_drives(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Json<Vec<Value>> {
    Json(list_drives(&state))
}

async fn directory_contents(
    _session: AuthSession,
    Query(query): Query<DirectoryQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    let path = query
        .path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少目录路径".to_string()))?;
    let include_files = query.include_files.unwrap_or(false);
    let include_directories = query.include_directories.unwrap_or(true);
    let items = list_directory_items(FsPath::new(path), include_files, include_directories)?;
    Ok(Json(items))
}

async fn parent_path(
    _session: AuthSession,
    Query(query): Query<PathQuery>,
) -> impl IntoResponse {
    let parent = query
        .path
        .as_deref()
        .and_then(|path| FsPath::new(path).parent())
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    parent
}

async fn network_devices(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn network_shares(
    _session: AuthSession,
    Query(_query): Query<PathQuery>,
) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn validate_path(
    _session: AuthSession,
    Form(payload): Form<ValidatePathRequest>,
) -> Result<StatusCode, AppError> {
    let path = FsPath::new(payload.path.trim());
    if !path.exists() {
        return Err(AppError::NotFound("路径不存在".to_string()));
    }
    if payload.validate_writeable.unwrap_or(false) && !is_path_writeable(path) {
        return Err(AppError::Internal("目录不可写".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn devices(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let sessions = repository::list_all_sessions(&state.pool).await?;
    let mut grouped = BTreeMap::<String, Value>::new();

    for session in sessions {
        let device_id = session
            .device_id
            .clone()
            .unwrap_or_else(|| session.access_token.clone());
        grouped.entry(device_id.clone()).or_insert_with(|| {
            json!({
                "Id": device_id,
                "Name": session.device_name.clone().unwrap_or_else(|| "Unknown Device".to_string()),
                "AppName": session.client.clone().unwrap_or_else(|| "Movie Rust Client".to_string()),
                "LastUserId": session.user_id.to_string(),
                "LastUserName": session.user_name,
                "DateLastActivity": session.last_activity_at,
                "Capabilities": []
            })
        });
    }

    let items = grouped.into_values().collect::<Vec<_>>();
    Ok(Json(json!({
        "Items": items,
        "TotalRecordCount": items.len(),
        "StartIndex": 0
    })))
}

async fn delete_device(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let sessions = repository::list_all_sessions(&state.pool).await?;
    for session in sessions {
        if session.device_id.as_deref() == Some(id.as_str()) {
            repository::delete_session(&state.pool, &session.access_token).await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn camera_uploads(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0,
        "StartIndex": 0
    }))
}

async fn channels(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0,
        "StartIndex": 0
    }))
}

async fn scheduled_tasks(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![
        json!({
            "Id": "scan-library",
            "Name": "Scan Media Library",
            "Description": "Scans the media library for changes.",
            "Category": "Library",
            "Key": "ScanMediaLibrary",
            "State": "Idle",
            "IsHidden": false,
            "CurrentProgressPercentage": 0,
            "LastExecutionResult": {
                "Status": "Completed"
            }
        }),
        json!({
            "Id": "refresh-guide",
            "Name": "Refresh Guide",
            "Description": "Refreshes live tv guide data.",
            "Category": "LiveTV",
            "Key": "RefreshGuide",
            "State": "Idle",
            "IsHidden": false,
            "CurrentProgressPercentage": 0,
            "LastExecutionResult": {
                "Status": "Completed"
            }
        }),
        json!({
            "Id": "refresh-metadata",
            "Name": "Refresh Metadata",
            "Description": "Refreshes metadata and images.",
            "Category": "Library",
            "Key": "RefreshMetadata",
            "State": "Idle",
            "IsHidden": false,
            "CurrentProgressPercentage": 0,
            "LastExecutionResult": {
                "Status": "Completed"
            }
        }),
    ])
}

async fn start_task(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn stop_task(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn update_task_triggers(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    repository::update_named_system_configuration(
        &state.pool,
        &format!("scheduled_task_triggers_{id}"),
        &payload,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

fn list_drives(state: &AppState) -> Vec<Value> {
    #[cfg(windows)]
    {
        let mut items = Vec::new();
        for letter in 'A'..='Z' {
            let path = format!("{letter}:\\");
            if FsPath::new(&path).exists() {
                items.push(json!({
                    "Name": path,
                    "Path": path,
                    "Type": "Directory"
                }));
            }
        }
        if items.is_empty() {
            items.push(json!({
                "Name": state.config.static_dir.to_string_lossy().to_string(),
                "Path": state.config.static_dir.to_string_lossy().to_string(),
                "Type": "Directory"
            }));
        }
        items
    }
    #[cfg(not(windows))]
    {
        vec![json!({
            "Name": "/",
            "Path": "/",
            "Type": "Directory"
        })]
    }
}

fn list_directory_items(
    path: &FsPath,
    include_files: bool,
    include_directories: bool,
) -> Result<Vec<Value>, AppError> {
    if !path.exists() {
        return Err(AppError::NotFound("路径不存在".to_string()));
    }

    let mut items = std::fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let metadata = entry.metadata().ok()?;
            let file_type = if metadata.is_dir() {
                "Directory"
            } else if metadata.is_file() {
                "File"
            } else {
                return None;
            };

            if (file_type == "Directory" && !include_directories)
                || (file_type == "File" && !include_files)
            {
                return None;
            }

            let full_path: PathBuf = entry.path();
            Some(json!({
                "Name": entry.file_name().to_string_lossy().to_string(),
                "Path": full_path.to_string_lossy().to_string(),
                "Type": file_type
            }))
        })
        .collect::<Vec<_>>();

    items.sort_by(|a, b| {
        let a_type = a.get("Type").and_then(Value::as_str).unwrap_or_default();
        let b_type = b.get("Type").and_then(Value::as_str).unwrap_or_default();
        let a_name = a.get("Name").and_then(Value::as_str).unwrap_or_default();
        let b_name = b.get("Name").and_then(Value::as_str).unwrap_or_default();
        a_type.cmp(b_type).then_with(|| a_name.cmp(b_name))
    });

    Ok(items)
}

fn is_path_writeable(path: &FsPath) -> bool {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    !metadata.permissions().readonly()
}
