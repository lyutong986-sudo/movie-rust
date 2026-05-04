use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Datelike, NaiveTime, Timelike, Utc, Weekday};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::{
    auth::{require_admin, AuthSession},
    error::AppError,
    repository,
    scanner,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ScheduledTasks", get(list_tasks))
        .route("/scheduledtasks", get(list_tasks))
        .route("/ScheduledTasks/{task_id}", get(get_task))
        .route("/scheduledtasks/{task_id}", get(get_task))
        .route("/ScheduledTasks/{task_id}/Triggers", post(update_triggers).put(update_triggers))
        .route("/scheduledtasks/{task_id}/triggers", post(update_triggers).put(update_triggers))
        .route("/ScheduledTasks/Running/{task_id}", post(start_task))
        .route("/scheduledtasks/running/{task_id}", post(start_task))
        .route(
            "/ScheduledTasks/Running/{task_id}/Cancel",
            post(cancel_task),
        )
        .route(
            "/scheduledtasks/running/{task_id}/cancel",
            post(cancel_task),
        )
        .route(
            "/ScheduledTasks/Running/{task_id}/Delete",
            post(cancel_task),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TaskListQuery {
    #[serde(default, alias = "isHidden", alias = "IsHidden", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    is_hidden: Option<bool>,
}

fn builtin_tasks() -> Vec<TaskDescriptor> {
    vec![
        TaskDescriptor {
            id: "library-scan",
            name: "媒体库扫描",
            description: "对所有媒体库执行增量更新（含增/改/删）：本地库扫描磁盘，远端 Emby 库通过 API 同步。",
            category: "Library",
            default_triggers: vec![TriggerInfo {
                trigger_type: "IntervalTrigger",
                interval_ticks: Some(30_000 * 10_000_000),
                ..TriggerInfo::default()
            }],
        },
        TaskDescriptor {
            id: "metadata-refresh",
            name: "元数据刷新",
            description: "对本地媒体库（Movie/Series/Season/Episode）的「缺失元数据」与「过期 7 天及以上」的条目，\
                调用 TMDB 等外部源补全/刷新；缺失元数据的条目即使是远端 Emby 条目也会被纳入兜底，\
                避免远端 Emby 与 TMDB 都没有的占位条目永远卡在空白状态；\
                有完整数据的远端 Emby 条目仍交由远端同步链路单独处理，\
                锁定元数据的条目跳过。",
            category: "Metadata",
            default_triggers: vec![TriggerInfo {
                trigger_type: "DailyTrigger",
                time_of_day_ticks: Some(3 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            }],
        },
        TaskDescriptor {
            id: "translation-fallback",
            name: "翻译兜底",
            description: "对所有未锁定的媒体条目（Movie/Series/Season/Episode）按「翻译兜底」设置批量调用\
                有道大模型翻译。已是目标语言的字段会被语种粗检测跳过，相同原文命中 \
                translation_cache 不会重复计费；可在 /settings/translation 调整字段开关 / 触发位 / 限流。\
                建议放在 metadata-refresh 之后跑（默认 04:00），先让 TMDB 把能补的中文补完。",
            category: "Metadata",
            default_triggers: vec![TriggerInfo {
                trigger_type: "DailyTrigger",
                time_of_day_ticks: Some(4 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            }],
        },
        TaskDescriptor {
            id: "cleanup-transcodes",
            name: "清理转码临时目录",
            description: "删除遗留的 HLS / 分片缓存文件。",
            category: "Maintenance",
            default_triggers: vec![TriggerInfo {
                trigger_type: "DailyTrigger",
                time_of_day_ticks: Some(5 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            }],
        },
        TaskDescriptor {
            id: "cleanup-activity-log",
            name: "清理活动日志",
            description: "删除超过保留天数的活动日志条目。",
            category: "Maintenance",
            default_triggers: vec![TriggerInfo {
                trigger_type: "IntervalTrigger",
                interval_ticks: Some(24 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            }],
        },
        TaskDescriptor {
            id: "cleanup-sessions",
            name: "清理过期会话",
            description: "删除已过期或 30 天无活动的会话，防止 sessions 表无限膨胀。",
            category: "Maintenance",
            default_triggers: vec![TriggerInfo {
                trigger_type: "IntervalTrigger",
                interval_ticks: Some(24 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            }],
        },
    ]
}

#[derive(Clone)]
struct TaskDescriptor {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: &'static str,
    default_triggers: Vec<TriggerInfo>,
}

#[derive(Clone, Default)]
struct TriggerInfo {
    trigger_type: &'static str,
    interval_ticks: Option<i64>,
    time_of_day_ticks: Option<i64>,
}

impl TriggerInfo {
    fn to_value(&self) -> Value {
        let mut v = json!({ "Type": self.trigger_type });
        if let Some(ticks) = self.interval_ticks {
            v["IntervalTicks"] = json!(ticks);
        }
        if let Some(ticks) = self.time_of_day_ticks {
            v["TimeOfDayTicks"] = json!(ticks);
        }
        v
    }
}

async fn read_state(pool: &sqlx::PgPool, task_id: &str) -> Result<TaskRuntimeState, AppError> {
    let running = repository::get_setting_value(pool, &format!("task:{task_id}:state"))
        .await?
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "Idle".to_string());

    let progress = repository::get_setting_value(pool, &format!("task:{task_id}:progress"))
        .await?
        .and_then(|v| v.as_f64());

    let last_end = repository::get_setting_value(pool, &format!("task:{task_id}:last_end"))
        .await?
        .and_then(|v| {
            v.as_str().and_then(|s| {
                DateTime::parse_from_rfc3339(s)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            })
        });

    let last_exec =
        repository::get_setting_value(pool, &format!("task:{task_id}:last_exec")).await?;

    let triggers = repository::get_setting_value(pool, &format!("task:{task_id}:triggers")).await?;

    // PB49 (UX)：library-scan 实时阶段/计数细节，由 run_task 的轮询桥接写入。
    // 其他任务通常没这条记录，读出来 None 即可。
    let detail = repository::get_setting_value(pool, &format!("task:{task_id}:progress_detail"))
        .await?;

    Ok(TaskRuntimeState {
        status: running,
        progress,
        last_end_time: last_end,
        last_execution_result: last_exec,
        triggers,
        detail,
    })
}

struct TaskRuntimeState {
    status: String,
    progress: Option<f64>,
    last_end_time: Option<DateTime<Utc>>,
    last_execution_result: Option<Value>,
    triggers: Option<Value>,
    /// PB49 (UX)：实时阶段细节（phase / current_library / 计数）。
    /// 仅 library-scan 在运行中持续写入；其余任务为 None。
    detail: Option<Value>,
}

fn descriptor_to_value(desc: &TaskDescriptor, state: &TaskRuntimeState) -> Value {
    let triggers_value = state.triggers.clone().unwrap_or_else(|| {
        Value::Array(desc.default_triggers.iter().map(|t| t.to_value()).collect())
    });

    // PB49 (UX)：把 library-scan 桥接写入的实时细节展开到顶层字段，前端不用再去
    // 二次解析 progress_detail JSON。仅在 task 是 Running 状态时有意义；任务
    // 结束后 cleanup 会把这个键也清掉（见 spawn_task_execution 末尾）。
    let detail = state.detail.as_ref();
    let phase = detail
        .and_then(|d| d.get("Phase"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let current_library = detail
        .and_then(|d| d.get("CurrentLibrary"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let total_files = detail
        .and_then(|d| d.get("TotalFiles"))
        .and_then(|v| v.as_u64());
    let scanned_files = detail
        .and_then(|d| d.get("ScannedFiles"))
        .and_then(|v| v.as_u64());
    let imported_items = detail
        .and_then(|d| d.get("ImportedItems"))
        .and_then(|v| v.as_u64());
    let skipped_remote_strm = detail
        .and_then(|d| d.get("SkippedRemoteStrm"))
        .and_then(|v| v.as_u64());

    json!({
        "Name": desc.name,
        "Description": desc.description,
        "Category": desc.category,
        "Id": desc.id,
        "Key": desc.id,
        "State": state.status,
        "CurrentProgressPercentage": state.progress,
        "Triggers": triggers_value,
        "LastExecutionResult": state.last_execution_result,
        "IsHidden": false,
        "Phase": phase,
        "CurrentLibrary": current_library,
        "TotalFiles": total_files,
        "ScannedFiles": scanned_files,
        "ImportedItems": imported_items,
        "SkippedRemoteStrm": skipped_remote_strm,
    })
}

fn get_task_triggers(desc: &TaskDescriptor, state: &TaskRuntimeState) -> Vec<Value> {
    match &state.triggers {
        Some(Value::Array(arr)) => arr.clone(),
        _ => desc.default_triggers.iter().map(|t| t.to_value()).collect(),
    }
}

async fn list_tasks(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    require_admin(&session)?;
    let descriptors = builtin_tasks();
    let mut out = Vec::with_capacity(descriptors.len());
    for desc in descriptors {
        let runtime = read_state(&state.pool, desc.id).await?;
        let task_value = descriptor_to_value(&desc, &runtime);
        if let Some(hidden) = query.is_hidden {
            let is_hidden = task_value["IsHidden"].as_bool().unwrap_or(false);
            if is_hidden != hidden {
                continue;
            }
        }
        out.push(task_value);
    }
    Ok(Json(out))
}

async fn get_task(
    session: AuthSession,
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    require_admin(&session)?;
    let descriptors = builtin_tasks();
    let Some(desc) = descriptors
        .iter()
        .find(|d| d.id.eq_ignore_ascii_case(&task_id))
    else {
        return Err(AppError::NotFound(format!("任务不存在: {task_id}")));
    };
    let runtime = read_state(&state.pool, desc.id).await?;
    Ok(Json(descriptor_to_value(desc, &runtime)))
}

async fn update_triggers(
    session: AuthSession,
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    require_admin(&session)?;
    let descriptors = builtin_tasks();
    if !descriptors
        .iter()
        .any(|d| d.id.eq_ignore_ascii_case(&task_id))
    {
        return Err(AppError::NotFound(format!("任务不存在: {task_id}")));
    }
    repository::set_setting_value(
        &state.pool,
        &format!("task:{}:triggers", task_id.to_ascii_lowercase()),
        payload,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start_task(
    session: AuthSession,
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_admin(&session)?;
    let key = task_id.to_ascii_lowercase();
    let descriptors = builtin_tasks();
    let Some(desc) = descriptors.iter().find(|d| d.id.eq_ignore_ascii_case(&key)) else {
        return Err(AppError::NotFound(format!("任务不存在: {task_id}")));
    };

    {
        let tokens = state.task_tokens.read().await;
        if tokens.contains_key(desc.id) {
            return Ok(StatusCode::NO_CONTENT);
        }
    }

    spawn_task_execution(state.clone(), desc.id.to_string()).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn cancel_task(
    session: AuthSession,
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_admin(&session)?;
    let key = task_id.to_ascii_lowercase();
    {
        let tokens = state.task_tokens.read().await;
        if let Some(token) = tokens.get(&key) {
            token.cancel();
        }
    }
    repository::set_setting_value(
        &state.pool,
        &format!("task:{key}:state"),
        json!("Cancelling"),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn spawn_task_execution(state: AppState, task_id: String) {
    let cancel_token = CancellationToken::new();
    {
        let mut tokens = state.task_tokens.write().await;
        tokens.insert(task_id.clone(), cancel_token.clone());
    }

    let _ = repository::set_setting_value(
        &state.pool,
        &format!("task:{}:state", task_id),
        json!("Running"),
    )
    .await;
    let _ = repository::set_setting_value(
        &state.pool,
        &format!("task:{}:progress", task_id),
        json!(0.0),
    )
    .await;

    let pool = state.pool.clone();
    let state_clone = state.clone();
    let task_id_clone = task_id.clone();
    tokio::spawn(async move {
        let start = Utc::now();
        let result = tokio::select! {
            res = run_task(&state_clone, &task_id_clone) => res,
            _ = cancel_token.cancelled() => {
                Err(AppError::Internal("任务已被取消".to_string()))
            }
        };
        let end = Utc::now();
        let duration_ticks = (end.signed_duration_since(start).num_milliseconds() * 10_000).max(0);
        let (status, error) = match &result {
            Ok(_) => ("Completed", None),
            Err(err) => {
                let msg = err.to_string();
                if msg.contains("取消") {
                    ("Cancelled", Some(msg))
                } else {
                    ("Failed", Some(msg))
                }
            }
        };
        let _ = repository::set_setting_value(&pool, &format!("task:{task_id_clone}:state"), json!("Idle")).await;
        let _ = repository::set_setting_value(&pool, &format!("task:{task_id_clone}:progress"), json!(100.0)).await;
        // PB49 (UX)：清掉运行时桥接写入的实时细节，避免下一次 list_tasks 把
        // 上一轮残留的 phase 当成「正在跑」展示出来。
        let _ = repository::set_setting_value(
            &pool,
            &format!("task:{task_id_clone}:progress_detail"),
            Value::Null,
        )
        .await;
        let _ = repository::set_setting_value(
            &pool,
            &format!("task:{task_id_clone}:last_end"),
            json!(end.to_rfc3339()),
        )
        .await;
        let _ = repository::set_setting_value(
            &pool,
            &format!("task:{task_id_clone}:last_exec"),
            json!({
                "Status": status,
                "StartTime": start.to_rfc3339(),
                "EndTime": end.to_rfc3339(),
                "DurationTicks": duration_ticks,
                "ErrorMessage": error,
            }),
        )
        .await;
        {
            let mut tokens = state_clone.task_tokens.write().await;
            tokens.remove(&task_id_clone);
        }
    });
}

async fn set_progress(pool: &sqlx::PgPool, task_id: &str, pct: f64) {
    let _ =
        repository::set_setting_value(pool, &format!("task:{task_id}:progress"), json!(pct)).await;
}

/// PB52-Missing：判断一个 media_items 行是否「缺元数据」，需要 metadata-refresh
/// 任务即使在远端来源 / 7 天内动过的情况下也强制走一次刷新链路。
///
/// 判定标准（任一命中即视为缺失）：
/// - `overview` 为空 / 全空白 —— 最强烈的缺数据信号，远端 + TMDB 都没拉到东西。
/// - Movie / Series 的 `provider_ids` 里完全没有 TMDB id —— 仍是 filename-导入的
///   占位条目，没有 TMDB id 就走不了任何外部刷新链。
///   Episode / Season 不在此范围：它们的 TMDB id 是借父 Series 的，单条没 TMDB id
///   是常态，不算缺失。
///
/// 设计约束：宽松判，宁可让本地条目多刷一次，也不要让真正缺数据的远端条目
/// 永远卡在空白状态——这恰恰是用户报告的「有些媒体没有 tmdb 也没有远程媒体库」
/// 的根因。
fn item_has_missing_metadata(item: &crate::models::DbMediaItem) -> bool {
    let overview_empty = item
        .overview
        .as_deref()
        .map(str::trim)
        .map(str::is_empty)
        .unwrap_or(true);
    if overview_empty {
        return true;
    }
    let kind = item.item_type.as_str();
    let needs_tmdb_id =
        kind.eq_ignore_ascii_case("Movie") || kind.eq_ignore_ascii_case("Series");
    if !needs_tmdb_id {
        return false;
    }
    let has_tmdb = item
        .provider_ids
        .as_object()
        .map(|obj| {
            ["Tmdb", "TMDb", "tmdb"]
                .iter()
                .any(|key| obj.get(*key).is_some_and(|v: &Value| !v.is_null()))
        })
        .unwrap_or(false);
    !has_tmdb
}

async fn run_task(state: &AppState, task_id: &str) -> Result<(), AppError> {
    match task_id {
        "library-scan" => {
            // PB49 (UX)：之前这里 `progress=None`，导致 /settings/scheduled-tasks 页
            // 在远端 sync 跑了 30 分钟期间只有「Running 0%」一动不动。
            //
            // 现在创建一个 ScanProgress 句柄交给 incremental_update_all_libraries——
            // 该函数会把 phase 写到 ScanProgress（并通过 incremental_update_library 内
            // 的远端→scanner 桥接把 RemoteSync/FetchingRemoteIndex 等阶段也回填进来），
            // 我们再起一条 1s 轮询任务把 ScanProgress.snapshot 写到 settings 表，
            // 前端 /settings/scheduled-tasks 直接读这条记录就能看到实时阶段+计数。
            //
            // 桥接任务在主任务结束（无论 Ok/Err）后立刻 abort，避免泄漏；
            // 然后 spawn_task_execution 末尾的 cleanup 会把 progress_detail 键删掉，
            // 防止前端在任务结束后继续看到陈旧的 phase。
            let scan_progress = scanner::ScanProgress::new();
            let bridge_task_id = task_id.to_string();
            let bridge_pool = state.pool.clone();
            let bridge_progress = scan_progress.clone();
            let bridge_handle = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    let snap = bridge_progress.snapshot().await;
                    let detail = json!({
                        "Phase": snap.phase,
                        "CurrentLibrary": snap.current_library,
                        "TotalFiles": snap.total_files,
                        "ScannedFiles": snap.scanned_files,
                        "ImportedItems": snap.imported_items,
                        "SkippedRemoteStrm": snap.skipped_remote_strm,
                        "Percent": snap.percent,
                    });
                    let _ = repository::set_setting_value(
                        &bridge_pool,
                        &format!("task:{bridge_task_id}:progress_detail"),
                        detail,
                    )
                    .await;
                    if snap.percent > 0.0 {
                        let _ = repository::set_setting_value(
                            &bridge_pool,
                            &format!("task:{bridge_task_id}:progress"),
                            json!(snap.percent),
                        )
                        .await;
                    }
                }
            });
            let result = crate::routes::admin::incremental_update_all_libraries(
                state,
                Some(scan_progress),
                Some(state.scan_db_semaphore.clone()),
                true,
            )
            .await;
            bridge_handle.abort();
            result?;
            set_progress(&state.pool, task_id, 100.0).await;
            Ok(())
        }
        "metadata-refresh" => {
            // PB51-Refresh + PB52-Missing：metadata-refresh 调度任务。
            //
            // 历史包袱（已修复）：
            //   1) `limit: 200`，不分页 → 大库（如 549K 条）每天只摸前 200 条。
            //   2) 仅 Movie/Series → Episode/Season 元数据漂移走不到这一刷。
            //   3) 不区分远端与本地 → 远端条目在 03:00 集中撞上游 Emby，
            //      与 AutoInterval / GlobalScheduled 同步叠加 = 触发封号。
            //
            // 当前链路：
            //   - 全量分页：稳定按 start_index 步进，不依赖 SQL stale 过滤。
            //   - 包含 Movie / Series / Season / Episode 四类。
            //   - 跳过 lock_data：尊重用户「锁定元数据」语义（始终生效）。
            //   - 双轨判定：
            //       * 「缺失元数据」(item_has_missing_metadata) → 不论来源、不论 staleness
            //         都进 do_refresh_item_metadata 走「远端 → TMDB → 翻译」三段式
            //         兜底链路，让远端没数据 + 没 TMDB id 的占位条目能被补全。
            //       * 「数据完整 + 远端来源」→ 跳过（交回 remote-emby 同步链路）。
            //       * 「数据完整 + 本地 + 已新（< STALE_DAYS 内动过）」→ 跳过。
            //       * 其余（数据完整 + 本地 + stale）→ 进刷新。
            //   - 单条失败仅 warn，不中断整个任务。
            //   - 翻译兜底通过 do_refresh_item_metadata_with 内部接入点自动接入，
            //     本任务无需额外处理。
            const PAGE_SIZE: i64 = 200;
            const STALE_DAYS: i64 = 7;
            let stale_cutoff = Utc::now() - chrono::Duration::days(STALE_DAYS);
            let mut start_index: i64 = 0;
            let mut total_count: i64 = 0;
            let mut processed: i64 = 0;
            let mut refreshed: i64 = 0;
            let mut refreshed_missing: i64 = 0;
            let mut skipped_remote: i64 = 0;
            let mut skipped_locked: i64 = 0;
            let mut skipped_fresh: i64 = 0;
            let mut errored: i64 = 0;
            loop {
                let page = repository::list_media_items(
                    &state.pool,
                    repository::ItemListOptions {
                        include_types: vec![
                            "Movie".into(),
                            "Series".into(),
                            "Season".into(),
                            "Episode".into(),
                        ],
                        recursive: true,
                        start_index,
                        limit: PAGE_SIZE,
                        // 仅在第一页拉一次 total，后续页省掉 fast_count 子查询
                        enable_total_record_count: start_index == 0,
                        ..Default::default()
                    },
                )
                .await?;
                if start_index == 0 {
                    total_count = page.total_record_count.max(1);
                    tracing::info!(
                        total_count,
                        stale_cutoff = %stale_cutoff,
                        "metadata-refresh: 开始全量分页刷新"
                    );
                }
                if page.items.is_empty() {
                    break;
                }
                let page_len = page.items.len() as i64;
                for item in &page.items {
                    processed += 1;
                    let pct = (processed as f64 / total_count as f64).min(1.0) * 100.0;
                    if processed % 200 == 0 || processed == total_count {
                        set_progress(&state.pool, task_id, pct).await;
                    }
                    if item.lock_data {
                        skipped_locked += 1;
                        continue;
                    }
                    let missing = item_has_missing_metadata(item);
                    let is_remote =
                        crate::remote_emby::remote_marker_for_db_item(item).is_some();
                    if is_remote && !missing {
                        skipped_remote += 1;
                        continue;
                    }
                    if !is_remote && !missing && item.date_modified > stale_cutoff {
                        skipped_fresh += 1;
                        continue;
                    }
                    set_progress(&state.pool, task_id, pct).await;
                    if let Err(err) =
                        crate::routes::items::do_refresh_item_metadata(state, item.id).await
                    {
                        errored += 1;
                        tracing::warn!(
                            item_id = %item.id,
                            name = %item.name,
                            missing,
                            is_remote,
                            error = %err,
                            "metadata-refresh: 单条刷新失败，继续下一条"
                        );
                    } else {
                        refreshed += 1;
                        if missing {
                            refreshed_missing += 1;
                        }
                    }
                }
                if page_len < PAGE_SIZE {
                    break;
                }
                start_index += PAGE_SIZE;
            }
            tracing::info!(
                processed,
                refreshed,
                refreshed_missing,
                skipped_remote,
                skipped_locked,
                skipped_fresh,
                errored,
                "metadata-refresh: 任务完成"
            );
            set_progress(&state.pool, task_id, 100.0).await;
            Ok(())
        }
        "translation-fallback" => {
            // PB52：独立的翻译兜底调度任务。与 metadata-refresh 解耦后的好处：
            //   - 单独的进度条 / 取消按钮，UI 可见；
            //   - 单独的频率（默认 04:00，紧跟 metadata-refresh 完成之后）；
            //   - 用户可以单独关闭 / 重启它，不影响 TMDB 刷新链路；
            //   - 翻译 settings 关闭时本任务直接 noop 退出，进度跳到 100%。
            //
            // 与其它接入点（手动刷新 / 远端同步 / metadata-refresh 末尾的 inline
            // 翻译）共享同一个 translation_cache，所以这里大量条目会通过缓存命中
            // 秒回，不会真打 Youdao。
            let translator_handle = match state.translator.as_ref() {
                Some(t) => t,
                None => {
                    tracing::info!("translation-fallback: 翻译服务未初始化，跳过");
                    set_progress(&state.pool, task_id, 100.0).await;
                    return Ok(());
                }
            };
            let settings = translator_handle.current().await;
            if !settings.ready() {
                tracing::info!(
                    enabled = settings.enabled,
                    has_app_key = !settings.app_key.is_empty(),
                    has_app_secret = !settings.app_secret.is_empty(),
                    "translation-fallback: 翻译未启用或缺 appKey/appSecret，跳过"
                );
                set_progress(&state.pool, task_id, 100.0).await;
                return Ok(());
            }
            if !settings.trigger_scheduled_task {
                tracing::info!(
                    "translation-fallback: settings.trigger_scheduled_task=false，跳过"
                );
                set_progress(&state.pool, task_id, 100.0).await;
                return Ok(());
            }

            // PB52-Translate：审计发现旧实现有两个致命 bug 让 04:00 调度任务 3 秒就 Completed：
            //   1) `list_media_items(..)..Default::default()` 隐含 `group_items_into_collections=true`，
            //      首页 200 条原始记录会被 `deduplicate_media_items` 按 provider id 折叠
            //      成 N 条。当 N < PAGE_SIZE 时 `if page_len < PAGE_SIZE { break }` 会
            //      在「我们以为还有数据」的情况下直接退出，第二页都不会拉。和 hills
            //      客户端 60 条 bug 是同一个根因。
            //   2) ORDER BY sort_name + OFFSET 翻页：sort_name 不是稳定排序键（同名
            //      条目顺序在不同 OFFSET 之间漂移），多页之间会出现遗漏 / 重复。OFFSET
            //      在百万级条目时也会越翻越慢。
            //
            // 修复：抛掉 `list_media_items`，直接走 SQL，按 id 主键升序游标分页
            // (`AND id > $cursor ORDER BY id ASC LIMIT $page`)。`media_items.id` 是
            // PK，用主键索引扫描既稳定又快；`lock_data=false` 直接下推到 SQL，省掉
            // 客户端跳过逻辑。
            const PAGE_SIZE: i64 = 200;
            let total_count: i64 = sqlx::query_scalar(
                r#"
                SELECT COUNT(*)::bigint FROM media_items
                WHERE item_type IN ('Movie','Series','Season','Episode')
                  AND lock_data = false
                "#,
            )
            .fetch_one(&state.pool)
            .await?;
            let total_for_progress = total_count.max(1);
            tracing::info!(
                total_count,
                target_lang = %settings.target_lang,
                "translation-fallback: 开始全量翻译兜底"
            );

            let mut cursor: Option<uuid::Uuid> = None;
            let mut processed: i64 = 0;
            let mut translated: i64 = 0;
            let mut errored: i64 = 0;
            loop {
                let rows: Vec<(uuid::Uuid, String)> = sqlx::query_as(
                    r#"
                    SELECT id, name FROM media_items
                    WHERE item_type IN ('Movie','Series','Season','Episode')
                      AND lock_data = false
                      AND ($1::uuid IS NULL OR id > $1)
                    ORDER BY id ASC
                    LIMIT $2
                    "#,
                )
                .bind(cursor)
                .bind(PAGE_SIZE)
                .fetch_all(&state.pool)
                .await?;
                if rows.is_empty() {
                    break;
                }
                let page_len = rows.len() as i64;
                for (item_id, item_name) in &rows {
                    processed += 1;
                    let pct = (processed as f64 / total_for_progress as f64).min(1.0) * 100.0;
                    set_progress(&state.pool, task_id, pct).await;
                    match crate::metadata::translator::translate_item_text_fields(
                        state,
                        *item_id,
                        crate::metadata::translator::TranslationTrigger::ScheduledTask,
                    )
                    .await
                    {
                        Ok(true) => translated += 1,
                        Ok(false) => {}
                        Err(err) => {
                            errored += 1;
                            tracing::warn!(
                                item_id = %item_id,
                                name = %item_name,
                                error = %err,
                                "translation-fallback: 单条翻译失败，继续下一条"
                            );
                        }
                    }
                }
                cursor = rows.last().map(|(id, _)| *id);
                if page_len < PAGE_SIZE {
                    break;
                }
            }
            tracing::info!(
                processed,
                translated,
                errored,
                target_lang = %settings.target_lang,
                "translation-fallback: 任务完成"
            );
            set_progress(&state.pool, task_id, 100.0).await;
            Ok(())
        }
        "cleanup-transcodes" => {
            let dir = &state.config.transcode_dir;
            if let Ok(mut read) = tokio::fs::read_dir(dir).await {
                while let Ok(Some(entry)) = read.next_entry().await {
                    let path = entry.path();
                    if path.is_dir() {
                        let _ = tokio::fs::remove_dir_all(&path).await;
                    } else {
                        let _ = tokio::fs::remove_file(&path).await;
                    }
                }
            }
            set_progress(&state.pool, task_id, 100.0).await;
            Ok(())
        }
        "cleanup-activity-log" => {
            sqlx::query(
                "DELETE FROM playback_events WHERE created_at < (now() - interval '30 days')",
            )
            .execute(&state.pool)
            .await?;
            set_progress(&state.pool, task_id, 100.0).await;
            Ok(())
        }
        "cleanup-sessions" => {
            let deleted = repository::cleanup_stale_sessions(&state.pool).await?;
            if deleted > 0 {
                tracing::info!(deleted, "cleanup-sessions: 已清理过期/不活跃会话");
            }
            set_progress(&state.pool, task_id, 100.0).await;
            Ok(())
        }
        _ => Err(AppError::NotFound(format!("未知任务: {task_id}"))),
    }
}

// ────────────────────────────────────────────────────────────────────────
// 定时调度器
// ────────────────────────────────────────────────────────────────────────

pub async fn run_scheduler(state: AppState) {
    tracing::info!("计划任务调度器已启动");

    // StartupTrigger: 启动后 5 秒执行
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    fire_startup_triggers(&state).await;

    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;
        check_and_fire_triggers(&state).await;
        // Emby/Jellyfin: 空闲会话检测 — 清理超过 5 分钟无心跳的 NowPlaying 条目
        match crate::repository::cleanup_stale_play_queue(&state.pool, 5).await {
            Ok(n) if n > 0 => {
                tracing::info!(cleaned = n, "清理了 {n} 条超时的播放队列条目");
            }
            Err(err) => {
                tracing::warn!(?err, "清理超时播放队列失败");
            }
            _ => {}
        }
    }
}

async fn fire_startup_triggers(state: &AppState) {
    let descriptors = builtin_tasks();
    for desc in &descriptors {
        let runtime = match read_state(&state.pool, desc.id).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        let triggers = get_task_triggers(desc, &runtime);
        let has_startup = triggers.iter().any(|t| {
            t.get("Type")
                .and_then(|v| v.as_str())
                .map_or(false, |s| s == "StartupTrigger")
        });
        if has_startup && runtime.status == "Idle" {
            tracing::info!(task = desc.id, "StartupTrigger: 启动时触发任务");
            spawn_task_execution(state.clone(), desc.id.to_string()).await;
        }
    }
}

async fn check_and_fire_triggers(state: &AppState) {
    let descriptors = builtin_tasks();
    let now = Utc::now();

    for desc in &descriptors {
        let runtime = match read_state(&state.pool, desc.id).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        if runtime.status != "Idle" {
            continue;
        }

        let triggers = get_task_triggers(desc, &runtime);
        let should_fire = triggers.iter().any(|trigger| {
            let ttype = trigger
                .get("Type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            match ttype {
                "IntervalTrigger" => {
                    let interval_ticks = trigger
                        .get("IntervalTicks")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if interval_ticks <= 0 {
                        return false;
                    }
                    let interval_secs = interval_ticks / 10_000_000;
                    let last_end = runtime.last_end_time.unwrap_or(DateTime::UNIX_EPOCH);
                    let elapsed = now.signed_duration_since(last_end).num_seconds();
                    elapsed >= interval_secs
                }
                "DailyTrigger" => {
                    let time_ticks = trigger
                        .get("TimeOfDayTicks")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let target_secs = (time_ticks / 10_000_000) as u32;
                    let target_time = NaiveTime::from_num_seconds_from_midnight_opt(
                        target_secs.min(86399),
                        0,
                    )
                    .unwrap_or_default();
                    let now_time = now.time();
                    let diff_secs =
                        (now_time.num_seconds_from_midnight() as i64) - (target_time.num_seconds_from_midnight() as i64);
                    if !(0..=120).contains(&diff_secs) {
                        return false;
                    }
                    let last_end = runtime.last_end_time.unwrap_or(DateTime::UNIX_EPOCH);
                    now.signed_duration_since(last_end).num_hours() >= 12
                }
                "WeeklyTrigger" => {
                    let day_str = trigger
                        .get("DayOfWeek")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Monday");
                    let target_weekday = match day_str {
                        "Sunday" => Weekday::Sun,
                        "Monday" => Weekday::Mon,
                        "Tuesday" => Weekday::Tue,
                        "Wednesday" => Weekday::Wed,
                        "Thursday" => Weekday::Thu,
                        "Friday" => Weekday::Fri,
                        "Saturday" => Weekday::Sat,
                        _ => Weekday::Mon,
                    };
                    if now.weekday() != target_weekday {
                        return false;
                    }
                    let time_ticks = trigger
                        .get("TimeOfDayTicks")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    let target_secs = (time_ticks / 10_000_000) as u32;
                    let target_time = NaiveTime::from_num_seconds_from_midnight_opt(
                        target_secs.min(86399),
                        0,
                    )
                    .unwrap_or_default();
                    let now_time = now.time();
                    let diff_secs =
                        (now_time.num_seconds_from_midnight() as i64) - (target_time.num_seconds_from_midnight() as i64);
                    if !(0..=120).contains(&diff_secs) {
                        return false;
                    }
                    let last_end = runtime.last_end_time.unwrap_or(DateTime::UNIX_EPOCH);
                    now.signed_duration_since(last_end).num_hours() >= 36
                }
                _ => false,
            }
        });

        if should_fire {
            tracing::info!(task = desc.id, "调度器触发任务执行");
            spawn_task_execution(state.clone(), desc.id.to_string()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduled_tasks_router_builds_without_conflicts() {
        let _ = router();
    }

    #[test]
    fn builtin_tasks_registers_expected_categories() {
        let ids: Vec<&str> = builtin_tasks().into_iter().map(|t| t.id).collect();
        for expected in [
            "library-scan",
            "metadata-refresh",
            "translation-fallback",
            "cleanup-transcodes",
            "cleanup-activity-log",
            "cleanup-sessions",
        ] {
            assert!(ids.contains(&expected), "missing task {expected}");
        }
    }
}
