use crate::{
    config::Config,
    error::AppError,
    media_analyzer::{MediaAnalysisResult, MediaChapterInfo, MediaFormatInfo, MediaStreamInfo},
    models::{DbLibrary, DbRemoteEmbySource, ScanSummary},
    repository, scanner,
    state::AppState,
};
use axum::{
    body::Body,
    http::{header, HeaderMap, Method, StatusCode},
    response::Response,
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use uuid::Uuid;

const REMOTE_PAGE_SIZE: i64 = 200;
const PLAYBACK_INFO_CACHE_TTL_SECS: u64 = 300;
const PLAYBACK_INFO_CACHE_MAX_ENTRIES: usize = 512;

struct CachedPlaybackInfo {
    info: RemotePlaybackInfo,
    inserted_at: Instant,
}

static PLAYBACK_INFO_CACHE: std::sync::LazyLock<RwLock<HashMap<String, CachedPlaybackInfo>>> =
    std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

fn playback_info_cache_key(source_id: Uuid, remote_item_id: &str, media_source_id: Option<&str>) -> String {
    format!("{}:{}:{}", source_id, remote_item_id, media_source_id.unwrap_or(""))
}

async fn get_cached_playback_info(key: &str) -> Option<RemotePlaybackInfo> {
    let cache = PLAYBACK_INFO_CACHE.read().await;
    cache.get(key).and_then(|entry| {
        if entry.inserted_at.elapsed().as_secs() < PLAYBACK_INFO_CACHE_TTL_SECS {
            Some(entry.info.clone())
        } else {
            None
        }
    })
}

async fn set_cached_playback_info(key: String, info: RemotePlaybackInfo) {
    let mut cache = PLAYBACK_INFO_CACHE.write().await;
    if cache.len() >= PLAYBACK_INFO_CACHE_MAX_ENTRIES {
        let cutoff = Instant::now() - std::time::Duration::from_secs(PLAYBACK_INFO_CACHE_TTL_SECS);
        cache.retain(|_, v| v.inserted_at > cutoff);
        if cache.len() >= PLAYBACK_INFO_CACHE_MAX_ENTRIES {
            if let Some(oldest_key) = cache
                .iter()
                .min_by_key(|(_, v)| v.inserted_at)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&oldest_key);
            }
        }
    }
    cache.insert(key, CachedPlaybackInfo { info, inserted_at: Instant::now() });
}
const DEFAULT_SPOOFED_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) EmbyTheater/3.0.20 Chrome/124.0.0.0 Safari/537.36";
const REMOTE_DISPLAY_MODE_SEPARATE: &str = "separate";
const REMOTE_DISPLAY_MODE_MERGE: &str = "merge";

