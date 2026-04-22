use crate::{
    auth,
    auth::AuthSession,
    error::AppError,
    models::{uuid_to_emby_guid, QueryResult},
    repository,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::StatusCode,
    routing::{delete, get},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Devices", get(get_devices).delete(delete_device_by_query))
        .route("/Devices/Info", get(get_device_info))
        .route("/Devices/Options", get(get_device_options).post(update_device_options))
        .route("/Devices/CameraUploads", get(camera_uploads).post(camera_upload))
        .route("/Devices/{id}", delete(delete_device_by_path))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DevicesQuery {
    #[serde(default, alias = "sortOrder")]
    sort_order: Option<String>,
    #[serde(default, alias = "id")]
    id: Option<String>,
}

#[derive(Debug)]
struct DeviceRow {
    id: String,
    name: String,
    app_name: String,
    app_version: String,
    last_user_id: String,
    last_user_name: String,
    date_last_activity: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CameraUploadQuery {
    #[serde(default, alias = "id")]
    id: Option<String>,
    #[serde(default, alias = "album")]
    album: Option<String>,
    #[serde(default, alias = "name")]
    name: Option<String>,
}

async fn get_devices(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DevicesQuery>,
) -> Result<Json<QueryResult<Value>>, AppError> {
    auth::require_admin(&session)?;

    let mut devices = BTreeMap::<String, DeviceRow>::new();
    for session in repository::list_all_sessions(&state.pool).await? {
        let Some(device_id) = session.device_id.as_ref().map(String::as_str) else {
            continue;
        };
        if device_id.trim().is_empty() {
            continue;
        }

        let should_replace = devices
            .get(device_id)
            .is_none_or(|current| session.last_activity_at > current.date_last_activity);
        if should_replace {
            let custom_name = repository::get_device_custom_name(&state.pool, device_id).await?;
            devices.insert(
                device_id.to_string(),
                DeviceRow {
                    id: device_id.to_string(),
                    name: custom_name.unwrap_or_else(|| {
                        session
                            .device_name
                            .clone()
                            .unwrap_or_else(|| "Unknown device".to_string())
                    }),
                    app_name: session
                        .client
                        .clone()
                        .unwrap_or_else(|| "Movie Rust Client".to_string()),
                    app_version: session
                        .application_version
                        .clone()
                        .unwrap_or_else(|| "0.1.0".to_string()),
                    last_user_id: uuid_to_emby_guid(&session.user_id),
                    last_user_name: session.user_name,
                    date_last_activity: session.last_activity_at,
                },
            );
        }
    }

    let mut items = devices
        .into_values()
        .map(|device| {
            json!({
                "Id": device.id,
                "Name": device.name,
                "AppName": device.app_name,
                "AppVersion": device.app_version,
                "LastUserId": device.last_user_id,
                "LastUserName": device.last_user_name,
                "DateLastActivity": device.date_last_activity,
                "CameraUploadPath": null,
                "Capabilities": {}
            })
        })
        .collect::<Vec<_>>();

    let descending = query
        .sort_order
        .as_deref()
        .is_none_or(|value| value.eq_ignore_ascii_case("Descending"));
    items.sort_by(|left, right| {
        let left_date = left.get("DateLastActivity").and_then(Value::as_str).unwrap_or_default();
        let right_date = right.get("DateLastActivity").and_then(Value::as_str).unwrap_or_default();
        if descending {
            right_date.cmp(left_date)
        } else {
            left_date.cmp(right_date)
        }
    });

    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        start_index: Some(0),
        items,
    }))
}

async fn get_device_info(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DevicesQuery>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let id = required_device_id(query.id.as_deref())?;
    let device = latest_device_row(&state, id).await?;
    Ok(Json(device_to_json(device)))
}

async fn get_device_options(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DevicesQuery>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let id = required_device_id(query.id.as_deref())?;
    let custom_name = repository::get_device_custom_name(&state.pool, id).await?;
    Ok(Json(json!({
        "CustomName": custom_name.unwrap_or_default()
    })))
}

async fn update_device_options(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DevicesQuery>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let id = required_device_id(query.id.as_deref())?;
    let custom_name = payload
        .get("CustomName")
        .or_else(|| payload.get("customName"))
        .and_then(Value::as_str);
    repository::set_device_custom_name(&state.pool, id, custom_name).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn camera_uploads(
    _session: AuthSession,
    Query(query): Query<CameraUploadQuery>,
) -> Json<Value> {
    Json(json!({
        "DeviceId": query.id.unwrap_or_default(),
        "FilesUploaded": []
    }))
}

async fn camera_upload(
    _session: AuthSession,
    Query(query): Query<CameraUploadQuery>,
    _request: Request<Body>,
) -> StatusCode {
    let _metadata = (query.id, query.album, query.name);
    StatusCode::NO_CONTENT
}

async fn delete_device_by_query(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DevicesQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let id = query
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("Device Id is required".to_string()))?;
    repository::delete_sessions_by_device_id(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_device_by_path(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    repository::delete_sessions_by_device_id(&state.pool, id.trim()).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn required_device_id(id: Option<&str>) -> Result<&str, AppError> {
    id.map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("Device Id is required".to_string()))
}

async fn latest_device_row(state: &AppState, device_id: &str) -> Result<DeviceRow, AppError> {
    let custom_name = repository::get_device_custom_name(&state.pool, device_id).await?;
    repository::list_all_sessions(&state.pool)
        .await?
        .into_iter()
        .filter(|session| session.device_id.as_deref() == Some(device_id))
        .max_by_key(|session| session.last_activity_at)
        .map(|session| DeviceRow {
            id: device_id.to_string(),
            name: custom_name.unwrap_or_else(|| {
                session
                    .device_name
                    .unwrap_or_else(|| "Unknown device".to_string())
            }),
            app_name: session
                .client
                .unwrap_or_else(|| "Movie Rust Client".to_string()),
            app_version: session
                .application_version
                .unwrap_or_else(|| "0.1.0".to_string()),
            last_user_id: uuid_to_emby_guid(&session.user_id),
            last_user_name: session.user_name,
            date_last_activity: session.last_activity_at,
        })
        .ok_or_else(|| AppError::NotFound("Device not found".to_string()))
}

fn device_to_json(device: DeviceRow) -> Value {
    json!({
        "Id": device.id,
        "ReportedDeviceId": device.id,
        "Name": device.name,
        "AppName": device.app_name,
        "AppVersion": device.app_version,
        "LastUserId": device.last_user_id,
        "LastUserName": device.last_user_name,
        "DateLastActivity": device.date_last_activity,
        "IconUrl": null,
        "IpAddress": null,
        "CameraUploadPath": null,
        "Capabilities": {}
    })
}
