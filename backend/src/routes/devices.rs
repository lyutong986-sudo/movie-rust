use crate::{auth, auth::AuthSession, error::AppError, models::QueryResult, repository, state::AppState};
use axum::{
    extract::{Path, Query, State},
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
            devices.insert(
                device_id.to_string(),
                DeviceRow {
                    id: device_id.to_string(),
                    name: session
                        .device_name
                        .clone()
                        .unwrap_or_else(|| "Unknown device".to_string()),
                    app_name: session
                        .client
                        .clone()
                        .unwrap_or_else(|| "Movie Rust Client".to_string()),
                    app_version: session
                        .application_version
                        .clone()
                        .unwrap_or_else(|| "0.1.0".to_string()),
                    last_user_id: session.user_id.to_string(),
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