#[derive(Debug, Clone)]
pub struct RemoteEmbySyncResult {
    pub source_id: Uuid,
    pub source_name: String,
    pub written_files: usize,
    pub source_root: String,
    pub scan_summary: ScanSummary,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteViewPreview {
    pub id: String,
    pub name: String,
    pub collection_type: Option<String>,
}

/// 预览远端 Emby 源时返回的结果，包含服务器名称和媒体库列表
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct RemotePreviewResult {
    /// 远端 Emby 服务器名称（从 /System/Info 获取）
    pub server_name: String,
    pub views: Vec<RemoteViewPreview>,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct RemoteSyncProgressSnapshot {
    pub phase: String,
    pub total_items: u64,
    pub fetched_items: u64,
    pub written_files: u64, // 兼容前端字段名，语义为已入库条目数
    pub progress: f64,
}

#[derive(Clone, Default)]
pub struct RemoteSyncProgress {
    snapshot: Arc<RwLock<RemoteSyncProgressSnapshot>>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl RemoteSyncProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn snapshot(&self) -> RemoteSyncProgressSnapshot {
        self.snapshot.read().await.clone()
    }

    pub fn set_phase(&self, phase: impl Into<String>, progress: f64) {
        let value = phase.into();
        let snapshot = self.snapshot.clone();
        let progress = progress.clamp(0.0, 100.0);
        if let Ok(mut guard) = snapshot.try_write() {
            guard.phase = value;
            guard.progress = progress;
            return;
        }
        tokio::spawn(async move {
            let mut guard = snapshot.write().await;
            guard.phase = value;
            guard.progress = progress;
        });
    }

    #[allow(dead_code)]
    pub fn set_fetch_progress(&self, fetched_items: u64, total_items: u64) {
        let snapshot = self.snapshot.clone();
        if let Ok(mut guard) = snapshot.try_write() {
            guard.phase = "FetchingRemoteItems".to_string();
            guard.total_items = total_items;
            guard.fetched_items = fetched_items.min(total_items);
            let ratio = if total_items == 0 {
                1.0
            } else {
                guard.fetched_items as f64 / total_items as f64
            };
            guard.progress = (5.0 + ratio * 35.0).clamp(5.0, 40.0);
            return;
        }
        tokio::spawn(async move {
            let mut guard = snapshot.write().await;
            guard.phase = "FetchingRemoteItems".to_string();
            guard.total_items = total_items;
            guard.fetched_items = fetched_items.min(total_items);
            let ratio = if total_items == 0 {
                1.0
            } else {
                guard.fetched_items as f64 / total_items as f64
            };
            guard.progress = (5.0 + ratio * 35.0).clamp(5.0, 40.0);
        });
    }

    #[allow(dead_code)]
    pub fn set_write_progress(&self, written_files: u64, total_items: u64) {
        let snapshot = self.snapshot.clone();
        if let Ok(mut guard) = snapshot.try_write() {
            guard.phase = "UpsertingVirtualItems".to_string();
            guard.total_items = total_items;
            guard.written_files = written_files.min(total_items);
            let ratio = if total_items == 0 {
                1.0
            } else {
                guard.written_files as f64 / total_items as f64
            };
            guard.progress = (40.0 + ratio * 30.0).clamp(40.0, 70.0);
            return;
        }
        tokio::spawn(async move {
            let mut guard = snapshot.write().await;
            guard.phase = "UpsertingVirtualItems".to_string();
            guard.total_items = total_items;
            guard.written_files = written_files.min(total_items);
            let ratio = if total_items == 0 {
                1.0
            } else {
                guard.written_files as f64 / total_items as f64
            };
            guard.progress = (40.0 + ratio * 30.0).clamp(40.0, 70.0);
        });
    }

    pub fn set_streaming_progress(&self, fetched_items: u64, written_files: u64, total_items: u64) {
        let snapshot = self.snapshot.clone();
        if let Ok(mut guard) = snapshot.try_write() {
            guard.phase = "SyncingRemoteItems".to_string();
            guard.total_items = total_items;
            guard.fetched_items = fetched_items.min(total_items);
            guard.written_files = written_files.min(total_items);
            let ratio = if total_items == 0 {
                1.0
            } else {
                guard.fetched_items as f64 / total_items as f64
            };
            guard.progress = (10.0 + ratio * 89.0).clamp(10.0, 99.0);
            return;
        }
        tokio::spawn(async move {
            let mut guard = snapshot.write().await;
            guard.phase = "SyncingRemoteItems".to_string();
            guard.total_items = total_items;
            guard.fetched_items = fetched_items.min(total_items);
            guard.written_files = written_files.min(total_items);
            let ratio = if total_items == 0 {
                1.0
            } else {
                guard.fetched_items as f64 / total_items as f64
            };
            guard.progress = (10.0 + ratio * 89.0).clamp(10.0, 99.0);
        });
    }

    #[allow(dead_code)]
    pub fn apply_scan_snapshot(&self, scan: &scanner::ScanProgressSnapshot) {
        let snapshot = self.snapshot.clone();
        let scan_percent = scan.percent.clamp(0.0, 96.0);
        let progress = 70.0 + (scan_percent / 96.0) * 30.0;
        let phase = if scan.phase.is_empty() {
            "ScanningLibrary".to_string()
        } else {
            format!("ScanningLibrary/{}", scan.phase)
        };
        if let Ok(mut guard) = snapshot.try_write() {
            guard.phase = phase;
            guard.progress = progress.clamp(70.0, 99.5);
            return;
        }
        tokio::spawn(async move {
            let mut guard = snapshot.write().await;
            guard.phase = phase;
            guard.progress = progress.clamp(70.0, 99.5);
        });
    }

    pub fn request_cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn mark_completed(&self) {
        let snapshot = self.snapshot.clone();
        if let Ok(mut guard) = snapshot.try_write() {
            guard.phase = "Completed".to_string();
            guard.progress = 100.0;
            return;
        }
        tokio::spawn(async move {
            let mut guard = snapshot.write().await;
            guard.phase = "Completed".to_string();
            guard.progress = 100.0;
        });
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteLoginResponse {
    access_token: String,
    user: RemoteLoginUser,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteLoginUser {
    id: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteSystemInfo {
    #[serde(default)]
    server_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteItemsResult {
    #[serde(default)]
    items: Vec<RemoteBaseItem>,
    total_record_count: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteViewsResult {
    #[serde(default)]
    items: Vec<RemoteLibraryView>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteLibraryView {
    id: String,
    name: String,
    #[serde(default)]
    collection_type: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteSyncItem {
    item: RemoteBaseItem,
    view_id: String,
    view_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteBaseItem {
    id: String,
    name: String,
    #[serde(rename = "Type")]
    item_type: String,
    #[serde(default)]
    overview: Option<String>,
    production_year: Option<i32>,
    #[serde(default)]
    official_rating: Option<String>,
    #[serde(default)]
    community_rating: Option<f64>,
    #[serde(default)]
    critic_rating: Option<f64>,
    #[serde(default)]
    premiere_date: Option<String>,
    #[serde(default)]
    run_time_ticks: Option<i64>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
    series_name: Option<String>,
    #[serde(default)]
    season_name: Option<String>,
    parent_index_number: Option<i32>,
    index_number: Option<i32>,
    #[serde(default)]
    provider_ids: Value,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    genres: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    studios: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    tags: Vec<String>,
    #[serde(default)]
    media_streams: Vec<RemoteItemMediaStream>,
    media_sources: Option<Vec<RemoteMediaSource>>,
    #[serde(default)]
    image_tags: Option<Value>,
    #[serde(default)]
    backdrop_image_tags: Option<Value>,
    #[serde(default)]
    series_id: Option<String>,
    #[serde(default)]
    season_id: Option<String>,
    #[serde(default)]
    series_primary_image_tag: Option<String>,
    #[serde(default)]
    parent_backdrop_image_tags: Option<Value>,
    #[serde(default)]
    parent_backdrop_item_id: Option<String>,
    #[serde(default)]
    parent_logo_item_id: Option<String>,
    #[serde(default)]
    parent_logo_image_tag: Option<String>,
}

fn deserialize_string_list_lossy<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    let mut result = Vec::new();
    match value {
        Value::Null => {}
        Value::Array(items) => {
            for item in items {
                if let Some(text) = value_to_lossy_string(&item) {
                    result.push(text);
                }
            }
        }
        other => {
            if let Some(text) = value_to_lossy_string(&other) {
                result.push(text);
            }
        }
    }
    Ok(result)
}

fn value_to_lossy_string(value: &Value) -> Option<String> {
    let raw = match value {
        Value::String(text) => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Object(map) => {
            for key in [
                "Name", "name", "Value", "value", "Text", "text", "Title", "title",
            ] {
                if let Some(found) = map.get(key) {
                    if let Some(normalized) = value_to_lossy_string(found) {
                        return Some(normalized);
                    }
                }
            }
            None
        }
        _ => None,
    }?;
    if raw.is_empty() {
        None
    } else {
        Some(raw)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteMediaSource {
    id: String,
    #[serde(default)]
    media_streams: Vec<RemoteItemMediaStream>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteItemMediaStream {
    index: i32,
    #[serde(rename = "Type")]
    stream_type: String,
    #[serde(default)]
    codec: Option<String>,
    #[serde(default, rename = "IsExternal")]
    is_external: Option<bool>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    title: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemotePlaybackInfo {
    #[serde(default)]
    media_sources: Vec<RemotePlaybackMediaSource>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemotePlaybackMediaSource {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    container: Option<String>,
    #[serde(default)]
    size: Option<i64>,
    #[serde(default)]
    bitrate: Option<i64>,
    #[serde(default)]
    run_time_ticks: Option<i64>,
    #[serde(default)]
    direct_stream_url: Option<String>,
    #[serde(default)]
    transcoding_url: Option<String>,
    #[serde(default)]
    add_api_key_to_direct_stream_url: Option<bool>,
    #[serde(default)]
    required_http_headers: HashMap<String, String>,
    #[serde(default)]
    media_streams: Vec<RemotePlaybackMediaStream>,
    #[serde(default)]
    chapters: Vec<RemotePlaybackChapter>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemotePlaybackMediaStream {
    index: i32,
    #[serde(rename = "Type")]
    stream_type: String,
    #[serde(default)]
    codec: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    profile: Option<String>,
    #[serde(default)]
    bit_rate: Option<i64>,
    #[serde(default)]
    bit_depth: Option<i32>,
    #[serde(default)]
    channels: Option<i32>,
    #[serde(default)]
    sample_rate: Option<i32>,
    #[serde(default)]
    channel_layout: Option<String>,
    #[serde(default)]
    width: Option<i32>,
    #[serde(default)]
    height: Option<i32>,
    #[serde(default)]
    average_frame_rate: Option<f64>,
    #[serde(default)]
    real_frame_rate: Option<f64>,
    #[serde(default)]
    aspect_ratio: Option<String>,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    is_forced: bool,
    #[serde(default)]
    is_hearing_impaired: bool,
    #[serde(default)]
    is_interlaced: bool,
    #[serde(default)]
    is_text_subtitle_stream: Option<bool>,
    #[serde(default)]
    color_space: Option<String>,
    #[serde(default)]
    color_transfer: Option<String>,
    #[serde(default)]
    color_primaries: Option<String>,
    #[serde(default)]
    video_range: Option<String>,
    #[serde(default)]
    level: Option<i32>,
    #[serde(default)]
    pixel_format: Option<String>,
    #[serde(default)]
    ref_frames: Option<i32>,
    #[serde(default)]
    time_base: Option<String>,
    #[serde(default)]
    is_anamorphic: Option<bool>,
    #[serde(default)]
    attachment_size: Option<i32>,
    #[serde(default)]
    extended_video_sub_type: Option<String>,
    #[serde(default)]
    extended_video_sub_type_description: Option<String>,
    #[serde(default)]
    extended_video_type: Option<String>,
    /// 外挂字幕的 Emby 原始交付 URL（如 /Videos/.../Subtitles/.../Stream.srt）
    #[serde(default)]
    delivery_url: Option<String>,
    /// 是否为外挂流（对字幕有意义）
    #[serde(default)]
    is_external: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemotePlaybackChapter {
    chapter_index: i32,
    start_position_ticks: i64,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    marker_type: Option<String>,
}

pub fn default_spoofed_user_agent() -> &'static str {
    DEFAULT_SPOOFED_USER_AGENT
}

pub async fn preview_remote_views(
    server_url: &str,
    username: &str,
    password: &str,
    spoofed_user_agent: &str,
) -> Result<RemotePreviewResult, AppError> {
    let server_url = normalize_server_url(server_url);
    if server_url.trim().is_empty() {
        return Err(AppError::BadRequest("远端 Emby 地址不能为空".to_string()));
    }
    let username = username.trim();
    if username.is_empty() {
        return Err(AppError::BadRequest("远端 Emby 用户名不能为空".to_string()));
    }
    if password.trim().is_empty() {
        return Err(AppError::BadRequest("远端 Emby 密码不能为空".to_string()));
    }
    let spoofed_user_agent = spoofed_user_agent.trim();
    if spoofed_user_agent.is_empty() {
        return Err(AppError::BadRequest("伪装 User-Agent 不能为空".to_string()));
    }

    let source_id = Uuid::new_v4();
    let device_id = format!("movie-rust-preview-{}", source_id.simple());
    let client = &*crate::http_client::SHARED;
    let auth_endpoint = format!("{server_url}/Users/AuthenticateByName");
    let login_response = client
        .post(&auth_endpoint)
        .query(&[("reqformat", "json")])
        .header(header::USER_AGENT.as_str(), spoofed_user_agent)
        .header(header::ACCEPT.as_str(), "application/json")
        .header(header::ACCEPT_ENCODING.as_str(), "identity")
        .header(
            "X-Emby-Authorization",
            emby_auth_header_for_device(device_id.as_str(), None),
        )
        .json(&serde_json::json!({
            "Username": username,
            "Pw": password,
            "Password": password,
        }))
        .send()
        .await?;

    if !login_response.status().is_success() {
        let status = login_response.status();
        let body = login_response.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "远端 Emby 登录失败: {} {}",
            status.as_u16(),
            body
        )));
    }
    let login: RemoteLoginResponse =
        parse_remote_json_response(login_response, auth_endpoint.as_str()).await?;

    let views_endpoint = format!("{server_url}/Users/{}/Views", login.user.id);
    let views_response = client
        .get(&views_endpoint)
        .query(&[
            ("Fields", "CollectionType,ChildCount,RecursiveItemCount"),
            ("EnableTotalRecordCount", "true"),
            ("reqformat", "json"),
            ("api_key", login.access_token.as_str()),
        ])
        .header(header::USER_AGENT.as_str(), spoofed_user_agent)
        .header(header::ACCEPT.as_str(), "application/json")
        .header(header::ACCEPT_ENCODING.as_str(), "identity")
        .header("X-Emby-Token", login.access_token.as_str())
        .header(
            "X-Emby-Authorization",
            emby_auth_header_for_device(device_id.as_str(), Some(login.access_token.as_str())),
        )
        .send()
        .await?;
    if !views_response.status().is_success() {
        let status = views_response.status();
        let body = views_response.text().await.unwrap_or_default();
        return Err(AppError::Internal(format!(
            "拉取远端媒体库失败: {} {}",
            status.as_u16(),
            body
        )));
    }
    let mut views: RemoteViewsResult =
        parse_remote_json_response(views_response, views_endpoint.as_str()).await?;
    views
        .items
        .retain(|view| !view.id.trim().is_empty() && !view.name.trim().is_empty());

    // 获取远端服务器名称（/System/Info 需要认证，/System/Info/Public 公开可访问）
    let server_name = {
        let info_endpoint = format!("{server_url}/System/Info");
        let info_resp = client
            .get(&info_endpoint)
            .query(&[("api_key", login.access_token.as_str())])
            .header(header::USER_AGENT.as_str(), spoofed_user_agent)
            .header(header::ACCEPT.as_str(), "application/json")
            .header(header::ACCEPT_ENCODING.as_str(), "identity")
            .header("X-Emby-Token", login.access_token.as_str())
            .send()
            .await;
        match info_resp {
            Ok(resp) if resp.status().is_success() => {
                let info: Result<RemoteSystemInfo, _> =
                    parse_remote_json_response(resp, info_endpoint.as_str()).await;
                match info {
                    Ok(i) if !i.server_name.trim().is_empty() => i.server_name,
                    _ => server_url.to_string(),
                }
            }
            _ => server_url.to_string(),
        }
    };

    Ok(RemotePreviewResult {
        server_name,
        views: views
            .items
            .into_iter()
            .map(|view| RemoteViewPreview {
                id: view.id,
                name: view.name,
                collection_type: view.collection_type,
            })
            .collect(),
    })
}

fn normalize_display_mode(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case(REMOTE_DISPLAY_MODE_MERGE) {
        REMOTE_DISPLAY_MODE_MERGE
    } else {
        REMOTE_DISPLAY_MODE_SEPARATE
    }
}

pub async fn sync_source_with_progress(
    state: &AppState,
    source_id: Uuid,
    progress: Option<RemoteSyncProgress>,
) -> Result<RemoteEmbySyncResult, AppError> {
    let mut source = repository::get_remote_emby_source(&state.pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))?;
    if !source.enabled {
        return Err(AppError::BadRequest("远端 Emby 源已禁用".to_string()));
    }

    if let Some(handle) = &progress {
        handle.set_phase("Preparing", 1.0);
        if handle.is_cancelled() {
            return Err(AppError::BadRequest("同步任务已被取消".to_string()));
        }
    }
    let result = sync_source_inner(state, &mut source, progress.clone()).await;
    let sync_state_result = match &result {
        Ok(_) => {
            repository::update_remote_emby_source_sync_state(&state.pool, source.id, None).await
        }
        Err(error) => {
            let error_message = error.to_string();
            repository::update_remote_emby_source_sync_state(
                &state.pool,
                source.id,
                Some(error_message.as_str()),
            )
            .await
        }
    };
    if let Err(error) = sync_state_result {
        tracing::warn!(
            source_id = %source.id,
            error = %error,
            "更新远端 Emby 同步状态失败"
        );
    }
    if result.is_ok() {
        if let Some(handle) = &progress {
            handle.mark_completed();
        }
    }
    result
}

async fn sync_source_inner(
    state: &AppState,
    source: &mut DbRemoteEmbySource,
    progress: Option<RemoteSyncProgress>,
) -> Result<RemoteEmbySyncResult, AppError> {
    let display_mode_str = normalize_display_mode(source.display_mode.as_str());
    let target_library_opt = repository::get_library(&state.pool, source.target_library_id).await?;

    if let Some(handle) = &progress {
        handle.set_phase("CountingRemoteItems", 3.0);
    }
    let user_id = ensure_authenticated(&state.pool, source, false).await?;
    let mut views = fetch_remote_views(&state.pool, source, user_id.as_str()).await?;
    let selected_remote_views = normalize_remote_view_ids(source.remote_view_ids.as_slice());
    if !selected_remote_views.is_empty() {
        let selected_set: HashSet<String> = selected_remote_views
            .iter()
            .map(|value| value.to_ascii_lowercase())
            .collect();
        views.retain(|view| selected_set.contains(&view.id.to_ascii_lowercase()));
        if views.is_empty() {
            return Err(AppError::BadRequest(
                "未匹配到已选择的远端媒体库，请重新获取远端媒体库列表并保存".to_string(),
            ));
        }
    }

    // ── 灵活映射：为每个 View 解析目标本地库 ────────────────────
    // view_library_map 统一适用于 merge 和 separate 模式：
    //   - map 中有明确映射 → 使用该库（已验证存在）
    //   - map 中无映射 + merge 模式 → fallback 到 target_library_id
    //   - map 中无映射 + separate 模式 → 自动创建独立库
    let mut view_library_id_map: HashMap<String, Uuid> = HashMap::new();
    let map_obj = source.view_library_map
        .as_object()
        .cloned()
        .unwrap_or_default();
    for view in &views {
        let explicit_lib_id = map_obj
            .get(&view.id)
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<Uuid>().ok());

        // 验证显式映射的库是否存在
        let validated_lib_id = if let Some(lib_id) = explicit_lib_id {
            if repository::get_library(&state.pool, lib_id).await?.is_some() {
                Some(lib_id)
            } else {
                None
            }
        } else {
            None
        };

        let lib_id = if let Some(id) = validated_lib_id {
            id
        } else if display_mode_str == REMOTE_DISPLAY_MODE_MERGE {
            // merge 模式 fallback: 使用全局 target_library_id
            if target_library_opt.is_none() {
                return Err(AppError::BadRequest(
                    format!("远端库「{}」未指定目标本地库且全局目标库不存在", view.name),
                ));
            }
            source.target_library_id
        } else {
            // separate 模式: 自动创建独立库
            repository::ensure_view_library(
                &state.pool,
                source.id,
                &source.name,
                &view.id,
                &view.name,
                view.collection_type.as_deref(),
                None,
            )
            .await?
        };
        view_library_id_map.insert(view.id.clone(), lib_id);
    }
    // 持久化更新后的映射
    let updated_map: serde_json::Map<String, Value> = view_library_id_map
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.to_string())))
        .collect();
    let updated_map_value = Value::Object(updated_map);
    source.view_library_map = updated_map_value.clone();
    repository::update_source_view_library_map(&state.pool, source.id, &updated_map_value)
        .await?;

    // 将远程 View 虚拟路径注册到对应本地库的 PathInfos（合并时需要）
    for view in &views {
        if let Some(&lib_id) = view_library_id_map.get(&view.id) {
            repository::ensure_remote_view_path_in_library(
                &state.pool,
                lib_id,
                source.id,
                &view.id,
            )
            .await?;
        }
    }

    // 增量同步：有 last_sync_at 则仅同步变更条目；否则全量同步
    let incremental_since = source.last_sync_at;
    let is_incremental = incremental_since.is_some();

    let mut total_items = 0u64;
    for view in &views {
        let view_count = fetch_remote_items_total_count_for_view(
            &state.pool,
            source,
            user_id.as_str(),
            view.id.as_str(),
            incremental_since,
        )
        .await?;
        total_items = total_items.saturating_add(view_count);
    }

    // merge 模式的 source_root（用于清理旧 STRM 路径）
    let source_root = target_library_opt
        .as_ref()
        .map(|lib| source_root_path(lib, source))
        .unwrap_or_else(|| PathBuf::from(format!("__nonexistent_{}", source.id.simple())));

    let playback_token = source
        .access_token
        .as_ref()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| AppError::Internal("远端令牌为空，无法写入 STRM/同步条目".into()))?
        .clone();

    let strm_workspace = strm_workspace_for_source(source)?;

    if is_incremental {
        // ── 增量模式 ─────────────────────────────────────────────────────
        // STRM 工作区保持现有内容，不整体清空，但确保根目录存在（首次增量时可能还不存在）
        tokio::fs::create_dir_all(&strm_workspace)
            .await
            .map_err(|e| AppError::Internal(format!("创建 STRM 工作区失败: {e}")))?;

        if let Some(handle) = &progress {
            handle.set_phase("FetchingRemoteIndex", 4.0);
        }
        // 1. 获取远端全量 ID 集合，检测已删除条目
        let remote_id_set = fetch_all_remote_item_ids(
            &state.pool,
            source,
            user_id.as_str(),
            &views,
        )
        .await?;

        if let Some(handle) = &progress {
            handle.set_phase("PruningStaleItems", 5.0);
        }
        // 2. 从 DB 中删除远端已不存在的条目
        let deleted = delete_stale_items_for_source(
            &state.pool,
            source.id,
            source.target_library_id,
            &remote_id_set,
            Some(strm_workspace.as_path()),
        )
        .await?;
        if deleted > 0 {
            tracing::info!(
                source_id = %source.id,
                deleted,
                "增量同步：清理过时条目"
            );
        }
    } else {
        // ── 全量模式 ─────────────────────────────────────────────────────
        // 清空 STRM 工作区并清理 DB 中旧条目
        let _ = tokio::fs::remove_dir_all(&strm_workspace).await.ok();
        tokio::fs::create_dir_all(&strm_workspace)
            .await
            .map_err(|e| AppError::Internal(format!("创建 STRM 工作区失败: {e}")))?;
        if let Some(handle) = &progress {
            handle.set_phase("PreparingTargetLibrary", 5.0);
        }
        cleanup_remote_source_items(
            &state.pool,
            source.target_library_id,
            source.id,
            source_root.as_path(),
        )
        .await?;
    }

    let mut tvshow_roots_written: HashSet<PathBuf> = HashSet::new();
    // separate 模式不再需要虚拟根目录文件夹（每个 View 已是独立库）
    let mut series_parent_map: HashMap<String, Uuid> = HashMap::new();
    let mut season_parent_map: HashMap<String, Uuid> = HashMap::new();
    let mut fetched_count = 0u64;
    let mut written_files = 0usize;
    if let Some(handle) = &progress {
        handle.set_streaming_progress(0, 0, total_items);
    }

    for view in &views {
        let item_library_id = *view_library_id_map.get(&view.id).ok_or_else(|| {
            AppError::Internal(format!("View「{}」未找到对应本地库映射", view.name))
        })?;

        // 每个 View 在源根目录下独占子目录：{strm_root}/{source_name}/{view_name}/
        let view_strm_workspace: PathBuf =
            strm_workspace.join(sanitize_segment(view.name.as_str()));
        if let Err(err) = tokio::fs::create_dir_all(&view_strm_workspace).await {
            tracing::warn!(
                view = %view.name,
                error = %err,
                "创建 View STRM 子目录失败，继续尝试同步"
            );
        }

        let mut start_index = 0i64;
        loop {
            if let Some(handle) = &progress {
                if handle.is_cancelled() {
                    return Err(AppError::BadRequest("同步任务已被取消".to_string()));
                }
            }

            let page = fetch_remote_items_page_for_view(
                &state.pool,
                source,
                user_id.as_str(),
                view.id.as_str(),
                start_index,
                REMOTE_PAGE_SIZE,
                incremental_since,
            )
            .await?;
            if page.items.is_empty() {
                break;
            }

            for base_item in page.items {
                if let Some(handle) = &progress {
                    if handle.is_cancelled() {
                        return Err(AppError::BadRequest("同步任务已被取消".to_string()));
                    }
                }
                fetched_count = fetched_count.saturating_add(1);
                let item = RemoteSyncItem {
                    item: base_item,
                    view_id: view.id.clone(),
                    view_name: view.name.clone(),
                };

                // separate 模式：电影/剧集直接放在 View 库根层，无视图文件夹
                let mut parent_id: Option<Uuid> = None;
                let mut series_db_id: Option<Uuid> = None;

                if item.item.item_type.eq_ignore_ascii_case("Episode") {
                    let series_view_scope = item.view_id.as_str();
                    let series_parent_id = ensure_remote_series_folder(
                        &state.pool,
                        source,
                        &item,
                        None,
                        series_view_scope,
                        view_strm_workspace.as_path(),
                        item_library_id,
                        &mut series_parent_map,
                    )
                    .await?;
                    series_db_id = Some(series_parent_id);

                    let season_parent_id = ensure_remote_season_folder(
                        &state.pool,
                        source,
                        &item,
                        series_parent_id,
                        view_strm_workspace.as_path(),
                        item_library_id,
                        &mut season_parent_map,
                    )
                    .await?;
                    parent_id = Some(season_parent_id);
                }

                let media_source_id = first_media_source_id(&item);
                let analysis = fetch_remote_playback_analysis(
                    &state.pool,
                    source,
                    item.item.id.as_str(),
                    media_source_id,
                )
                .await
                .ok()
                .flatten();
                let strm_bundle = match write_remote_strm_bundle(
                    state,
                    source,
                    view_strm_workspace.as_path(),
                    playback_token.as_str(),
                    &item,
                    media_source_id,
                    &mut tvshow_roots_written,
                )
                .await
                {
                    Ok(paths) => paths,
                    Err(error) => {
                        // 单条写盘失败不再降级到虚拟路径，跳过该条目继续处理下一项，
                        // 避免一处磁盘异常污染整库 path 字段。
                        tracing::warn!(
                            remote_item_id = %item.item.id,
                            error = %error,
                            "STRM 旁路写入失败，跳过此条目"
                        );
                        continue;
                    }
                };
                let (strm_path, poster_path, backdrop_path, logo_path) = strm_bundle;
                let upserted = upsert_remote_media_item(
                    &state.pool,
                    source,
                    &item,
                    parent_id,
                    item_library_id,
                    media_source_id,
                    analysis.as_ref(),
                    strm_path.as_path(),
                    poster_path.as_deref(),
                    backdrop_path.as_deref(),
                    logo_path.as_deref(),
                    series_db_id,
                )
                .await?;
                if let Some(analysis) = analysis {
                    repository::save_media_streams(&state.pool, upserted, &analysis).await?;
                    repository::update_media_item_metadata(&state.pool, upserted, &analysis)
                        .await?;
                }

                written_files = written_files.saturating_add(1);
                if let Some(handle) = &progress {
                    handle.set_streaming_progress(fetched_count, written_files as u64, total_items);
                }
            }

            start_index += REMOTE_PAGE_SIZE;
            if start_index >= page.total_record_count {
                break;
            }
        }
    }

    let mode_label = if is_incremental { "增量" } else { "全量" };
    tracing::info!(
        source_id = %source.id,
        mode = mode_label,
        fetched = fetched_count,
        written = written_files,
        "远端 Emby 同步完成"
    );

    let scan_summary = ScanSummary {
        libraries: 1,
        scanned_files: fetched_count as i64,
        imported_items: written_files as i64,
    };

    Ok(RemoteEmbySyncResult {
        source_id: source.id,
        source_name: source.name.clone(),
        written_files,
        source_root: strm_workspace.to_string_lossy().to_string(),
        scan_summary,
    })
}

pub async fn proxy_item_stream(
    state: &AppState,
    source_id: Uuid,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    signature: &str,
    method: Method,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let mut source = repository::get_remote_emby_source(&state.pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))?;
    if !source.enabled {
        return Err(AppError::NotFound("远端 Emby 源已禁用".to_string()));
    }

    let expected_signature =
        build_proxy_signature(source.source_secret, remote_item_id, media_source_id);
    if signature.trim() != expected_signature {
        return Err(AppError::Forbidden);
    }

    proxy_item_stream_internal_with_source(
        state,
        &mut source,
        remote_item_id,
        media_source_id,
        method,
        headers,
    )
    .await
}

pub async fn proxy_item_stream_internal(
    state: &AppState,
    source_id: Uuid,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    method: Method,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let mut source = repository::get_remote_emby_source(&state.pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))?;
    if !source.enabled {
        return Err(AppError::NotFound("远端 Emby 源已禁用".to_string()));
    }
    proxy_item_stream_internal_with_source(
        state,
        &mut source,
        remote_item_id,
        media_source_id,
        method,
        headers,
    )
    .await
}

async fn proxy_item_stream_internal_with_source(
    state: &AppState,
    source: &mut DbRemoteEmbySource,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    method: Method,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    // --- redirect 模式：302 直链重定向，客户端直连远端，节省本地带宽 ---
    if source.is_redirect_mode() {
        let token = ensure_authenticated(&state.pool, source, false).await?;
        let redirect_url =
            build_remote_stream_redirect_url(source, remote_item_id, media_source_id, &token);
        return Response::builder()
            .status(StatusCode::FOUND)
            .header(header::LOCATION, redirect_url.as_str())
            .header(header::CACHE_CONTROL, "no-store")
            .body(Body::empty())
            .map_err(|e| AppError::Internal(format!("构建重定向响应失败: {e}")));
    }

    // --- proxy 模式（默认）：本地中转流量 ---
    let remote_response = send_remote_stream_request(
        &state.pool,
        source,
        remote_item_id,
        media_source_id,
        &method,
        headers,
    )
    .await?;

    let status =
        StatusCode::from_u16(remote_response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let upstream_headers = remote_response.headers().clone();
    let mut response_builder = Response::builder().status(status);
    for (key, value) in upstream_headers.iter() {
        if is_hop_by_hop_header(key.as_str()) {
            continue;
        }
        if let Ok(value) = value.to_str() {
            response_builder = response_builder.header(key.as_str(), value);
        }
    }

    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(remote_response.bytes_stream())
    };
    response_builder
        .body(body)
        .map_err(|error| AppError::Internal(format!("构建远端代理响应失败: {error}")))
}

/// 为 redirect 模式构建远端直播流 URL（含 api_key / MediaSourceId）
fn build_remote_stream_redirect_url(
    source: &DbRemoteEmbySource,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    token: &str,
) -> String {
    let base = source.server_url.trim_end_matches('/');
    let mut url = format!(
        "{base}/emby/Videos/{remote_item_id}/stream?Static=true&api_key={token}"
    );
    if let Some(msid) = media_source_id.filter(|s| !s.trim().is_empty()) {
        url.push_str("&MediaSourceId=");
        url.push_str(msid);
    }
    url
}

fn source_root_path(library: &DbLibrary, source: &DbRemoteEmbySource) -> PathBuf {
    PathBuf::from(&library.path)
        .join("_remote_emby")
        .join(sanitize_segment(&source.name))
}

#[derive(Debug, Clone)]
pub struct RemoteVirtualMediaPath {
    pub source_id: Uuid,
    pub remote_item_id: String,
}

pub fn parse_virtual_media_path(path: &str) -> Option<RemoteVirtualMediaPath> {
    let normalized = path.trim().replace('\\', "/");
    let segments = normalized.split('/').collect::<Vec<_>>();
    if segments.len() < 4 {
        return None;
    }
    if !segments[0].eq_ignore_ascii_case("REMOTE_EMBY")
        || !segments[2].eq_ignore_ascii_case("items")
    {
        return None;
    }
    let source_id = Uuid::parse_str(segments[1]).ok()?;
    let remote_item_id = segments[3].trim().to_string();
    if remote_item_id.is_empty() {
        return None;
    }
    Some(RemoteVirtualMediaPath {
        source_id,
        remote_item_id,
    })
}

/// 从 `media_items.provider_ids` 标记字段反查远端 source_id + remote item id。
///
/// 这是 STRM 输出根目录改必填后判定"是否远端条目"的主入口；
/// 旧版本依赖 `parse_virtual_media_path(&item.path)`，对真实物理路径无效。
/// 新增本函数后，路由层先用 provider_ids 判定，必要时再回退到 parse_virtual_media_path
/// 以兼容尚未触发再同步的旧虚拟路径数据。
pub fn remote_marker_for_item(provider_ids: &Value) -> Option<RemoteVirtualMediaPath> {
    let source_id_str = provider_ids
        .get("RemoteEmbySourceId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let remote_item_id = provider_ids
        .get("RemoteEmbyItemId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?
        .to_string();
    let source_id = Uuid::parse_str(source_id_str).ok()?;
    Some(RemoteVirtualMediaPath {
        source_id,
        remote_item_id,
    })
}

/// 路由层使用：先按 provider_ids 标记判定，再兜底解析旧虚拟字符串路径。
pub fn remote_marker_for_db_item(item: &crate::models::DbMediaItem) -> Option<RemoteVirtualMediaPath> {
    remote_marker_for_item(&item.provider_ids).or_else(|| parse_virtual_media_path(&item.path))
}

pub fn remote_default_media_source_id(provider_ids: &Value) -> Option<String> {
    provider_ids
        .get("RemoteEmbyMediaSourceId")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// 旧版本（虚拟路径阶段）所有 Folder/View/Series/Season/Item 都用 `REMOTE_EMBY/{source}/...`
/// 形式落库。现在所有远端条目都改为真实 strm 物理目录，但 `cleanup_remote_source_items`
/// 仍要清理历史虚拟字符串行，因此保留这个前缀作为兼容判定。
fn legacy_virtual_root_prefix(source_id: Uuid) -> String {
    format!("REMOTE_EMBY/{}/root", source_id)
}

fn remote_image_url(server_url: &str, remote_item_id: &str, image_type: &str, tag: &str) -> String {
    let base = server_url.trim_end_matches('/');
    format!(
        "{base}/emby/Items/{remote_item_id}/Images/{image_type}?tag={tag}&quality=90&maxWidth=1920"
    )
}

fn extract_remote_image_urls(
    server_url: &str,
    remote_item_id: &str,
    image_tags: &Option<Value>,
    backdrop_image_tags: &Option<Value>,
) -> (Option<String>, Option<String>) {
    let primary = image_tags.as_ref().and_then(|tags| {
        let tag = match tags {
            Value::Object(map) => map.get("Primary").and_then(Value::as_str),
            _ => None,
        };
        tag.map(|t| remote_image_url(server_url, remote_item_id, "Primary", t))
    });
    let backdrop = backdrop_image_tags.as_ref().and_then(|tags| {
        match tags {
            Value::Array(arr) => arr.first().and_then(Value::as_str),
            Value::Object(map) => map.values().next().and_then(Value::as_str),
            Value::String(s) => Some(s.as_str()),
            _ => None,
        }
        .map(|t| remote_image_url(server_url, remote_item_id, "Backdrop", t))
    });
    (primary, backdrop)
}

fn parse_remote_premiere_date(value: Option<&str>) -> Option<NaiveDate> {
    let raw = value?.trim();
    if raw.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.date_naive())
        .or_else(|| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok())
}

fn remote_marker_provider_ids(
    source_id: Uuid,
    remote_item_id: Option<&str>,
    view_id: Option<&str>,
    media_source_id: Option<&str>,
) -> Value {
    json!({
        "RemoteEmbySourceId": source_id.to_string(),
        "RemoteEmbyItemId": remote_item_id.unwrap_or_default(),
        "RemoteEmbyViewId": view_id.unwrap_or_default(),
        "RemoteEmbyMediaSourceId": media_source_id.unwrap_or_default()
    })
}

fn merge_provider_ids(base: &Value, markers: Value) -> Value {
    let mut object = base.as_object().cloned().unwrap_or_default();
    if let Some(extra) = markers.as_object() {
        for (key, value) in extra {
            object.insert(key.clone(), value.clone());
        }
    }
    Value::Object(object)
}

async fn cleanup_remote_source_items(
    pool: &sqlx::PgPool,
    _library_id: Uuid,
    source_id: Uuid,
    legacy_source_root: &Path,
) -> Result<u64, AppError> {
    let virtual_prefix = format!("{}%", legacy_virtual_root_prefix(source_id));
    let virtual_prefix_windows = virtual_prefix.replace('/', "\\");
    let legacy_prefix = format!("{}%", legacy_source_root.to_string_lossy());
    // 不再限制 library_id，支持 separate 模式下条目分散在多个库
    let result = sqlx::query(
        r#"
        DELETE FROM media_items
        WHERE (
            provider_ids ->> 'RemoteEmbySourceId' = $1
            OR path LIKE $2
            OR path LIKE $3
            OR path LIKE $4
        )
        "#,
    )
    .bind(source_id.to_string())
    .bind(virtual_prefix)
    .bind(virtual_prefix_windows)
    .bind(legacy_prefix)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn cleanup_source_mapped_items(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
) -> Result<u64, AppError> {
    // 清理时需要一个 source_root 路径（用于清理 legacy STRM 文件），从目标库或任意视图库取
    let target_lib_opt = repository::get_library(pool, source.target_library_id).await?;
    // 提供一个空路径占位，cleanup_remote_source_items 主要靠 RemoteEmbySourceId 识别
    let source_root = target_lib_opt
        .as_ref()
        .map(|lib| source_root_path(lib, source))
        .unwrap_or_else(|| PathBuf::from(format!("__nonexistent_{}", source.id.simple())));
    let deleted = cleanup_remote_source_items(
        pool,
        source.target_library_id,
        source.id,
        source_root.as_path(),
    )
    .await?;

    match tokio::fs::remove_dir_all(&source_root).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                source_id = %source.id,
                source_root = %source_root.to_string_lossy(),
                error = %error,
                "删除远端 Emby 旧映射目录失败"
            );
        }
    }

    Ok(deleted)
}

async fn ensure_remote_series_folder(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    item: &RemoteSyncItem,
    parent_id: Option<Uuid>,
    view_scope: &str,
    view_workspace: &Path,
    library_id: Uuid,
    series_parent_map: &mut HashMap<String, Uuid>,
) -> Result<Uuid, AppError> {
    let raw_series_name = item
        .item
        .series_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Unknown Series");
    let series_name = raw_series_name.trim().to_string();
    // 优先使用 SeriesId 做去重 key，避免同名不同剧冲突
    let series_key = if let Some(sid) = item.item.series_id.as_deref().filter(|s| !s.trim().is_empty()) {
        format!("{view_scope}::{sid}")
    } else {
        format!("{view_scope}::{}", sanitize_segment(series_name.as_str()))
    };
    if let Some(existing) = series_parent_map.get(series_key.as_str()).copied() {
        return Ok(existing);
    }
    // 物理目录：{view_workspace}/{sanitize(series_name)}/
    // 与 build_relative_strm_path 中 episode 落盘目录保持一致，
    // 这样 tvshow.nfo / poster.jpg / fanart.jpg 都能落到 series 真实目录。
    let series_dir = view_workspace.join(sanitize_segment(series_name.as_str()));
    if let Err(err) = tokio::fs::create_dir_all(&series_dir).await {
        tracing::warn!(
            series_dir = %series_dir.to_string_lossy(),
            error = %err,
            "创建 Series 物理目录失败"
        );
    }
    let path_string = series_dir.to_string_lossy().to_string();
    let path_ref = Path::new(path_string.as_str());
    let empty = Vec::<String>::new();
    let series_primary_url = item
        .item
        .series_id
        .as_deref()
        .map(|sid| {
            if let Some(tag) = item.item.series_primary_image_tag.as_deref() {
                remote_image_url(source.server_url.as_str(), sid, "Primary", tag)
            } else {
                let base = source.server_url.trim_end_matches('/');
                format!("{base}/emby/Items/{sid}/Images/Primary?quality=90&maxWidth=1920")
            }
        });
    let series_backdrop_url = item
        .item
        .series_id
        .as_deref()
        .and_then(|sid| {
            let tag = item.item.parent_backdrop_image_tags.as_ref().and_then(|tags| {
                match tags {
                    Value::Array(arr) => arr.first().and_then(Value::as_str),
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                }
            });
            tag.map(|t| remote_image_url(source.server_url.as_str(), sid, "Backdrop", t))
        });
    // Logo: 从 Episode 的 ParentLogoItemId + ParentLogoImageTag 推导 Series logo URL
    let series_logo_url = item
        .item
        .parent_logo_item_id
        .as_deref()
        .and_then(|logo_item_id| {
            item.item
                .parent_logo_image_tag
                .as_deref()
                .map(|tag| remote_image_url(source.server_url.as_str(), logo_item_id, "Logo", tag))
        });
    let series_primary_path = series_primary_url.as_ref().map(|s| Path::new(s.as_str()));
    let series_backdrop_path = series_backdrop_url.as_ref().map(|s| Path::new(s.as_str()));
    let series_logo_path = series_logo_url.as_ref().map(|s| Path::new(s.as_str()));
    let (item_id, _was_new) = repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id,
            parent_id,
            name: series_name.as_str(),
            item_type: "Series",
            media_type: "Video",
            path: path_ref,
            container: None,
            original_title: None,
            overview: None,
            production_year: item.item.production_year,
            official_rating: item.item.official_rating.as_deref(),
            community_rating: item.item.community_rating,
            critic_rating: item.item.critic_rating,
            runtime_ticks: None,
            premiere_date: parse_remote_premiere_date(item.item.premiere_date.as_deref()),
            status: item.item.status.as_deref(),
            end_date: parse_remote_premiere_date(item.item.end_date.as_deref()),
            air_days: &empty,
            air_time: None,
            provider_ids: remote_marker_provider_ids(source.id, None, Some(&item.view_id), None),
            genres: &item.item.genres,
            studios: &item.item.studios,
            tags: &item.item.tags,
            production_locations: &empty,
            image_primary_path: series_primary_path,
            backdrop_path: series_backdrop_path,
            logo_path: series_logo_path,
            thumb_path: None,
            art_path: None,
            banner_path: None,
            disc_path: None,
            backdrop_paths: &empty,
            remote_trailers: &empty,
            series_name: Some(series_name.as_str()),
            season_name: None,
            index_number: None,
            index_number_end: None,
            parent_index_number: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
            series_id: None,
        },
    )
    .await?;
    series_parent_map.insert(series_key, item_id);
    Ok(item_id)
}

async fn ensure_remote_season_folder(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    item: &RemoteSyncItem,
    series_parent_id: Uuid,
    view_workspace: &Path,
    library_id: Uuid,
    season_parent_map: &mut HashMap<String, Uuid>,
) -> Result<Uuid, AppError> {
    let season_number = item.item.parent_index_number.unwrap_or(0).clamp(0, 999);
    let season_key = format!("{}::{season_number}", series_parent_id);
    if let Some(existing) = season_parent_map.get(season_key.as_str()).copied() {
        return Ok(existing);
    }
    let series_name_raw = item
        .item
        .series_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Unknown Series");
    // 物理目录：{view_workspace}/{sanitize(series_name)}/Season {NN}/
    // 与 build_relative_strm_path 中 episode 落盘的 Season 目录完全一致。
    let season_dir = view_workspace
        .join(sanitize_segment(series_name_raw))
        .join(format!("Season {season_number:02}"));
    if let Err(err) = tokio::fs::create_dir_all(&season_dir).await {
        tracing::warn!(
            season_dir = %season_dir.to_string_lossy(),
            error = %err,
            "创建 Season 物理目录失败"
        );
    }
    let path_string = season_dir.to_string_lossy().to_string();
    let path_ref = Path::new(path_string.as_str());
    let season_name = item
        .item
        .season_name
        .clone()
        .unwrap_or_else(|| format!("Season {season_number:02}"));
    let empty = Vec::<String>::new();
    let season_primary_url = item
        .item
        .season_id
        .as_deref()
        .map(|sid| {
            let base = source.server_url.trim_end_matches('/');
            format!("{base}/emby/Items/{sid}/Images/Primary?tag=0&quality=90&maxWidth=1920")
        });
    let season_primary_path = season_primary_url.as_ref().map(|s| Path::new(s.as_str()));
    let (item_id, _was_new) = repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id,
            parent_id: Some(series_parent_id),
            name: season_name.as_str(),
            item_type: "Season",
            media_type: "Video",
            path: path_ref,
            container: None,
            original_title: None,
            overview: None,
            production_year: item.item.production_year,
            official_rating: item.item.official_rating.as_deref(),
            community_rating: item.item.community_rating,
            critic_rating: item.item.critic_rating,
            runtime_ticks: None,
            premiere_date: parse_remote_premiere_date(item.item.premiere_date.as_deref()),
            status: item.item.status.as_deref(),
            end_date: parse_remote_premiere_date(item.item.end_date.as_deref()),
            air_days: &empty,
            air_time: None,
            provider_ids: remote_marker_provider_ids(source.id, None, Some(&item.view_id), None),
            genres: &item.item.genres,
            studios: &item.item.studios,
            tags: &item.item.tags,
            production_locations: &empty,
            image_primary_path: season_primary_path,
            backdrop_path: None,
            logo_path: None,
            thumb_path: None,
            art_path: None,
            banner_path: None,
            disc_path: None,
            backdrop_paths: &empty,
            remote_trailers: &empty,
            series_name: item.item.series_name.as_deref(),
            season_name: Some(season_name.as_str()),
            index_number: Some(season_number),
            index_number_end: None,
            parent_index_number: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
            series_id: Some(series_parent_id),
        },
    )
    .await?;
    season_parent_map.insert(season_key, item_id);
    Ok(item_id)
}

/// 向 DB 中 upsert 一个远端 Movie/Episode。
///
/// 自 STRM 输出根目录改为必填后，本函数要求调用方必须传入真实 strm 物理路径
/// （由 `write_remote_strm_bundle` 返回），不再保留虚拟字符串兜底路径。
async fn upsert_remote_media_item(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    item: &RemoteSyncItem,
    parent_id: Option<Uuid>,
    library_id: Uuid,
    media_source_id: Option<&str>,
    analysis: Option<&MediaAnalysisResult>,
    strm_path: &Path,
    local_poster: Option<&Path>,
    local_backdrop: Option<&Path>,
    local_logo: Option<&Path>,
    series_db_id: Option<Uuid>,
) -> Result<Uuid, AppError> {
    let container = analysis.and_then(|value| value.format.format_name.as_deref());
    let video_codec = analysis.and_then(|value| {
        value
            .streams
            .iter()
            .find(|stream| stream.codec_type == "video")
            .and_then(|stream| stream.codec_name.as_deref())
    });
    let audio_codec = analysis.and_then(|value| {
        value
            .streams
            .iter()
            .find(|stream| stream.codec_type == "audio")
            .and_then(|stream| stream.codec_name.as_deref())
    });
    let provider_ids = merge_provider_ids(
        &item.item.provider_ids,
        remote_marker_provider_ids(
            source.id,
            Some(item.item.id.as_str()),
            Some(item.view_id.as_str()),
            media_source_id,
        ),
    );
    let path_ref = strm_path;
    let is_episode = item.item.item_type.eq_ignore_ascii_case("Episode");
    let item_type = if is_episode { "Episode" } else { "Movie" };
    let runtime_ticks = item.item.run_time_ticks.or_else(|| {
        analysis.and_then(|value| {
            value
                .format
                .duration
                .as_deref()
                .and_then(|raw| raw.parse::<f64>().ok())
                .map(|seconds| (seconds * 10_000_000.0).round() as i64)
        })
    });
    let (remote_primary, remote_backdrop) = extract_remote_image_urls(
        source.server_url.as_str(),
        item.item.id.as_str(),
        &item.item.image_tags,
        &item.item.backdrop_image_tags,
    );
    // Episode 回退：当自身没有 Primary 图时，使用 Series 的 Primary 图
    let series_fallback_primary = if remote_primary.is_none() && is_episode {
        item.item.series_id.as_deref().map(|sid| {
            if let Some(tag) = item.item.series_primary_image_tag.as_deref() {
                remote_image_url(source.server_url.as_str(), sid, "Primary", tag)
            } else {
                let base = source.server_url.trim_end_matches('/');
                format!("{base}/emby/Items/{sid}/Images/Primary?quality=90&maxWidth=1920")
            }
        })
    } else {
        None
    };
    let image_primary_path = local_poster
        .or_else(|| remote_primary.as_ref().map(|s| Path::new(s.as_str())))
        .or_else(|| series_fallback_primary.as_ref().map(|s| Path::new(s.as_str())));
    // Episode 回退：当自身没有 Backdrop 时，使用 ParentBackdropItemId 的 Backdrop
    let series_fallback_backdrop = if remote_backdrop.is_none() && is_episode {
        let backdrop_item = item
            .item
            .parent_backdrop_item_id
            .as_deref()
            .or(item.item.series_id.as_deref());
        backdrop_item.and_then(|bid| {
            let tag = item.item.parent_backdrop_image_tags.as_ref().and_then(|tags| {
                match tags {
                    Value::Array(arr) => arr.first().and_then(Value::as_str),
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                }
            });
            tag.map(|t| remote_image_url(source.server_url.as_str(), bid, "Backdrop", t))
        })
    } else {
        None
    };
    let backdrop_path = local_backdrop
        .or_else(|| remote_backdrop.as_ref().map(|s| Path::new(s.as_str())))
        .or_else(|| series_fallback_backdrop.as_ref().map(|s| Path::new(s.as_str())));
    let empty = Vec::<String>::new();
    let empty_backdrops = Vec::<String>::new();
    let empty_trailers = Vec::<String>::new();
    repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id,
            parent_id,
            name: item.item.name.as_str(),
            item_type,
            media_type: "Video",
            path: path_ref,
            container,
            original_title: None,
            overview: item.item.overview.as_deref(),
            production_year: item.item.production_year,
            official_rating: item.item.official_rating.as_deref(),
            community_rating: item.item.community_rating,
            critic_rating: item.item.critic_rating,
            runtime_ticks,
            premiere_date: parse_remote_premiere_date(item.item.premiere_date.as_deref()),
            status: None,
            end_date: None,
            air_days: &empty,
            air_time: None,
            provider_ids,
            genres: &item.item.genres,
            studios: &item.item.studios,
            tags: &item.item.tags,
            production_locations: &empty,
            image_primary_path,
            backdrop_path,
            logo_path: local_logo,
            thumb_path: None,
            art_path: None,
            banner_path: None,
            disc_path: None,
            backdrop_paths: &empty_backdrops,
            remote_trailers: &empty_trailers,
            series_name: item.item.series_name.as_deref(),
            season_name: item.item.season_name.as_deref(),
            index_number: item.item.index_number,
            index_number_end: None,
            parent_index_number: item.item.parent_index_number,
            width: None,
            height: None,
            video_codec,
            audio_codec,
            series_id: series_db_id,
        },
    )
    .await
    .map(|(id, _was_new)| id)
}

