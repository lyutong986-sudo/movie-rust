use crate::{
    auth::{self, AuthSession},
    error::AppError,
    remote_emby, repository,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf, sync::OnceLock, time::Duration};
use tokio::sync::RwLock;
use uuid::Uuid;

/// 校验并规范化 STRM 输出根目录，必填且必须可写。
///
/// 远端媒体的 strm、元数据图、NFO、字幕都会写到此目录下，
/// 因此必须在创建/更新源时强制校验，避免落到虚拟字符串路径。
async fn validate_strm_output_path(raw: Option<&str>) -> Result<String, AppError> {
    let trimmed = raw
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("STRM 输出根目录为必填项".to_string()))?;
    let path = PathBuf::from(trimmed);
    if let Err(err) = tokio::fs::create_dir_all(&path).await {
        return Err(AppError::BadRequest(format!(
            "无法创建/访问 STRM 输出根目录 {trimmed}: {err}"
        )));
    }
    Ok(trimmed.to_string())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/admin/remote-emby/sources",
            get(list_remote_emby_sources).post(create_remote_emby_source),
        )
        .route(
            "/api/admin/remote-emby/views/preview",
            post(preview_remote_emby_views),
        )
        .route(
            "/api/admin/remote-emby/sources/{source_id}",
            delete(delete_remote_emby_source).put(update_remote_emby_source),
        )
        .route(
            "/api/admin/remote-emby/sources/{source_id}/sync",
            post(sync_remote_emby_source),
        )
        .route(
            "/api/admin/remote-emby/sync/operations",
            get(list_remote_emby_sync_operations),
        )
        .route(
            "/api/admin/remote-emby/sync/operations/{operation_id}",
            get(get_remote_emby_sync_operation),
        )
        .route(
            "/api/admin/remote-emby/sync/operations/{operation_id}/cancel",
            post(cancel_remote_emby_sync_operation),
        )
        .route(
            "/api/admin/remote-emby/cleanup-orphan-libraries",
            post(cleanup_orphan_remote_libraries),
        )
        .route(
            "/api/remote-emby/proxy/{source_id}/{remote_item_id}",
            get(proxy_remote_emby_item).head(proxy_remote_emby_item),
        )
}

