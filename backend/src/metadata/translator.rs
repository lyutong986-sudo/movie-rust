//! PB52：翻译兜底（Youdao LLM Translation）
//!
//! 背景
//! ----
//! TMDB 与远端 Emby 同步偶尔返回非中文内容（典型如 BBC CBeebies 出品的
//! `Wonderblocks`：series 中文 OK，但每集 overview 是 TMDB 唯一存在的英文版本），
//! 导致首页字幕、剧集列表、推荐摘要在中文用户视角下「半中半英」。本模块作为
//! **元数据兜底层**：在 TMDB / 远端 Emby 写库后，对仍非目标语言的 name/overview
//! 二次调用 [Youdao 大模型翻译 API](https://openapi.youdao.com/proxy/http/llm-trans)
//! 翻译并覆盖回库。
//!
//! 设计要点
//! --------
//! 1. **目标语种检测**：用 CJK 字符占比快速判断是否需要翻译，避免无谓 API 计费。
//!    - target=`zh-CHS` 时，CJK 字符占比 ≥ 30% 视作已是中文。
//!    - target=`en` 时，ASCII 字母占比 ≥ 50% 视作已是英文。
//! 2. **结果缓存**：translation_cache(source_hash, target_lang, provider) 防止
//!    海量集英文简介在不同 episode 上重复计费。SHA-256 over `text.trim()`。
//! 3. **限流**：`WorkLimiterKind::Translation`（默认 4 并发），与 TMDB / 远端
//!    sync 的限流互不抢资源，但能阻止任意调度风暴打爆 Youdao 配额。
//! 4. **可选**：`TranslationSettings.enabled = false` 时所有外部调用直接 no-op
//!    返回原文，对其他链路完全透明。
//! 5. **热重载**：`TranslatorService` 持有 `RwLock<TranslationSettings>`；
//!    `/admin/translation/settings PUT` 后调用 `reload(&pool)` 即时生效。
//!
//! 与其它模块的关系
//! ----------------
//! - 三个接入点：
//!   - 手动刷新：`routes::items::do_refresh_item_metadata_with`
//!   - 远端同步：`remote_emby::process_one_remote_sync_item` /
//!     `fetch_and_upsert_series_detail`
//!   - 调度任务：`scheduled_tasks::run_task("metadata-refresh")`
//!     （已经复用 `do_refresh_item_metadata_with`，无需独立调用）
//! - 调用方均通过 `translate_item_text_fields(state, item_id)` 这一对外入口，
//!   屏蔽 enabled / 字段开关 / 触发位开关 / 错误吞掉 等细节。

use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::error::AppError;
use crate::http_client::SHARED as SHARED_HTTP_CLIENT;
use crate::work_limiter::{WorkLimiterKind, WorkLimiters};

/// system_settings 中存放本模块配置的键。
pub const TRANSLATION_SETTINGS_KEY: &str = "translation_settings";
const PROVIDER_YOUDAO: &str = "youdao";
const YOUDAO_API_URL: &str = "https://openapi.youdao.com/proxy/http/llm-trans";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TranslationSettings {
    /// 总开关。关闭后所有调用直接返回原文，零 API 调用。
    pub enabled: bool,
    /// 翻译服务商。当前仅支持 `youdao`，预留扩展位（google / deepl / openai…）。
    pub provider: String,
    pub app_key: String,
    pub app_secret: String,
    /// Youdao 兼容的目标语种码（zh-CHS / en / ja / ko / zh-CHT 等）。
    pub target_lang: String,
    /// Youdao 源语种码，默认 `auto` 让模型自动识别。
    pub from_lang: String,
    /// 字段级开关：是否翻译 Movie/Series 的 name。
    pub translate_name: bool,
    /// 字段级开关：是否翻译 Movie/Series/Season 的 overview。
    pub translate_overview: bool,
    /// 字段级开关：是否翻译 Episode 的 name + overview。
    pub translate_episode: bool,
    /// 字段级开关：是否翻译 Season 的 name（一般 `第 X 季` 是本地生成的，关掉）。
    pub translate_season_name: bool,
    /// 字段级开关：是否翻译人物 People.overview（演员/导演传记）。
    pub translate_person_overview: bool,
    /// 触发开关：手动「刷新元数据」是否走兜底翻译。
    pub trigger_manual_refresh: bool,
    /// 触发开关：「元数据刷新」调度任务是否走兜底翻译。
    pub trigger_scheduled_task: bool,
    /// 触发开关：远端 Emby 同步落库后是否走兜底翻译。
    pub trigger_remote_sync: bool,
}