#[allow(dead_code)]
async fn fetch_all_remote_items(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    progress: Option<&RemoteSyncProgress>,
) -> Result<Vec<RemoteSyncItem>, AppError> {
    let user_id = ensure_authenticated(pool, source, false).await?;
    let views = fetch_remote_views(pool, source, user_id.as_str()).await?;
    let mut total_count = 0u64;
    for view in &views {
        let view_count = fetch_remote_items_total_count_for_view(
            pool,
            source,
            user_id.as_str(),
            view.id.as_str(),
            None,
        )
        .await?;
        total_count = total_count.saturating_add(view_count);
    }

    let mut fetched_count = 0u64;
    let mut all_items = Vec::new();
    for view in views {
        let mut start_index = 0i64;
        loop {
            let page = fetch_remote_items_page_for_view(
                pool,
                source,
                user_id.as_str(),
                view.id.as_str(),
                start_index,
                REMOTE_PAGE_SIZE,
                None,
            )
            .await?;

            if page.items.is_empty() {
                break;
            }

            fetched_count = fetched_count.saturating_add(page.items.len() as u64);
            all_items.extend(page.items.into_iter().map(|item| RemoteSyncItem {
                item,
                view_id: view.id.clone(),
                view_name: view.name.clone(),
            }));
            start_index += REMOTE_PAGE_SIZE;

            if let Some(handle) = progress {
                let expected = total_count.max(fetched_count);
                handle.set_fetch_progress(fetched_count, expected);
            }

            if start_index >= page.total_record_count {
                break;
            }
        }
    }

    if let Some(handle) = progress {
        let expected = total_count.max(fetched_count);
        handle.set_fetch_progress(fetched_count, expected);
    }
    Ok(all_items)
}

