use axum::{
    extract::{Path, Query, State},
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
        .route("/ScheduledTasks", get(list_tasks))
        .route("/scheduledtasks", get(list_tasks))
        .route("/ScheduledTasks/{task_id}", get(get_task))
        .route("/scheduledtasks/{task_id}", get(get_task))
        .route(
            "/ScheduledTasks/{task_id}/Triggers",
            post(update_triggers),
        )
        .route(
            "/scheduledtasks/{task_id}/triggers",
            post(update_triggers),
        )
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TaskListQuery {
    #[serde(default, alias = "isHidden", alias = "IsHidden")]
    is_hidden: Option<bool>,
}

/// 内置可用的后台任务清单。目前以元数据刷新 / 媒体库扫描为主，
/// 客户端的任务面板可直接显示这些。
fn builtin_tasks() -> Vec<TaskDescriptor> {
    vec![
        TaskDescriptor {
            id: "library-scan",
            name: "媒体库扫描",
            description: "扫描所有媒体库目录，发现并录入新的媒体文件。",
            category: "Library",
            default_trigger: TriggerInfo {
                trigger_type: "IntervalTrigger",
                interval_ticks: Some(30_000 * 10_000_000),
                ..TriggerInfo::default()
            },
        },
        TaskDescriptor {
            id: "metadata-refresh",
            name: "元数据刷新",
            description: "对缺少或过期元数据的条目调用 TMDB 等外部源进行刷新。",
            category: "Metadata",
            default_trigger: TriggerInfo {
                trigger_type: "DailyTrigger",
                time_of_day_ticks: Some(3 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            },
        },
        TaskDescriptor {
            id: "cleanup-transcodes",
            name: "清理转码临时目录",
            description: "删除遗留的 HLS / 分片缓存文件。",
            category: "Maintenance",
            default_trigger: TriggerInfo {
                trigger_type: "DailyTrigger",
                time_of_day_ticks: Some(5 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            },
        },
        TaskDescriptor {
            id: "cleanup-activity-log",
            name: "清理活动日志",
            description: "删除超过保留天数的活动日志条目。",
            category: "Maintenance",
            default_trigger: TriggerInfo {
                trigger_type: "IntervalTrigger",
                interval_ticks: Some(24 * 3600 * 10_000_000),
                ..TriggerInfo::default()
            },
        },
    ]
}

#[derive(Clone)]
struct TaskDescriptor {
    id: &'static str,
    name: &'static str,
    description: &'static str,
    category: &'static str,
    default_trigger: TriggerInfo,
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
        .and_then(|v| v.as_str().and_then(|s| DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))));

    let last_exec = repository::get_setting_value(pool, &format!("task:{task_id}:last_exec"))
        .await?;

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
    let triggers_value = state
        .triggers
        .clone()
        .unwrap_or_else(|| Value::Array(vec![desc.default_trigger.to_value()]));

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

async fn list_tasks(
    session: AuthSession,
    State(state): State<AppState>,
    Query(_query): Query<TaskListQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    require_admin(&session)?;
    let descriptors = builtin_tasks();
    let mut out = Vec::with_capacity(descriptors.len());
    for desc in descriptors {
        let runtime = read_state(&state.pool, desc.id).await?;
        out.push(descriptor_to_value(&desc, &runtime));
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
    let Some(desc) = descriptors.iter().find(|d| d.id.eq_ignore_ascii_case(&task_id)) else {
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
    if !descriptors.iter().any(|d| d.id.eq_ignore_ascii_case(&task_id)) {
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

    repository::set_setting_value(&state.pool, &format!("task:{}:state", desc.id), json!("Running"))
        .await?;
    repository::set_setting_value(&state.pool, &format!("task:{}:progress", desc.id), json!(0.0))
        .await?;

    // 按任务类型触发真实后台任务。不阻塞响应。
    let pool = state.pool.clone();
    let state_clone = state.clone();
    let task_id = desc.id.to_string();
    tokio::spawn(async move {
        let start = Utc::now();
        let result = run_task(&state_clone, &task_id).await;
        let end = Utc::now();
        let duration_ticks = (end.signed_duration_since(start).num_milliseconds() * 10_000).max(0);
        let (status, error) = match &result {
            Ok(_) => ("Completed", None),
            Err(err) => ("Failed", Some(err.to_string())),
        };
        let key_state = format!("task:{task_id}:state");
        let _ = repository::set_setting_value(&pool, &key_state, json!("Idle")).await;
        let _ = repository::set_setting_value(
            &pool,
            &format!("task:{task_id}:progress"),
            json!(100.0),
        )
        .await;
        let _ = repository::set_setting_value(
            &pool,
            &format!("task:{task_id}:last_end"),
            json!(end.to_rfc3339()),
        )
        .await;
        let _ = repository::set_setting_value(
            &pool,
            &format!("task:{task_id}:last_exec"),
            json!({
                "Status": status,
                "StartTime": start.to_rfc3339(),
                "EndTime": end.to_rfc3339(),
                "DurationTicks": duration_ticks,
                "ErrorMessage": error,
            }),
        )
        .await;
    });

    Ok(StatusCode::NO_CONTENT)
}

async fn cancel_task(
    session: AuthSession,
    State(state): State<AppState>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, AppError> {
    require_admin(&session)?;
    let key = task_id.to_ascii_lowercase();
    repository::set_setting_value(&state.pool, &format!("task:{key}:state"), json!("Cancelled"))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn run_task(state: &AppState, task_id: &str) -> Result<(), AppError> {
    match task_id {
        "library-scan" => {
            crate::scanner::scan_all_libraries(
                &state.pool,
                state.metadata_manager.clone(),
                &state.config,
                state.work_limiters.clone(),
            )
            .await?;
            Ok(())
        }
        "metadata-refresh" => {
            // 刷新若干需要更新的条目。命中 do_refresh_item_metadata。
            let candidates = repository::list_media_items(
                &state.pool,
                repository::ItemListOptions {
                    include_types: vec!["Movie".into(), "Series".into(), "Episode".into()],
                    recursive: true,
                    start_index: 0,
                    limit: 50,
                    ..Default::default()
                },
            )
            .await?;
            for item in candidates.items {
                let _ = crate::routes::items::do_refresh_item_metadata(state, item.id).await;
            }
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
            Ok(())
        }
        "cleanup-activity-log" => {
            sqlx::query("DELETE FROM playback_events WHERE created_at < (now() - interval '30 days')")
                .execute(&state.pool)
                .await?;
            Ok(())
        }
        _ => Err(AppError::NotFound(format!("未知任务: {task_id}"))),
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
