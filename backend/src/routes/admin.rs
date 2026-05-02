use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::{
        AddVirtualFolderDto, BaseItemDto, CreateLibraryRequest, LibraryMediaFolderDto,
        LibrarySubFolderDto, MediaPathDto, ScanSummary, UpdateLibraryOptionsDto,
        UpdateMediaPathRequestDto, VirtualFolderInfoDto, VirtualFolderQuery,
    },
    remote_emby, repository, scanner,
    state::AppState,
};
use axum::{
    extract::{OriginalUri, Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path as StdPath, PathBuf},
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
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
        .route(
            "/Library/VirtualFolders/Delete",
            post(remove_virtual_folder),
        )
        .route("/Library/VirtualFolders/Query", get(virtual_folders))
        .route("/Library/VirtualFolders/Name", post(rename_virtual_folder))
        .route(
            "/Library/VirtualFolders/Paths",
            post(add_media_path).delete(remove_media_path),
        )
        .route(
            "/Library/VirtualFolders/Paths/Delete",
            post(remove_media_path),
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
        .route(
            "/Environment/NetworkDevices",
            get(environment_network_devices),
        )
        .route(
            "/Environment/NetworkShares",
            get(environment_network_shares),
        )
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
        .route("/api/admin/scan/operations", get(list_scan_operations))
        .route(
            "/api/admin/scan/operations/{operation_id}",
            get(get_scan_operation),
        )
        .route(
            "/api/admin/scan/operations/{operation_id}/cancel",
            post(cancel_scan_operation),
        )
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ScanOperationDto {
    id: String,
    trigger: String,
    scope_key: String,
    library_id: Option<String>,
    library_name: Option<String>,
    status: String,
    progress: f64,
    phase: String,
    current_library: Option<String>,
    total_files: u64,
    scanned_files: u64,
    imported_items: u64,
    scan_rate_per_sec: f64,
    queued: bool,
    running: bool,
    done: bool,
    cancel_requested: bool,
    created_at: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    attempts: i32,
    max_attempts: i32,
    result: Option<ScanSummary>,
    error: Option<String>,
    monitor_url: String,
}

#[derive(Debug, Clone)]
struct ScanOperationState {
    id: Uuid,
    trigger: String,
    scope_key: String,
    library_id: Option<Uuid>,
    library_name: Option<String>,
    status: &'static str,
    progress: f64,
    phase: String,
    current_library: Option<String>,
    total_files: u64,
    scanned_files: u64,
    imported_items: u64,
    scan_rate_per_sec: f64,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    attempts: i32,
    max_attempts: i32,
    result: Option<ScanSummary>,
    error: Option<String>,
    cancel_requested: bool,
}

impl ScanOperationState {
    fn is_done(&self) -> bool {
        matches!(self.status, "Succeeded" | "Failed" | "Cancelled")
    }

    fn monitor_url(&self) -> String {
        format!("/api/admin/scan/operations/{}", self.id)
    }

    fn to_dto(&self) -> ScanOperationDto {
        ScanOperationDto {
            id: self.id.to_string(),
            trigger: self.trigger.clone(),
            scope_key: self.scope_key.clone(),
            library_id: self.library_id.map(|value| value.to_string()),
            library_name: self.library_name.clone(),
            status: self.status.to_string(),
            progress: self.progress,
            phase: self.phase.clone(),
            current_library: self.current_library.clone(),
            total_files: self.total_files,
            scanned_files: self.scanned_files,
            imported_items: self.imported_items,
            scan_rate_per_sec: self.scan_rate_per_sec,
            queued: self.status == "Queued",
            running: matches!(self.status, "Running" | "Cancelling"),
            done: self.is_done(),
            cancel_requested: self.cancel_requested,
            created_at: self.created_at.to_rfc3339(),
            started_at: self.started_at.map(|value| value.to_rfc3339()),
            completed_at: self.completed_at.map(|value| value.to_rfc3339()),
            attempts: self.attempts,
            max_attempts: self.max_attempts,
            result: self.result.clone(),
            error: self.error.clone(),
            monitor_url: self.monitor_url(),
        }
    }
}

#[derive(Default)]
struct ScanOperationRegistry {
    active_operation_ids: BTreeMap<String, Uuid>,
    operations: BTreeMap<Uuid, ScanOperationState>,
}

fn scan_registry() -> &'static RwLock<ScanOperationRegistry> {
    static REGISTRY: OnceLock<RwLock<ScanOperationRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(ScanOperationRegistry::default()))
}