async fn fetch_remote_views(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
) -> Result<Vec<RemoteLibraryView>, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Views");
    let query = vec![
        (
            "Fields".to_string(),
            "CollectionType,ChildCount,RecursiveItemCount".to_string(),
        ),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
    ];
    let mut response: RemoteViewsResult =
        get_json_with_retry(pool, source, &endpoint, &query).await?;
    response
        .items
        .retain(|view| !view.id.trim().is_empty() && !view.name.trim().is_empty());
    Ok(response.items)
}

async fn fetch_remote_items_total_count_for_view(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    min_date_last_saved: Option<chrono::DateTime<Utc>>,
) -> Result<u64, AppError> {
    let page = fetch_remote_items_page_for_view(pool, source, user_id, view_id, 0, 1, min_date_last_saved).await?;
    Ok(page.total_record_count.max(0) as u64)
}

async fn fetch_remote_items_page_for_view(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    start_index: i64,
    limit: i64,
    min_date_last_saved: Option<chrono::DateTime<Utc>>,
) -> Result<RemoteItemsResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    let mut query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("ParentId".to_string(), view_id.to_string()),
        ("IncludeItemTypes".to_string(), "Movie,Episode".to_string()),
        (
            "Fields".to_string(),
            "SeriesName,SeasonName,ProductionYear,ParentIndexNumber,IndexNumber,Overview,\
             OfficialRating,CommunityRating,CriticRating,PremiereDate,RunTimeTicks,\
             ProviderIds,Genres,Studios,Tags,MediaSources,MediaStreams,\
             ImageTags,BackdropImageTags,SeriesId,SeasonId,\
             SeriesPrimaryImageTag,ParentBackdropImageTags,ParentBackdropItemId,\
             ParentLogoItemId,ParentLogoImageTag,Status,EndDate"
                .to_string(),
        ),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("SortBy".to_string(), "SortName".to_string()),
        ("SortOrder".to_string(), "Ascending".to_string()),
        ("StartIndex".to_string(), start_index.to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    if let Some(since) = min_date_last_saved {
        query.push(("MinDateLastSaved".to_string(), since.to_rfc3339()));
    }
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 仅拉取远端条目的 Id 列表（用于增量刷新中的删除检测）
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteItemIdEntry {
    id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteItemIdsResult {
    #[serde(default)]
    items: Vec<RemoteItemIdEntry>,
    total_record_count: i64,
}

const REMOTE_ID_PAGE_SIZE: i64 = 1000;

async fn fetch_remote_item_ids_page(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    start_index: i64,
    limit: i64,
) -> Result<RemoteItemIdsResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    let query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("ParentId".to_string(), view_id.to_string()),
        ("IncludeItemTypes".to_string(), "Movie,Episode".to_string()),
        ("Fields".to_string(), "Id".to_string()),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("StartIndex".to_string(), start_index.to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 拉取所有视图下的远端条目 ID 集合，用于增量同步时检测已删除的条目
async fn fetch_all_remote_item_ids(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    views: &[RemoteLibraryView],
) -> Result<HashSet<String>, AppError> {
    let mut all_ids = HashSet::new();
    for view in views {
        let mut start_index = 0i64;
        loop {
            let page = fetch_remote_item_ids_page(
                pool,
                source,
                user_id,
                &view.id,
                start_index,
                REMOTE_ID_PAGE_SIZE,
            )
            .await?;
            for entry in &page.items {
                all_ids.insert(entry.id.clone());
            }
            start_index += REMOTE_ID_PAGE_SIZE;
            if page.items.is_empty() || start_index >= page.total_record_count {
                break;
            }
        }
    }
    Ok(all_ids)
}

/// 删除本地 DB 中不再存在于远端的条目，并清理对应的 STRM 文件
async fn delete_stale_items_for_source(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    _library_id: Uuid,
    remote_id_set: &HashSet<String>,
    strm_workspace: Option<&Path>,
) -> Result<u64, AppError> {
    let source_id_str = source_id.to_string();
    struct StaleRow {
        id: Uuid,
        path: Option<String>,
        remote_id: Option<String>,
    }
    // 不再限制 library_id，支持 separate 模式下条目分散在多个库
    let rows: Vec<StaleRow> = sqlx::query(
        r#"
        SELECT id, path, provider_ids->>'RemoteEmbyItemId' AS remote_id
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND provider_ids->>'RemoteEmbyItemId' IS NOT NULL
        "#,
    )
    .bind(&source_id_str)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        use sqlx::Row;
        StaleRow {
            id: row.get("id"),
            path: row.get("path"),
            remote_id: row.get("remote_id"),
        }
    })
    .collect();

    let mut deleted = 0u64;
    for row in rows {
        let Some(ref remote_id) = row.remote_id else {
            continue;
        };
        if remote_id_set.contains(remote_id.as_str()) {
            continue;
        }
        // 清理 STRM 文件及旁路文件
        if let Some(workspace) = strm_workspace {
            if let Some(ref path_str) = row.path {
                let item_path = Path::new(path_str.as_str());
                if path_str.ends_with(".strm") && item_path.starts_with(workspace) {
                    // 删除同目录下同文件名前缀的所有旁路文件
                    if let Some(parent) = item_path.parent() {
                        if let Some(stem) = item_path.file_stem().and_then(|s| s.to_str()) {
                            let stem = stem.to_string();
                            if let Ok(mut dir_entries) = tokio::fs::read_dir(parent).await {
                                while let Ok(Some(entry)) = dir_entries.next_entry().await {
                                    let fname = entry.file_name();
                                    let fname_str = fname.to_string_lossy();
                                    if fname_str.starts_with(stem.as_str())
                                        || fname_str == "poster.jpg"
                                        || fname_str == "backdrop.jpg"
                                        || fname_str == "logo.png"
                                        || fname_str == "movie.nfo"
                                    {
                                        let _ = tokio::fs::remove_file(entry.path()).await;
                                    }
                                }
                            }
                            // 尝试删除父目录（如已空）
                            let _ = tokio::fs::remove_dir(parent).await;
                        }
                    }
                }
            }
        }
        // 删除 DB 记录
        sqlx::query("DELETE FROM media_items WHERE id = $1")
            .bind(row.id)
            .execute(pool)
            .await?;
        deleted += 1;
    }
    Ok(deleted)
}

/// 构造远端 Emby 的 Static 直链 URL，跳过 PlaybackInfo 往返
fn build_remote_static_stream_url(
    server_url: &str,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    token: &str,
    device_id: &str,
) -> String {
    let base = server_url.trim_end_matches('/');
    let msid = media_source_id
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(remote_item_id);
    format!(
        "{base}/emby/videos/{remote_item_id}/stream?Static=true\
         &MediaSourceId={msid}\
         &DeviceId={device_id}\
         &api_key={token}"
    )
}

/// 向远端发起流请求，附带认证头和客户端透传头
fn build_remote_stream_builder(
    client: &reqwest::Client,
    endpoint: &str,
    method: &Method,
    source: &DbRemoteEmbySource,
    token: &str,
    request_headers: &HeaderMap,
    extra_headers: &HashMap<String, String>,
) -> reqwest::RequestBuilder {
    let normalized_method = if *method == Method::HEAD {
        Method::HEAD
    } else {
        Method::GET
    };
    let mut builder = if normalized_method == Method::HEAD {
        client.head(endpoint)
    } else {
        client.get(endpoint)
    };
    builder = builder
        .header(
            header::USER_AGENT.as_str(),
            source.spoofed_user_agent.as_str(),
        )
        .header("X-Emby-Token", token)
        .header(
            "X-Emby-Authorization",
            emby_auth_header(source, Some(token)),
        );
    for (key, value) in extra_headers {
        if key.trim().is_empty() || value.trim().is_empty() {
            continue;
        }
        if is_hop_by_hop_header(key.as_str())
            || key.eq_ignore_ascii_case("Host")
            || key.eq_ignore_ascii_case("Content-Length")
        {
            continue;
        }
        builder = builder.header(key.as_str(), value.as_str());
    }
    for name in [
        header::RANGE,
        header::IF_RANGE,
        header::ACCEPT,
        header::ACCEPT_LANGUAGE,
    ] {
        if let Some(value) = request_headers
            .get(&name)
            .and_then(|value| value.to_str().ok())
        {
            builder = builder.header(name.as_str(), value);
        }
    }
    builder
}

async fn send_remote_stream_request(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    method: &Method,
    request_headers: &HeaderMap,
) -> Result<reqwest::Response, AppError> {
    let client = &*crate::http_client::SHARED;
    let empty_headers = HashMap::new();

    for attempt in 0..2 {
        let _user_id = ensure_authenticated(pool, source, attempt > 0).await?;
        let token = source
            .access_token
            .clone()
            .ok_or_else(|| AppError::Internal("远端登录令牌为空".to_string()))?;
        let server_url = normalize_server_url(&source.server_url);
        let device_id = format!("movie-rust-{}", source.id.simple());

        // ── 快速路径：直接构造 Static URL，跳过 PlaybackInfo 往返 ──
        let static_url = build_remote_static_stream_url(
            &server_url,
            remote_item_id,
            media_source_id,
            &token,
            &device_id,
        );
        let builder = build_remote_stream_builder(
            client,
            &static_url,
            method,
            source,
            &token,
            request_headers,
            &empty_headers,
        );
        match builder.send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() || status == reqwest::StatusCode::PARTIAL_CONTENT {
                    return Ok(response);
                }
                if matches!(
                    status,
                    reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
                ) && attempt == 0
                {
                    repository::clear_remote_emby_source_auth_state(pool, source.id).await?;
                    source.access_token = None;
                    source.remote_user_id = None;
                    continue;
                }
                // Static URL 返回了非成功状态，回退到 PlaybackInfo 路径
            }
            Err(_) => {
                // 网络错误，回退到 PlaybackInfo
            }
        }

        // ── 回退路径：通过 PlaybackInfo 获取 DirectStreamUrl/TranscodingUrl ──
        // token 在本轮已获取且未被清除，直接复用
        let user_id = source
            .remote_user_id
            .clone()
            .ok_or_else(|| AppError::Internal("远端用户ID为空".to_string()))?;

        let cache_key = playback_info_cache_key(source.id, remote_item_id, media_source_id);
        let playback_info = if let Some(cached) = get_cached_playback_info(&cache_key).await {
            cached
        } else {
            let fresh = fetch_remote_playback_info(
                pool,
                source,
                user_id.as_str(),
                remote_item_id,
                media_source_id,
                false,
            )
            .await?;
            set_cached_playback_info(cache_key.clone(), fresh.clone()).await;
            fresh
        };

        let media_source = select_remote_playback_media_source(
            playback_info.media_sources.as_slice(),
            media_source_id,
        )
        .ok_or_else(|| AppError::BadRequest("远端 PlaybackInfo 未返回可用媒体源".to_string()))?;

        let endpoint = resolve_playback_info_stream_endpoint(
            &server_url,
            &token,
            remote_item_id,
            media_source,
        );

        let builder = build_remote_stream_builder(
            client,
            &endpoint,
            method,
            source,
            &token,
            request_headers,
            &media_source.required_http_headers,
        );

        let response = builder.send().await?;
        let status = response.status();
        if matches!(
            status,
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) && attempt == 0
        {
            {
                let mut cache = PLAYBACK_INFO_CACHE.write().await;
                cache.remove(&cache_key);
            }
            repository::clear_remote_emby_source_auth_state(pool, source.id).await?;
            source.access_token = None;
            source.remote_user_id = None;
            continue;
        }
        return Ok(response);
    }

    Err(AppError::Unauthorized)
}