impl Default for TranslationSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: PROVIDER_YOUDAO.into(),
            app_key: String::new(),
            app_secret: String::new(),
            target_lang: "zh-CHS".into(),
            from_lang: "auto".into(),
            translate_name: true,
            translate_overview: true,
            translate_episode: true,
            translate_season_name: false,
            translate_person_overview: false,
            trigger_manual_refresh: true,
            trigger_scheduled_task: true,
            trigger_remote_sync: true,
        }
    }
}

impl TranslationSettings {
    /// 调用方安全展示用：去掉 secret，避免 GET /admin/translation/settings 把
    /// app_secret 明文回吐到前端。
    pub fn redacted(&self) -> Self {
        let mut clone = self.clone();
        if !clone.app_secret.is_empty() {
            clone.app_secret = "***".into();
        }
        clone
    }

    pub fn ready(&self) -> bool {
        self.enabled
            && !self.app_key.trim().is_empty()
            && !self.app_secret.trim().is_empty()
            && !self.target_lang.trim().is_empty()
    }
}

/// 翻译触发位枚举。`translate_item_text_fields` 根据当前触发位是否在
/// settings 里被勾选决定要不要走 API。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslationTrigger {
    ManualRefresh,
    ScheduledTask,
    RemoteSync,
}

impl TranslationTrigger {
    fn allowed(&self, settings: &TranslationSettings) -> bool {
        match self {
            Self::ManualRefresh => settings.trigger_manual_refresh,
            Self::ScheduledTask => settings.trigger_scheduled_task,
            Self::RemoteSync => settings.trigger_remote_sync,
        }
    }
}

/// 进程级翻译服务。`AppState.translator` 持有它的 Arc，所有接入点都通过它读
/// 设置和缓存。
pub struct TranslatorService {
    settings: RwLock<TranslationSettings>,
    work_limiters: WorkLimiters,
}

impl TranslatorService {
    pub fn new(initial: TranslationSettings, work_limiters: WorkLimiters) -> Arc<Self> {
        Arc::new(Self {
            settings: RwLock::new(initial),
            work_limiters,
        })
    }

    pub async fn current(&self) -> TranslationSettings {
        self.settings.read().await.clone()
    }

    /// 从 system_settings 表重读配置（PUT /admin/translation/settings 之后调用）。
    pub async fn reload(&self, pool: &PgPool) -> Result<(), AppError> {
        let next = load_settings(pool).await?;
        let mut guard = self.settings.write().await;
        *guard = next;
        Ok(())
    }

    /// 直接覆盖内存中的设置。`save_settings` 写库 + 调本函数即可。
    pub async fn replace(&self, settings: TranslationSettings) {
        let mut guard = self.settings.write().await;
        *guard = settings;
    }

