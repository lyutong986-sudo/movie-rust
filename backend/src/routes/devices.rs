use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::{require_admin, AuthSession},
    error::AppError,
    repository,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Devices", get(list_devices).delete(delete_device))
        .route("/Devices/", get(list_devices))
        .route("/Devices/Info", get(device_info))
        .route("/Devices/Options", post(update_device_options))
        .route("/Devices/Delete", post(delete_device))
        .route("/devices", get(list_devices).delete(delete_device))
        .route("/devices/info", get(device_info))
        .route("/devices/options", post(update_device_options))
        .route("/devices/delete", post(delete_device))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DeviceQuery {
    #[serde(default, alias = "id", alias = "Id", alias = "deviceId")]
    id: Option<String>,
    #[serde(default, alias = "supportsSync", alias = "SupportsSync")]
    supports_sync: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DeviceOptionsPayload {
    #[serde(default, alias = "id", alias = "Id")]
    id: Option<String>,
    #[serde(default, alias = "deviceId", alias = "DeviceId")]
    device_id: Option<String>,
    #[serde(default, alias = "customName", alias = "CustomName")]
    custom_name: Option<String>,
}

async fn list_devices(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DeviceQuery>,
) -> Result<Json<Value>, AppError> {
    // 按 device_id 聚合 sessions 表，按 EmbySDK DeviceInfo 结构返回。
    let sessions = repository::list_sessions(&state.pool).await?;

    // 非管理员仅能看到自己设备
    let visible_user = if session.is_admin {
        None
    } else {
        Some(session.user_id)
    };

    let mut by_device: std::collections::BTreeMap<String, DeviceAccumulator> =
        std::collections::BTreeMap::new();
    for s in sessions {
        if let Some(uid) = visible_user {
            if s.user_id != uid {
                continue;
            }
        }
        let device_id = match s.device_id.clone() {
            Some(v) if !v.trim().is_empty() => v,
            _ => continue,
        };
        if let Some(filter) = query.id.as_deref() {
            if filter != device_id {
                continue;
            }
        }
        let entry = by_device
            .entry(device_id.clone())
            .or_insert_with(|| DeviceAccumulator {
                id: device_id.clone(),
                name: s
                    .device_name
                    .clone()
                    .unwrap_or_else(|| "Unknown Device".to_string()),
                app_name: s.client.clone().unwrap_or_default(),
                app_version: s.application_version.clone().unwrap_or_default(),
                last_user_id: s.user_id,
                last_user_name: s.user_name.clone(),
                date_last_activity: s.last_activity_at,
            });
        if s.last_activity_at > entry.date_last_activity {
            entry.date_last_activity = s.last_activity_at;
            entry.last_user_id = s.user_id;
            entry.last_user_name = s.user_name.clone();
            if let Some(client) = s.client.as_ref() {
                entry.app_name = client.clone();
            }
            if let Some(version) = s.application_version.as_ref() {
                entry.app_version = version.clone();
            }
        }
    }

    let items: Vec<Value> = by_device
        .into_values()
        .map(|d| {
            let custom_name = futures_like_placeholder_custom_name(&state, &d.id);
            json!({
                "Name": d.name,
                "CustomName": custom_name,
                "AccessToken": Value::Null,
                "Id": d.id,
                "LastUserName": d.last_user_name,
                "LastUserId": crate::models::uuid_to_emby_guid(&d.last_user_id),
                "AppName": d.app_name,
                "AppVersion": d.app_version,
                "DateLastActivity": d.date_last_activity,
                "IconUrl": Value::Null,
                "SupportsSync": query.supports_sync.unwrap_or(false),
                "IsInFamily": false,
            })
        })
        .collect();

    Ok(Json(json!({
        "Items": items,
        "TotalRecordCount": items.len(),
    })))
}

struct DeviceAccumulator {
    id: String,
    name: String,
    app_name: String,
    app_version: String,
    last_user_id: uuid::Uuid,
    last_user_name: String,
    date_last_activity: DateTime<Utc>,
}

fn futures_like_placeholder_custom_name(_state: &AppState, _id: &str) -> Value {
    // 设备别名由 /Devices/Options 写入 system_settings；这里不做阻塞查询避免放大请求。
    // 实际读取在 device_info 完成。
    Value::Null
}

async fn device_info(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DeviceQuery>,
) -> Result<Json<Value>, AppError> {
    let id = query
        .id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 Id 参数".to_string()))?;

    let sessions = repository::list_sessions(&state.pool).await?;
    let mut latest: Option<crate::models::AuthSessionRow> = None;
    for s in sessions {
        if s.device_id.as_deref() != Some(id) {
            continue;
        }
        if !session.is_admin && s.user_id != session.user_id {
            continue;
        }
        latest = match latest {
            None => Some(s),
            Some(prev) => {
                if s.last_activity_at > prev.last_activity_at {
                    Some(s)
                } else {
                    Some(prev)
                }
            }
        };
    }

    let Some(s) = latest else {
        return Err(AppError::NotFound(format!("设备不存在: {id}")));
    };

    let custom_name = repository::get_setting_value(&state.pool, &format!("device:{id}:name"))
        .await?
        .and_then(|v| v.as_str().map(|s| s.to_string()));

    Ok(Json(json!({
        "Name": s.device_name.clone().unwrap_or_else(|| "Unknown".to_string()),
        "CustomName": custom_name,
        "Id": id,
        "LastUserName": s.user_name,
        "LastUserId": crate::models::uuid_to_emby_guid(&s.user_id),
        "AppName": s.client.clone().unwrap_or_default(),
        "AppVersion": s.application_version.clone().unwrap_or_default(),
        "DateLastActivity": s.last_activity_at,
        "SupportsSync": false,
        "IsInFamily": false,
    })))
}

async fn update_device_options(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DeviceQuery>,
    Json(payload): Json<DeviceOptionsPayload>,
) -> Result<StatusCode, AppError> {
    require_admin(&session)?;
    let id = payload
        .id
        .or(payload.device_id)
        .or(query.id)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 Id 参数".to_string()))?;

    let custom_name = payload.custom_name.unwrap_or_default();
    let key = format!("device:{id}:name");
    if custom_name.trim().is_empty() {
        repository::delete_setting_value(&state.pool, &key).await?;
    } else {
        repository::set_setting_value(&state.pool, &key, json!(custom_name)).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_device(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<DeviceQuery>,
) -> Result<StatusCode, AppError> {
    let id = query
        .id
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 Id 参数".to_string()))?;

    let sessions = repository::list_sessions(&state.pool).await?;
    for s in sessions {
        if s.device_id.as_deref() != Some(id.as_str()) {
            continue;
        }
        if !session.is_admin && s.user_id != session.user_id {
            return Err(AppError::Forbidden);
        }
        repository::delete_session(&state.pool, &s.access_token).await?;
    }
    let _ = repository::delete_setting_value(&state.pool, &format!("device:{id}:name")).await;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    #[test]
    fn devices_router_builds_without_conflicts() {
        let _ = super::router();
    }
}
