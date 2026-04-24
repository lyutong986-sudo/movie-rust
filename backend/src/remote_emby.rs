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
use chrono::NaiveDate;
use reqwest::{redirect::Policy, Client};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::RwLock;
use uuid::Uuid;

const REMOTE_PAGE_SIZE: i64 = 200;
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
    media_sources: Option<Vec<RemoteMediaSource>>,
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemotePlaybackInfo {
    #[serde(default)]
    media_sources: Vec<RemotePlaybackMediaSource>,
    #[serde(default)]
    play_session_id: Option<String>,
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
) -> Result<Vec<RemoteViewPreview>, AppError> {
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
    let client = Client::new();
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
    Ok(views
        .items
        .into_iter()
        .map(|view| RemoteViewPreview {
            id: view.id,
            name: view.name,
            collection_type: view.collection_type,
        })
        .collect())
}

fn normalize_display_mode(value: &str) -> &'static str {
    if value.trim().eq_ignore_ascii_case(REMOTE_DISPLAY_MODE_MERGE) {
        REMOTE_DISPLAY_MODE_MERGE
    } else {
        REMOTE_DISPLAY_MODE_SEPARATE
    }
}

pub async fn sync_source(
    state: &AppState,
    source_id: Uuid,
) -> Result<RemoteEmbySyncResult, AppError> {
    sync_source_with_progress(state, source_id, None).await
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
    let target_library = repository::get_library(&state.pool, source.target_library_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("目标媒体库不存在".to_string()))?;

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
    let mut total_items = 0u64;
    for view in &views {
        let view_count = fetch_remote_items_total_count_for_view(
            &state.pool,
            source,
            user_id.as_str(),
            view.id.as_str(),
        )
        .await?;
        total_items = total_items.saturating_add(view_count);
    }

    let source_root = source_root_path(&target_library, source);
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

    let display_mode = normalize_display_mode(source.display_mode.as_str());
    let root_item_id = if display_mode == REMOTE_DISPLAY_MODE_SEPARATE {
        Some(upsert_virtual_root_item(&state.pool, source).await?)
    } else {
        None
    };
    let mut view_parent_map: HashMap<String, Uuid> = HashMap::new();
    let mut series_parent_map: HashMap<String, Uuid> = HashMap::new();
    let mut season_parent_map: HashMap<String, Uuid> = HashMap::new();
    let mut fetched_count = 0u64;
    let mut written_files = 0usize;
    if let Some(handle) = &progress {
        handle.set_streaming_progress(0, 0, total_items);
    }

    for view in &views {
        let mut start_index = 0i64;
        loop {
            let page = fetch_remote_items_page_for_view(
                &state.pool,
                source,
                user_id.as_str(),
                view.id.as_str(),
                start_index,
                REMOTE_PAGE_SIZE,
            )
            .await?;
            if page.items.is_empty() {
                break;
            }

            for base_item in page.items {
                fetched_count = fetched_count.saturating_add(1);
                let item = RemoteSyncItem {
                    item: base_item,
                    view_id: view.id.clone(),
                    view_name: view.name.clone(),
                };

                let mut parent_id = if display_mode == REMOTE_DISPLAY_MODE_SEPARATE {
                    let view_parent_id = ensure_virtual_view_folder(
                        &state.pool,
                        source,
                        root_item_id.ok_or_else(|| {
                            AppError::Internal("远端虚拟根目录不存在".to_string())
                        })?,
                        item.view_id.as_str(),
                        item.view_name.as_str(),
                        &mut view_parent_map,
                    )
                    .await?;
                    Some(view_parent_id)
                } else {
                    None
                };
                if item.item.item_type.eq_ignore_ascii_case("Episode") {
                    let series_view_scope = if display_mode == REMOTE_DISPLAY_MODE_SEPARATE {
                        item.view_id.as_str()
                    } else {
                        "_merged"
                    };
                    let series_parent_id = ensure_virtual_series_folder(
                        &state.pool,
                        source,
                        &item,
                        parent_id,
                        series_view_scope,
                        &mut series_parent_map,
                    )
                    .await?;

                    let season_parent_id = ensure_virtual_season_folder(
                        &state.pool,
                        source,
                        &item,
                        series_parent_id,
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
                let upserted = upsert_virtual_media_item(
                    &state.pool,
                    source,
                    &item,
                    parent_id,
                    media_source_id,
                    analysis.as_ref(),
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

    let scan_summary = ScanSummary {
        libraries: 1,
        scanned_files: fetched_count as i64,
        imported_items: written_files as i64,
    };

    Ok(RemoteEmbySyncResult {
        source_id: source.id,
        source_name: source.name.clone(),
        written_files,
        source_root: build_virtual_root_path(source.id),
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

pub fn remote_default_media_source_id(provider_ids: &Value) -> Option<String> {
    provider_ids
        .get("RemoteEmbyMediaSourceId")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_virtual_root_path(source_id: Uuid) -> String {
    format!("REMOTE_EMBY/{}/root", source_id)
}

fn build_virtual_view_path(source_id: Uuid, view_id: &str) -> String {
    format!(
        "REMOTE_EMBY/{}/views/{}",
        source_id,
        sanitize_segment(view_id)
    )
}

fn build_virtual_series_path(source_id: Uuid, view_id: &str, series_key: &str) -> String {
    format!(
        "REMOTE_EMBY/{}/views/{}/series/{}",
        source_id,
        sanitize_segment(view_id),
        sanitize_segment(series_key)
    )
}

fn build_virtual_season_path(
    source_id: Uuid,
    view_id: &str,
    series_key: &str,
    season_number: i32,
) -> String {
    format!(
        "REMOTE_EMBY/{}/views/{}/series/{}/season/{}",
        source_id,
        sanitize_segment(view_id),
        sanitize_segment(series_key),
        season_number.max(0)
    )
}

fn build_virtual_item_path(source_id: Uuid, remote_item_id: &str) -> String {
    format!("REMOTE_EMBY/{}/items/{}", source_id, remote_item_id.trim())
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
    library_id: Uuid,
    source_id: Uuid,
    legacy_source_root: &Path,
) -> Result<u64, AppError> {
    let virtual_prefix = format!("{}%", build_virtual_root_path(source_id));
    let virtual_prefix_windows = virtual_prefix.replace('/', "\\");
    let legacy_prefix = format!("{}%", legacy_source_root.to_string_lossy());
    let result = sqlx::query(
        r#"
        DELETE FROM media_items
        WHERE library_id = $1
          AND (
              provider_ids ->> 'RemoteEmbySourceId' = $2
              OR path LIKE $3
              OR path LIKE $4
              OR path LIKE $5
          )
        "#,
    )
    .bind(library_id)
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
    let Some(library) = repository::get_library(pool, source.target_library_id).await? else {
        return Ok(0);
    };
    let source_root = source_root_path(&library, source);
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

async fn upsert_virtual_root_item(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
) -> Result<Uuid, AppError> {
    let path = build_virtual_root_path(source.id);
    let path_ref = Path::new(path.as_str());
    let empty = Vec::<String>::new();
    repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id: source.target_library_id,
            parent_id: None,
            name: &source.name,
            item_type: "Folder",
            media_type: "Video",
            path: path_ref,
            container: None,
            original_title: None,
            overview: Some("远端 Emby 虚拟根目录"),
            production_year: None,
            official_rating: None,
            community_rating: None,
            critic_rating: None,
            runtime_ticks: None,
            premiere_date: None,
            status: None,
            end_date: None,
            air_days: &empty,
            air_time: None,
            provider_ids: remote_marker_provider_ids(source.id, None, None, None),
            genres: &empty,
            studios: &empty,
            tags: &empty,
            production_locations: &empty,
            image_primary_path: None,
            backdrop_path: None,
            logo_path: None,
            thumb_path: None,
            art_path: None,
            banner_path: None,
            disc_path: None,
            backdrop_paths: &empty,
            remote_trailers: &empty,
            series_name: None,
            season_name: None,
            index_number: None,
            index_number_end: None,
            parent_index_number: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
        },
    )
    .await
}

async fn ensure_virtual_view_folder(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    root_item_id: Uuid,
    view_id: &str,
    view_name: &str,
    view_parent_map: &mut HashMap<String, Uuid>,
) -> Result<Uuid, AppError> {
    if let Some(existing) = view_parent_map.get(view_id).copied() {
        return Ok(existing);
    }
    let path = build_virtual_view_path(source.id, view_id);
    let path_ref = Path::new(path.as_str());
    let empty = Vec::<String>::new();
    let item_id = repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id: source.target_library_id,
            parent_id: Some(root_item_id),
            name: view_name,
            item_type: "CollectionFolder",
            media_type: "Video",
            path: path_ref,
            container: None,
            original_title: None,
            overview: None,
            production_year: None,
            official_rating: None,
            community_rating: None,
            critic_rating: None,
            runtime_ticks: None,
            premiere_date: None,
            status: None,
            end_date: None,
            air_days: &empty,
            air_time: None,
            provider_ids: remote_marker_provider_ids(source.id, None, Some(view_id), None),
            genres: &empty,
            studios: &empty,
            tags: &empty,
            production_locations: &empty,
            image_primary_path: None,
            backdrop_path: None,
            logo_path: None,
            thumb_path: None,
            art_path: None,
            banner_path: None,
            disc_path: None,
            backdrop_paths: &empty,
            remote_trailers: &empty,
            series_name: None,
            season_name: None,
            index_number: None,
            index_number_end: None,
            parent_index_number: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
        },
    )
    .await?;
    view_parent_map.insert(view_id.to_string(), item_id);
    Ok(item_id)
}

async fn ensure_virtual_series_folder(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    item: &RemoteSyncItem,
    parent_id: Option<Uuid>,
    view_scope: &str,
    series_parent_map: &mut HashMap<String, Uuid>,
) -> Result<Uuid, AppError> {
    let raw_series_name = item
        .item
        .series_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Unknown Series");
    let series_name = raw_series_name.trim().to_string();
    let series_key = format!("{view_scope}::{}", sanitize_segment(series_name.as_str()));
    if let Some(existing) = series_parent_map.get(series_key.as_str()).copied() {
        return Ok(existing);
    }
    let path = build_virtual_series_path(source.id, view_scope, series_name.as_str());
    let path_ref = Path::new(path.as_str());
    let empty = Vec::<String>::new();
    let item_id = repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id: source.target_library_id,
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
            status: None,
            end_date: None,
            air_days: &empty,
            air_time: None,
            provider_ids: remote_marker_provider_ids(source.id, None, Some(&item.view_id), None),
            genres: &empty,
            studios: &empty,
            tags: &empty,
            production_locations: &empty,
            image_primary_path: None,
            backdrop_path: None,
            logo_path: None,
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
        },
    )
    .await?;
    series_parent_map.insert(series_key, item_id);
    Ok(item_id)
}

async fn ensure_virtual_season_folder(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    item: &RemoteSyncItem,
    series_parent_id: Uuid,
    season_parent_map: &mut HashMap<String, Uuid>,
) -> Result<Uuid, AppError> {
    let season_number = item.item.parent_index_number.unwrap_or(0).clamp(0, 999);
    let season_key = format!("{}::{season_number}", series_parent_id);
    if let Some(existing) = season_parent_map.get(season_key.as_str()).copied() {
        return Ok(existing);
    }
    let series_key = item
        .item
        .series_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Unknown Series");
    let path =
        build_virtual_season_path(source.id, item.view_id.as_str(), series_key, season_number);
    let path_ref = Path::new(path.as_str());
    let season_name = item
        .item
        .season_name
        .clone()
        .unwrap_or_else(|| format!("Season {season_number:02}"));
    let empty = Vec::<String>::new();
    let item_id = repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id: source.target_library_id,
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
            status: None,
            end_date: None,
            air_days: &empty,
            air_time: None,
            provider_ids: remote_marker_provider_ids(source.id, None, Some(&item.view_id), None),
            genres: &empty,
            studios: &empty,
            tags: &empty,
            production_locations: &empty,
            image_primary_path: None,
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
        },
    )
    .await?;
    season_parent_map.insert(season_key, item_id);
    Ok(item_id)
}

async fn upsert_virtual_media_item(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    item: &RemoteSyncItem,
    parent_id: Option<Uuid>,
    media_source_id: Option<&str>,
    analysis: Option<&MediaAnalysisResult>,
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
    let item_path = build_virtual_item_path(source.id, item.item.id.as_str());
    let path_ref = Path::new(item_path.as_str());
    let item_type = if item.item.item_type.eq_ignore_ascii_case("Episode") {
        "Episode"
    } else {
        "Movie"
    };
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
    let empty = Vec::<String>::new();
    let empty_backdrops = Vec::<String>::new();
    let empty_trailers = Vec::<String>::new();
    repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id: source.target_library_id,
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
            image_primary_path: None,
            backdrop_path: None,
            logo_path: None,
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
        },
    )
    .await
}

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
) -> Result<u64, AppError> {
    let page = fetch_remote_items_page_for_view(pool, source, user_id, view_id, 0, 1).await?;
    Ok(page.total_record_count.max(0) as u64)
}

