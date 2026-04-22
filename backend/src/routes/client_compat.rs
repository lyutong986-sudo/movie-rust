use crate::{
    auth::AuthSession,
    error::AppError,
    repository,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Notifications/Types", get(notification_types))
        .route("/Notifications/Services", get(notification_services))
        .route("/Notifications/{user_id}/Summary", get(notification_summary))
        .route("/Notifications/{user_id}", get(notification_list))
        .route("/Notifications/{user_id}/Read", post(mark_notifications_read))
        .route("/Notifications/{user_id}/Unread", post(mark_notifications_unread))
        .route("/Search/Hints", get(search_hints))
        .route("/Playback/BitrateTest", get(playback_bitrate_test))
        .route("/LiveStreams/MediaInfo", post(live_stream_media_info))
        .route("/Sync/OfflineActions", post(sync_offline_actions))
        .route("/Sync/Data", post(sync_data))
        .route("/Sync/Items/Ready", get(sync_ready_items))
        .route("/Sync/JobItems/{id}/Transferred", post(sync_job_item_transferred))
        .route("/Sync/{target_id}/Items", delete(sync_cancel_items))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NotificationIdsQuery {
    #[serde(default)]
    ids: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BitrateQuery {
    #[serde(default)]
    size: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SyncReadyQuery {
    #[serde(default, alias = "TargetId")]
    target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SyncCancelQuery {
    #[serde(default, alias = "ItemIds")]
    item_ids: Option<String>,
}

async fn notification_types(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    let value = repository::named_system_configuration(&state.pool, "notifications")
        .await?
        .unwrap_or_else(default_notifications_configuration);

    let enabled_map = value
        .get("Options")
        .and_then(Value::as_array)
        .map(|items| {
            items.iter()
                .filter_map(|item| {
                    Some((
                        item.get("Type")?.as_str()?.to_string(),
                        item.get("Enabled").and_then(Value::as_bool).unwrap_or(false),
                    ))
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    Ok(Json(vec![
        json!({
            "Type": "PlaybackStart",
            "Name": "Playback Started",
            "Category": "Playback",
            "IsBasedOnUserEvent": true,
            "Enabled": enabled_map.get("PlaybackStart").copied().unwrap_or(false)
        }),
        json!({
            "Type": "PlaybackStopped",
            "Name": "Playback Stopped",
            "Category": "Playback",
            "IsBasedOnUserEvent": true,
            "Enabled": enabled_map.get("PlaybackStopped").copied().unwrap_or(false)
        }),
        json!({
            "Type": "SystemUpdate",
            "Name": "System Update",
            "Category": "System",
            "IsBasedOnUserEvent": false,
            "Enabled": enabled_map.get("SystemUpdate").copied().unwrap_or(false)
        }),
    ]))
}

async fn notification_services(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![
        json!({
            "Id": "log",
            "Name": "Server Log"
        }),
        json!({
            "Id": "webhook",
            "Name": "Webhook"
        }),
    ])
}

async fn notification_summary(
    _session: AuthSession,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "UnreadCount": 0,
        "MaxUnreadNotificationLevel": "Normal"
    }))
}

async fn notification_list(
    _session: AuthSession,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0,
        "StartIndex": 0
    }))
}

async fn mark_notifications_read(
    _session: AuthSession,
    Path(_user_id): Path<String>,
    Query(_query): Query<NotificationIdsQuery>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn mark_notifications_unread(
    _session: AuthSession,
    Path(_user_id): Path<String>,
    Query(_query): Query<NotificationIdsQuery>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn search_hints(
    _session: AuthSession,
) -> Json<Value> {
    Json(json!({
        "SearchHints": [],
        "TotalRecordCount": 0
    }))
}

async fn playback_bitrate_test(
    _session: AuthSession,
    Query(query): Query<BitrateQuery>,
) -> impl IntoResponse {
    let size = query.size.unwrap_or(1024 * 512).clamp(1, 8 * 1024 * 1024);
    let bytes = vec![0u8; size];
    ([(CONTENT_TYPE, "application/octet-stream")], bytes)
}

async fn live_stream_media_info(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "MediaSources": [],
        "PlaySessionId": null
    }))
}

async fn sync_offline_actions(_session: AuthSession) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn sync_data(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Items": []
    }))
}

async fn sync_ready_items(
    _session: AuthSession,
    Query(_query): Query<SyncReadyQuery>,
) -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0
    }))
}

async fn sync_job_item_transferred(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn sync_cancel_items(
    _session: AuthSession,
    Path(_target_id): Path<String>,
    Query(_query): Query<SyncCancelQuery>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

fn default_notifications_configuration() -> Value {
    json!({
        "Options": []
    })
}
