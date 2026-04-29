//! 出向 webhook 配置与测试 API（emby Webhooks 插件协议兼容层）。
//!
//! 路由：
//! - `GET    /Webhooks`                 列出所有 webhook
//! - `POST   /Webhooks`                 新建一个 webhook
//! - `GET    /Webhooks/{id}`            取单个
//! - `POST   /Webhooks/{id}`            覆盖更新（emby 老 plugin UI 习惯 POST）
//! - `DELETE /Webhooks/{id}`            删除
//! - `POST   /Webhooks/{id}/Test`       立即触发一条 `webhook.test` 事件
//! - `GET    /Notifications/Services`   兼容 emby 内置 GUI（列出"webhook"作为推送服务）
//! - `GET    /Notifications/Types`      兼容 emby 内置 GUI（列出可订阅事件类型）
//!
//! 全部需要 admin 权限。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{self, AuthSession},
    error::AppError,
    state::AppState,
    webhooks::{self, DbWebhook, UpsertWebhookRequest, WebhookDto},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Webhooks", get(list).post(create))
        .route("/Webhooks/{id}", get(detail).post(update).delete(remove))
        .route("/Webhooks/{id}/Test", post(test_one))
        .route("/Notifications/Services", get(notifications_services))
        .route("/Notifications/Types", get(notifications_types))
        // jellyfin Webhook plugin 风格的"读取插件配置"端点；让对接它的客户端不再 404。
        .route("/Webhook/Configuration", get(webhook_plugin_configuration))
}

async fn list(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<WebhookDto>>, AppError> {
    auth::require_admin(&session)?;
    let rows = sqlx::query_as::<_, DbWebhook>(
        r#"
        SELECT id, name, url, enabled, events, content_type, secret, headers_json,
               created_at, updated_at, last_status, last_error, last_triggered_at
          FROM webhooks
         ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows.into_iter().map(WebhookDto::from).collect()))
}