async fn fetch_remote_items_page_for_view(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    start_index: i64,
    limit: i64,
) -> Result<RemoteItemsResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    let query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("ParentId".to_string(), view_id.to_string()),
        ("IncludeItemTypes".to_string(), "Movie,Episode".to_string()),
        (
            "Fields".to_string(),
            "SeriesName,SeasonName,ProductionYear,ParentIndexNumber,IndexNumber,Overview,OfficialRating,CommunityRating,CriticRating,PremiereDate,RunTimeTicks,ProviderIds,Genres,Studios,Tags,MediaSources".to_string(),
        ),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("SortBy".to_string(), "SortName".to_string()),
        ("SortOrder".to_string(), "Ascending".to_string()),
        ("StartIndex".to_string(), start_index.to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

async fn send_remote_stream_request(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    remote_item_id: &str,
    media_source_id: Option<&str>,
    method: &Method,
    request_headers: &HeaderMap,
) -> Result<reqwest::Response, AppError> {
    let client = Client::builder()
        .redirect(Policy::none())
        .build()
        .map_err(AppError::from)?;
    let normalized_method = if *method == Method::HEAD {
        Method::HEAD
    } else {
        Method::GET
    };

    'auth_attempt: for attempt in 0..2 {
        let user_id = ensure_authenticated(pool, source, attempt > 0).await?;
        let token = source
            .access_token
            .clone()
            .ok_or_else(|| AppError::Internal("远端登录令牌为空".to_string()))?;
        let server_url = normalize_server_url(&source.server_url);
        let playback_info = fetch_remote_playback_info(
            pool,
            source,
            user_id.as_str(),
            remote_item_id,
            media_source_id,
            false,
        )
        .await?;
        let mut playback_session_id = playback_info.play_session_id.clone();
        let mut media_source = select_remote_playback_media_source(
            playback_info.media_sources.as_slice(),
            media_source_id,
        )
        .cloned()
        .ok_or_else(|| AppError::BadRequest("远端 PlaybackInfo 未返回可用媒体源".to_string()))?;

        let mut stream_path = media_source
            .direct_stream_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                media_source
                    .transcoding_url
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
            })
            .map(str::to_string);

        // 某些远端 Emby 仅在 IsPlayback=true 时返回可用流地址，先走 false，再兜底 true。
        if stream_path.is_none() {
            let fallback_playback_info = fetch_remote_playback_info(
                pool,
                source,
                user_id.as_str(),
                remote_item_id,
                media_source_id,
                true,
            )
            .await?;
            if fallback_playback_info
                .play_session_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                playback_session_id = fallback_playback_info.play_session_id.clone();
            }
            media_source = select_remote_playback_media_source(
                fallback_playback_info.media_sources.as_slice(),
                media_source_id,
            )
            .cloned()
            .ok_or_else(|| {
                AppError::BadRequest("远端 PlaybackInfo 未返回可用媒体源".to_string())
            })?;
            stream_path = media_source
                .direct_stream_url
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    media_source
                        .transcoding_url
                        .as_deref()
                        .filter(|value| !value.trim().is_empty())
                })
                .map(str::to_string);
        }
        if stream_path.is_none() {
            let device_id = source.source_secret.to_string();
            stream_path = build_fallback_playback_stream_url(
                server_url.as_str(),
                remote_item_id,
                media_source_id,
                &media_source,
                device_id.as_str(),
                token.as_str(),
                playback_session_id.as_deref(),
            );
        }
        let stream_path = stream_path
            .ok_or_else(|| AppError::BadRequest("远端 PlaybackInfo 未提供可播放流".to_string()))?;
        let mut endpoint = absolutize_remote_url(server_url.as_str(), stream_path.as_str());
        if media_source
            .add_api_key_to_direct_stream_url
            .unwrap_or(false)
            && !endpoint.contains("api_key=")
        {
            endpoint = append_query_pair(endpoint.as_str(), "api_key", token.as_str());
        }
        let mut redirect_count = 0usize;
        let mut redirected_from: Option<String> = None;
        loop {
            let mut builder = if normalized_method == Method::HEAD {
                client.head(&endpoint)
            } else {
                client.get(&endpoint)
            };
            builder = builder.header(
                header::USER_AGENT.as_str(),
                source.spoofed_user_agent.as_str(),
            );
            if is_same_origin_url(endpoint.as_str(), server_url.as_str()) {
                builder = builder.header("X-Emby-Token", token.as_str()).header(
                    "X-Emby-Authorization",
                    emby_auth_header(source, Some(token.as_str())),
                );
            }
            for (key, value) in &media_source.required_http_headers {
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
            if let Some(previous) = redirected_from.as_deref() {
                builder = builder.header(header::REFERER.as_str(), previous);
            }

            let response = builder.send().await?;
            if matches!(
                response.status(),
                reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
            ) && attempt == 0
            {
                repository::clear_remote_emby_source_auth_state(pool, source.id).await?;
                source.access_token = None;
                source.remote_user_id = None;
                continue 'auth_attempt;
            }

            if response.status().is_redirection() {
                let location = response
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|value| value.to_str().ok())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        AppError::Internal(format!(
                            "远端流重定向缺少 Location: endpoint={}, status={}",
                            endpoint,
                            response.status().as_u16()
                        ))
                    })?;
                if redirect_count >= 8 {
                    return Err(AppError::Internal(format!(
                        "远端流重定向过多: endpoint={}",
                        endpoint
                    )));
                }
                let previous_endpoint = endpoint.clone();
                endpoint =
                    resolve_redirect_url(previous_endpoint.as_str(), location, server_url.as_str());
                redirected_from = Some(previous_endpoint);
                redirect_count += 1;
                continue;
            }
            return Ok(response);
        }
    }

    Err(AppError::Unauthorized)
}