fn resolve_playback_info_stream_endpoint(
    server_url: &str,
    token: &str,
    remote_item_id: &str,
    media_source: &RemotePlaybackMediaSource,
) -> String {
    if let Some(direct_url) = media_source
        .direct_stream_url
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        let mut endpoint = absolutize_remote_url(server_url, direct_url);
        if media_source
            .add_api_key_to_direct_stream_url
            .unwrap_or(false)
            && !endpoint.contains("api_key=")
        {
            endpoint = append_query_pair(&endpoint, "api_key", token);
        }
        return endpoint;
    }
    if let Some(transcoding_url) = media_source
        .transcoding_url
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        return absolutize_remote_url(server_url, transcoding_url);
    }
    let msid = media_source
        .id
        .as_deref()
        .unwrap_or(remote_item_id);
    format!(
        "{}/Videos/{}/stream?Static=true&MediaSourceId={}&api_key={}",
        server_url.trim_end_matches('/'),
        remote_item_id,
        msid,
        token
    )
}

async fn get_json_with_retry<T: serde::de::DeserializeOwned>(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    endpoint: &str,
    query: &[(String, String)],
) -> Result<T, AppError> {
    let client = &*crate::http_client::SHARED;
    for attempt in 0..2 {
        let _user_id = ensure_authenticated(pool, source, attempt > 0).await?;
        let token = source
            .access_token
            .clone()
            .ok_or_else(|| AppError::Internal("远端登录令牌为空".to_string()))?;
        let mut normalized_query = query.to_vec();
        if !normalized_query
            .iter()
            .any(|(key, _)| key.eq_ignore_ascii_case("reqformat"))
        {
            normalized_query.push(("reqformat".to_string(), "json".to_string()));
        }
        let mut request = client
            .get(endpoint)
            .query(&normalized_query)
            .header(
                header::USER_AGENT.as_str(),
                source.spoofed_user_agent.as_str(),
            )
            .header(header::ACCEPT.as_str(), "application/json")
            .header(header::ACCEPT_ENCODING.as_str(), "identity")
            .header("X-Emby-Token", token.as_str())
            .header(
                "X-Emby-Authorization",
                emby_auth_header(source, Some(token.as_str())),
            );

        request = request.query(&[("api_key", token.as_str())]);
        let response = request.send().await?;
        if matches!(
            response.status(),
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) && attempt == 0
        {
            repository::clear_remote_emby_source_auth_state(pool, source.id).await?;
            source.access_token = None;
            source.remote_user_id = None;
            continue;
        }

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "远端 Emby 请求失败: {} {}",
                status.as_u16(),
                body
            )));
        }
        return parse_remote_json_response(response, endpoint).await;
    }

    Err(AppError::Unauthorized)
}