async fn detail(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WebhookDto>, AppError> {
    auth::require_admin(&session)?;
    let row = fetch_one(&state.pool, id).await?;
    Ok(Json(row.into()))
}

async fn create(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<UpsertWebhookRequest>,
) -> Result<(StatusCode, Json<WebhookDto>), AppError> {
    auth::require_admin(&session)?;
    validate(&payload)?;
    let id = Uuid::new_v4();
    let content_type = payload
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("application/json")
        .to_string();
    let headers = ensure_object(payload.headers);
    let row = sqlx::query_as::<_, DbWebhook>(
        r#"
        INSERT INTO webhooks (id, name, url, enabled, events, content_type, secret, headers_json)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, name, url, enabled, events, content_type, secret, headers_json,
                  created_at, updated_at, last_status, last_error, last_triggered_at
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.url)
    .bind(payload.enabled)
    .bind(&payload.events)
    .bind(&content_type)
    .bind(payload.secret.as_deref().filter(|s| !s.is_empty()))
    .bind(&headers)
    .fetch_one(&state.pool)
    .await?;
    Ok((StatusCode::CREATED, Json(row.into())))
}

async fn update(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpsertWebhookRequest>,
) -> Result<Json<WebhookDto>, AppError> {
    auth::require_admin(&session)?;
    validate(&payload)?;
    let _ = fetch_one(&state.pool, id).await?;
    let content_type = payload
        .content_type
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("application/json")
        .to_string();
    let headers = ensure_object(payload.headers);
    let row = sqlx::query_as::<_, DbWebhook>(
        r#"
        UPDATE webhooks
           SET name = $2, url = $3, enabled = $4, events = $5,
               content_type = $6,
               secret = COALESCE($7, secret),
               headers_json = $8,
               updated_at = now()
         WHERE id = $1
         RETURNING id, name, url, enabled, events, content_type, secret, headers_json,
                   created_at, updated_at, last_status, last_error, last_triggered_at
        "#,
    )
    .bind(id)
    .bind(&payload.name)
    .bind(&payload.url)
    .bind(payload.enabled)
    .bind(&payload.events)
    .bind(&content_type)
    .bind(payload.secret.as_deref().filter(|s| !s.is_empty()))
    .bind(&headers)
    .fetch_one(&state.pool)
    .await?;
    Ok(Json(row.into()))
}

async fn remove(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let result = sqlx::query("DELETE FROM webhooks WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("Webhook 不存在".into()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn test_one(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let hook = fetch_one(&state.pool, id).await?;
    if !hook.enabled {
        return Err(AppError::BadRequest("webhook 当前已禁用，请先启用".into()));
    }
    let payload = json!({
        "WebhookId": hook.id,
        "WebhookName": hook.name,
        "Test": true,
        "Message": "Movie Rust webhook test",
    });
    // **绕过订阅检查**：测试按钮要求即便 hook 没订阅 `webhook.test` 也直接送达，
    // 否则用户配置好新 hook 还没勾选事件时无法验证联通性。
    webhooks::dispatch_to_hook(state.pool.clone(), state.config.server_id, state.config.server_name.clone(), hook.clone(), "webhook.test".to_owned(), payload);
    Ok(Json(json!({
        "Status": "queued",
        "WebhookId": hook.id,
        "Message": "测试事件已加入派发队列，请稍后查看 last_status / last_error"
    })))
}

/// emby 的 `/Notifications/Services` 端点：用于内置 GUI 列出"已安装"的推送服务。
/// 我们把"webhook"封装为一个内置 service，让 emby 风格客户端能识别。
async fn notifications_services(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM webhooks WHERE enabled")
        .fetch_one(&state.pool)
        .await
        .unwrap_or(0);
    Ok(Json(json!([
        {
            "Name": "Webhook",
            "Id":   "webhook",
            "EnabledCount": count,
            "Description": "出向 HTTP webhook，兼容 emby Webhooks 插件协议",
            "ConfigurationUrl": "/web/index.html#/settings/integrations"
        }
    ])))
}

/// emby 的 `/Notifications/Types` 端点：可订阅事件类型清单。
/// 与 `webhooks::events::ALL` 同步，前端 UI 可以直接渲染成"事件订阅"复选框。
async fn notifications_types(_session: AuthSession) -> Result<Json<Value>, AppError> {
    let items: Vec<Value> = webhooks::events::ALL
        .iter()
        .map(|name| {
            json!({
                "Type": name,
                "Name": name,
                "Category": event_category(name),
                "IsBasedOnUserEvent": is_user_event(name),
            })
        })
        .collect();
    Ok(Json(Value::Array(items)))
}

fn event_category(name: &str) -> &'static str {
    if name.starts_with("playback.") || name == "session.start" {
        "Playback"
    } else if name.starts_with("user.") {
        "Authentication"
    } else if name.starts_with("library.") || name.starts_with("item.") {
        "Library"
    } else {
        "General"
    }
}

fn is_user_event(name: &str) -> bool {
    matches!(
        name,
        "user.authenticated"
            | "user.authenticationfailed"
            | "item.favorited"
            | "item.unfavorited"
            | "playback.start"
            | "playback.progress"
            | "playback.stop"
    )
}

/// jellyfin `Webhook` plugin 的 "GET 配置" 端点。它返回 plugin 自身的 JSON 配置，
/// 我们把它实现为"返回当前所有 webhook + 元信息"，便于第三方工具用同一接口探测。
async fn webhook_plugin_configuration(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let rows = sqlx::query_as::<_, DbWebhook>(
        r#"
        SELECT id, name, url, enabled, events, content_type, secret, headers_json,
               created_at, updated_at, last_status, last_error, last_triggered_at
          FROM webhooks
         ORDER BY created_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await?;
    let dtos: Vec<WebhookDto> = rows.into_iter().map(WebhookDto::from).collect();
    Ok(Json(serde_json::json!({
        "ServerUrl":        "",
        "Webhooks":         dtos,
        "DiscordWebhooks":  [],
        "GenericWebhooks":  dtos,
        "AvailableEvents":  webhooks::events::ALL,
    })))
}

async fn fetch_one(pool: &sqlx::PgPool, id: Uuid) -> Result<DbWebhook, AppError> {
    sqlx::query_as::<_, DbWebhook>(
        r#"
        SELECT id, name, url, enabled, events, content_type, secret, headers_json,
               created_at, updated_at, last_status, last_error, last_triggered_at
          FROM webhooks
         WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Webhook 不存在".into()))
}

fn validate(payload: &UpsertWebhookRequest) -> Result<(), AppError> {
    if payload.name.trim().is_empty() {
        return Err(AppError::BadRequest("webhook 名称不能为空".into()));
    }
    if payload.url.trim().is_empty() {
        return Err(AppError::BadRequest("webhook url 不能为空".into()));
    }
    if !(payload.url.starts_with("http://") || payload.url.starts_with("https://")) {
        return Err(AppError::BadRequest("webhook url 必须以 http(s):// 开头".into()));
    }
    Ok(())
}

fn ensure_object(value: Value) -> Value {
    match value {
        Value::Object(_) => value,
        _ => Value::Object(Default::default()),
    }
}