/// 对单个媒体库执行"增量更新"：根据库是否绑定远端 Emby 源，分发到本地扫描或远端同步。
///
/// 行为约定：
/// - library 绑定了 `remote_emby_sources` → 先对每个相关源调用 `sync_source_with_progress`（增 / 改 / 删），
///   再 **始终**调用一次本地扫描（含 STRM 物理目录，参见 `library_scan_paths_union_remote_strm`），
///   以确保混合库的本地磁盘与 strm 手工删改能及时反映到 DB。
/// - 否则 → 仅调用 `scanner::scan_single_library_with_db_semaphore` 走本地扫描。
///
/// 返回的 `ScanSummary` 是该库在本次更新中处理的合并结果。
pub async fn incremental_update_library(
    state: &AppState,
    library_id: Uuid,
    progress: Option<scanner::ScanProgress>,
    db_semaphore: Option<std::sync::Arc<tokio::sync::Semaphore>>,
) -> Result<ScanSummary, AppError> {
    let remote_sources =
        repository::find_remote_sources_for_library(&state.pool, library_id).await?;
    if !remote_sources.is_empty() {
        let mut summary = ScanSummary {
            libraries: 0,
            scanned_files: 0,
            imported_items: 0,
        };
        for source in &remote_sources {
            // 远端同步使用独立的 RemoteSyncProgress 体系，与 scanner::ScanProgress 不互通；
            // 这里传 None，让远端同步自管理日志，与现有 enqueue_library_scan 行为保持一致。
            match remote_emby::sync_source_with_progress(state, source.id, None).await {
                Ok(result) => {
                    summary.libraries += 1;
                    summary.scanned_files += result.scan_summary.scanned_files;
                    summary.imported_items += result.scan_summary.imported_items;
                }
                // PB49：另一个 sync 正在跑（per-source 互斥），跳过本源不算硬失败。
                // 不识别这条错误的话，定时扫描会被 auto-retry 3 次全部失败，反而
                // 让用户在 UI 上看到「整轮扫描失败」假象，而真正在跑的手动 sync
                // 其实仍在正常推进。
                //
                // 注意：跳过的源不计入 summary.libraries——它根本没参与这一次扫描，
                // 否则会让上层 UI「本次扫描 X 个库」的统计失真。
                Err(err) if err.to_string().contains(remote_emby::SOURCE_SYNC_BUSY_TAG) => {
                    tracing::info!(
                        source_id = %source.id,
                        library_id = %library_id,
                        "跳过远端源同步：另一个同步任务正在进行中（per-source 互斥保护）"
                    );
                    continue;
                }
                Err(err) => {
                    tracing::warn!(
                        source_id = %source.id,
                        library_id = %library_id,
                        error = %err,
                        "远端 Emby 源同步失败"
                    );
                    return Err(err);
                }
            }
        }
        match scanner::scan_single_library_with_db_semaphore(
            &state.pool,
            state.metadata_manager.clone(),
            &state.config,
            state.work_limiters.clone(),
            library_id,
            progress,
            db_semaphore.clone(),
        )
        .await
        {
            Ok(scan_sum) => {
                summary.scanned_files += scan_sum.scanned_files;
                summary.imported_items += scan_sum.imported_items;
            }
            Err(err) => {
                tracing::warn!(
                    library_id = %library_id,
                    error = %err,
                    "远端同步已成功，本地/STRM 目录增量扫描失败"
                );
                return Err(err);
            }
        }
        Ok(summary)
    } else {
        scanner::scan_single_library_with_db_semaphore(
            &state.pool,
            state.metadata_manager.clone(),
            &state.config,
            state.work_limiters.clone(),
            library_id,
            progress,
            db_semaphore,
        )
        .await
    }
}