/// PB24：管理员一次性清理"孤儿远端虚拟路径"——历史上 PB23 修复之前删除过的远端源
/// 在 `libraries` 表里残留的 `__remote_view_<source_id>_*` 独立库 / merge 库 PathInfos entry。
/// 现存远端源的虚拟路径不动；仅删那些 source_id 已不存在的孤儿。
///
/// 返回 `{ "deleted_libraries": u64, "updated_libraries": u64, "orphan_source_ids": u64 }`。
async fn cleanup_orphan_remote_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    auth::require_admin(&session)?;
    let (deleted, updated, orphan_ids) =
        repository::cleanup_orphan_remote_view_paths(&state.pool).await?;
    tracing::info!(
        deleted_libraries = deleted,
        updated_libraries = updated,
        orphan_source_ids = orphan_ids,
        "管理员手动触发：清理孤儿远端虚拟路径完成"
    );
    Ok(Json(serde_json::json!({
        "DeletedLibraries": deleted,
        "UpdatedLibraries": updated,
        "OrphanSourceIds": orphan_ids,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CreateRemoteEmbySourceRequest {
    #[serde(alias = "name")]
    name: String,
    #[serde(alias = "serverUrl", alias = "url")]
    server_url: String,
    #[serde(alias = "userName", alias = "user")]
    username: String,
    #[serde(alias = "pw", alias = "pwd")]
    password: String,
    /// 可选：separate 模式下可不填，后端会自动使用「远端 Emby 中转」库；空字符串也视为未填
    #[serde(default, alias = "targetLibraryId", deserialize_with = "crate::models::deserialize_optional_uuid")]
    target_library_id: Option<Uuid>,
    #[serde(default, alias = "displayMode")]
    display_mode: Option<String>,
    #[serde(default, alias = "remoteViewIds")]
    remote_view_ids: Vec<String>,
    #[serde(default, alias = "remoteViews")]
    remote_views: Vec<CreateRemoteEmbySourceRemoteView>,
    #[serde(default, alias = "spoofedUserAgent", alias = "UserAgent")]
    spoofed_user_agent: Option<String>,
    #[serde(default, alias = "isEnabled", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    enabled: Option<bool>,
    #[serde(default, alias = "strmOutputPath", alias = "strm_output_path")]
    strm_output_path: Option<String>,
    #[serde(
        default,
        alias = "syncMetadata",
        alias = "sync_metadata",
        deserialize_with = "crate::models::deserialize_option_bool_lenient"
    )]
    sync_metadata: Option<bool>,
    #[serde(
        default,
        alias = "syncSubtitles",
        alias = "sync_subtitles",
        deserialize_with = "crate::models::deserialize_option_bool_lenient"
    )]
    sync_subtitles: Option<bool>,
    #[serde(default, alias = "tokenRefreshIntervalSecs", alias = "token_refresh_interval_secs")]
    token_refresh_interval_secs: Option<i32>,
    /// 流量模式：`"proxy"`（本地中转，默认）或 `"redirect"`（302 直链）
    #[serde(default, alias = "proxyMode", alias = "proxy_mode")]
    proxy_mode: Option<String>,
    #[serde(default, alias = "viewLibraryMap", alias = "view_library_map")]
    view_library_map: Option<serde_json::Value>,
    /// 自动增量同步间隔（分钟），0 = 关闭。
    #[serde(default, alias = "autoSyncIntervalMinutes", alias = "auto_sync_interval_minutes")]
    auto_sync_interval_minutes: Option<i32>,
    /// 拉取速率：每页拉条目数，默认 200，clamp [50, 1000]。
    #[serde(default, alias = "pageSize", alias = "page_size")]
    page_size: Option<i32>,
    /// 拉取速率：两次 HTTP 请求最小间隔（毫秒），默认 0=不限，clamp [0, 60000]。
    #[serde(default, alias = "requestIntervalMs", alias = "request_interval_ms")]
    request_interval_ms: Option<i32>,
    /// PB39：单设备身份伪装。前端可通过「常见客户端预设」一键填入，
    /// 默认 Infuse-Direct on Apple TV / 8.2.4。空值由 repository 层兜底。
    #[serde(default, alias = "spoofedClient", alias = "spoofed_client")]
    spoofed_client: Option<String>,
    #[serde(default, alias = "spoofedDeviceName", alias = "spoofed_device_name")]
    spoofed_device_name: Option<String>,
    /// 32 位 hex；不传由 repository 用 `Uuid::new_v4` 派生一个。
    #[serde(default, alias = "spoofedDeviceId", alias = "spoofed_device_id")]
    spoofed_device_id: Option<String>,
    #[serde(default, alias = "spoofedAppVersion", alias = "spoofed_app_version")]
    spoofed_app_version: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UpdateRemoteEmbySourceRequest {
    #[serde(alias = "name")]
    name: String,
    #[serde(alias = "serverUrl", alias = "url")]
    server_url: String,
    #[serde(alias = "userName", alias = "user")]
    username: String,
    #[serde(default, alias = "pw", alias = "pwd")]
    password: Option<String>,
    /// 可选：separate 模式下可不填，后端会自动使用「远端 Emby 中转」库；空字符串也视为未填
    #[serde(default, alias = "targetLibraryId", deserialize_with = "crate::models::deserialize_optional_uuid")]
    target_library_id: Option<Uuid>,
    #[serde(default, alias = "displayMode")]
    display_mode: Option<String>,
    #[serde(default, alias = "remoteViewIds")]
    remote_view_ids: Vec<String>,
    #[serde(default, alias = "remoteViews")]
    remote_views: Vec<CreateRemoteEmbySourceRemoteView>,
    #[serde(default, alias = "spoofedUserAgent", alias = "UserAgent")]
    spoofed_user_agent: Option<String>,
    #[serde(default, alias = "isEnabled", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    enabled: Option<bool>,
    #[serde(default, alias = "strmOutputPath", alias = "strm_output_path")]
    strm_output_path: Option<String>,
    #[serde(
        default,
        alias = "syncMetadata",
        alias = "sync_metadata",
        deserialize_with = "crate::models::deserialize_option_bool_lenient"
    )]
    sync_metadata: Option<bool>,
    #[serde(
        default,
        alias = "syncSubtitles",
        alias = "sync_subtitles",
        deserialize_with = "crate::models::deserialize_option_bool_lenient"
    )]
    sync_subtitles: Option<bool>,
    #[serde(default, alias = "tokenRefreshIntervalSecs", alias = "token_refresh_interval_secs")]
    token_refresh_interval_secs: Option<i32>,
    /// 流量模式：`"proxy"`（本地中转，默认）或 `"redirect"`（302 直链）
    #[serde(default, alias = "proxyMode", alias = "proxy_mode")]
    proxy_mode: Option<String>,
    #[serde(default, alias = "viewLibraryMap", alias = "view_library_map")]
    view_library_map: Option<serde_json::Value>,
    /// 自动增量同步间隔（分钟），0 = 关闭。
    #[serde(default, alias = "autoSyncIntervalMinutes", alias = "auto_sync_interval_minutes")]
    auto_sync_interval_minutes: Option<i32>,
    /// 拉取速率：每页拉条目数，默认 200，clamp [50, 1000]。
    #[serde(default, alias = "pageSize", alias = "page_size")]
    page_size: Option<i32>,
    /// 拉取速率：两次 HTTP 请求最小间隔（毫秒），默认 0=不限，clamp [0, 60000]。
    #[serde(default, alias = "requestIntervalMs", alias = "request_interval_ms")]
    request_interval_ms: Option<i32>,
    /// PB39：单设备身份伪装。Update 路径下未传或空字符串则保留 DB 原值（不能覆盖为空，
    /// 否则远端 Devices 表里这台 device 突然换 ID 会触发 admin 告警）。
    #[serde(default, alias = "spoofedClient", alias = "spoofed_client")]
    spoofed_client: Option<String>,
    #[serde(default, alias = "spoofedDeviceName", alias = "spoofed_device_name")]
    spoofed_device_name: Option<String>,
    #[serde(default, alias = "spoofedDeviceId", alias = "spoofed_device_id")]
    spoofed_device_id: Option<String>,
    #[serde(default, alias = "spoofedAppVersion", alias = "spoofed_app_version")]
    spoofed_app_version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