    /// 单条翻译（含缓存命中 / 限流）。`from`/`to` 为空时使用 settings 里的默认值。
    pub async fn translate(
        &self,
        pool: &PgPool,
        text: &str,
        from: Option<&str>,
        to: Option<&str>,
    ) -> Result<String, AppError> {
        if text.trim().is_empty() {
            return Ok(text.to_string());
        }
        let settings = self.current().await;
        if !settings.ready() {
            return Ok(text.to_string());
        }
        let target = to.unwrap_or(settings.target_lang.as_str()).to_string();
        let source = from.unwrap_or(settings.from_lang.as_str()).to_string();

        // 命中缓存（按规范化原文 + 目标语 + provider 三元主键）。
        let hash = sha256_hex(text);
        if let Some(cached) =
            fetch_cache(pool, &hash, &target, &settings.provider).await?
        {
            return Ok(cached);
        }

        // 限流（避免任意调度风暴打爆 Youdao 配额）。
        let _permit = self.work_limiters.acquire(WorkLimiterKind::Translation).await;

        let translated = match settings.provider.as_str() {
            PROVIDER_YOUDAO => {
                youdao_translate(
                    &settings.app_key,
                    &settings.app_secret,
                    text,
                    &source,
                    &target,
                )
                .await?
            }
            other => {
                tracing::warn!(provider = %other, "未知的翻译 provider，跳过翻译");
                return Ok(text.to_string());
            }
        };

        if !translated.is_empty() && translated != text {
            // 失败的写缓存不是致命错误，吞掉。
            if let Err(err) = write_cache(
                pool,
                &hash,
                &source,
                &target,
                &settings.provider,
                text,
                &translated,
            )
            .await
            {
                tracing::warn!(?err, "写入 translation_cache 失败");
            }
        }
        Ok(translated)
    }
}

/// 在判断「是否需要翻译」之前先做语种粗检测。
///
/// 出发点是省钱：一句已经是中文的简介调一次 Youdao 也要消耗 token 和并发槽位。
/// 这里宽松一些（30% CJK 即视作中文），宁可漏译几条偏远字段，也不要重复翻译。
pub fn looks_like_target_language(text: &str, target_lang: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return true;
    }
    let total = trimmed
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .count()
        .max(1);
    let target = target_lang.to_ascii_lowercase();
    if target.starts_with("zh") {
        let cjk = trimmed
            .chars()
            .filter(|c| {
                let v = *c as u32;
                (0x4E00..=0x9FFF).contains(&v)
                    || (0x3400..=0x4DBF).contains(&v)
                    || (0x20000..=0x2A6DF).contains(&v)
                    || (0xF900..=0xFAFF).contains(&v)
            })
            .count();
        return (cjk as f64 / total as f64) >= 0.3;
    }
    if target == "en" {
        let alpha = trimmed.chars().filter(|c| c.is_ascii_alphabetic()).count();
        return (alpha as f64 / total as f64) >= 0.5;
    }
    if target == "ja" {
        let kana = trimmed
            .chars()
            .filter(|c| {
                let v = *c as u32;
                (0x3040..=0x309F).contains(&v) || (0x30A0..=0x30FF).contains(&v)
            })
            .count();
        return (kana as f64 / total as f64) >= 0.2;
    }
    // 未知目标语种：保守起见认为已经是目标语，避免无效计费。
    true
}

// ---------------------------------------------------------------------------
// system_settings 持久化
// ---------------------------------------------------------------------------

pub async fn load_settings(pool: &PgPool) -> Result<TranslationSettings, AppError> {
    if let Some(value) =
        crate::repository::get_setting_value(pool, TRANSLATION_SETTINGS_KEY).await?
    {
        if let Ok(settings) = serde_json::from_value::<TranslationSettings>(value) {
            return Ok(settings);
        }
    }
    Ok(TranslationSettings::default())
}

pub async fn save_settings(
    pool: &PgPool,
    settings: &TranslationSettings,
) -> Result<(), AppError> {
    crate::repository::set_setting_value(pool, TRANSLATION_SETTINGS_KEY, json!(settings)).await
}

// ---------------------------------------------------------------------------
// 翻译缓存
// ---------------------------------------------------------------------------

fn sha256_hex(text: &str) -> Vec<u8> {
    let normalized = text.trim();
    Sha256::digest(normalized.as_bytes()).to_vec()
}

async fn fetch_cache(
    pool: &PgPool,
    hash: &[u8],
    target_lang: &str,
    provider: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<(String,)> = sqlx::query_as(
        r#"
        SELECT translated_text
        FROM translation_cache
        WHERE source_hash = $1 AND target_lang = $2 AND provider = $3
        "#,
    )
    .bind(hash)
    .bind(target_lang)
    .bind(provider)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(t,)| t))
}