fn is_same_origin_url(left: &str, right: &str) -> bool {
    let Ok(left) = url::Url::parse(left) else {
        return false;
    };
    let Ok(right) = url::Url::parse(right) else {
        return false;
    };
    left.scheme() == right.scheme()
        && left.domain() == right.domain()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn resolve_redirect_url(current_endpoint: &str, location: &str, server_url: &str) -> String {
    let target = location.trim();
    if target.starts_with("http://") || target.starts_with("https://") {
        return target.to_string();
    }
    if target.starts_with("//") {
        if let Ok(base) = url::Url::parse(current_endpoint) {
            return format!("{}:{}", base.scheme(), target);
        }
    }
    if let Ok(base) = url::Url::parse(current_endpoint) {
        if let Ok(joined) = base.join(target) {
            return joined.to_string();
        }
    }
    absolutize_remote_url(server_url, target)
}

fn build_fallback_playback_stream_url(
    server_url: &str,
    remote_item_id: &str,
    requested_media_source_id: Option<&str>,
    media_source: &RemotePlaybackMediaSource,
    device_id: &str,
    token: &str,
    play_session_id: Option<&str>,
) -> Option<String> {
    let media_source_id = media_source
        .id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            requested_media_source_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })?;
    let remote_item_id = remote_item_id.trim();
    if remote_item_id.is_empty() {
        return None;
    }
    let container = media_source
        .container
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("mp4");
    let mut endpoint = format!("{server_url}/videos/{remote_item_id}/original.{container}");
    endpoint = append_query_pair(endpoint.as_str(), "DeviceId", device_id);
    endpoint = append_query_pair(endpoint.as_str(), "MediaSourceId", media_source_id);
    let play_session_id = play_session_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    endpoint = append_query_pair(endpoint.as_str(), "PlaySessionId", play_session_id.as_str());
    endpoint = append_query_pair(endpoint.as_str(), "api_key", token);
    Some(endpoint)
}