/// 遍历所有媒体库，为每个库各自调度本地扫描或远端同步。
pub async fn incremental_update_all_libraries(
    state: &AppState,
    progress: Option<scanner::ScanProgress>,
    db_semaphore: Option<std::sync::Arc<tokio::sync::Semaphore>>,
) -> Result<ScanSummary, AppError> {
    use futures::stream::{self, StreamExt};
    use std::sync::atomic::{AtomicI64, Ordering};

    let libraries = repository::list_libraries(&state.pool).await?;

    // PB49 (D2)：库间并发度。每个库的 sync_source_with_progress 内部已经
    // 由 REMOTE_SYNC_INNER_CONCURRENCY 控制单源并发，所以这里 2 是个保守的
    // 上限——既能让「等远端 ID 索引枚举」的 IO 等待时间和「另一个库的
    // upsert/STRM 写盘」时间互相重叠，也不会触发远端 Emby 的速率限制。
    //
    // 注意 incremental_update_library 内部还会按 source 个数串行——不同库
    // 的并发不会跟同源 sync 冲突（per-source mutex 拦截）。
    const LIBRARY_CONCURRENCY: usize = 2;

    let total_libraries = AtomicI64::new(0);
    let total_scanned = AtomicI64::new(0);
    let total_imported = AtomicI64::new(0);

    stream::iter(libraries.into_iter())
        .for_each_concurrent(LIBRARY_CONCURRENCY, |library| {
            let progress = progress.clone();
            let db_semaphore = db_semaphore.clone();
            let total_libraries = &total_libraries;
            let total_scanned = &total_scanned;
            let total_imported = &total_imported;
            async move {
                match incremental_update_library(state, library.id, progress, db_semaphore).await {
                    Ok(s) => {
                        total_libraries.fetch_add(s.libraries.max(1), Ordering::Relaxed);
                        total_scanned.fetch_add(s.scanned_files, Ordering::Relaxed);
                        total_imported.fetch_add(s.imported_items, Ordering::Relaxed);
                    }
                    Err(err) => {
                        tracing::warn!(
                            library_id = %library.id,
                            library_name = %library.name,
                            error = %err,
                            "媒体库增量更新失败，继续处理后续媒体库"
                        );
                    }
                }
            }
        })
        .await;

    Ok(ScanSummary {
        libraries: total_libraries.load(Ordering::Relaxed),
        scanned_files: total_scanned.load(Ordering::Relaxed),
        imported_items: total_imported.load(Ordering::Relaxed),
    })
}

