//! 出向 webhook 推送（emby Webhooks 插件协议兼容层）。
//!
//! 设计目标：
//! - 与 emby 第三方 [Webhooks 插件](https://github.com/MediaBrowser/Emby/wiki) 的 payload 一致，
//!   让 `Sakura_embyboss` 等下游接收方零改造直接使用：
//!   ```json
//!   {
//!     "Event":   "item.added" | "playback.start" | "user.authenticated" | ...,
//!     "Date":    "2026-04-30T00:00:00Z",
//!     "Server":  { "Id": "...", "Name": "..." },
//!     "Item":    { "Id": "...", "Name": "...", "Type": "...", "UserData": {...}, "SeriesName": "..." },
//!     "User":    { "Id": "...", "Name": "..." },
//!     "Session": { "Id": "...", "Client": "...", "DeviceName": "...", "RemoteAddress": "..." }
//!   }
//!   ```
//! - 异步推送，不阻塞业务路径。失败重试 3 次（指数回退 1s/3s/9s），最终失败仅写
//!   `webhooks.last_error` 不让上层失败。
//! - HMAC 签名（可选 `secret`）：`X-Webhook-Signature: sha256=<hex>`，便于下游做认证。
//!
//! 使用方式：
//! ```ignore
//! WebhookDispatcher::dispatch(&state, "item.added", payload_value);
//! ```
//! 调用方只管"喊一声"，内部 spawn task 完成 fanout + 重试 + 状态回写。

use std::time::Duration;

use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::FromRow;
use uuid::Uuid;

use crate::state::AppState;

type HmacSha256 = Hmac<Sha256>;

/// 已知事件名（Sakura 实际订阅的全部事件）。约束新代码使用同一组字符串字面量，
/// 防止两侧 typo 不一致。
pub mod events {
    pub const ITEM_ADDED: &str = "item.added";
    pub const LIBRARY_NEW: &str = "library.new";
    pub const PLAYBACK_START: &str = "playback.start";
    pub const PLAYBACK_PROGRESS: &str = "playback.progress";
    pub const PLAYBACK_STOP: &str = "playback.stop";
    pub const SESSION_START: &str = "session.start";
    pub const USER_AUTHENTICATED: &str = "user.authenticated";
    pub const USER_AUTH_FAILED: &str = "user.authenticationfailed";
    pub const ITEM_FAVORITED: &str = "item.favorited";
    pub const ITEM_UNFAVORITED: &str = "item.unfavorited";

    /// 上游 emby/jellyfin Webhooks plugin 暴露的全部事件名（用于 /Notifications/Types 列表）。
    pub const ALL: &[&str] = &[
        ITEM_ADDED,
        LIBRARY_NEW,
        PLAYBACK_START,
        PLAYBACK_PROGRESS,
        PLAYBACK_STOP,
        SESSION_START,
        USER_AUTHENTICATED,
        USER_AUTH_FAILED,
        ITEM_FAVORITED,
        ITEM_UNFAVORITED,
    ];
}