async fn parse_remote_json_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
    endpoint: &str,
) -> Result<T, AppError> {
    let status = response.status();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("-")
        .to_string();
    let bytes = response.bytes().await.map_err(|error| {
        AppError::Internal(format!(
            "远端响应读取失败: endpoint={endpoint}, status={}, content_type={content_type}, error={error}",
            status.as_u16()
        ))
    })?;
    serde_json::from_slice::<T>(bytes.as_ref()).map_err(|error| {
        let preview = String::from_utf8_lossy(bytes.as_ref())
            .chars()
            .take(280)
            .collect::<String>();
        AppError::Internal(format!(
            "远端JSON解析失败: endpoint={endpoint}, status={}, content_type={content_type}, error={error}, body预览={preview}",
            status.as_u16()
        ))
    })
}

async fn fetch_remote_playback_info(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    is_playback: bool,
) -> Result<RemotePlaybackInfo, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Items/{remote_item_id}/PlaybackInfo");
    let mut query = vec![
        ("UserId".to_string(), user_id.to_string()),
        ("StartTimeTicks".to_string(), "0".to_string()),
        (
            "IsPlayback".to_string(),
            if is_playback { "true" } else { "false" }.to_string(),
        ),
        ("AutoOpenLiveStream".to_string(), "false".to_string()),
        ("MaxStreamingBitrate".to_string(), "200000000".to_string()),
        ("reqformat".to_string(), "json".to_string()),
    ];
    if let Some(value) = media_source_id {
        if !value.trim().is_empty() {
            query.push(("MediaSourceId".to_string(), value.trim().to_string()));
        }
    }
    get_json_with_retry(pool, source, &endpoint, &query).await
}

fn select_remote_playback_media_source<'a>(
    media_sources: &'a [RemotePlaybackMediaSource],
    media_source_id: Option<&str>,
) -> Option<&'a RemotePlaybackMediaSource> {
    if let Some(value) = media_source_id {
        let requested = value.trim();
        if !requested.is_empty() {
            if let Some(source) = media_sources.iter().find(|source| {
                source
                    .id
                    .as_deref()
                    .is_some_and(|id| id.eq_ignore_ascii_case(requested))
            }) {
                return Some(source);
            }
        }
    }
    media_sources
        .iter()
        .find(|source| {
            source
                .direct_stream_url
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
                || source
                    .transcoding_url
                    .as_deref()
                    .is_some_and(|value| !value.trim().is_empty())
        })
        .or_else(|| media_sources.first())
}

fn absolutize_remote_url(server_url: &str, value: &str) -> String {
    let target = value.trim();
    if target.starts_with("http://") || target.starts_with("https://") {
        return target.to_string();
    }
    if target.starts_with('/') {
        return format!("{server_url}{target}");
    }
    format!("{server_url}/{target}")
}

fn append_query_pair(url: &str, key: &str, value: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) => {
            parsed.query_pairs_mut().append_pair(key, value);
            parsed.to_string()
        }
        Err(_) => {
            if url.contains('?') {
                format!("{url}&{key}={value}")
            } else {
                format!("{url}?{key}={value}")
            }
        }
    }
}

async fn fetch_remote_playback_analysis(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    remote_item_id: &str,
    media_source_id: Option<&str>,
) -> Result<Option<MediaAnalysisResult>, AppError> {
    let user_id = ensure_authenticated(pool, source, false).await?;
    let playback_info = fetch_remote_playback_info(
        pool,
        source,
        user_id.as_str(),
        remote_item_id,
        media_source_id,
        false,
    )
    .await?;
    let media_source = select_remote_playback_media_source(
        playback_info.media_sources.as_slice(),
        media_source_id,
    );
    Ok(media_source.and_then(|source| remote_playback_source_to_analysis(remote_item_id, source)))
}

fn remote_playback_source_to_analysis(
    remote_item_id: &str,
    source: &RemotePlaybackMediaSource,
) -> Option<MediaAnalysisResult> {
    if source.media_streams.is_empty()
        && source.chapters.is_empty()
        && source.run_time_ticks.is_none()
    {
        return None;
    }
    let duration = source
        .run_time_ticks
        .map(|ticks| format!("{}", ticks as f64 / 10_000_000.0));
    let format = MediaFormatInfo {
        filename: source
            .path
            .clone()
            .unwrap_or_else(|| remote_item_id.to_string()),
        format_name: source.container.clone(),
        format_long_name: source.container.clone(),
        duration,
        size: source.size.map(|value| value.to_string()),
        bit_rate: source.bitrate.map(|value| value.to_string()),
    };
    let streams = source
        .media_streams
        .iter()
        .map(remote_playback_stream_to_analysis_stream)
        .collect::<Vec<_>>();
    let chapters = source
        .chapters
        .iter()
        .map(|chapter| MediaChapterInfo {
            chapter_index: chapter.chapter_index,
            start_position_ticks: chapter.start_position_ticks,
            name: chapter.name.clone(),
            marker_type: chapter.marker_type.clone(),
        })
        .collect::<Vec<_>>();
    Some(MediaAnalysisResult {
        streams,
        chapters,
        format,
    })
}

fn remote_playback_stream_to_analysis_stream(
    stream: &RemotePlaybackMediaStream,
) -> MediaStreamInfo {
    let mut tags = serde_json::Map::new();
    if let Some(time_base) = stream.time_base.clone() {
        tags.insert(
            "time_base".to_string(),
            serde_json::Value::String(time_base),
        );
    }
    if let Some(bit_depth) = stream.bit_depth {
        tags.insert(
            "bits_per_raw_sample".to_string(),
            serde_json::Value::String(bit_depth.to_string()),
        );
    }
    let tags = if tags.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(tags))
    };

    MediaStreamInfo {
        index: stream.index,
        codec_type: stream.stream_type.to_ascii_lowercase(),
        codec_name: stream.codec.clone(),
        codec_long_name: stream.codec.clone(),
        width: stream.width,
        height: stream.height,
        bit_rate: stream.bit_rate.map(|value| value.to_string()),
        channels: stream.channels,
        channel_layout: stream.channel_layout.clone(),
        sample_rate: stream.sample_rate.map(|value| value.to_string()),
        language: stream.language.clone(),
        title: stream.title.clone(),
        profile: stream.profile.clone(),
        average_frame_rate: stream.average_frame_rate.map(|value| value as f32),
        real_frame_rate: stream.real_frame_rate.map(|value| value as f32),
        aspect_ratio: stream.aspect_ratio.clone(),
        is_default: stream.is_default,
        is_forced: stream.is_forced,
        is_hearing_impaired: stream.is_hearing_impaired,
        is_interlaced: stream.is_interlaced,
        color_range: stream.video_range.clone(),
        color_space: stream.color_space.clone(),
        color_transfer: stream.color_transfer.clone(),
        color_primaries: stream.color_primaries.clone(),
        level: stream.level,
        pixel_format: stream.pixel_format.clone(),
        ref_frames: stream.ref_frames,
        stream_start_time_ticks: None,
        attachment_size: stream.attachment_size,
        extended_video_sub_type: stream.extended_video_sub_type.clone(),
        extended_video_sub_type_description: stream.extended_video_sub_type_description.clone(),
        extended_video_type: stream.extended_video_type.clone(),
        is_anamorphic: stream.is_anamorphic,
        is_avc: stream
            .codec
            .as_deref()
            .map(|codec| codec.eq_ignore_ascii_case("h264")),
        // 外挂字幕：保存远端 DeliveryUrl，后续代理时使用
        is_external_url: if stream.is_external.unwrap_or(false)
            && stream.stream_type.eq_ignore_ascii_case("Subtitle")
        {
            stream.delivery_url.clone().filter(|u| !u.trim().is_empty())
        } else {
            None
        },
        is_text_subtitle_stream: stream.is_text_subtitle_stream,
        tags,
    }
}