async fn get_json_with_retry<T: serde::de::DeserializeOwned>(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    endpoint: &str,
    query: &[(String, String)],
) -> Result<T, AppError> {
    let client = Client::new();
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
    let client = Client::new();
    for attempt in 0..2 {
        let _ = ensure_authenticated(pool, source, attempt > 0).await?;
        let token = source
            .access_token
            .clone()
            .ok_or_else(|| AppError::Internal("远端登录令牌为空".to_string()))?;

        let mut query = vec![
            ("UserId".to_string(), user_id.to_string()),
            ("StartTimeTicks".to_string(), "0".to_string()),
            (
                "IsPlayback".to_string(),
                if is_playback { "true" } else { "false" }.to_string(),
            ),
            (
                "AutoOpenLiveStream".to_string(),
                if is_playback { "true" } else { "false" }.to_string(),
            ),
            ("MaxStreamingBitrate".to_string(), "6790000".to_string()),
            ("reqformat".to_string(), "json".to_string()),
        ];
        if let Some(value) = media_source_id {
            if !value.trim().is_empty() {
                query.push(("MediaSourceId".to_string(), value.trim().to_string()));
            }
        }

        let response = client
            .post(&endpoint)
            .query(&query)
            .query(&[("api_key", token.as_str())])
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
            )
            .json(&serde_json::json!({
                "DeviceProfile": {
                    "MaxStaticBitrate": 200000000,
                    "MaxStreamingBitrate": 200000000,
                    "DirectPlayProfiles": [
                        { "Type": "Video" },
                        { "Type": "Audio" }
                    ],
                    "TranscodingProfiles": [
                        {
                            "Container": "ts",
                            "Type": "Video",
                            "AudioCodec": "aac,mp3,ac3,eac3,opus",
                            "VideoCodec": "h264",
                            "Context": "Streaming",
                            "Protocol": "hls",
                            "MaxAudioChannels": "6",
                            "MinSegments": "1",
                            "BreakOnNonKeyFrames": true,
                            "ManifestSubtitles": "vtt"
                        }
                    ],
                    "ContainerProfiles": [],
                    "SubtitleProfiles": [
                        { "Format": "vtt", "Method": "External" },
                        { "Format": "webvtt", "Method": "External" },
                        { "Format": "srt", "Method": "External" },
                        { "Format": "ass", "Method": "External" },
                        { "Format": "ssa", "Method": "External" },
                        { "Format": "subrip", "Method": "Embed" },
                        { "Format": "srt", "Method": "Embed" },
                        { "Format": "ass", "Method": "Embed" },
                        { "Format": "ssa", "Method": "Embed" },
                        { "Format": "vtt", "Method": "Hls" }
                    ]
                }
            }))
            .send()
            .await?;

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
                "远端 PlaybackInfo 请求失败: {} {}",
                status.as_u16(),
                body
            )));
        }
        return parse_remote_json_response(response, &endpoint).await;
    }
    Err(AppError::Unauthorized)
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
        is_external_url: None,
        is_text_subtitle_stream: stream.is_text_subtitle_stream,
        tags,
    }
}

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
    let client = Client::new();
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

