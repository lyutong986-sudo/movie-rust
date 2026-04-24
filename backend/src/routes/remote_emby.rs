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
use std::{collections::BTreeMap, sync::OnceLock, time::Duration};
use tokio::sync::RwLock;
use uuid::Uuid;

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
            delete(delete_remote_emby_source),
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
            "/api/remote-emby/proxy/{source_id}/{remote_item_id}",
            get(proxy_remote_emby_item).head(proxy_remote_emby_item),
        )
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
    #[serde(alias = "targetLibraryId")]
    target_library_id: Uuid,
    #[serde(default, alias = "displayMode")]
    display_mode: Option<String>,
    #[serde(default, alias = "remoteViewIds")]
    remote_view_ids: Vec<String>,
    #[serde(default, alias = "remoteViews")]
    remote_views: Vec<CreateRemoteEmbySourceRemoteView>,
    #[serde(default, alias = "spoofedUserAgent", alias = "UserAgent")]
    spoofed_user_agent: Option<String>,
    #[serde(default, alias = "isEnabled")]
    enabled: Option<bool>,
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

#[derive(Debug, Clone)]
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
        payload.target_library_id,
        payload.display_mode.as_deref().unwrap_or("separate"),
        payload.remote_view_ids.as_slice(),
        &remote_views,
        payload.enabled.unwrap_or(true),
    )
    .await?;
    Ok(Json(remote_emby_source_to_dto(source)))
}

async fn preview_remote_emby_views(
    session: AuthSession,
    Json(payload): Json<PreviewRemoteEmbyViewsRequest>,
) -> Result<Json<Vec<remote_emby::RemoteViewPreview>>, AppError> {
    auth::require_admin(&session)?;
    let views = remote_emby::preview_remote_views(
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
    Ok(Json(views))
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
                    operation.status = "Succeeded";
                    operation.phase = "Completed".to_string();
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
                    operation.status = "Failed";
                    operation.phase = "Failed".to_string();
                    operation.progress = 100.0;
                    operation.error = Some(error.to_string());
                    operation.completed_at = Some(Utc::now());
                }
            }
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
        created_at: source.created_at.to_rfc3339(),
        updated_at: source.updated_at.to_rfc3339(),
    }
}