#[allow(dead_code)]
async fn apply_playback_metadata_to_scanned_items(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    source_root: &Path,
    metadata_by_item_key: &HashMap<String, MediaAnalysisResult>,
) -> Result<(), AppError> {
    if metadata_by_item_key.is_empty() {
        return Ok(());
    }
    let like_prefix = format!("{}%", source_root.to_string_lossy());
    let item_types = vec!["Movie".to_string(), "Episode".to_string()];
    let scanned_items = sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT id, path
        FROM media_items
        WHERE library_id = $1
          AND path LIKE $2
          AND item_type = ANY($3::text[])
        "#,
    )
    .bind(library_id)
    .bind(like_prefix)
    .bind(item_types)
    .fetch_all(pool)
    .await?;

    for (item_id, item_path) in scanned_items {
        let Some(item_key) = remote_item_key_from_path(item_path.as_str()) else {
            continue;
        };
        let Some(analysis) = metadata_by_item_key.get(item_key.as_str()) else {
            continue;
        };
        repository::update_media_item_metadata(pool, item_id, analysis).await?;
    }
    Ok(())
}

#[allow(dead_code)]
fn remote_item_key_from_path(path: &str) -> Option<String> {
    let file_name = Path::new(path).file_name()?.to_string_lossy();
    if !file_name.to_ascii_lowercase().ends_with(".strm") {
        return None;
    }
    let open_idx = file_name.rfind('[')?;
    let close_idx = file_name.rfind(']')?;
    if close_idx <= open_idx + 1 {
        return None;
    }
    let raw = &file_name[(open_idx + 1)..close_idx];
    if raw.trim().is_empty() {
        return None;
    }
    Some(sanitize_segment(raw))
}

async fn ensure_authenticated(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    force_refresh: bool,
) -> Result<String, AppError> {
    if !force_refresh
        && source
            .remote_user_id
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
        && source
            .access_token
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
    {
        return Ok(source.remote_user_id.clone().unwrap_or_default());
    }

    let login = login_remote(source).await?;
    repository::update_remote_emby_source_auth_state(
        pool,
        source.id,
        login.user.id.as_str(),
        login.access_token.as_str(),
    )
    .await?;
    source.remote_user_id = Some(login.user.id.clone());
    source.access_token = Some(login.access_token.clone());
    Ok(login.user.id)
}

async fn login_remote(source: &DbRemoteEmbySource) -> Result<RemoteLoginResponse, AppError> {
    let client = &*crate::http_client::SHARED;
    let endpoint = format!(
        "{}/Users/AuthenticateByName",
        normalize_server_url(&source.server_url)
    );
    let response = client
        .post(&endpoint)
        .query(&[("reqformat", "json")])
        .header(
            header::USER_AGENT.as_str(),
            source.spoofed_user_agent.as_str(),
        )
        .header(header::ACCEPT.as_str(), "application/json")
        .header(header::ACCEPT_ENCODING.as_str(), "identity")
        .header("X-Emby-Authorization", emby_auth_header(source, None))
        .json(&serde_json::json!({
            "Username": source.username,
            "Pw": source.password,
            "Password": source.password,
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::BadRequest(format!(
            "远端 Emby 登录失败: {} {}",
            status.as_u16(),
            body
        )));
    }
    parse_remote_json_response(response, endpoint.as_str()).await
}

/// 构建 STRM 在 view workspace 内的相对路径。
/// 注意：view 子目录已由调用方（view_strm_workspace）提供，此处不再重复拼 view_name。
fn build_relative_strm_path(item: &RemoteSyncItem) -> Option<PathBuf> {
    if item.item.id.trim().is_empty() || item.item.name.trim().is_empty() {
        return None;
    }
    let item_type = item.item.item_type.trim().to_ascii_lowercase();
    if item_type == "movie" {
        let title = sanitize_segment(item.item.name.as_str());
        let year_part = item
            .item
            .production_year
            .map(|year| format!(" ({year})"))
            .unwrap_or_default();
        let movie_folder = format!("{title}{year_part}");
        let filename = format!("{title}{year_part}.strm");
        return Some(PathBuf::from(movie_folder).join(filename));
    }

    if item_type == "episode" {
        let series_name =
            sanitize_segment(item.item.series_name.as_deref().unwrap_or("Unknown Series"));
        let season_number = item.item.parent_index_number.unwrap_or(0).clamp(0, 999);
        let episode_number = item.item.index_number.unwrap_or(0).clamp(0, 9999);
        let title = sanitize_segment(item.item.name.as_str());
        let filename = format!("S{season_number:02}E{episode_number:02} - {title}.strm");
        let season_folder = format!("Season {season_number:02}");
        return Some(
            PathBuf::from(series_name)
                .join(season_folder)
                .join(filename),
        );
    }

    None
}

/// 判定一个 sidecar 文件是否已经"完整存在"。
///
/// 用于避免远端同步在用户已经手动刷新元数据/字幕后再次覆盖：
/// - 文件存在且大小 > 0 → 跳过下载
/// - 不存在或长度为 0 → 允许写入
async fn sidecar_exists_nonempty(path: &Path) -> bool {
    match tokio::fs::metadata(path).await {
        Ok(meta) => meta.is_file() && meta.len() > 0,
        Err(_) => false,
    }
}

/// STRM 源根目录：`{STRM_OUTPUT_PATH}/{SanitizedSourceName}/`
/// 每个远端媒体库（View）在此基础上再建子目录：`{source_root}/{SanitizedViewName}/`
///
/// API 层在创建/更新源时已强制 `strm_output_path` 必填，因此理论上不会读到空值；
/// 旧库（NULL/空字符串）作为兜底返回 Err，让调用方拒绝同步而非降级到虚拟路径。
fn strm_workspace_for_source(source: &DbRemoteEmbySource) -> Result<PathBuf, AppError> {
    let raw = source
        .strm_output_path
        .as_deref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "远端 Emby 源「{}」未配置 STRM 输出根目录，请先在编辑表单中补填后再同步",
                source.name
            ))
        })?;
    Ok(Path::new(raw).join(sanitize_segment(source.name.as_str())))
}


fn append_remote_api_key_param(url: &str, token: &str) -> String {
    if url.contains("api_key=") {
        return url.to_string();
    }
    let sep = if url.contains('?') { '&' } else { '?' };
    format!("{url}{sep}api_key={token}")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}

fn subtitle_file_extension(codec: Option<&str>) -> &'static str {
    let raw = codec.unwrap_or("").to_ascii_lowercase();
    if raw.contains("subrip") || raw == "srt" {
        return "srt";
    }
    if raw.contains("ass") || raw == "ssa" {
        return "ass";
    }
    if raw.contains("vtt") {
        return "vtt";
    }
    if raw.contains("pgs") {
        return "sup";
    }
    "srt"
}

fn remote_subtitle_stream_url(
    server_url: &str,
    remote_item_id: &str,
    media_source_id: &str,
    stream_index: i32,
    ext: &str,
    token: &str,
) -> String {
    let base = normalize_server_url(server_url);
    format!(
        "{base}/emby/Videos/{remote_item_id}/{media_source_id}/Subtitles/{stream_index}/Stream.{ext}?api_key={token}",
    )
}

fn nfo_header_line() -> &'static str {
    r#"<?xml version="1.0" encoding="utf-8" standalone="yes"?>"#
}

fn build_movie_nfo_xml(item: &RemoteSyncItem) -> String {
    let title = xml_escape(item.item.name.as_str());
    let plot = item
        .item
        .overview
        .as_deref()
        .map(xml_escape)
        .unwrap_or_default();
    let year_line = item
        .item
        .production_year
        .map(|y| format!("  <year>{y}</year>\n"))
        .unwrap_or_default();
    let runtime_line = item
        .item
        .run_time_ticks
        .map(|ticks| {
            let mins = ((ticks / 10_000_000).max(0) / 60).max(1) as i64;
            format!("  <runtime>{mins}</runtime>\n")
        })
        .unwrap_or_default();
    format!(
        "{hdr}\n<movie>\n  <title>{title}</title>\n  <plot>{plot}</plot>\n{year_line}{runtime_line}</movie>\n",
        hdr = nfo_header_line(),
    )
}

fn build_episode_nfo_xml(item: &RemoteSyncItem) -> String {
    let title = xml_escape(item.item.name.as_str());
    let show = xml_escape(item.item.series_name.as_deref().unwrap_or(""));
    let plot = item
        .item
        .overview
        .as_deref()
        .map(xml_escape)
        .unwrap_or_default();
    let sn = item.item.parent_index_number.unwrap_or(1);
    let en = item.item.index_number.unwrap_or(1);
    format!(
        "{hdr}\n<episodedetails>\n  <title>{title}</title>\n  <showtitle>{show}</showtitle>\n  <season>{sn}</season>\n  <episode>{en}</episode>\n  <plot>{plot}</plot>\n</episodedetails>\n",
        hdr = nfo_header_line(),
    )
}

fn build_tvshow_nfo_xml_from_episode(item: &RemoteSyncItem) -> String {
    let title = xml_escape(item.item.series_name.as_deref().unwrap_or("Unknown Series"));
    format!(
        "{hdr}\n<tvshow>\n  <title>{title}</title>\n</tvshow>\n",
        hdr = nfo_header_line(),
    )
}

fn media_source_row<'a>(
    item: &'a RemoteBaseItem,
    preferred_msid: Option<&str>,
) -> Option<(&'a str, &'a [RemoteItemMediaStream])> {
    let sources = item.media_sources.as_ref()?;
    if let Some(want) = preferred_msid.filter(|s| !s.trim().is_empty()) {
        for ms in sources {
            if ms.id.trim() == want.trim() {
                return Some((ms.id.as_str(), ms.media_streams.as_slice()));
            }
        }
    }
    sources
        .first()
        .map(|ms| (ms.id.as_str(), ms.media_streams.as_slice()))
}


async fn emby_download_bytes(
    source: &DbRemoteEmbySource,
    token: &str,
    url: &str,
) -> Result<Vec<u8>, AppError> {
    let resp = crate::http_client::SHARED
        .get(url)
        .header(header::USER_AGENT.as_str(), source.spoofed_user_agent.as_str())
        .header("X-Emby-Token", token)
        .header("X-Emby-Authorization", emby_auth_header(source, Some(token)))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("下载远端资源失败: {e}: {url}")))?;
    if !resp.status().is_success() {
        return Err(AppError::Internal(format!(
            "下载远端 HTTP {} {}",
            resp.status().as_u16(),
            url
        )));
    }
    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| AppError::Internal(format!("读取远端字节失败: {e}")))
}

