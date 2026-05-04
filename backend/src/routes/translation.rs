//! PB52：翻译兜底 (Youdao) 管理 API。
//!
//! 路由：
//! - `GET    /Admin/Translation/Settings`    读配置（app_secret 已脱敏）
//! - `PUT    /Admin/Translation/Settings`    覆盖配置；空字符串的 secret 字段
//!                                            视作"不变更"——前端发回脱敏值时不
//!                                            清空原 secret。
//! - `POST   /Admin/Translation/Test`        连通性测试，body: `{ "text": "hi" }`
//! - `POST   /Admin/Translation/Items`       手动批量翻译 item id 列表
//!
//! 全部需要 admin 权限。

use axum::{extract::State, routing::get, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{self, AuthSession},
    error::AppError,
    metadata::translator::{
        self, TranslationSettings, TranslationTrigger, TRANSLATION_SETTINGS_KEY,
    },
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/Admin/Translation/Settings",
            get(get_settings).put(put_settings),
        )
        .route(
            "/Admin/Translation/Test",
            axum::routing::post(post_test),
        )
        .route(
            "/Admin/Translation/Items",
            axum::routing::post(post_translate_items),
        )
}

async fn get_settings(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<TranslationSettings>, AppError> {
    auth::require_admin(&session)?;
    let settings = match state.translator.as_ref() {
        Some(t) => t.current().await,
        None => translator::load_settings(&state.pool).await?,
    };
    Ok(Json(settings.redacted()))
}

async fn put_settings(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<TranslationSettings>,
) -> Result<Json<TranslationSettings>, AppError> {
    auth::require_admin(&session)?;
    let mut next = payload;

    // PB52：secret 留空（或脱敏占位 `***`）= 不变更，避免前端读取脱敏值后回写
    // 把 app_secret 真正清空。
    let prev = match state.translator.as_ref() {
        Some(t) => t.current().await,
        None => translator::load_settings(&state.pool).await?,
    };
    if next.app_secret.trim().is_empty() || next.app_secret.trim_matches('*').is_empty() {
        next.app_secret = prev.app_secret.clone();
    }

    // 简单字符串 trim，避免前端把多余空格存进数据库。
    next.app_key = next.app_key.trim().to_string();
    next.target_lang = next.target_lang.trim().to_string();
    next.from_lang = next.from_lang.trim().to_string();
    if next.target_lang.is_empty() {
        next.target_lang = "zh-CHS".into();
    }
    if next.from_lang.is_empty() {
        next.from_lang = "auto".into();
    }
    if next.provider.trim().is_empty() {
        next.provider = "youdao".into();
    }

    translator::save_settings(&state.pool, &next).await?;
    if let Some(t) = state.translator.as_ref() {
        t.replace(next.clone()).await;
    }

    tracing::info!(
        enabled = next.enabled,
        provider = %next.provider,
        target_lang = %next.target_lang,
        ready = next.ready(),
        key = %TRANSLATION_SETTINGS_KEY,
        "翻译兜底配置已更新"
    );

    Ok(Json(next.redacted()))
}

#[derive(Debug, Deserialize)]
struct TestBody {
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
}

async fn post_test(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<TestBody>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let translator = state
        .translator
        .as_ref()
        .ok_or_else(|| AppError::Internal("翻译服务未初始化".into()))?;
    let settings = translator.current().await;
    if !settings.ready() {
        return Err(AppError::BadRequest(
            "翻译服务未启用或未配置 appKey/appSecret".into(),
        ));
    }
    let text = payload
        .text
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Hello, world!")
        .to_string();
    let started = std::time::Instant::now();
    let translated = translator
        .translate(&state.pool, &text, payload.from.as_deref(), payload.to.as_deref())
        .await?;
    Ok(Json(json!({
        "source_text": text,
        "translated_text": translated,
        "elapsed_ms": started.elapsed().as_millis() as u64,
        "provider": settings.provider,
        "target_lang": payload.to.unwrap_or(settings.target_lang),
    })))
}

#[derive(Debug, Deserialize)]
struct TranslateItemsBody {
    #[serde(default)]
    item_ids: Vec<Uuid>,
}

async fn post_translate_items(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<TranslateItemsBody>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    if payload.item_ids.is_empty() {
        return Ok(Json(json!({"processed": 0})));
    }
    let translator = state
        .translator
        .as_ref()
        .ok_or_else(|| AppError::Internal("翻译服务未初始化".into()))?;
    let settings = translator.current().await;
    if !settings.ready() {
        return Err(AppError::BadRequest(
            "翻译服务未启用或未配置 appKey/appSecret".into(),
        ));
    }
    // 手动触发：无视 trigger 开关——这里就是用户主动按下「翻译」按钮的入口。
    translator::translate_items_bulk(&state, &payload.item_ids, TranslationTrigger::ManualRefresh)
        .await?;
    Ok(Json(json!({
        "processed": payload.item_ids.len(),
    })))
}