async fn write_cache(
    pool: &PgPool,
    hash: &[u8],
    source_lang: &str,
    target_lang: &str,
    provider: &str,
    source_text: &str,
    translated_text: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO translation_cache
            (source_hash, source_lang, target_lang, provider, source_text, translated_text)
        VALUES ($1, $2, $3, $4, $5, $6)
        ON CONFLICT (source_hash, target_lang, provider) DO UPDATE
            SET translated_text = EXCLUDED.translated_text,
                source_lang     = EXCLUDED.source_lang,
                source_text     = EXCLUDED.source_text
        "#,
    )
    .bind(hash)
    .bind(source_lang)
    .bind(target_lang)
    .bind(provider)
    .bind(source_text)
    .bind(translated_text)
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Youdao LLM 翻译实现（对应 模板项目/llmtrans-demo.py）
// ---------------------------------------------------------------------------

fn youdao_sign(app_key: &str, app_secret: &str, text: &str, salt: &str, curtime: &str) -> String {
    // 与 demo 一致的 input 截断：长文本只取首 10 + 长度 + 末 10 字符。
    let chars: Vec<char> = text.chars().collect();
    let input = if chars.len() <= 20 {
        text.to_string()
    } else {
        let head: String = chars.iter().take(10).collect();
        let tail: String = chars.iter().rev().take(10).collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        format!("{}{}{}", head, chars.len(), tail)
    };
    let sign_str = format!("{app_key}{input}{salt}{curtime}{app_secret}");
    let digest = Sha256::digest(sign_str.as_bytes());
    hex::encode(digest)
}

async fn youdao_translate(
    app_key: &str,
    app_secret: &str,
    text: &str,
    from: &str,
    to: &str,
) -> Result<String, AppError> {
    let salt = Uuid::new_v4().to_string();
    let curtime = Utc::now().timestamp().to_string();
    let sign = youdao_sign(app_key, app_secret, text, &salt, &curtime);

    let form = [
        ("appKey", app_key),
        ("salt", salt.as_str()),
        ("signType", "v3"),
        ("sign", sign.as_str()),
        ("curtime", curtime.as_str()),
        ("i", text),
        ("from", from),
        ("to", to),
        ("streamType", "full"),
    ];

    let response = SHARED_HTTP_CLIENT
        .post(YOUDAO_API_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&form)
        .send()
        .await
        .map_err(|err| AppError::Internal(format!("Youdao 请求失败: {err}")))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|err| AppError::Internal(format!("Youdao 响应读取失败: {err}")))?;

    if !status.is_success() {
        return Err(AppError::Internal(format!(
            "Youdao 返回非 2xx ({}): {}",
            status.as_u16(),
            body.chars().take(400).collect::<String>()
        )));
    }

    parse_youdao_stream(&body)
}

/// Youdao LLM 翻译走 SSE 流式：每条事件一行 `data: {json}`。我们用 `streamType=full`
/// 让每条 JSON 的 `data.transFull` 都是「截至当前的完整翻译」，因此只需取**最后**
/// 一条 `successful=true` 的 transFull 即可拿到最终结果。
fn parse_youdao_stream(body: &str) -> Result<String, AppError> {
    let mut last_full = String::new();
    let mut last_error: Option<String> = None;
    for line in body.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("data:") {
            let json_part = rest.trim();
            if json_part.is_empty() {
                continue;
            }
            match serde_json::from_str::<Value>(json_part) {
                Ok(value) => {
                    let successful = value
                        .get("successful")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if successful {
                        if let Some(full) = value
                            .get("data")
                            .and_then(|d| d.get("transFull"))
                            .and_then(Value::as_str)
                        {
                            if !full.is_empty() {
                                last_full = full.to_string();
                            }
                        }
                    } else {
                        let code = value
                            .get("code")
                            .map(|v| v.to_string())
                            .unwrap_or_default();
                        let message = value
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown error");
                        last_error = Some(format!("code={code} message={message}"));
                    }
                }
                Err(err) => {
                    tracing::trace!(?err, line = %json_part, "Youdao 流式响应行解析失败，忽略");
                }
            }
        }
    }
    if !last_full.is_empty() {
        return Ok(last_full);
    }
    if let Some(err) = last_error {
        return Err(AppError::Internal(format!("Youdao 翻译失败: {err}")));
    }
    Err(AppError::Internal(
        "Youdao 翻译响应为空（未解析出 transFull）".into(),
    ))
}