async fn write_remote_strm_bundle(
    state: &AppState,
    source: &DbRemoteEmbySource,
    workspace_root: &Path,
    playback_token: &str,
    item: &RemoteSyncItem,
    media_source_id: Option<&str>,
    tvshow_written: &mut HashSet<PathBuf>,
) -> Result<(PathBuf, Option<PathBuf>, Option<PathBuf>, Option<PathBuf>), AppError> {
    let relative = build_relative_strm_path(item).ok_or_else(|| {
        AppError::Internal(format!(
            "无法为远端条目 {} 生成 STRM 相对路径",
            item.item.id
        ))
    })?;
    let strm_path = workspace_root.join(&relative);
    if let Some(parent) = strm_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::Internal(format!("创建 STRM 目录失败: {e}")))?;
    }

    // 无论 proxy_mode 是 proxy 还是 redirect，strm 文件统一写入本地代理 URL，
    // 不向磁盘暴露远端服务器地址、api_key 或 token。
    // - proxy 模式：服务端在收到代理请求时流式中转远端响应。
    // - redirect 模式：服务端在收到代理请求时返回 302，动态构建远端直链（包含最新 token）。
    // 这样可以避免 token 刷新后 strm 文件失效，也避免直链信息持久化到磁盘。
    let signature = build_proxy_signature(source.source_secret, item.item.id.as_str(), media_source_id);
    let stream_line = build_local_proxy_url(
        &state.config,
        source.id,
        item.item.id.as_str(),
        media_source_id,
        signature.as_str(),
    );
    tokio::fs::write(&strm_path, format!("{}\n", stream_line.trim()))
        .await
        .map_err(|e| AppError::Internal(format!("写入 STRM 失败: {e}")))?;

    // 下载侧车文件（图片/NFO/字幕）仍需直连远端服务器
    let base = normalize_server_url(&source.server_url);

    let mut local_poster: Option<PathBuf> = None;
    let mut local_backdrop: Option<PathBuf> = None;
    let mut local_logo: Option<PathBuf> = None;

    let sidecar_dir = strm_path
        .parent()
        .ok_or_else(|| AppError::Internal("STRM 缺父目录".into()))?;

    // Episode 与 Movie 共用同一目录的情况不同：
    // - Movie：每部电影独占一个文件夹，使用 poster.jpg / backdrop.jpg / logo.png 即可
    // - Episode：同一季所有集共享 Season XX/ 目录，必须用 strm 文件名做前缀，避免互相覆盖
    let is_episode = item.item.item_type.eq_ignore_ascii_case("Episode");
    let strm_stem = strm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media")
        .to_string();
    let poster_filename = if is_episode {
        format!("{strm_stem}-thumb.jpg")
    } else {
        "poster.jpg".to_string()
    };
    let backdrop_filename = if is_episode {
        format!("{strm_stem}-fanart.jpg")
    } else {
        "backdrop.jpg".to_string()
    };
    let logo_filename = if is_episode {
        format!("{strm_stem}-clearlogo.png")
    } else {
        "logo.png".to_string()
    };

    if source.sync_metadata {
        // 下载 Primary 封面（poster）
        {
            let path = sidecar_dir.join(&poster_filename);
            if sidecar_exists_nonempty(&path).await {
                local_poster = Some(path);
            } else {
                let primary_tag = item
                    .item
                    .image_tags
                    .as_ref()
                    .and_then(|v| v.as_object())
                    .and_then(|m| m.get("Primary").and_then(Value::as_str));
                let url = if let Some(tag) = primary_tag {
                    append_remote_api_key_param(
                        remote_image_url(base.as_str(), item.item.id.as_str(), "Primary", tag).as_str(),
                        playback_token.trim(),
                    )
                } else {
                    append_remote_api_key_param(
                        &format!(
                            "{}/emby/Items/{}/Images/Primary?quality=90&maxWidth=1920",
                            base.trim_end_matches('/'),
                            item.item.id
                        ),
                        playback_token.trim(),
                    )
                };
                match emby_download_bytes(source, playback_token, url.as_str()).await {
                    Ok(bytes) if !bytes.is_empty() => {
                        if tokio::fs::write(&path, &bytes).await.is_ok() {
                            local_poster = Some(path);
                        }
                    }
                    _ => {}
                }
            }
        }

        // 下载 Backdrop（背景图）
        {
            let path = sidecar_dir.join(&backdrop_filename);
            if sidecar_exists_nonempty(&path).await {
                local_backdrop = Some(path);
            } else {
                let backdrop_tag = item.item.backdrop_image_tags.as_ref().and_then(|tags| match tags {
                    Value::Array(arr) => arr.first().and_then(Value::as_str),
                    Value::Object(map) => map.values().next().and_then(Value::as_str),
                    Value::String(s) => Some(s.as_str()),
                    _ => None,
                });
                let url = if let Some(tag) = backdrop_tag {
                    append_remote_api_key_param(
                        remote_image_url(base.as_str(), item.item.id.as_str(), "Backdrop", tag).as_str(),
                        playback_token.trim(),
                    )
                } else {
                    append_remote_api_key_param(
                        &format!(
                            "{}/emby/Items/{}/Images/Backdrop?quality=90&maxWidth=1920",
                            base.trim_end_matches('/'),
                            item.item.id
                        ),
                        playback_token.trim(),
                    )
                };
                match emby_download_bytes(source, playback_token, url.as_str()).await {
                    Ok(bytes) if !bytes.is_empty() => {
                        if tokio::fs::write(&path, &bytes).await.is_ok() {
                            local_backdrop = Some(path);
                        }
                    }
                    _ => {}
                }
            }
        }

        // 下载 Logo
        {
            let path = sidecar_dir.join(&logo_filename);
            if sidecar_exists_nonempty(&path).await {
                local_logo = Some(path);
            } else if let Some(tag) = item
                .item
                .image_tags
                .as_ref()
                .and_then(|v| v.as_object())
                .and_then(|m| m.get("Logo").and_then(Value::as_str))
            {
                let url = append_remote_api_key_param(
                    remote_image_url(base.as_str(), item.item.id.as_str(), "Logo", tag).as_str(),
                    playback_token.trim(),
                );
                if let Ok(bytes) = emby_download_bytes(source, playback_token, url.as_str()).await {
                    if tokio::fs::write(&path, &bytes).await.is_ok() {
                        local_logo = Some(path);
                    }
                }
            }
        }

        // 已存在的 NFO 不覆盖：手动 POST /Items/{id}/Refresh 写入的 NFO 享有更高优先级。
        let nfo_path = strm_path.with_extension("nfo");
        if !sidecar_exists_nonempty(&nfo_path).await {
            let nfo_body = if item.item.item_type.eq_ignore_ascii_case("Episode") {
                build_episode_nfo_xml(item)
            } else {
                build_movie_nfo_xml(item)
            };
            let _ = tokio::fs::write(&nfo_path, nfo_body).await;
        }

        if item.item.item_type.eq_ignore_ascii_case("Episode") {
            if let Some(series_dir) = strm_path.parent().and_then(Path::parent) {
                if tvshow_written.insert(series_dir.to_path_buf()) {
                    let tvshow_path = series_dir.join("tvshow.nfo");
                    if !sidecar_exists_nonempty(&tvshow_path).await {
                        let show_body = build_tvshow_nfo_xml_from_episode(item);
                        let _ = tokio::fs::write(&tvshow_path, show_body).await;
                    }
                }
            }
        }
    }

    if source.sync_subtitles {
        if let Some((ms_id, streams)) = media_source_row(&item.item, media_source_id) {
            let stem = strm_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("media");
            for stream in streams {
                if !stream.stream_type.eq_ignore_ascii_case("Subtitle") {
                    continue;
                }
                if !stream.is_external.unwrap_or(false) {
                    continue;
                }
                let ext = subtitle_file_extension(stream.codec.as_deref());
                let lang = stream
                    .language
                    .as_deref()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or("und");
                let safe_lang = sanitize_segment(lang);
                let fname = format!("{stem}.{safe_lang}.{ext}");
                let sub_path = sidecar_dir.join(fname);
                if sidecar_exists_nonempty(&sub_path).await {
                    continue;
                }
                let url = remote_subtitle_stream_url(
                    base.as_str(),
                    item.item.id.as_str(),
                    ms_id,
                    stream.index,
                    ext,
                    playback_token.trim(),
                );
                if let Ok(bytes) = emby_download_bytes(source, playback_token, url.as_str()).await {
                    let _ = tokio::fs::write(&sub_path, bytes).await;
                }
            }
        }
    }

    Ok((strm_path, local_poster, local_backdrop, local_logo))
}

/// 定期强制刷新远端 Emby 登录令牌，确保代理鉴权不过期。
/// STRM 文件内存的是本地代理 URL（不含远端 token），无需重写文件内容。
async fn refresh_single_remote_token(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
) -> Result<(), AppError> {
    ensure_authenticated(pool, source, true).await?;
    repository::update_remote_emby_source_last_token_refresh(pool, source.id).await?;
    Ok(())
}

async fn run_remote_emby_token_refresh_pass(pool: &sqlx::PgPool) -> Result<(), AppError> {
    let sources = repository::list_remote_emby_sources(pool).await?;
    let now = Utc::now();
    for mut source in sources {
        if !source.enabled {
            continue;
        }
        let interval_secs = source.token_refresh_interval_secs.max(0);
        if interval_secs <= 0 {
            continue;
        }
        let due = match source.last_token_refresh_at {
            None => true,
            Some(ts) => {
                now.signed_duration_since(ts)
                    .num_seconds()
                    .max(-1)
                    >= i64::from(interval_secs)
            }
        };
        if !due {
            continue;
        }
        if let Err(error) = refresh_single_remote_token(pool, &mut source).await {
            tracing::warn!(
                source_id = %source.id,
                source_name = %source.name,
                error = %error,
                "远端 Emby token 主动刷新失败"
            );
        }
    }
    Ok(())
}

/// 每分钟检查是否需要主动刷新远端 Emby 登录令牌（防止长时间无访问导致 token 失效）。
pub async fn remote_emby_token_refresh_loop(pool: sqlx::PgPool) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    loop {
        ticker.tick().await;
        if let Err(error) = run_remote_emby_token_refresh_pass(&pool).await {
            tracing::warn!(error = %error, "远端 Emby token 刷新批次失败");
        }
    }
}

/// 远端媒体库实时监控轮询：每 5 分钟检查启用了 EnableRealtimeMonitor 的远端库，
/// 若远端有变更则触发增量同步。
pub async fn remote_library_monitor_loop(state: crate::state::AppState) {
    const POLL_INTERVAL_SECS: u64 = 300;
    let mut ticker = tokio::time::interval(Duration::from_secs(POLL_INTERVAL_SECS));
    ticker.tick().await; // 跳过首次立即触发
    loop {
        ticker.tick().await;
        if let Err(err) = run_remote_library_monitor_pass(&state).await {
            tracing::warn!(error = %err, "远端库实时监控轮询失败");
        }
    }
}

async fn run_remote_library_monitor_pass(
    state: &crate::state::AppState,
) -> Result<(), AppError> {
    let sources = repository::list_remote_emby_sources(&state.pool).await?;
    for source in &sources {
        if !source.enabled {
            continue;
        }
        // 检查该源关联的所有本地库是否启用了 EnableRealtimeMonitor
        let map_obj = source.view_library_map.as_object();
        let mut monitor_enabled = false;
        if let Some(map) = map_obj {
            for (_view_id, lib_id_val) in map {
                if let Some(lib_id_str) = lib_id_val.as_str() {
                    if let Ok(lib_id) = lib_id_str.parse::<Uuid>() {
                        if let Ok(Some(lib)) = repository::get_library(&state.pool, lib_id).await {
                            let opts = repository::library_options(&lib);
                            if opts.enable_realtime_monitor {
                                monitor_enabled = true;
                                break;
                            }
                        }
                    }
                }
            }
        }
        // 也检查 target_library_id
        if !monitor_enabled {
            if let Ok(Some(lib)) = repository::get_library(&state.pool, source.target_library_id).await {
                let opts = repository::library_options(&lib);
                if opts.enable_realtime_monitor {
                    monitor_enabled = true;
                }
            }
        }
        if !monitor_enabled {
            continue;
        }
        // 只有 last_sync_at 不为空的源才进行增量轮询
        if source.last_sync_at.is_none() {
            continue;
        }
        tracing::info!(
            source_id = %source.id,
            source_name = %source.name,
            "远端库实时监控：检测到启用监控，触发增量同步"
        );
        match sync_source_with_progress(state, source.id, None).await {
            Ok(result) => {
                tracing::info!(
                    source_id = %source.id,
                    written = result.written_files,
                    "远端库实时监控：增量同步完成"
                );
            }
            Err(err) => {
                tracing::warn!(
                    source_id = %source.id,
                    error = %err,
                    "远端库实时监控：增量同步失败"
                );
            }
        }
    }
    Ok(())
}

fn build_local_proxy_url(
    config: &Config,
    source_id: Uuid,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    signature: &str,
) -> String {
    let mut query_pairs = vec![("sig".to_string(), signature.to_string())];
    if let Some(value) = media_source_id {
        if !value.trim().is_empty() {
            query_pairs.push(("msid".to_string(), value.trim().to_string()));
        }
    }

    let query = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for (key, value) in query_pairs {
            serializer.append_pair(key.as_str(), value.as_str());
        }
        serializer.finish()
    };

    let base = config
        .public_url
        .as_deref()
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| format!("http://127.0.0.1:{}", config.port));
    format!(
        "{base}/api/remote-emby/proxy/{source_id}/{remote_item_id}?{query}"
    )
}

pub async fn build_signed_proxy_url(
    pool: &sqlx::PgPool,
    config: &Config,
    source_id: Uuid,
    remote_item_id: &str,
    media_source_id: Option<&str>,
) -> Result<String, AppError> {
    let source = repository::get_remote_emby_source(pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))?;
    let signature = build_proxy_signature(source.source_secret, remote_item_id, media_source_id);
    Ok(build_local_proxy_url(
        config,
        source_id,
        remote_item_id,
        media_source_id,
        signature.as_str(),
    ))
}

pub fn build_proxy_signature(
    source_secret: Uuid,
    remote_item_id: &str,
    media_source_id: Option<&str>,
) -> String {
    let payload = format!(
        "{}|{}",
        remote_item_id.trim(),
        media_source_id.unwrap_or_default().trim()
    );
    Uuid::new_v5(&source_secret, payload.as_bytes())
        .simple()
        .to_string()
}

fn first_media_source_id(item: &RemoteSyncItem) -> Option<&str> {
    item.item
        .media_sources
        .as_ref()
        .and_then(|sources| sources.first())
        .map(|source| source.id.trim())
        .filter(|value| !value.is_empty())
}

fn emby_auth_header(source: &DbRemoteEmbySource, token: Option<&str>) -> String {
    let device_id = format!("movie-rust-{}", source.id.simple());
    emby_auth_header_for_device(device_id.as_str(), token)
}

fn emby_auth_header_for_device(device_id: &str, token: Option<&str>) -> String {
    let client = "MovieRustTransit";
    let device = "MovieRustProxy";
    let version = "1.0.0";
    if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
        format!(
            "MediaBrowser Client=\"{client}\", Device=\"{device}\", DeviceId=\"{}\", Version=\"{version}\", Token=\"{}\"",
            device_id.trim(),
            token.trim()
        )
    } else {
        format!(
            "MediaBrowser Client=\"{client}\", Device=\"{device}\", DeviceId=\"{}\", Version=\"{version}\"",
            device_id.trim()
        )
    }
}

fn normalize_remote_view_ids(values: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for raw in values {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        if !normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(value))
        {
            normalized.push(value.to_string());
        }
    }
    normalized
}

fn normalize_server_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

fn sanitize_segment(raw: &str) -> String {
    let mut output = String::with_capacity(raw.len());
    for ch in raw.chars() {
        let safe = match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            _ => ch,
        };
        if safe.is_control() {
            continue;
        }
        output.push(safe);
    }
    let collapsed = output.split_whitespace().collect::<Vec<_>>().join(" ");
    let collapsed = collapsed.trim().trim_matches('.');
    if collapsed.is_empty() {
        "unknown".to_string()
    } else {
        collapsed.to_string()
    }
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