struct CreateRemoteEmbySourceRemoteView {
    id: String,
    name: String,
    #[serde(default)]
    collection_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PreviewRemoteEmbyViewsRequest {
    #[serde(alias = "serverUrl", alias = "url")]
    server_url: String,
    #[serde(alias = "userName", alias = "user")]
    username: String,
    #[serde(alias = "pw", alias = "pwd")]
    password: String,
    #[serde(default, alias = "spoofedUserAgent", alias = "UserAgent")]
    spoofed_user_agent: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteEmbySourceDto {
    id: String,
    name: String,
    server_url: String,
    username: String,
    target_library_id: String,
    display_mode: String,
    remote_view_ids: Vec<String>,
    remote_views: Vec<remote_emby::RemoteViewPreview>,
    enabled: bool,
    spoofed_user_agent: String,
    remote_user_id: Option<String>,
    has_access_token: bool,
    last_sync_at: Option<String>,
    last_sync_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strm_output_path: Option<String>,
    sync_metadata: bool,
    sync_subtitles: bool,
    token_refresh_interval_secs: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_token_refresh_at: Option<String>,
    /// view_id -> local_library_id 映射（separate 模式下自动创建的视图库）
    view_library_map: serde_json::Value,
    /// 流量模式：`"proxy"`（本地中转）或 `"redirect"`（302 直链）
    proxy_mode: String,
    /// 自动增量同步间隔（分钟），0 = 关闭。
    auto_sync_interval_minutes: i32,
    /// 拉取速率：每页拉条目数。默认 200，可调 50–1000。
    page_size: i32,
    /// 拉取速率：两次请求之间最小间隔（毫秒）。默认 0 = 不限，可调 0–60000。
    request_interval_ms: i32,
    /// PB39：身份伪装四元组（默认 Infuse-Direct / Apple TV / 8.2.4，DeviceId 32 位 hex）。
    spoofed_client: String,
    spoofed_device_name: String,
    spoofed_device_id: String,
    spoofed_app_version: String,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteEmbySyncResponse {
    source_id: String,
    source_name: String,
    written_files: usize,
    source_root: String,
    scan_summary: crate::models::ScanSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteEmbySyncOperationDto {
    id: String,
    source_id: String,
    source_name: String,
    status: String,
    progress: f64,
    phase: String,
    total_items: u64,
    fetched_items: u64,
    written_files: u64,
    queued: bool,
    running: bool,
    done: bool,
    cancel_requested: bool,
    created_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    result: Option<RemoteEmbySyncResponse>,
    error: Option<String>,
    monitor_url: String,
}

#[derive(Clone)]
struct RemoteEmbySyncOperationState {
    id: Uuid,
    source_id: Uuid,
    source_name: String,
    status: &'static str,
    progress: f64,
    phase: String,
    total_items: u64,
    fetched_items: u64,
    written_files: u64,
    cancel_requested: bool,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    result: Option<RemoteEmbySyncResponse>,
    error: Option<String>,
    sync_progress_handle: Option<remote_emby::RemoteSyncProgress>,
}

impl RemoteEmbySyncOperationState {
    fn is_done(&self) -> bool {
        matches!(self.status, "Succeeded" | "Failed" | "Cancelled")
    }

    fn monitor_url(&self) -> String {
        format!("/api/admin/remote-emby/sync/operations/{}", self.id)
    }

    fn to_dto(&self) -> RemoteEmbySyncOperationDto {
        RemoteEmbySyncOperationDto {
            id: self.id.to_string(),
            source_id: self.source_id.to_string(),
            source_name: self.source_name.clone(),
            status: self.status.to_string(),
            progress: self.progress,
            phase: self.phase.clone(),
            total_items: self.total_items,
            fetched_items: self.fetched_items,
            written_files: self.written_files,
            queued: self.status == "Queued",
            running: matches!(self.status, "Running" | "Cancelling"),
            done: self.is_done(),
            cancel_requested: self.cancel_requested,
            created_at: self.created_at.to_rfc3339(),
            started_at: self.started_at.map(|value| value.to_rfc3339()),
            completed_at: self.completed_at.map(|value| value.to_rfc3339()),
            result: self.result.clone(),
            error: self.error.clone(),
            monitor_url: self.monitor_url(),
        }
    }
}

#[derive(Default)]
struct RemoteEmbySyncRegistry {
    active_operation_ids: BTreeMap<Uuid, Uuid>,
    operations: BTreeMap<Uuid, RemoteEmbySyncOperationState>,
}

fn remote_emby_sync_registry() -> &'static RwLock<RemoteEmbySyncRegistry> {
    static REGISTRY: OnceLock<RwLock<RemoteEmbySyncRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(RemoteEmbySyncRegistry::default()))
}

#[derive(Debug, Deserialize)]
struct ProxyQuery {
    #[serde(default, alias = "Sig")]
    sig: Option<String>,
    #[serde(default, alias = "Msid", alias = "MediaSourceId")]
    msid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SyncOperationsQuery {
    #[serde(default, rename = "Limit", alias = "limit")]
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct DeleteRemoteEmbySourceQuery {
    #[serde(
        default,
        rename = "KeepMappedItems",
        alias = "keepMappedItems",
        alias = "keep_mapped_items"
    )]
    keep_mapped_items: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteEmbySyncQueuedResponse {
    queued: bool,
    message: String,
    operation: RemoteEmbySyncOperationDto,
}

async fn list_remote_emby_sources(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<RemoteEmbySourceDto>>, AppError> {
    auth::require_admin(&session)?;
    let sources = repository::list_remote_emby_sources(&state.pool).await?;
    Ok(Json(
        sources
            .into_iter()
            .map(remote_emby_source_to_dto)
            .collect::<Vec<_>>(),
    ))
}

async fn create_remote_emby_source(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<CreateRemoteEmbySourceRequest>,
) -> Result<Json<RemoteEmbySourceDto>, AppError> {
    auth::require_admin(&session)?;
    let remote_views =
        serde_json::to_value(&payload.remote_views).unwrap_or_else(|_| serde_json::json!([]));
    let strm_output_path = validate_strm_output_path(payload.strm_output_path.as_deref()).await?;
    // target_library_id 可选：未填时自动使用「远端 Emby 中转」库
    let target_library_id = match payload.target_library_id {
        Some(id) if id != Uuid::nil() => id,
        _ => repository::ensure_remote_transit_library(&state.pool).await?.id,
    };
    let source = repository::create_remote_emby_source(
        &state.pool,
        &payload.name,
        &payload.server_url,
        &payload.username,
        &payload.password,
        payload
            .spoofed_user_agent
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(remote_emby::default_spoofed_user_agent()),
        target_library_id,
        payload.display_mode.as_deref().unwrap_or("merge"),
        payload.remote_view_ids.as_slice(),
        &remote_views,
        payload.enabled.unwrap_or(true),
        Some(strm_output_path.as_str()),
        payload.sync_metadata.unwrap_or(true),
        payload.sync_subtitles.unwrap_or(true),
        payload.token_refresh_interval_secs.unwrap_or(3600),
        payload.proxy_mode.as_deref().unwrap_or("proxy"),
        payload.view_library_map.as_ref(),
        payload.auto_sync_interval_minutes.unwrap_or(0),
        payload.page_size.unwrap_or(200),
        payload.request_interval_ms.unwrap_or(0),
        payload.spoofed_client.as_deref(),
        payload.spoofed_device_name.as_deref(),
        payload.spoofed_device_id.as_deref(),
        payload.spoofed_app_version.as_deref(),
    )
    .await?;
    Ok(Json(remote_emby_source_to_dto(source)))
}

async fn update_remote_emby_source(
    session: AuthSession,
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
    Json(payload): Json<UpdateRemoteEmbySourceRequest>,
) -> Result<Json<RemoteEmbySourceDto>, AppError> {
    auth::require_admin(&session)?;
    let remote_views =
        serde_json::to_value(&payload.remote_views).unwrap_or_else(|_| serde_json::json!([]));
    let strm_output_path = validate_strm_output_path(payload.strm_output_path.as_deref()).await?;
    // target_library_id 可选：未填时自动使用「远端 Emby 中转」库
    let target_library_id = match payload.target_library_id {
        Some(id) if id != Uuid::nil() => id,
        _ => repository::ensure_remote_transit_library(&state.pool).await?.id,
    };
    let source = repository::update_remote_emby_source(
        &state.pool,
        source_id,
        &payload.name,
        &payload.server_url,
        &payload.username,
        payload.password.as_deref(),
        payload
            .spoofed_user_agent
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(remote_emby::default_spoofed_user_agent()),
        target_library_id,
        payload.display_mode.as_deref().unwrap_or("merge"),
        payload.remote_view_ids.as_slice(),
        &remote_views,
        payload.enabled.unwrap_or(true),
        Some(strm_output_path.as_str()),
        payload.sync_metadata.unwrap_or(true),
        payload.sync_subtitles.unwrap_or(true),
        payload.token_refresh_interval_secs.unwrap_or(3600),
        payload.proxy_mode.as_deref().unwrap_or("proxy"),
        payload.view_library_map.as_ref(),
        payload.auto_sync_interval_minutes.unwrap_or(0),
        payload.page_size.unwrap_or(200),
        payload.request_interval_ms.unwrap_or(0),
        payload.spoofed_client.as_deref(),
        payload.spoofed_device_name.as_deref(),
        payload.spoofed_device_id.as_deref(),
        payload.spoofed_app_version.as_deref(),
    )
    .await?;
    Ok(Json(remote_emby_source_to_dto(source)))
}

async fn preview_remote_emby_views(
    session: AuthSession,
    Json(payload): Json<PreviewRemoteEmbyViewsRequest>,
) -> Result<Json<remote_emby::RemotePreviewResult>, AppError> {
    auth::require_admin(&session)?;
    let result = remote_emby::preview_remote_views(
        payload.server_url.as_str(),
        payload.username.as_str(),
        payload.password.as_str(),
        payload
            .spoofed_user_agent
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(remote_emby::default_spoofed_user_agent()),
    )
    .await?;
    Ok(Json(result))
}

async fn delete_remote_emby_source(
    session: AuthSession,
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
    Query(query): Query<DeleteRemoteEmbySourceQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let source = repository::get_remote_emby_source(&state.pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))?;
    if !query.keep_mapped_items {
        let deleted = remote_emby::cleanup_source_mapped_items(&state.pool, &source).await?;
        tracing::info!(
            source_id = %source.id,
            target_library_id = %source.target_library_id,
            deleted_items = deleted,
            "删除远端 Emby 源前完成映射媒体清理"
        );
    }
    repository::delete_remote_emby_source(&state.pool, source_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_remote_emby_source(
    session: AuthSession,
    State(state): State<AppState>,
    Path(source_id): Path<Uuid>,
) -> Result<Response, AppError> {
    auth::require_admin(&session)?;
    let operation_id = enqueue_remote_emby_sync(&state, source_id).await?;
    let operation = {
        let registry = remote_emby_sync_registry().read().await;
        registry
            .operations
            .get(&operation_id)
            .cloned()
            .ok_or_else(|| AppError::Internal("远端同步任务状态创建失败".to_string()))?
            .to_dto()
    };
    let monitor_url = operation.monitor_url.clone();
    let mut response = (
        StatusCode::ACCEPTED,
        Json(RemoteEmbySyncQueuedResponse {
            queued: true,
            message: "远端 Emby 同步任务已加入队列".to_string(),
            operation,
        }),
    )
        .into_response();
    if let Ok(location) = HeaderValue::from_str(&monitor_url) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
    Ok(response)
}

async fn list_remote_emby_sync_operations(
    session: AuthSession,
    Query(query): Query<SyncOperationsQuery>,
) -> Result<Json<Vec<RemoteEmbySyncOperationDto>>, AppError> {
    auth::require_admin(&session)?;
    let limit = query.limit.unwrap_or(20).clamp(1, 200) as usize;
    let registry = remote_emby_sync_registry().read().await;
    let operations = registry
        .operations
        .values()
        .rev()
        .take(limit)
        .map(RemoteEmbySyncOperationState::to_dto)
        .collect::<Vec<_>>();
    Ok(Json(operations))
}

async fn get_remote_emby_sync_operation(
    session: AuthSession,
    Path(operation_id): Path<Uuid>,
) -> Result<Response, AppError> {
    auth::require_admin(&session)?;
    let registry = remote_emby_sync_registry().read().await;
    let operation = registry
        .operations
        .get(&operation_id)
        .ok_or_else(|| AppError::NotFound("远端同步任务不存在".to_string()))?
        .to_dto();
    let status = if operation.done {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    let mut response = (status, Json(operation)).into_response();
    if status == StatusCode::ACCEPTED {
        response
            .headers_mut()
            .insert(header::RETRY_AFTER, HeaderValue::from_static("2"));
    }
    Ok(response)
}

async fn cancel_remote_emby_sync_operation(
    session: AuthSession,
    Path(operation_id): Path<Uuid>,
) -> Result<Json<RemoteEmbySyncOperationDto>, AppError> {
    auth::require_admin(&session)?;
    let mut registry = remote_emby_sync_registry().write().await;
    let operation = registry
        .operations
        .get_mut(&operation_id)
        .ok_or_else(|| AppError::NotFound("远端同步任务不存在".to_string()))?;
    if operation.is_done() {
        return Ok(Json(operation.to_dto()));
    }
    if operation.status == "Queued" {
        operation.status = "Cancelled";
        operation.phase = "Cancelled".to_string();
        operation.progress = 100.0;
        operation.completed_at = Some(Utc::now());
        let source_id = operation.source_id;
        let dto = operation.to_dto();
        if registry.active_operation_ids.get(&source_id).copied() == Some(operation_id) {
            registry.active_operation_ids.remove(&source_id);
        }
        return Ok(Json(dto));
    }
    operation.cancel_requested = true;
    operation.status = "Cancelling";
    if let Some(handle) = &operation.sync_progress_handle {
        handle.request_cancel();
    }
    Ok(Json(operation.to_dto()))
}

async fn proxy_remote_emby_item(
    State(state): State<AppState>,
    Path((source_id, remote_item_id)): Path<(Uuid, String)>,
    Query(query): Query<ProxyQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let signature = query
        .sig
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少远端代理签名".to_string()))?;
    remote_emby::proxy_item_stream(
        &state,
        source_id,
        remote_item_id.as_str(),
        query.msid.as_deref(),
        signature,
        request.method().clone(),
        request.headers(),
    )
    .await
}

async fn enqueue_remote_emby_sync(state: &AppState, source_id: Uuid) -> Result<Uuid, AppError> {
    let source = repository::get_remote_emby_source(&state.pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))?;
    if !source.enabled {
        return Err(AppError::BadRequest("远端 Emby 源已禁用".to_string()));
    }

    let operation_id = {
        let mut registry = remote_emby_sync_registry().write().await;
        if let Some(active_id) = registry.active_operation_ids.get(&source_id).copied() {
            if let Some(active) = registry.operations.get(&active_id) {
                if !active.is_done() {
                    // SF2：之前这里只看 `is_done()`，结果用户「中断同步」后旧 task 还在
                    // 跑（特别是卡在 fetch_all_remote_item_ids 那种长循环），用户再点
                    // 「立即同步」会立即被复用回**同一个**正在退出的旧 task id，前端就
                    // 永远看到那个停滞的 4% phase，看上去像「再点同步一直拉不到」。
                    // 现在区分三种情况：
                    //  1) cancel_requested + 还没退完：返回明确错误「上次取消尚未完成」，
                    //     前端可以提示用户稍候，旧 task 会在最长一页 HTTP 周期内（搭配
                    //     SF1 的 cancel 检查）真退出，再点就能起新 task。
                    //  2) 仍在跑且没要求取消：保留原行为返回 active_id（同一任务二次查看进度）。
                    if active.cancel_requested {
                        return Err(AppError::BadRequest(
                            "上一次取消尚未完成，旧任务正在退出，请稍候 1–2 秒再重试".to_string(),
                        ));
                    }
                    return Ok(active_id);
                }
            }
        }

        let id = Uuid::new_v4();
        registry.active_operation_ids.insert(source_id, id);
        registry.operations.insert(
            id,
            RemoteEmbySyncOperationState {
                id,
                source_id,
                source_name: source.name.clone(),
                status: "Queued",
                progress: 0.0,
                phase: "Queued".to_string(),
                total_items: 0,
                fetched_items: 0,
                written_files: 0,
                cancel_requested: false,
                created_at: Utc::now(),
                started_at: None,
                completed_at: None,
                result: None,
                error: None,
                sync_progress_handle: None,
            },
        );

        const KEEP_LATEST: usize = 100;
        while registry.operations.len() > KEEP_LATEST {
            if let Some(oldest_id) = registry.operations.keys().next().copied() {
                if registry
                    .active_operation_ids
                    .values()
                    .any(|active| *active == oldest_id)
                {
                    break;
                }
                registry.operations.remove(&oldest_id);
            } else {
                break;
            }
        }
        id
    };

    let source_id_for_task = source_id;
    let operation_id_for_task = operation_id;
    let state_for_task = state.clone();
    tokio::spawn(async move {
        {
            let mut registry = remote_emby_sync_registry().write().await;
            if let Some(operation) = registry.operations.get_mut(&operation_id_for_task) {
                operation.status = "Running";
                operation.phase = "Preparing".to_string();
                operation.progress = 1.0;
                operation.started_at = Some(Utc::now());
                operation.error = None;
            }
        }

        let progress = remote_emby::RemoteSyncProgress::new();
        {
            let mut registry = remote_emby_sync_registry().write().await;
            if let Some(operation) = registry.operations.get_mut(&operation_id_for_task) {
                operation.sync_progress_handle = Some(progress.clone());
            }
        }
        let poller_progress = progress.clone();
        let poller_operation_id = operation_id_for_task;
        let poller = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(1000)).await;
                let snap = poller_progress.snapshot().await;
                let mut registry = remote_emby_sync_registry().write().await;
                let Some(operation) = registry.operations.get_mut(&poller_operation_id) else {
                    break;
                };
                if operation.is_done() {
                    break;
                }
                if operation.cancel_requested {
                    poller_progress.request_cancel();
                }
                operation.phase = if snap.phase.is_empty() {
                    "Running".to_string()
                } else {
                    snap.phase
                };
                operation.progress = snap.progress.clamp(0.0, 99.5);
                operation.total_items = snap.total_items;
                operation.fetched_items = snap.fetched_items;
                operation.written_files = snap.written_files;
            }
        });

        let sync_result = remote_emby::sync_source_with_progress(
            &state_for_task,
            source_id_for_task,
            Some(progress),
        )
        .await;
        poller.abort();

        let mut registry = remote_emby_sync_registry().write().await;
        if let Some(operation) = registry.operations.get_mut(&operation_id_for_task) {
            match sync_result {
                Ok(result) => {
                    if operation.cancel_requested {
                        operation.status = "Cancelled";
                        operation.phase = "Cancelled".to_string();
                    } else {
                        operation.status = "Succeeded";
                        operation.phase = "Completed".to_string();
                    }
                    operation.progress = 100.0;
                    operation.written_files = result.written_files as u64;
                    operation.result = Some(RemoteEmbySyncResponse {
                        source_id: result.source_id.to_string(),
                        source_name: result.source_name,
                        written_files: result.written_files,
                        source_root: result.source_root,
                        scan_summary: result.scan_summary,
                    });
                    operation.error = None;
                    operation.completed_at = Some(Utc::now());
                }
                Err(error) => {
                    if operation.cancel_requested {
                        operation.status = "Cancelled";
                        operation.phase = "Cancelled".to_string();
                        operation.error = None;
                    } else {
                        operation.status = "Failed";
                        operation.phase = "Failed".to_string();
                        operation.error = Some(error.to_string());
                    }
                    operation.progress = 100.0;
                    operation.completed_at = Some(Utc::now());
                }
            }
            operation.sync_progress_handle = None;
        }
        if registry
            .active_operation_ids
            .get(&source_id_for_task)
            .copied()
            == Some(operation_id_for_task)
        {
            registry.active_operation_ids.remove(&source_id_for_task);
        }
    });

    Ok(operation_id)
}

fn remote_emby_source_to_dto(source: crate::models::DbRemoteEmbySource) -> RemoteEmbySourceDto {
    let remote_views =
        serde_json::from_value::<Vec<remote_emby::RemoteViewPreview>>(source.remote_views.clone())
            .unwrap_or_else(|_| {
                source
                    .remote_view_ids
                    .iter()
                    .map(|id| remote_emby::RemoteViewPreview {
                        id: id.clone(),
                        name: id.clone(),
                        collection_type: None,
                    })
                    .collect()
            });
    // PB39：先把伪装四元组从 source 借出（effective_* 内部 trim 后回落到默认），再 move 其它字段。
    let spoofed_client = source.effective_spoofed_client().to_string();
    let spoofed_device_name = source.effective_spoofed_device_name().to_string();
    let spoofed_device_id = source.effective_spoofed_device_id();
    let spoofed_app_version = source.effective_spoofed_app_version().to_string();
    RemoteEmbySourceDto {
        id: source.id.to_string(),
        name: source.name,
        server_url: source.server_url,
        username: source.username,
        target_library_id: source.target_library_id.to_string(),
        display_mode: source.display_mode,
        remote_view_ids: source.remote_view_ids,
        remote_views,
        enabled: source.enabled,
        spoofed_user_agent: source.spoofed_user_agent,
        remote_user_id: source.remote_user_id,
        has_access_token: source
            .access_token
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty()),
        last_sync_at: source.last_sync_at.map(|value| value.to_rfc3339()),
        last_sync_error: source.last_sync_error,
        strm_output_path: source.strm_output_path.clone(),
        sync_metadata: source.sync_metadata,
        sync_subtitles: source.sync_subtitles,
        token_refresh_interval_secs: source.token_refresh_interval_secs,
        last_token_refresh_at: source
            .last_token_refresh_at
            .map(|value| value.to_rfc3339()),
        view_library_map: source.view_library_map,
        auto_sync_interval_minutes: source.auto_sync_interval_minutes.max(0),
        proxy_mode: if source.proxy_mode.trim().is_empty() {
            "proxy".to_string()
        } else {
            source.proxy_mode
        },
        page_size: if source.page_size <= 0 { 200 } else { source.page_size },
        request_interval_ms: source.request_interval_ms.max(0),
        spoofed_client,
        spoofed_device_name,
        spoofed_device_id,
        spoofed_app_version,
        created_at: source.created_at.to_rfc3339(),
        updated_at: source.updated_at.to_rfc3339(),
    }
}