// ---------------------------------------------------------------------------
// 高层接口：对一条 media_item 兜底翻译 name / overview
// ---------------------------------------------------------------------------

/// 对单个 media_items 行进行兜底翻译。
///
/// 行为：
/// 1. settings.enabled == false 或当前 trigger 未勾选 → 立即返回 Ok(())。
/// 2. 读 media_items name / overview。
/// 3. 按字段开关 + 语言粗检测决定是否翻译。
/// 4. 翻译成功后只更新被改动的字段（覆盖式 UPDATE，更新 date_modified）。
///
/// 永远 swallow 单条翻译错误并 warn，不让兜底翻译影响主链路。
pub async fn translate_item_text_fields(
    state: &crate::state::AppState,
    item_id: Uuid,
    trigger: TranslationTrigger,
) -> Result<(), AppError> {
    let Some(translator) = state.translator.as_ref() else {
        return Ok(());
    };
    let settings = translator.current().await;
    if !settings.ready() || !trigger.allowed(&settings) {
        return Ok(());
    }

    let row: Option<(Option<String>, Option<String>, String)> = sqlx::query_as(
        r#"SELECT name, overview, item_type FROM media_items WHERE id = $1"#,
    )
    .bind(item_id)
    .fetch_optional(&state.pool)
    .await?;
    let Some((name, overview, item_type)) = row else {
        return Ok(());
    };

    // 字段开关：根据 item_type 与 settings 切换是否要翻译 name / overview。
    let (allow_name, allow_overview) = match item_type.as_str() {
        "Movie" | "Series" => (settings.translate_name, settings.translate_overview),
        "Season" => (
            settings.translate_season_name,
            settings.translate_overview,
        ),
        "Episode" => (settings.translate_episode, settings.translate_episode),
        "Person" => (false, settings.translate_person_overview),
        _ => (settings.translate_name, settings.translate_overview),
    };

    let mut new_name: Option<String> = None;
    let mut new_overview: Option<String> = None;

    if allow_name {
        if let Some(current) = name.as_deref() {
            if !current.trim().is_empty()
                && !looks_like_target_language(current, &settings.target_lang)
            {
                match translator.translate(&state.pool, current, None, None).await {
                    Ok(translated) if translated != current && !translated.is_empty() => {
                        new_name = Some(translated);
                    }
                    Ok(_) => {}
                    Err(err) => tracing::warn!(
                        item_id = %item_id, ?err, "翻译 name 失败，保留原文"
                    ),
                }
            }
        }
    }

    if allow_overview {
        if let Some(current) = overview.as_deref() {
            if !current.trim().is_empty()
                && !looks_like_target_language(current, &settings.target_lang)
            {
                match translator.translate(&state.pool, current, None, None).await {
                    Ok(translated) if translated != current && !translated.is_empty() => {
                        new_overview = Some(translated);
                    }
                    Ok(_) => {}
                    Err(err) => tracing::warn!(
                        item_id = %item_id, ?err, "翻译 overview 失败，保留原文"
                    ),
                }
            }
        }
    }

    if new_name.is_some() || new_overview.is_some() {
        sqlx::query(
            r#"
            UPDATE media_items
            SET name          = COALESCE($2, name),
                overview      = COALESCE($3, overview),
                date_modified = now()
            WHERE id = $1
            "#,
        )
        .bind(item_id)
        .bind(new_name.as_deref())
        .bind(new_overview.as_deref())
        .execute(&state.pool)
        .await?;
    }

    Ok(())
}

/// 批量版本：对一组 item id 顺序兜底翻译。仅在触发位允许的情况下逐条调用。
/// 单条失败不阻断整体（已在 `translate_item_text_fields` 内部 swallow）。
pub async fn translate_items_bulk(
    state: &crate::state::AppState,
    item_ids: &[Uuid],
    trigger: TranslationTrigger,
) -> Result<(), AppError> {
    for id in item_ids {
        translate_item_text_fields(state, *id, trigger).await?;
    }
    Ok(())
}