#[derive(Debug, Clone, FromRow)]
pub struct DbWebhook {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub events: Vec<String>,
    pub content_type: String,
    pub secret: Option<String>,
    pub headers_json: Value,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub last_status: Option<i32>,
    pub last_error: Option<String>,
    pub last_triggered_at: Option<chrono::DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct WebhookDto {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub events: Vec<String>,
    pub content_type: String,
    pub has_secret: bool,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub headers: Value,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub last_status: Option<i32>,
    pub last_error: Option<String>,
    pub last_triggered_at: Option<chrono::DateTime<Utc>>,
}

impl From<DbWebhook> for WebhookDto {
    fn from(value: DbWebhook) -> Self {
        Self {
            id: value.id,
            name: value.name,
            url: value.url,
            enabled: value.enabled,
            events: value.events,
            content_type: value.content_type,
            has_secret: value.secret.as_deref().is_some_and(|s| !s.is_empty()),
            headers: value.headers_json,
            created_at: value.created_at,
            updated_at: value.updated_at,
            last_status: value.last_status,
            last_error: value.last_error,
            last_triggered_at: value.last_triggered_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct UpsertWebhookRequest {
    pub name: String,
    pub url: String,
    pub enabled: bool,
    pub events: Vec<String>,
    pub content_type: Option<String>,
    pub secret: Option<String>,
    pub headers: Value,
}

impl Default for UpsertWebhookRequest {
    fn default() -> Self {
        Self {
            name: String::new(),
            url: String::new(),
            enabled: true,
            events: Vec::new(),
            content_type: None,
            secret: None,
            headers: Value::Object(Default::default()),
        }
    }
}

/// 异步推送一个事件给所有订阅的 webhook。**不阻塞调用方**——内部 spawn 一个 task 处理 fanout。
///
/// 业务路径里直接 `webhooks::dispatch(&state, events::ITEM_ADDED, payload)` 即可，
/// 任何 webhook 失败都不会传染上层。
pub fn dispatch(state: &AppState, event: &str, payload: Value) {
    dispatch_raw(
        state.pool.clone(),
        state.config.server_id,
        state.config.server_name.clone(),
        event.to_owned(),
        payload,
    );
}

/// 直接把单个 webhook（已知具体一行）送达，**绕过订阅过滤**——给"测试按钮"用，
/// 让用户配置好但还没勾选事件的 webhook 也能立刻收到一条 `webhook.test` 验证联通性。
pub fn dispatch_to_hook(
    pool: sqlx::PgPool,
    server_id: Uuid,
    server_name: String,
    hook: DbWebhook,
    event: String,
    payload: Value,
) {
    tokio::spawn(async move {
        let now = Utc::now();
        let final_payload = wrap_payload(&event, payload, server_id, &server_name, now);
        deliver_one(&pool, hook, &event, final_payload).await;
    });
}

/// `dispatch` 的"原始底盘"版本，给 scanner 等没有完整 `AppState`、只有 `pool +
/// server_id + server_name` 的环境使用。语义与 `dispatch` 完全一致。
pub fn dispatch_raw(
    pool: sqlx::PgPool,
    server_id: Uuid,
    server_name: String,
    event: String,
    payload: Value,
) {
    tokio::spawn(async move {
        let now = Utc::now();
        let final_payload = wrap_payload(&event, payload, server_id, &server_name, now);
        let hooks = match list_subscribers(&pool, &event).await {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!(?err, event, "webhooks: 拉取订阅列表失败");
                return;
            }
        };
        for hook in hooks {
            let pool = pool.clone();
            let payload = final_payload.clone();
            let event_name = event.clone();
            tokio::spawn(async move {
                deliver_one(&pool, hook, &event_name, payload).await;
            });
        }
    });
}

fn wrap_payload(
    event: &str,
    inner: Value,
    server_id: Uuid,
    server_name: &str,
    now: chrono::DateTime<Utc>,
) -> Value {
    let mut out = match inner {
        Value::Object(map) => Value::Object(map),
        other => json!({ "Data": other }),
    };
    if let Value::Object(map) = &mut out {
        map.insert("Event".to_string(), Value::String(event.to_owned()));
        map.insert("Date".to_string(), Value::String(now.to_rfc3339()));
        if !map.contains_key("Server") {
            map.insert(
                "Server".to_string(),
                json!({
                    "Id":   crate::models::uuid_to_emby_guid(&server_id),
                    "Name": server_name,
                }),
            );
        }
    }
    out
}

async fn list_subscribers(
    pool: &sqlx::PgPool,
    event: &str,
) -> Result<Vec<DbWebhook>, sqlx::Error> {
    sqlx::query_as::<_, DbWebhook>(
        r#"
        SELECT id, name, url, enabled, events, content_type, secret, headers_json,
               created_at, updated_at, last_status, last_error, last_triggered_at
          FROM webhooks
         WHERE enabled
           AND (
                 cardinality(events) = 0          -- events 为空 = 订阅全部
              OR $1 = ANY(events)
           )
        "#,
    )
    .bind(event)
    .fetch_all(pool)
    .await
}

async fn deliver_one(pool: &sqlx::PgPool, hook: DbWebhook, event: &str, payload: Value) {
    let body_bytes: Vec<u8> = match hook.content_type.to_ascii_lowercase().as_str() {
        "application/x-www-form-urlencoded" => {
            // emby 老 Webhooks plugin 默认 form-data：data=<json>
            let json_str = serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into());
            form_urlencoded::Serializer::new(String::new())
                .append_pair("data", &json_str)
                .finish()
                .into_bytes()
        }
        _ => serde_json::to_vec(&payload).unwrap_or_default(),
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_str(&hook.content_type).unwrap_or(HeaderValue::from_static("application/json")),
    );
    headers.insert(
        HeaderName::from_static("x-webhook-event"),
        HeaderValue::from_str(event).unwrap_or(HeaderValue::from_static("unknown")),
    );
    if let Some(secret) = hook.secret.as_deref() {
        if !secret.is_empty() {
            if let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) {
                mac.update(&body_bytes);
                let sig = hex::encode(mac.finalize().into_bytes());
                if let Ok(v) = HeaderValue::from_str(&format!("sha256={sig}")) {
                    headers.insert(HeaderName::from_static("x-webhook-signature"), v);
                }
            }
        }
    }
    if let Value::Object(extra) = &hook.headers_json {
        for (k, v) in extra {
            let Some(s) = v.as_str() else { continue };
            if let (Ok(name), Ok(val)) = (
                k.parse::<HeaderName>(),
                HeaderValue::from_str(s),
            ) {
                headers.insert(name, val);
            }
        }
    }