fn build_relative_strm_path(item: &RemoteSyncItem) -> Option<PathBuf> {
    if item.item.id.trim().is_empty() || item.item.name.trim().is_empty() {
        return None;
    }
    let view_folder = sanitize_segment(item.view_name.as_str());
    let item_type = item.item.item_type.trim().to_ascii_lowercase();
    if item_type == "movie" {
        let title = sanitize_segment(item.item.name.as_str());
        let year_part = item
            .item
            .production_year
            .map(|year| format!(" ({year})"))
            .unwrap_or_default();
        let filename = format!(
            "{title}{year_part} [{}].strm",
            sanitize_segment(item.item.id.as_str())
        );
        return Some(PathBuf::from(view_folder).join(filename));
    }

    if item_type == "episode" {
        let series_name =
            sanitize_segment(item.item.series_name.as_deref().unwrap_or("Unknown Series"));
        let season_number = item.item.parent_index_number.unwrap_or(0).clamp(0, 999);
        let episode_number = item.item.index_number.unwrap_or(0).clamp(0, 9999);
        let title = sanitize_segment(item.item.name.as_str());
        let filename = format!(
            "S{season_number:02}E{episode_number:02} - {title} [{}].strm",
            sanitize_segment(item.item.id.as_str())
        );
        let season_folder = format!("Season {season_number:02}");
        return Some(
            PathBuf::from(view_folder)
                .join(series_name)
                .join(season_folder)
                .join(filename),
        );
    }

    None
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

    format!(
        "http://127.0.0.1:{}/api/remote-emby/proxy/{}/{remote_item_id}?{query}",
        config.port, source_id
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