async fn enqueue_library_scan(
    state: &AppState,
    trigger: &str,
    library_id: Option<Uuid>,
) -> Result<Uuid, AppError> {
    const MAX_ATTEMPTS: i32 = 3;
    let (scope_key, library_name) = match library_id {
        Some(id) => {
            let library = repository::get_library(&state.pool, id)
                .await?
                .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;
            (format!("library:{id}"), Some(library.name))
        }
        None => ("all".to_string(), None),
    };
    let operation_id = {
        let mut registry = scan_registry().write().await;
        if let Some(active_id) = registry.active_operation_ids.get(&scope_key).copied() {
            if let Some(active) = registry.operations.get(&active_id) {
                if !active.is_done() {
                    return Ok(active_id);
                }
            }
        }

        let id = Uuid::new_v4();
        registry.active_operation_ids.insert(scope_key.clone(), id);
        registry.operations.insert(
            id,
            ScanOperationState {
                id,
                trigger: trigger.to_string(),
                scope_key: scope_key.clone(),
                library_id,
                library_name: library_name.clone(),
                status: "Queued",
                progress: 0.0,
                phase: "Queued".to_string(),
                current_library: None,
                total_files: 0,
                scanned_files: 0,
                imported_items: 0,
                scan_rate_per_sec: 0.0,
                created_at: Utc::now(),
                started_at: None,
                completed_at: None,
                attempts: 0,
                max_attempts: MAX_ATTEMPTS,
                result: None,
                error: None,
                cancel_requested: false,
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

    let pool = state.pool.clone();
    let metadata_manager = state.metadata_manager.clone();
    let config = state.config.clone();
    let work_limiters = state.work_limiters.clone();
    let db_semaphore = Some(state.scan_db_semaphore.clone());
    let event_tx = state.event_tx.clone();
    let app_state = state.clone();
    let trigger = trigger.to_string();
    let scope_key_for_task = scope_key.clone();
    let library_id_for_task = library_id;

    tokio::spawn(async move {
        let mut attempt = 1i32;
        loop {
            {
                let mut registry = scan_registry().write().await;
                let Some(operation) = registry.operations.get_mut(&operation_id) else {
                    break;
                };
                if operation.cancel_requested {
                    operation.status = "Cancelled";
                    operation.progress = 100.0;
                    operation.phase = "Cancelled".to_string();
                    operation.completed_at = Some(Utc::now());
                    if registry
                        .active_operation_ids
                        .get(&scope_key_for_task)
                        .copied()
                        == Some(operation_id)
                    {
                        registry.active_operation_ids.remove(&scope_key_for_task);
                    }
                    break;
                }
                operation.status = "Running";
                // PB49：自动重试时不再重置已扫 / 已入库计数。
                //
                // 之前每次进入 attempt 都把 `total_files / scanned_files /
                // imported_items` 全部清零，配合 poller 把 fresh `ScanProgress`
                // 的 snap（attempt 内部从 0 开始）写回 operation，导致用户看到
                // 「11243 → 0 → 11243 → 0 …」反复跳——也就是用户报告的
                // 「资源会突然归 0 然后又重新开始增加」。
                //
                // 现在保留上一次 attempt 的最高水位线，poller 改成
                // `operation.* = max(operation.*, snap.*)`，UI 上的数字只能涨
                // 不会缩，重试感知更接近「卡了一下又恢复」而不是「白干一场」。
                if attempt == 1 {
                    operation.phase = "CollectingFiles".to_string();
                    operation.progress = 0.0;
                    operation.total_files = 0;
                    operation.scanned_files = 0;
                    operation.imported_items = 0;
                } else {
                    // 重试时让用户明确看到「这是第 N 次尝试」，而不是误以为
                    // 又回到「收集文件 0%」从零开始。
                    operation.phase = format!("Retrying({attempt}/{MAX_ATTEMPTS})");
                    // progress 给个保守的 5%，让进度条不至于回到 0。
                    operation.progress = operation.progress.max(5.0).min(99.0);
                }
                operation.scan_rate_per_sec = 0.0;
                operation.current_library = None;
                operation.attempts = attempt;
                operation.started_at.get_or_insert_with(Utc::now);
            }

            // 创建进度句柄，并启动一个后台轮询任务把 snapshot 写回注册表。
            let progress = scanner::ScanProgress::new();
            let poller_progress = progress.clone();
            let poller_op_id = operation_id;
            let poller_handle = tokio::spawn(async move {
                let mut last_scanned = 0u64;
                let mut last_tick = Instant::now();
                loop {
                    tokio::time::sleep(Duration::from_millis(1000)).await;
                    let snap = poller_progress.snapshot().await;
                    let now = Instant::now();
                    let elapsed = now.duration_since(last_tick).as_secs_f64();
                    let delta = snap.scanned_files.saturating_sub(last_scanned);
                    let scan_rate_per_sec = if elapsed <= f64::EPSILON {
                        0.0
                    } else {
                        (delta as f64 / elapsed).max(0.0)
                    };
                    last_scanned = snap.scanned_files;
                    last_tick = now;
                    let mut registry = scan_registry().write().await;
                    let Some(operation) = registry.operations.get_mut(&poller_op_id) else {
                        break;
                    };
                    if operation.is_done() || operation.cancel_requested {
                        break;
                    }
                    operation.phase = if snap.phase.is_empty() {
                        "CollectingFiles".to_string()
                    } else {
                        snap.phase.clone()
                    };
                    operation.current_library = snap.current_library.clone();
                    // PB49：单调推进——重试时上一次 attempt 的水位线不能被新 attempt
                    // 的「从 0 重新爬」snap 给覆盖（UI 会出现「归零再涨」的体感）。
                    // total_files 也用 max：不同 attempt 看到的总文件数偶尔会因为
                    // 文件系统变化而抖动，max 保证显示「目前已知的最大盘子」。
                    operation.total_files = operation.total_files.max(snap.total_files);
                    operation.scanned_files = operation.scanned_files.max(snap.scanned_files);
                    operation.imported_items = operation.imported_items.max(snap.imported_items);
                    // progress 也只许涨，避免重试瞬间从 80% 砸回 0%
                    operation.progress = operation.progress.max(snap.percent);
                    operation.scan_rate_per_sec = scan_rate_per_sec;
                }
            });

            enum ScanAttemptOutcome {
                Completed(Result<ScanSummary, AppError>),
                Cancelled,
            }

            let scan_future = {
                let progress = progress.clone();
                let db_semaphore = db_semaphore.clone();
                let state_for_dispatch = app_state.clone();
                async move {
                    if let Some(scan_library_id) = library_id_for_task {
                        incremental_update_library(
                            &state_for_dispatch,
                            scan_library_id,
                            Some(progress),
                            db_semaphore,
                        )
                        .await
                    } else {
                        incremental_update_all_libraries(
                            &state_for_dispatch,
                            Some(progress),
                            db_semaphore,
                        )
                        .await
                    }
                }
            };
            tokio::pin!(scan_future);

            let scan_outcome = loop {
                tokio::select! {
                    result = &mut scan_future => {
                        break ScanAttemptOutcome::Completed(result);
                    }
                    _ = tokio::time::sleep(Duration::from_millis(300)) => {
                        let cancel_requested = {
                            let registry = scan_registry().read().await;
                            registry
                                .operations
                                .get(&operation_id)
                                .map(|operation| operation.cancel_requested)
                                .unwrap_or(true)
                        };
                        if cancel_requested {
                            break ScanAttemptOutcome::Cancelled;
                        }
                    }
                }
            };
            poller_handle.abort();
            match scan_outcome {
                ScanAttemptOutcome::Cancelled => {
                    {
                        let mut registry = scan_registry().write().await;
                        if let Some(operation) = registry.operations.get_mut(&operation_id) {
                            operation.status = "Cancelled";
                            operation.progress = 100.0;
                            operation.phase = "Cancelled".to_string();
                            operation.scan_rate_per_sec = 0.0;
                            operation.completed_at = Some(Utc::now());
                        }
                        if registry
                            .active_operation_ids
                            .get(&scope_key_for_task)
                            .copied()
                            == Some(operation_id)
                        {
                            registry.active_operation_ids.remove(&scope_key_for_task);
                        }
                    }
                    tracing::info!(
                        operation_id = %operation_id,
                        trigger = %trigger,
                        attempt,
                        "后台媒体库扫描已取消"
                    );
                    break;
                }
                ScanAttemptOutcome::Completed(Ok(summary)) => {
                    {
                        let mut registry = scan_registry().write().await;
                        if let Some(operation) = registry.operations.get_mut(&operation_id) {
                            operation.status = "Succeeded";
                            operation.progress = 100.0;
                            operation.phase = "Completed".to_string();
                            // PB49：与 poller 的 max 单调语义保持一致——summary
                            // 是本次 attempt 的最终值，但若前几次 attempt 留下的
                            // 水位线更高（每次重试都会重跑同一批文件），这里也
                            // 不要让 UI 显示出回退。
                            operation.scanned_files = operation
                                .scanned_files
                                .max(summary.scanned_files as u64);
                            operation.imported_items = operation
                                .imported_items
                                .max(summary.imported_items as u64);
                            operation.scan_rate_per_sec = 0.0;
                            if operation.total_files == 0 {
                                operation.total_files = summary.scanned_files as u64;
                            }
                            operation.current_library = None;
                            operation.completed_at = Some(Utc::now());
                            operation.result = Some(summary.clone());
                            operation.error = None;
                            operation.attempts = attempt;
                        }
                        if registry
                            .active_operation_ids
                            .get(&scope_key_for_task)
                            .copied()
                            == Some(operation_id)
                        {
                            registry.active_operation_ids.remove(&scope_key_for_task);
                        }
                    }
                    if summary.imported_items > 0 {
                        let _ = event_tx.send(crate::state::ServerEvent::LibraryChanged {
                            items_added: Vec::new(),
                            items_updated: Vec::new(),
                            items_removed: Vec::new(),
                        });
                    }
                    tracing::info!(
                        operation_id = %operation_id,
                        trigger = %trigger,
                        attempt,
                        libraries = summary.libraries,
                        scanned_files = summary.scanned_files,
                        imported_items = summary.imported_items,
                        "后台媒体库扫描完成"
                    );
                    break;
                }
                ScanAttemptOutcome::Completed(Err(AppError::Sqlx(error)))
                    if attempt < MAX_ATTEMPTS =>
                {
                    {
                        let mut registry = scan_registry().write().await;
                        if let Some(operation) = registry.operations.get_mut(&operation_id) {
                            operation.status = "Queued";
                            operation.error = Some(error.to_string());
                            operation.attempts = attempt;
                            operation.scan_rate_per_sec = 0.0;
                        }
                    }
                    tracing::warn!(
                        operation_id = %operation_id,
                        trigger = %trigger,
                        attempt,
                        max_attempts = MAX_ATTEMPTS,
                        error = %error,
                        "后台媒体库扫描遇到数据库错误，将延迟重试"
                    );
                    attempt += 1;
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                ScanAttemptOutcome::Completed(Err(error)) => {
                    {
                        let mut registry = scan_registry().write().await;
                        if let Some(operation) = registry.operations.get_mut(&operation_id) {
                            operation.status = "Failed";
                            operation.progress = 100.0;
                            operation.phase = "Failed".to_string();
                            operation.scan_rate_per_sec = 0.0;
                            operation.completed_at = Some(Utc::now());
                            operation.error = Some(error.to_string());
                            operation.attempts = attempt;
                        }
                        if registry
                            .active_operation_ids
                            .get(&scope_key_for_task)
                            .copied()
                            == Some(operation_id)
                        {
                            registry.active_operation_ids.remove(&scope_key_for_task);
                        }
                    }
                    tracing::error!(
                        operation_id = %operation_id,
                        trigger = %trigger,
                        attempt,
                        max_attempts = MAX_ATTEMPTS,
                        error = %error,
                        "后台媒体库扫描失败"
                    );
                    break;
                }
            }
        }
    });
    Ok(operation_id)
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
        let _ = enqueue_library_scan(&state, "create_library", None).await?;
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
        let _ = enqueue_library_scan(&state, "delete_library", None).await?;
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
    crate::webhooks::dispatch(
        &state,
        crate::webhooks::events::LIBRARY_NEW,
        serde_json::json!({
            "Library": {
                "Name":           name,
                "CollectionType": collection_type,
                "Locations":      paths,
            }
        }),
    );
    if refresh_library {
        let _ = enqueue_library_scan(&state, "add_virtual_folder", None).await?;
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
        let _ = enqueue_library_scan(&state, "remove_virtual_folder", None).await?;
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
        let _ = enqueue_library_scan(&state, "add_media_path", None).await?;
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
        let _ = enqueue_library_scan(&state, "update_media_path", None).await?;
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
        let _ = enqueue_library_scan(&state, "remove_media_path", None).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn scan_libraries(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ScanLibrariesQuery>,
) -> Result<Response, AppError> {
    auth::require_admin(&session)?;
    if query.wait_for_completion.unwrap_or(false) {
        let db_sem = Some(state.scan_db_semaphore.clone());
        let summary = if let Some(library_id) = query.library_id {
            incremental_update_library(&state, library_id, None, db_sem).await?
        } else {
            incremental_update_all_libraries(&state, None, db_sem).await?
        };
        return Ok((StatusCode::OK, Json(summary)).into_response());
    }

    let operation_id = enqueue_library_scan(&state, "manual_scan", query.library_id).await?;
    let operation = {
        let registry = scan_registry().read().await;
        registry
            .operations
            .get(&operation_id)
            .cloned()
            .ok_or_else(|| AppError::Internal("扫描任务状态创建失败".to_string()))?
    };
    let monitor_url = operation.monitor_url();
    let mut response = (
        StatusCode::ACCEPTED,
        Json(ScanQueuedResponse {
            queued: true,
            message: "媒体库扫描任务已加入队列".to_string(),
            operation: operation.to_dto(),
        }),
    )
        .into_response();
    if let Ok(location) = HeaderValue::from_str(&monitor_url) {
        response.headers_mut().insert(header::LOCATION, location);
    }
    response
        .headers_mut()
        .insert(header::RETRY_AFTER, HeaderValue::from_static("3"));
    Ok(response)
}

async fn refresh_libraries(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let _ = enqueue_library_scan(&state, "refresh_libraries", None).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_scan_operations(
    session: AuthSession,
    Query(query): Query<ScanOperationsQuery>,
) -> Result<Json<Vec<ScanOperationDto>>, AppError> {
    auth::require_admin(&session)?;
    let limit = query.limit.unwrap_or(20).clamp(1, 200) as usize;
    let registry = scan_registry().read().await;
    let operations = registry
        .operations
        .values()
        .rev()
        .take(limit)
        .map(ScanOperationState::to_dto)
        .collect();
    Ok(Json(operations))
}

async fn get_scan_operation(
    session: AuthSession,
    Path(operation_id): Path<Uuid>,
) -> Result<Response, AppError> {
    auth::require_admin(&session)?;
    let registry = scan_registry().read().await;
    let operation = registry
        .operations
        .get(&operation_id)
        .ok_or_else(|| AppError::NotFound("扫描任务不存在".to_string()))?
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
            .insert(header::RETRY_AFTER, HeaderValue::from_static("3"));
    }
    Ok(response)
}

async fn cancel_scan_operation(
    session: AuthSession,
    Path(operation_id): Path<Uuid>,
) -> Result<Json<ScanOperationDto>, AppError> {
    auth::require_admin(&session)?;
    let mut registry = scan_registry().write().await;
    let mut clear_active = false;
    let dto = {
        let operation = registry
            .operations
            .get_mut(&operation_id)
            .ok_or_else(|| AppError::NotFound("扫描任务不存在".to_string()))?;
        if operation.is_done() {
            operation.to_dto()
        } else if operation.status == "Queued" {
            operation.status = "Cancelled";
            operation.progress = 100.0;
            operation.scan_rate_per_sec = 0.0;
            operation.completed_at = Some(Utc::now());
            clear_active = true;
            operation.to_dto()
        } else {
            operation.cancel_requested = true;
            operation.status = "Cancelling";
            operation.to_dto()
        }
    };
    if clear_active {
        let scope_key = dto.scope_key.clone();
        if registry.active_operation_ids.get(&scope_key).copied() == Some(operation_id) {
            registry.active_operation_ids.remove(&scope_key);
        }
    }
    Ok(Json(dto))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LibraryMediaUpdateInfo {
    #[serde(default, alias = "path")]
    path: String,
    #[serde(default, alias = "updateType")]
    update_type: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LibraryMediaUpdatedRequest {
    #[serde(default, alias = "updates")]
    updates: Vec<LibraryMediaUpdateInfo>,
}

fn normalize_match_path(path: &str) -> String {
    let mut normalized = path.trim().replace('\\', "/");
    while normalized.ends_with('/') {
        normalized.pop();
    }
    #[cfg(windows)]
    {
        normalized = normalized.to_ascii_lowercase();
    }
    normalized
}

fn path_matches_library(update_path: &str, library_path: &str) -> bool {
    let update = normalize_match_path(update_path);
    let library = normalize_match_path(library_path);
    if update.is_empty() || library.is_empty() {
        return false;
    }
    update == library
        || update.starts_with(&(library.clone() + "/"))
        || library.starts_with(&(update + "/"))
}

async fn resolve_libraries_for_media_updates(
    state: &AppState,
    updates: &[LibraryMediaUpdateInfo],
) -> Result<Vec<Uuid>, AppError> {
    let libraries = repository::list_libraries(&state.pool).await?;
    let mut target_ids = BTreeSet::new();
    for update in updates {
        let update_path = update.path.trim();
        if update_path.is_empty() {
            continue;
        }
        for library in &libraries {
            let _update_type = update.update_type.trim();
            let matches = repository::library_paths(library)
                .iter()
                .any(|library_path| path_matches_library(update_path, library_path));
            if matches {
                target_ids.insert(library.id);
            }
        }
    }
    Ok(target_ids.into_iter().collect())
}

async fn list_library_ids_by_collection_type(
    state: &AppState,
    collection_type: &str,
) -> Result<Vec<Uuid>, AppError> {
    let libraries = repository::list_libraries(&state.pool).await?;
    Ok(libraries
        .into_iter()
        .filter(|library| {
            library
                .collection_type
                .eq_ignore_ascii_case(collection_type)
        })
        .map(|library| library.id)
        .collect())
}

async fn library_notify(
    session: AuthSession,
    State(state): State<AppState>,
    uri: OriginalUri,
    payload: Option<Json<LibraryMediaUpdatedRequest>>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let request_path = uri.0.path().to_ascii_lowercase();
    let mut target_library_ids = if request_path.ends_with("/library/movies/added")
        || request_path.ends_with("/library/movies/updated")
    {
        list_library_ids_by_collection_type(&state, "movies").await?
    } else if request_path.ends_with("/library/series/added")
        || request_path.ends_with("/library/series/updated")
    {
        list_library_ids_by_collection_type(&state, "tvshows").await?
    } else {
        let updates = payload.map(|json| json.0.updates).unwrap_or_default();
        resolve_libraries_for_media_updates(&state, &updates).await?
    };
    target_library_ids.sort_unstable();
    target_library_ids.dedup();

    if target_library_ids.is_empty() {
        // 没提供变更路径，或路径无法映射到具体媒体库时，回退为全库扫描。
        let _ = enqueue_library_scan(&state, "library_media_updated_fallback", None).await?;
    } else {
        for library_id in target_library_ids {
            let _ = enqueue_library_scan(&state, "library_media_updated", Some(library_id)).await?;
        }
    }
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
    #[serde(default, rename = "IncludeFiles", alias = "includeFiles", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    include_files: Option<bool>,
    #[serde(default, rename = "IncludeDirectories", alias = "includeDirectories", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
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

#[derive(Debug, Deserialize)]
struct ScanLibrariesQuery {
    #[serde(
        default,
        rename = "WaitForCompletion",
        alias = "waitForCompletion",
        alias = "wait_for_completion",
        deserialize_with = "crate::models::deserialize_option_bool_lenient"
    )]
    wait_for_completion: Option<bool>,
    #[serde(
        default,
        rename = "LibraryId",
        alias = "libraryId",
        alias = "library_id"
    )]
    library_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct ScanOperationsQuery {
    #[serde(default, rename = "Limit", alias = "limit")]
    limit: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ScanQueuedResponse {
    queued: bool,
    message: String,
    operation: ScanOperationDto,
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
