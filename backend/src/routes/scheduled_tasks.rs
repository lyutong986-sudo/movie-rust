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
            description: "对缺少或过期元数据的条目调用 TMDB 等外部源进行刷新。",
            category: "Metadata",
            default_triggers: vec![TriggerInfo {
                trigger_type: "DailyTrigger",
                time_of_day_ticks: Some(3 * 3600 * 10_000_000),
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

    Ok(TaskRuntimeState {
        status: running,
        progress,
        last_end_time: last_end,
        last_execution_result: last_exec,
        triggers,
    })
}

struct TaskRuntimeState {
    status: String,
    progress: Option<f64>,
    last_end_time: Option<DateTime<Utc>>,
    last_execution_result: Option<Value>,
    triggers: Option<Value>,
}

fn descriptor_to_value(desc: &TaskDescriptor, state: &TaskRuntimeState) -> Value {
    let triggers_value = state.triggers.clone().unwrap_or_else(|| {
        Value::Array(desc.default_triggers.iter().map(|t| t.to_value()).collect())
    });

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

async fn run_task(state: &AppState, task_id: &str) -> Result<(), AppError> {
    match task_id {
        "library-scan" => {
            crate::routes::admin::incremental_update_all_libraries(
                state,
                None,
                Some(state.scan_db_semaphore.clone()),
            )
            .await?;
            set_progress(&state.pool, task_id, 100.0).await;
            Ok(())
        }
        "metadata-refresh" => {
            let candidates = repository::list_media_items(
                &state.pool,
                repository::ItemListOptions {
                    include_types: vec!["Movie".into(), "Series".into()],
                    recursive: true,
                    start_index: 0,
                    limit: 200,
                    ..Default::default()
                },
            )
            .await?;
            let total = candidates.items.len().max(1);
            for (i, item) in candidates.items.iter().enumerate() {
                let pct = (i as f64 / total as f64) * 100.0;
                set_progress(&state.pool, task_id, pct).await;
                let _ = crate::routes::items::do_refresh_item_metadata(state, item.id).await;
            }
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
            "cleanup-transcodes",
            "cleanup-activity-log",
        ] {
            assert!(ids.contains(&expected), "missing task {expected}");
        }
    }
}