    // 重试：1s / 3s / 9s
    let mut attempt = 0u32;
    let backoff_steps = [Duration::from_secs(1), Duration::from_secs(3), Duration::from_secs(9)];
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("movie-rust-webhooks/1.0")
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            mark_status(pool, hook.id, None, Some(format!("client build: {err}"))).await;
            return;
        }
    };

    let mut last_status: Option<i32> = None;
    let mut last_error: Option<String> = None;
    loop {
        let resp = client
            .post(&hook.url)
            .headers(headers.clone())
            .body(body_bytes.clone())
            .send()
            .await;
        match resp {
            Ok(r) => {
                last_status = Some(r.status().as_u16() as i32);
                if r.status().is_success() {
                    last_error = None;
                    break;
                }
                let snippet = r.text().await.unwrap_or_default();
                let snippet = snippet.chars().take(200).collect::<String>();
                last_error = Some(format!("HTTP {}: {snippet}", last_status.unwrap_or(0)));
            }
            Err(err) => {
                last_status = err.status().map(|s| s.as_u16() as i32);
                last_error = Some(err.to_string());
            }
        }
        if attempt >= backoff_steps.len() as u32 - 1 {
            break;
        }
        tokio::time::sleep(backoff_steps[attempt as usize]).await;
        attempt += 1;
    }

    if let Some(err) = &last_error {
        tracing::warn!(
            webhook_id = %hook.id,
            url = %hook.url,
            event,
            attempts = attempt + 1,
            err,
            "webhooks: 推送失败"
        );
    } else {
        tracing::debug!(webhook_id = %hook.id, event, "webhooks: 推送成功");
    }
    mark_status(pool, hook.id, last_status, last_error).await;
}

async fn mark_status(
    pool: &sqlx::PgPool,
    id: Uuid,
    status: Option<i32>,
    error: Option<String>,
) {
    let _ = sqlx::query(
        r#"UPDATE webhooks SET last_status = $1, last_error = $2, last_triggered_at = now() WHERE id = $3"#,
    )
    .bind(status)
    .bind(error)
    .bind(id)
    .execute(pool)
    .await;
}
