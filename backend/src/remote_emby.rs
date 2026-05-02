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
use dashmap::{DashMap, DashSet};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use uuid::Uuid;

/// PB43：远端同步主循环里"同一页 items"并发处理度。
///
/// - I/O 密集（每条目主要在等 DB / 磁盘 / 偶发 Series-detail HTTP），8 是稳定收益区间。
/// - 与每源的 `request_interval_ms` 节流共存：所有并发任务共享 `REMOTE_REQUEST_THROTTLE`
///   的 per-source mutex，所以同时并发的 HTTP 请求总数不会超过节流阈值。
/// - 太大（>16）会让 sqlx 连接池排队、磁盘小文件写竞争加剧，反而劣化速率。
const REMOTE_SYNC_INNER_CONCURRENCY: usize = 8;

/// PB46：远端 Series 详情后台并发度。
///
/// PB43 之前 `fetch_and_upsert_series_detail` 内联在 `process_one_remote_sync_item`
/// 的 `await` 链上：只要某个 series 的第一个 episode 落到某条 task 里，整个 task 就被
/// detail 那次远端 HTTP（最长 10s + 重试）+ DB 写 + people upsert 阻塞——把 episode
/// 主循环的并发槽白白占掉，导致整批 8 个 task 进度不齐。
///
/// PB46 把 detail 下沉到独立 spawn 池：
/// - episode 主循环（buffer_unordered=8）只负责把 episode 入库 → 立刻让位
/// - detail 任务统一在本 Semaphore 控制下后台跑，4 是相对保守值（detail 不只一次远端 HTTP，
///   还会触发 People 合并写人物表，比 episode 单条更重）
/// - sync_source_inner 末尾的 `FinalizingSeriesDetails` 阶段等齐所有 spawn handle 才回 Completed
const SERIES_DETAIL_CONCURRENCY: usize = 4;

const PLAYBACK_INFO_CACHE_TTL_SECS: u64 = 300;
const PLAYBACK_INFO_CACHE_MAX_ENTRIES: usize = 512;

/// per-source 拉取速率节流器：记录每个源「上一次发出 HTTP 请求的时间」，
/// 在 `get_json_with_retry` 入口处与 `request_interval_ms` 配合，串行串成
/// 「两次请求至少间隔 N 毫秒」。Mutex 包 Instant 是为了让多个异步任务争用
/// 同一源时也能形成串行屏障（同时跑也强制最小间隔）。
static REMOTE_REQUEST_THROTTLE: std::sync::LazyLock<
    RwLock<HashMap<Uuid, Arc<tokio::sync::Mutex<Instant>>>>,
> = std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// PB49：per-source 同步互斥锁。
///
/// 旧链路有两条并发入口，互不感知：
///   1. HTTP `/api/admin/remote-emby/sources/{id}/sync` → `enqueue_remote_emby_sync`
///      （`active_operation_ids` 自身的 dedup 只覆盖 HTTP 路径）
///   2. 计划任务 / 媒体库扫描 → `incremental_update_library` →
///      直接调 `sync_source_with_progress`，**完全绕过** dedup
///
/// 当用户手动点「立即同步」恰好和定时任务撞到同一秒，两个 `sync_source_inner`
/// 同时跑——一个 task 缓存了某个 Series/Season 父行的 UUID，另一个 task
/// 在 `delete_stale_items_for_source` 里把这条父行 cascade-delete 掉，
/// 第一个 task 接着 INSERT Episode 用了那个已被删的 parent_id，触发
/// `media_items_parent_id_fkey` 违例。这是用户报告的 FK 报错的主路径。
///
/// 修复：所有 `sync_source_with_progress` 入口共享 per-source `Mutex`。
/// 用 `try_lock_owned` 而不是 `lock_owned`，让重复触发立刻拿到 BadRequest
/// 反馈，不在后台累积排队任务。
static SOURCE_SYNC_LOCKS: std::sync::LazyLock<
    RwLock<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
> = std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// PB49：拿不到 per-source sync 锁时返回的 BadRequest 文案的稳定标识子串。
/// 调用方（如 `incremental_update_library` 在定时任务路径上）用 `err.to_string().contains(...)`
/// 识别这类「软失败」，记 info 后跳过该源，不让定时扫描整体失败重试 3 次。
pub const SOURCE_SYNC_BUSY_TAG: &str = "[remote-emby-sync-busy]";

/// 取/创建给定 source_id 的同步互斥锁（始终拿到同一个 Arc）。
async fn get_source_sync_lock(source_id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
    if let Some(slot) = SOURCE_SYNC_LOCKS.read().await.get(&source_id).cloned() {
        return slot;
    }
    let mut write = SOURCE_SYNC_LOCKS.write().await;
    write
        .entry(source_id)
        .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

/// PB49：series-detail 后台 spawn 池的 RAII 守卫。
///
/// 用户报告的「任务 Failed 但媒体数仍在涨」根因：`sync_source_inner` 的
/// 主循环用 `?` 早退（比如 FK 违例向上传播）时，`series_detail_handles`
/// 容器里堆积的 `JoinHandle<()>` 直接被 drop——tokio task 默认 detach 而非
/// abort，所以一连串 `fetch_and_upsert_series_detail` 仍在背景里继续拉
/// 远端 HTTP、写 person/person_role 表，前端早就看到「Failed」但 DB 计数
/// 还在涨。
///
/// 修复：进入 buffer_unordered 主循环前用 `DetailHandlesGuard` 包住 spawn 池
/// + progress；只要走到 `?` 早退路径，guard.drop() 会：
///   1. `progress.request_cancel()` 让所有 in-flight detail task 在下一次
///      `is_cancelled()` 检查时立即收尾
///   2. 把容器里所有 `JoinHandle` 全部 `abort()`，强行打断已经在 await 的
///      远端 HTTP / DB 写
/// 正常完成路径在 await 完所有 handle 后调用 `disarm()` 解除上述行为。
struct DetailHandlesGuard {
    handles: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    progress: Option<RemoteSyncProgress>,
    disarmed: bool,
}

impl DetailHandlesGuard {
    fn new(
        handles: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
        progress: Option<RemoteSyncProgress>,
    ) -> Self {
        Self {
            handles,
            progress,
            disarmed: false,
        }
    }

    /// 正常完成路径调用：解除 Drop 时的 abort 行为。
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for DetailHandlesGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        // 先发取消信号让 in-flight 任务自己收尾
        if let Some(progress) = &self.progress {
            progress.request_cancel();
        }
        // 再 spawn 一个清理协程把容器里的 JoinHandle 全部 abort——
        // 此处已在 Drop（同步上下文）里，没法 `lock().await`，必须 spawn。
        // abort() 是同步的、立即返回的，spawn 的协程通常 1 个 tick 内完成。
        let handles = self.handles.clone();
        tokio::spawn(async move {
            let mut guard = handles.lock().await;
            for jh in guard.drain(..) {
                jh.abort();
            }
        });
    }
}

/// 以 `source.request_interval_ms` 为节奏对该源做限速：先取/创建该源的 Mutex 槽，
/// 进入临界区后计算「距上一次发请求的时间差」，不足 `request_interval_ms` 就 sleep
/// 补齐，最后把「now」写回作为下一次基准。`request_interval_ms <= 0` 时直接返回。
async fn throttle_remote_request(source_id: Uuid, request_interval_ms: i32) {
    if request_interval_ms <= 0 {
        return;
    }
    let interval = Duration::from_millis(request_interval_ms.max(0) as u64);
    let slot = {
        let read = REMOTE_REQUEST_THROTTLE.read().await;
        read.get(&source_id).cloned()
    };
    let slot = if let Some(slot) = slot {
        slot
    } else {
        let mut write = REMOTE_REQUEST_THROTTLE.write().await;
        write
            .entry(source_id)
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(Instant::now() - interval)))
            .clone()
    };
    let mut last = slot.lock().await;
    let elapsed = last.elapsed();
    if elapsed < interval {
        tokio::time::sleep(interval - elapsed).await;
    }
    *last = Instant::now();
}

/// 钳到合法范围的 page_size：source.page_size <= 0 时退默认 200。
fn effective_page_size(source: &DbRemoteEmbySource) -> i64 {
    let raw = if source.page_size <= 0 { 200 } else { source.page_size };
    raw.clamp(50, 1000) as i64
}

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

/// PB15：远端 token 刷新 / 失效时调用，按 source_id 前缀清除全部 PlaybackInfo 缓存。
/// 缓存 key 格式 `<source_id>:<remote_item_id>:<media_source_id>`，token 写入新值后
/// 历史缓存内的直链可能仍带旧 api_key，命中即触发 401/403 → 浪费一次重试；这里在
/// `update_remote_emby_source_access_token` / 自动刷新 / 401 续签等路径上主动失效，
/// 让下一次 PlaybackInfo 走真实远端拿到带新 token 的直链。
pub async fn invalidate_playback_info_cache_for_source(source_id: Uuid) {
    let prefix = format!("{}:", source_id);
    let mut cache = PLAYBACK_INFO_CACHE.write().await;
    cache.retain(|k, _| !k.starts_with(&prefix));
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

    /// SF1：FetchingRemoteIndex 阶段实时进度。
    /// 之前 `set_phase("FetchingRemoteIndex", 4.0)` 是一次性写入 → 在拉远端 ID 索引的
    /// 长耗时循环里前端看到的永远是「4% / 远端抓取 0/0」，没法判断是卡死还是仍在拉，
    /// 用户报告"已运行 684 秒还在 4%"就是这个症状。
    /// 这里把已扫 ID 数写进 `fetched_items` 字段，progress 在 `[4.0, 5.0]` 区间随
    /// view 总数 + 当前 view 进度线性爬动，前端「远端抓取」卡片就能看到 ID 数实时增长。
    pub fn set_fetching_index_progress(
        &self,
        scanned_ids: u64,
        view_index: usize,
        view_count: usize,
    ) {
        let snapshot = self.snapshot.clone();
        // 每个 view 各占 `1.0 / view_count` 个百分点，平均铺到 [4.0, 5.0)。即使 view
        // 个数 = 0（理论不会）或 page.total_record_count 为 0 也不会除零。
        let view_count_safe = view_count.max(1) as f64;
        let view_idx_clamped = (view_index as f64).min(view_count_safe);
        let progress = 4.0 + (view_idx_clamped / view_count_safe).min(1.0);
        let progress = progress.clamp(4.0, 5.0);
        if let Ok(mut guard) = snapshot.try_write() {
            guard.phase = "FetchingRemoteIndex".to_string();
            guard.fetched_items = scanned_ids;
            guard.progress = progress;
            return;
        }
        tokio::spawn(async move {
            let mut guard = snapshot.write().await;
            guard.phase = "FetchingRemoteIndex".to_string();
            guard.fetched_items = scanned_ids;
            guard.progress = progress;
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
    // PB42：分页请求时 `Fields=MediaSources` 带回的 `MediaSource[]` 内嵌完整 `MediaStreams[]`，
    // 类型直接复用 `RemotePlaybackMediaSource`/`RemotePlaybackMediaStream`，
    // 这样同步阶段不再发 `/PlaybackInfo` 也能重建 MediaAnalysisResult。顶层 `MediaStreams`
    // 字段在 BaseItemDto 上是冗余镜像，这里不再单独反序列化（避免重复内存 + 减少警告）。
    media_sources: Option<Vec<RemotePlaybackMediaSource>>,
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
    // PB31-1：远端 People 字段（演职员）。Emby 标准结构：每项含 Id/Name/Role/Type/PrimaryImageTag/ProviderIds。
    // 不再依赖后续 TMDB 异步刷新 — 在远端 sync 阶段直接落 persons / person_roles 两表。
    #[serde(default)]
    people: Vec<RemotePersonEntry>,
    // PB34-1：远端字段映射补齐 — Emby BaseItemDto 直接返回的扩展字段。
    #[serde(default)]
    original_title: Option<String>,
    #[serde(default)]
    sort_name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    taglines: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    production_locations: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    air_days: Vec<String>,
    #[serde(default)]
    air_time: Option<String>,
    #[serde(default)]
    remote_trailers: Option<Value>,
}

/// PB31-1：远端 Emby 的 People 子项反序列化。
///
/// Emby 返回结构：
/// ```json
/// { "Id": "...", "Name": "...", "Role": "...", "Type": "Actor",
///   "PrimaryImageTag": "...", "ProviderIds": { "Tmdb": "..." } }
/// ```
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemotePersonEntry {
    #[serde(default)]
    id: Option<String>,
    name: String,
    #[serde(default, rename = "Role")]
    role: Option<String>,
    #[serde(default, rename = "Type")]
    person_type: Option<String>,
    #[serde(default)]
    primary_image_tag: Option<String>,
    #[serde(default)]
    provider_ids: Value,
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

// PB42：原来本地维护的 `RemoteMediaSource` / `RemoteItemMediaStream` 是 Emby 字段的精简子集
// （只反序列化 6 个字段），之后想用同步阶段自带的数据重建 MediaAnalysisResult 时缺一大半字段
// （width/height/bit_rate/channels/...）。直接复用 `RemotePlaybackMediaSource` /
// `RemotePlaybackMediaStream`，这两个结构体已在 PlaybackInfo 解析路径里覆盖了所有用得上的字段。

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

    // PB39：preview（创建 source 前的「连通性测试」）也走伪装链路，避免在远端 Devices 表
    // 先留一个 `movie-rust-preview-...` 痕迹再被覆盖；32 位 hex device_id 不含项目名前缀，
    // `emby_auth_header_for_device` 内部已经默认 Infuse-Direct on Apple TV / 8.2.4。
    let device_id = Uuid::new_v4().simple().to_string();
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
    // PB49：per-source 互斥锁。`try_lock_owned` 拿不到就立刻 BadRequest，
    // 不让两个 sync 在同一 source 上并发跑（避免父行被一方删、另一方还拿着
    // 父行 UUID 去 INSERT Episode 触发 FK 违例的 race）。
    //
    // 错误信息里加 `SOURCE_SYNC_BUSY_TAG` 稳定子串：定时任务路径
    // (`incremental_update_library`) 用它识别「另一个 sync 正在跑」的软失败，
    // 记 info 后跳过该源，避免定时任务整轮失败 + auto-retry 3 次。
    let source_lock = get_source_sync_lock(source_id).await;
    let _sync_guard = source_lock.try_lock_owned().map_err(|_| {
        AppError::BadRequest(format!(
            "该远端源已有同步任务在执行，请等待当前任务结束或先点「中断同步」 {SOURCE_SYNC_BUSY_TAG}"
        ))
    })?;

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
    // PB49：错误返回时，显式给 progress 推一个 cancel，避免任何「在错误抛出
    // 之后还可能被 spawn 拿到、并仍认为自己有效」的后台任务继续写库。
    // sync_source_inner 自身的 DetailHandlesGuard 在错误退出路径上已经 abort
    // 了所有 series-detail JoinHandle，这里再补一次 cancel 是双保险。
    if result.is_err() {
        if let Some(handle) = &progress {
            handle.request_cancel();
        }
    }
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

    // 同步语义统一为「增 / 改 / 删」三段式：
    //   - last_sync_at = Some  → 仅拉取远端在该水位线之后变更的条目（增量改），
    //     并允许覆盖本地已存在的 sidecar（poster/backdrop/logo/.nfo/字幕）。
    //   - last_sync_at = None  → 视作首次/恢复同步，拉取全部条目用于补齐 DB；
    //     但仍然不删除 strm_workspace、不全表 cleanup，已存在的 sidecar 一律保留，
    //     避免覆盖用户手动 POST /Items/{id}/Refresh 写入的 NFO/封面。
    // 任何情况下都通过 delete_stale_items_for_source 同步「删」远端已不存在的条目。
    let incremental_since = source.last_sync_at;
    let force_refresh_sidecar = incremental_since.is_some();

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

    // 仅用于日志/legacy 兼容（路径以 source_root 开头的旧 STRM 文件）
    let _legacy_source_root = target_library_opt
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

    // STRM 工作区保持现有内容，不整体清空，但确保根目录存在
    tokio::fs::create_dir_all(&strm_workspace)
        .await
        .map_err(|e| AppError::Internal(format!("创建 STRM 工作区失败: {e}")))?;

    if let Some(handle) = &progress {
        handle.set_phase("FetchingRemoteIndex", 4.0);
    }
    // 1. 获取远端全量 ID 集合，检测「删」（远端已删除但本地仍在 DB 的条目）
    // SF1：把 progress handle 传下去，让 fetch loop 能在每页之间检查 is_cancelled
    // 与上报已扫 ID 数（避免 4% 长卡死时前端无任何反馈）。
    let remote_id_set = fetch_all_remote_item_ids(
        &state.pool,
        source,
        user_id.as_str(),
        &views,
        progress.as_ref(),
    )
    .await?;

    if let Some(handle) = &progress {
        handle.set_phase("PruningStaleItems", 5.0);
    }
    // 2. 从 DB（与对应 STRM 文件夹）删除远端已不存在的条目
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
            "同步「删」：清理远端已下架条目"
        );
    }

    // PB43：把所有"跨条目共享、需要并发安全"的集合换成 DashMap/DashSet，让 buffer_unordered
    // 启动的多任务能直接访问 (per-key lock + lock-free read，无需手动 Mutex)。
    let tvshow_roots_written: Arc<DashSet<PathBuf>> = Arc::new(DashSet::new());
    let series_parent_map: Arc<DashMap<String, Uuid>> = Arc::new(DashMap::new());
    let season_parent_map: Arc<DashMap<String, Uuid>> = Arc::new(DashMap::new());
    // PB31-2：已成功拉过详情的 series_id 集合，避免每个 episode 都触发一次 Series 详情拉取。
    let series_detail_synced: Arc<DashSet<String>> = Arc::new(DashSet::new());
    // PB43：进度计数器换成原子，让并发任务无锁累加。
    let fetched_count = Arc::new(AtomicU64::new(0));
    let written_files = Arc::new(AtomicU64::new(0));
    if let Some(handle) = &progress {
        handle.set_streaming_progress(0, 0, total_items);
    }
    // PB43：source 在并行段共享，仅做只读访问。`fetch_and_upsert_series_detail` 仍需要
    // `&mut DbRemoteEmbySource`（为偶发 401 触发 ensure_authenticated），所以那里在每个
    // task 内部 clone 一份本地可变副本——auth 状态写回 DB 是幂等的，不会出现冲突。
    let source_arc: Arc<DbRemoteEmbySource> = Arc::new(source.clone());

    // PB46：series detail 下沉用的 spawn 池资源。
    // - semaphore 限制后台 detail 同时在飞的远端 HTTP 数量
    // - handles 收集所有 spawn 的 JoinHandle，sync_source_inner 末尾统一等齐
    //   （让前端"Completed" 真的等于"全部 series 元数据已落库"）
    let series_detail_semaphore = Arc::new(tokio::sync::Semaphore::new(SERIES_DETAIL_CONCURRENCY));
    let series_detail_handles: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));

    // PB49：把 series-detail spawn 池套上 RAII Guard。
    // 任何 `?` 早退路径（FK 违例 / 取消 / DB 致命错误等）都会触发 Drop，
    // 自动 cancel + abort 所有 in-flight detail task。正常完成路径在
    // 末尾的 await 循环之后 `disarm()`。
    let mut detail_handles_guard =
        DetailHandlesGuard::new(Arc::clone(&series_detail_handles), progress.clone());

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

        // PB49：尝试从持久化游标处续抓——只有当上次同步语义（incremental_since）
        // 和本次完全一致时才会返回 Some(idx)；否则总是 0 起步，避免「上次全量
        // 卡在第 N 页 / 这次想做增量」时错位。
        let resume_index = repository::get_view_progress(
            &state.pool,
            source.id,
            view.id.as_str(),
            incremental_since,
        )
        .await
        .unwrap_or(None);
        let mut start_index = resume_index.unwrap_or(0);
        if resume_index.is_some() && start_index > 0 {
            tracing::info!(
                source_id = %source.id,
                view = %view.name,
                resume_start_index = start_index,
                "PB49：从上次中断处续抓远端 View"
            );
        }
        // 拉取速率：单源可调 page_size（50–1000），影响每页带回的条目数（与 request_interval_ms
        // 一起决定单源对远端的实际 QPS / 带宽消耗）。
        let page_size = effective_page_size(source);
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
                page_size,
                incremental_since,
            )
            .await?;
            if page.items.is_empty() {
                break;
            }

            // PB43：内层循环改成 `buffer_unordered`，让同一页内 items 并发处理。
            // 这是真正的提速大头——之前每条 await 5+ 次 IO（DB 写入 + 磁盘 + Series detail HTTP）
            // 串行串成 ~700ms / 条，并发 8 之后在 IO bound 场景下能压到 ~80-100ms。
            // page 内并发不会破坏分页推进（外层 loop 仍按 start_index 顺序拉），也不会和
            // 删除检测 / Series detail 去重冲突（DashMap/DashSet 提供并发安全）。
            use futures::stream::{self, StreamExt};
            let view_strm_workspace_arc = Arc::new(view_strm_workspace.clone());
            let view_id_arc = Arc::new(view.id.clone());
            let view_name_arc = Arc::new(view.name.clone());
            let user_id_arc = Arc::new(user_id.clone());
            let playback_token_arc = Arc::new(playback_token.clone());

            let item_results = stream::iter(page.items.into_iter().map(|base_item| {
                let state_cloned = state.clone();
                let source_for_task = Arc::clone(&source_arc);
                let view_strm_workspace = Arc::clone(&view_strm_workspace_arc);
                let view_id = Arc::clone(&view_id_arc);
                let view_name = Arc::clone(&view_name_arc);
                let user_id = Arc::clone(&user_id_arc);
                let playback_token = Arc::clone(&playback_token_arc);
                let series_parent_map = Arc::clone(&series_parent_map);
                let season_parent_map = Arc::clone(&season_parent_map);
                let series_detail_synced = Arc::clone(&series_detail_synced);
                let tvshow_roots_written = Arc::clone(&tvshow_roots_written);
                let fetched_count = Arc::clone(&fetched_count);
                let written_files = Arc::clone(&written_files);
                let series_detail_semaphore = Arc::clone(&series_detail_semaphore);
                let series_detail_handles = Arc::clone(&series_detail_handles);
                let progress = progress.clone();
                async move {
                    process_one_remote_sync_item(
                        &state_cloned,
                        source_for_task.as_ref(),
                        base_item,
                        view_id.as_str(),
                        view_name.as_str(),
                        view_strm_workspace.as_path(),
                        item_library_id,
                        user_id.as_str(),
                        playback_token.as_str(),
                        &series_parent_map,
                        &season_parent_map,
                        &series_detail_synced,
                        &tvshow_roots_written,
                        &fetched_count,
                        &written_files,
                        &series_detail_semaphore,
                        &series_detail_handles,
                        total_items,
                        progress.as_ref(),
                        force_refresh_sidecar,
                    )
                    .await
                }
            }))
            .buffer_unordered(REMOTE_SYNC_INNER_CONCURRENCY)
            .collect::<Vec<Result<(), AppError>>>()
            .await;

            // PB43：并发任务里任何 hard error（取消 / DB 致命错误等）都向上传播。
            // 单条目软失败（write_remote_strm_bundle 写盘错、Series detail 拉取失败等）
            // 在 process_one_remote_sync_item 内部已 warn 后吃掉，不会出现在 results 里。
            for r in item_results {
                r?;
            }

            start_index += page_size;
            // PB49：每页处理完都把当前 start_index 持久化为下次续抓的游标。
            // 写入失败仅 warn 不阻塞主链路——续抓只是性能优化，丢一次 cursor
            // 大不了下次从 0 重头扫一遍（与不开续抓功能等价），不影响正确性。
            if let Err(error) = repository::save_view_progress(
                &state.pool,
                source.id,
                view.id.as_str(),
                start_index,
                page.total_record_count,
                incremental_since,
            )
            .await
            {
                tracing::warn!(
                    source_id = %source.id,
                    view = %view.name,
                    start_index,
                    error = %error,
                    "保存远端同步续抓游标失败（不阻塞主链路）"
                );
            }
            if start_index >= page.total_record_count {
                break;
            }
        }
    }

    // PB46：等齐所有 series detail spawn task。
    //
    // 时间预算：detail spawn 的并发度是 SERIES_DETAIL_CONCURRENCY=4，单条 detail 一般
    // 1-3 秒（HTTP + people upsert）。如果是大库（1 万 series），最坏要等 ~2-3 分钟。
    // 但实际上 episode 主循环跑了几小时，detail spawn 一直在背景滚，到这里大多数都已完成。
    //
    // 取消语义：spawn task 内部已经 check 过 progress.is_cancelled，被取消的会立即 return；
    // 这里 join 不需要再 abort。take 之后清空容器，handles 持有的 JoinHandle drop 即 detach。
    let pending_handles = std::mem::take(&mut *series_detail_handles.lock().await);
    if !pending_handles.is_empty() {
        let pending_count = pending_handles.len();
        if let Some(handle) = &progress {
            handle.set_phase("FinalizingSeriesDetails", 99.0);
        }
        for h in pending_handles {
            // detail spawn 任务自身永远不 panic（内部 await Result 都已 warn-and-swallow），
            // JoinError 只可能来自任务被强制 abort——本次同步路径不会 abort，所以忽略。
            let _ = h.await;
        }
        tracing::info!(
            source_id = %source.id,
            count = pending_count,
            "PB46：所有 Series 详情后台同步完成"
        );
    }
    // PB49：成功走到这里说明所有 detail handle 都已 await 完毕，解除 guard 的
    // Drop 行为；否则 guard 会在函数末尾 drop 时再 spawn 一次空 abort，浪费但
    // 无副作用。disarm 必须在所有 `?` 早退之后才发生。
    detail_handles_guard.disarm();

    // PB49：整次 sync 走到这里说明所有 view 都已扫完且所有 detail handle 都已 await。
    // 此时清空续抓游标——下次同步将从 start_index=0 重新开始（这是期望行为：
    // 续抓游标是为「失败重试」准备的，成功完成后 stale 数据应清掉以免污染下次）。
    if let Err(error) = repository::clear_source_view_progress(&state.pool, source.id).await {
        tracing::warn!(
            source_id = %source.id,
            error = %error,
            "清空远端同步续抓游标失败（不阻塞主链路；下次同步会用旧游标再处理一次）"
        );
    }

    let fetched_count = fetched_count.load(Ordering::Relaxed);
    let written_files = written_files.load(Ordering::Relaxed) as usize;

    let mode_label = if incremental_since.is_some() {
        "增量(增/改/删)"
    } else {
        "首次(增/删)"
    };
    tracing::info!(
        source_id = %source.id,
        mode = mode_label,
        fetched = fetched_count,
        written = written_files,
        deleted_stale = deleted,
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

/// PB43：处理单个 RemoteSyncItem 的全流程（Series/Season 父行 → STRM/NFO 写盘 → DB upsert
/// → MediaStreams/people 写入 → 进度推进）。
///
/// 这是为 buffer_unordered 设计的"per-task 工作单元"：所有跨任务共享状态都通过引用传入，
/// 自身只持有局部变量。只在以下两种情况才 return Err：
/// 1. 任务被取消（progress.is_cancelled）→ AppError::BadRequest
/// 2. 致命 DB / IO 错误（DB 连接池崩、文件系统挂等）→ 向上传播终止整次同步
///
/// 软失败（比如 Series detail 拉不下来、单条 STRM 写盘失败）记 warn 后继续，不污染整次同步。
#[allow(clippy::too_many_arguments)]
async fn process_one_remote_sync_item(
    state: &AppState,
    source: &DbRemoteEmbySource,
    base_item: RemoteBaseItem,
    view_id: &str,
    view_name: &str,
    view_strm_workspace: &Path,
    item_library_id: Uuid,
    user_id: &str,
    playback_token: &str,
    series_parent_map: &DashMap<String, Uuid>,
    season_parent_map: &DashMap<String, Uuid>,
    series_detail_synced: &DashSet<String>,
    tvshow_roots_written: &DashSet<PathBuf>,
    fetched_count: &AtomicU64,
    written_files: &AtomicU64,
    // PB46：series detail 后台 spawn 池资源。
    series_detail_semaphore: &Arc<tokio::sync::Semaphore>,
    series_detail_handles: &Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>>,
    total_items: u64,
    progress: Option<&RemoteSyncProgress>,
    force_refresh_sidecar: bool,
) -> Result<(), AppError> {
    if let Some(handle) = progress {
        if handle.is_cancelled() {
            return Err(AppError::BadRequest("同步任务已被取消".to_string()));
        }
    }

    let item = RemoteSyncItem {
        item: base_item,
        view_id: view_id.to_string(),
        view_name: view_name.to_string(),
    };

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
            view_strm_workspace,
            item_library_id,
            series_parent_map,
        )
        .await?;
        series_db_id = Some(series_parent_id);

        // PB31-2 + PB43：每个 series_id 在同一同步任务里只跑一次详情拉取。
        // DashSet::insert 返回 bool 是 lock-free 的（CAS 实现），并发安全。
        if let Some(remote_sid) = item
            .item
            .series_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if series_detail_synced.insert(remote_sid.to_string()) {
                // PB46：把 detail 拉取从同步主链路 await 链上摘下来——episode 主循环
                // 只要把 episode 入库就立刻让位，detail 让独立 spawn 池后台慢慢补元数据。
                // sync_source_inner 末尾的 FinalizingSeriesDetails 阶段会 join 所有 spawn handle，
                // 保证整个 sync 任务结束时（phase=Completed）series 元数据真的全部到位。
                let pool_owned = state.pool.clone();
                let source_owned = source.clone();
                let user_id_owned = user_id.to_string();
                let remote_sid_owned = remote_sid.to_string();
                let series_view_scope_owned = series_view_scope.to_string();
                let series_dir_owned = view_strm_workspace
                    .join(sanitize_segment(remote_series_display_name(&item)));
                let semaphore = Arc::clone(series_detail_semaphore);
                let progress_owned = progress.cloned();
                let series_parent_id_copy = series_parent_id;
                let item_library_id_copy = item_library_id;
                let source_id_copy = source.id;

                let handle = tokio::spawn(async move {
                    // 取消优先：进 spawn 之前任务已被取消就直接放弃；不会触发任何远端 HTTP。
                    if let Some(p) = &progress_owned {
                        if p.is_cancelled() {
                            return;
                        }
                    }
                    let _permit = match semaphore.acquire().await {
                        Ok(p) => p,
                        Err(_) => return,
                    };
                    // 拿到 permit 之后再 check 一次 cancel——可能在排队等 permit 时取消。
                    if let Some(p) = &progress_owned {
                        if p.is_cancelled() {
                            return;
                        }
                    }
                    let mut source_local = source_owned;
                    if let Err(error) = fetch_and_upsert_series_detail(
                        &pool_owned,
                        &mut source_local,
                        user_id_owned.as_str(),
                        remote_sid_owned.as_str(),
                        series_parent_id_copy,
                        item_library_id_copy,
                        None,
                        series_dir_owned.as_path(),
                        series_view_scope_owned.as_str(),
                    )
                    .await
                    {
                        tracing::warn!(
                            source_id = %source_id_copy,
                            remote_series_id = %remote_sid_owned,
                            error = %error,
                            "PB46：远端 Series 详情后台同步失败，忽略继续"
                        );
                    }
                });
                series_detail_handles.lock().await.push(handle);
            }
        }

        let season_parent_id = ensure_remote_season_folder(
            &state.pool,
            source,
            &item,
            series_parent_id,
            view_strm_workspace,
            item_library_id,
            season_parent_map,
        )
        .await?;
        parent_id = Some(season_parent_id);
    }

    let media_source_id = first_media_source_id(&item);
    // PB42：分页带回的 MediaStreams 已够用，无需 PlaybackInfo 多发一次 HTTP。
    let analysis = synthesize_analysis_from_base_item(&item.item, media_source_id);
    let strm_bundle = match write_remote_strm_bundle(
        state,
        source,
        view_strm_workspace,
        playback_token,
        &item,
        media_source_id,
        tvshow_roots_written,
        force_refresh_sidecar,
    )
    .await
    {
        Ok(paths) => paths,
        Err(error) => {
            tracing::warn!(
                remote_item_id = %item.item.id,
                error = %error,
                "PB43：STRM 旁路写入失败，跳过此条目"
            );
            // 软失败：跳过本条目但不终止整次同步（与 PB42 之前 for 循环里的 `continue` 等价）。
            fetched_count.fetch_add(1, Ordering::Relaxed);
            return Ok(());
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
        repository::update_media_item_metadata(&state.pool, upserted, &analysis).await?;
    }
    if !item.item.people.is_empty() {
        if let Err(error) =
            upsert_remote_people_for_item(&state.pool, source, upserted, &item.item.people).await
        {
            tracing::warn!(
                source_id = %source.id,
                remote_item_id = %item.item.id,
                error = %error,
                "PB31-1：写入远端 item 的 People 失败"
            );
        }
    }

    let f = fetched_count.fetch_add(1, Ordering::Relaxed) + 1;
    let w = written_files.fetch_add(1, Ordering::Relaxed) + 1;
    if let Some(handle) = progress {
        handle.set_streaming_progress(f, w, total_items);
    }
    Ok(())
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

/// PB40：远端图片 URL 全采集。让 7 类海报 + 全部 Backdrops 一次性从远端 BaseItemDto
/// 派生出来，不再只保留 Primary + 第一张 Backdrop（以前只取 (Option<String>, Option<String>)
/// 的方式让 Logo/Thumb/Banner/Art/Disc 全部丢失，详情页不得不去 TMDB 二次拉取）。
#[derive(Debug, Default, Clone)]
struct RemoteImageUrls {
    /// `ImageTags.Primary` → `/Items/{id}/Images/Primary?tag=...`
    primary: Option<String>,
    /// `ImageTags.Logo` → `/Items/{id}/Images/Logo?tag=...`
    logo: Option<String>,
    /// `ImageTags.Thumb`
    thumb: Option<String>,
    /// `ImageTags.Banner`
    banner: Option<String>,
    /// `ImageTags.Art`
    art: Option<String>,
    /// `ImageTags.Disc`
    disc: Option<String>,
    /// `BackdropImageTags[]` → 多张：`/Items/{id}/Images/Backdrop/{idx}?tag=...`
    /// 第一张兼容存到 `media_items.backdrop_path`，全部存到 `backdrop_paths` 数组列。
    backdrops: Vec<String>,
}

impl RemoteImageUrls {
    fn first_backdrop(&self) -> Option<&str> {
        self.backdrops.first().map(|s| s.as_str())
    }
}

/// 远端 backdrop 多张时，索引化生成 URL：`/Images/Backdrop/0`、`/Images/Backdrop/1` 等。
/// Emby 服务端按顺序返回 BackdropImageTags，索引等于数组下标。
fn remote_backdrop_indexed_url(
    server_url: &str,
    remote_item_id: &str,
    index: usize,
    tag: &str,
) -> String {
    let base = server_url.trim_end_matches('/');
    format!(
        "{base}/emby/Items/{remote_item_id}/Images/Backdrop/{index}?tag={tag}&quality=90&maxWidth=1920"
    )
}

fn extract_image_tag_field<'a>(image_tags: &'a Option<Value>, key: &str) -> Option<&'a str> {
    match image_tags.as_ref()? {
        Value::Object(map) => map.get(key).and_then(Value::as_str),
        _ => None,
    }
}

fn extract_remote_image_urls_full(
    server_url: &str,
    remote_item_id: &str,
    image_tags: &Option<Value>,
    backdrop_image_tags: &Option<Value>,
) -> RemoteImageUrls {
    // 7 类单图：直接按 ImageTags.Key 取 tag 字符串拼 URL；缺则保持 None，调用方决定是否回退。
    let url_for = |key: &str| -> Option<String> {
        extract_image_tag_field(image_tags, key)
            .map(|tag| remote_image_url(server_url, remote_item_id, key, tag))
    };
    // 多张 Backdrop：BackdropImageTags 通常是 `string[]`；个别旧版 Emby 可能返 `Object` 或 `String`，
    // 这里都兼容下来。索引一律用数组顺序，URL 走 `/Images/Backdrop/{idx}`。
    let mut backdrops: Vec<String> = Vec::new();
    if let Some(value) = backdrop_image_tags.as_ref() {
        match value {
            Value::Array(arr) => {
                for (idx, tag) in arr.iter().enumerate() {
                    if let Some(t) = tag.as_str().filter(|s| !s.is_empty()) {
                        backdrops.push(remote_backdrop_indexed_url(server_url, remote_item_id, idx, t));
                    }
                }
            }
            Value::Object(map) => {
                for (idx, (_, tag)) in map.iter().enumerate() {
                    if let Some(t) = tag.as_str().filter(|s| !s.is_empty()) {
                        backdrops.push(remote_backdrop_indexed_url(server_url, remote_item_id, idx, t));
                    }
                }
            }
            Value::String(s) => {
                if !s.is_empty() {
                    backdrops.push(remote_backdrop_indexed_url(server_url, remote_item_id, 0, s));
                }
            }
            _ => {}
        }
    }
    RemoteImageUrls {
        primary: url_for("Primary"),
        logo: url_for("Logo"),
        thumb: url_for("Thumb"),
        banner: url_for("Banner"),
        art: url_for("Art"),
        disc: url_for("Disc"),
        backdrops,
    }
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

    // 删除源时清掉它的拉取速率节流槽，避免 HashMap 累积陈旧条目（重新创建同 id 概率极低，但仍清理）
    {
        let mut throttle = REMOTE_REQUEST_THROTTLE.write().await;
        throttle.remove(&source.id);
    }

    // PB23：删除源时同步清掉 libraries 表里挂的虚拟路径——
    //   - separate 模式：`ensure_view_library` 自动建出来的独立库（path 以 `__remote_view_<source_id>_` 起头）整条删；
    //   - merge 模式：用户原有库 library_options.PathInfos 里的远端 view 路径剥掉；
    // 这样用户在 /settings/libraries 里就不会看到「片源 13113，路径 __remote_view_xxx」这种残留，
    // 也不会再被扫描器误进入虚拟路径触发文件不存在告警。
    match repository::cleanup_remote_view_paths_for_source(pool, source.id).await {
        Ok((deleted_libs, updated_libs)) => {
            if deleted_libs > 0 || updated_libs > 0 {
                tracing::info!(
                    source_id = %source.id,
                    deleted_libs,
                    updated_libs,
                    "已清理与远端源关联的 libraries（separate/merge 两类虚拟路径）"
                );
            }
        }
        Err(error) => {
            tracing::warn!(
                source_id = %source.id,
                error = %error,
                "清理 libraries 远端 view 路径失败（不阻塞源删除）"
            );
        }
    }

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

    // 当前版本同步真正写 STRM 的根目录是 `{source.strm_output_path}/{sanitize(source.name)}/`
    // （由 `try_strm_workspace_for_source` 计算，与 `sync_source_inner` 写盘 / watcher 监控一致）。
    // 上面的 `source_root` 是早期 strm 内嵌在 library 内部时的 legacy 路径，几乎从不命中，
    // 仅当作历史残留兜底；如果只删 `source_root`，用户在 strm_output_path 下仍会看到一个
    // 鬼魂 `{源名}/{视图名}/...` 子树（含历史 strm 与 sidecar），既占盘也容易被 watcher 误捡。
    // 这里专门补一次：删 source 时把当前 strm 工作目录整棵砍掉，与同步写盘路径形成闭环。
    if let Some(strm_workspace) = try_strm_workspace_for_source(source) {
        match tokio::fs::remove_dir_all(&strm_workspace).await {
            Ok(_) => {
                tracing::info!(
                    source_id = %source.id,
                    strm_workspace = %strm_workspace.to_string_lossy(),
                    "已删除远端 Emby 源对应的 STRM 工作目录"
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                tracing::warn!(
                    source_id = %source.id,
                    strm_workspace = %strm_workspace.to_string_lossy(),
                    error = %error,
                    "删除远端 Emby STRM 工作目录失败"
                );
            }
        }
    } else {
        tracing::debug!(
            source_id = %source.id,
            "源未配置 strm_output_path，跳过 STRM 工作目录删除"
        );
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
    // PB43：DashMap 提供 per-key 锁 + lock-free 读，多个并发任务处理同一 source 的不同 series
    // 时不会互相阻塞；同一 series 的并发请求由 `series_key` 短临界区天然串行。
    series_parent_map: &DashMap<String, Uuid>,
) -> Result<Uuid, AppError> {
    let series_name = remote_series_display_name(item).to_string();
    // 优先使用 SeriesId 做去重 key，避免同名不同剧冲突
    let series_key = if let Some(sid) = item.item.series_id.as_deref().filter(|s| !s.trim().is_empty()) {
        format!("{view_scope}::{sid}")
    } else {
        format!("{view_scope}::{}", sanitize_segment(series_name.as_str()))
    };
    if let Some(existing) = series_parent_map.get(series_key.as_str()) {
        return Ok(*existing.value());
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
    season_parent_map: &DashMap<String, Uuid>,
) -> Result<Uuid, AppError> {
    let season_number = item.item.parent_index_number.unwrap_or(0).clamp(0, 999);
    let season_key = format!("{}::{season_number}", series_parent_id);
    if let Some(existing) = season_parent_map.get(season_key.as_str()) {
        return Ok(*existing.value());
    }
    let series_name_raw = remote_series_display_name(item);
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
    // PB40：远端 7 类图 + 多张 Backdrop 一次性提取。之前只取 Primary + 第一张 Backdrop，
    // 把远端已经提供的 Logo / Thumb / Banner / Art / Disc / 后续 Backdrop 全丢了，
    // 然后详情页冷启动时再去 TMDB 补回——既慢又浪费配额。
    let remote_urls = extract_remote_image_urls_full(
        source.server_url.as_str(),
        item.item.id.as_str(),
        &item.item.image_tags,
        &item.item.backdrop_image_tags,
    );
    // Episode 回退：当自身没有 Primary 图时，使用 Series 的 Primary 图
    let series_fallback_primary = if remote_urls.primary.is_none() && is_episode {
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
    // Episode 回退（PB40 扩充）：自身缺 Logo/Backdrop 时分别用 ParentLogo* / ParentBackdrop* 去拼。
    // ParentLogoItemId / ParentLogoImageTag 是 Emby 在 Episode 上专门暴露的"剧集 Logo"指针。
    let series_fallback_logo = if remote_urls.logo.is_none() && is_episode {
        match (
            item.item.parent_logo_item_id.as_deref(),
            item.item.parent_logo_image_tag.as_deref(),
        ) {
            (Some(lid), Some(tag)) if !lid.is_empty() && !tag.is_empty() => {
                Some(remote_image_url(source.server_url.as_str(), lid, "Logo", tag))
            }
            _ => None,
        }
    } else {
        None
    };
    // 多 backdrop：自身没有则用 ParentBackdropItemId + ParentBackdropImageTags 数组全量回退。
    let series_fallback_backdrops: Vec<String> = if remote_urls.backdrops.is_empty() && is_episode {
        let backdrop_item = item
            .item
            .parent_backdrop_item_id
            .as_deref()
            .or(item.item.series_id.as_deref());
        if let Some(bid) = backdrop_item {
            match item.item.parent_backdrop_image_tags.as_ref() {
                Some(Value::Array(arr)) => arr
                    .iter()
                    .enumerate()
                    .filter_map(|(idx, v)| {
                        v.as_str()
                            .filter(|s| !s.is_empty())
                            .map(|t| remote_backdrop_indexed_url(source.server_url.as_str(), bid, idx, t))
                    })
                    .collect(),
                Some(Value::String(s)) if !s.is_empty() => {
                    vec![remote_backdrop_indexed_url(source.server_url.as_str(), bid, 0, s)]
                }
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let image_primary_path = local_poster
        .or_else(|| remote_urls.primary.as_ref().map(|s| Path::new(s.as_str())))
        .or_else(|| series_fallback_primary.as_ref().map(|s| Path::new(s.as_str())));
    // backdrop_path（兼容字段，单值）取自身第一张；缺则用 episode 的 series 回退第一张。
    let backdrop_path = local_backdrop
        .or_else(|| remote_urls.first_backdrop().map(Path::new))
        .or_else(|| series_fallback_backdrops.first().map(|s| Path::new(s.as_str())));
    // 7 类图剩余 6 类：全部从远端 ImageTags 提取；Logo 单独有 Episode→Series 回退。
    let logo_path_owned: Option<String> = remote_urls.logo.clone().or(series_fallback_logo);
    // local_logo 路径（来自 STRM 输出目录的 logo.png 等）优先级最高。
    let logo_path = local_logo.or_else(|| logo_path_owned.as_ref().map(|s| Path::new(s.as_str())));
    let thumb_path = remote_urls.thumb.as_ref().map(|s| Path::new(s.as_str()));
    let banner_path = remote_urls.banner.as_ref().map(|s| Path::new(s.as_str()));
    let art_path = remote_urls.art.as_ref().map(|s| Path::new(s.as_str()));
    let disc_path = remote_urls.disc.as_ref().map(|s| Path::new(s.as_str()));
    // 全部 backdrops（自身全部 + episode 缺则用 series 回退全部）；media_items.backdrop_paths 是 text[]。
    let all_backdrops: Vec<String> = if !remote_urls.backdrops.is_empty() {
        remote_urls.backdrops.clone()
    } else {
        series_fallback_backdrops.clone()
    };

    // PB34-1：把远端 BaseItemDto 的扩展字段透传到本地 DB（之前一律置 None / 空数组）。
    let trailers = remote_trailers_to_vec(item.item.remote_trailers.as_ref());
    let inserted_id = repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id,
            parent_id,
            name: item.item.name.as_str(),
            item_type,
            media_type: "Video",
            path: path_ref,
            container,
            original_title: item.item.original_title.as_deref(),
            overview: item.item.overview.as_deref(),
            production_year: item.item.production_year,
            official_rating: item.item.official_rating.as_deref(),
            community_rating: item.item.community_rating,
            critic_rating: item.item.critic_rating,
            runtime_ticks,
            premiere_date: parse_remote_premiere_date(item.item.premiere_date.as_deref()),
            status: item.item.status.as_deref(),
            end_date: parse_remote_premiere_date(item.item.end_date.as_deref()),
            air_days: &item.item.air_days,
            air_time: item.item.air_time.as_deref(),
            provider_ids,
            genres: &item.item.genres,
            studios: &item.item.studios,
            tags: &item.item.tags,
            production_locations: &item.item.production_locations,
            image_primary_path,
            backdrop_path,
            logo_path,
            thumb_path,
            art_path,
            banner_path,
            disc_path,
            backdrop_paths: &all_backdrops,
            remote_trailers: &trailers,
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
    .map(|(id, _was_new)| id)?;
    // PB34-1：taglines 通过 inline UPDATE 写入（DbMediaItem 字段绑定将在 PB32 一起补）。
    if !item.item.taglines.is_empty() {
        let _ = sqlx::query("UPDATE media_items SET taglines = $1, updated_at = now() WHERE id = $2")
            .bind(&item.item.taglines)
            .bind(inserted_id)
            .execute(pool)
            .await;
    }
    Ok(inserted_id)
}

/// PB31-1：把远端 Emby 返回的 `People[]` 同步落本地 `persons` / `person_roles` 两表。
///
/// 设计要点：
/// - 头像路径直接落 **远端 URL** 字符串（`{server}/Items/{personId}/Images/Primary?tag=...&...`）；
///   `serve_image / resolve_person_image_path` 已经能识别 `http://...` 做远端代理回源，无需先下载到磁盘。
///   这样大库一次同步几十万条目时不会因为下载头像放大 IO/带宽。
/// - 同时把远端 `personId` 写入 `persons.provider_ids` 的 `RemoteEmbyId` / `RemoteEmbySourceId`
///   marker 字段，方便后续按需替换为本地缓存（PB35-2 / PB35-4 P2-2）。
/// - 对每个角色按 `RemotePersonEntry.person_type` 写 `Actor/Director/Writer/Producer/GuestStar`，
///   `sort_order` 用枚举顺序（保持与 TMDB 同步逻辑一致）。
/// - 不主动删除已有 person_roles —— 让 `delete_tmdb_person_roles_except` 与
///   后续 TMDB 异步刷新统一管理。
async fn upsert_remote_people_for_item(
    pool: &sqlx::PgPool,
    source: &DbRemoteEmbySource,
    media_item_id: Uuid,
    people: &[RemotePersonEntry],
) -> Result<(), AppError> {
    if people.is_empty() {
        return Ok(());
    }
    let server_base = source.server_url.trim_end_matches('/').to_string();
    let mut sort_order = 0i32;
    for entry in people {
        let name = entry.name.trim();
        if name.is_empty() {
            continue;
        }
        let role_type = match entry.person_type.as_deref().map(str::trim) {
            Some(s) if !s.is_empty() => match s.to_ascii_lowercase().as_str() {
                "actor" => "Actor".to_string(),
                "director" => "Director".to_string(),
                "writer" => "Writer".to_string(),
                "producer" => "Producer".to_string(),
                "gueststar" => "GuestStar".to_string(),
                "composer" => "Composer".to_string(),
                _ => s.to_string(),
            },
            _ => "Actor".to_string(),
        };

        // 头像 URL：仅当远端确实给了 personId 才能拼，否则交给后续 TMDB 异步刷新
        let primary_image_url: Option<String> = match (
            entry.id.as_deref().filter(|s| !s.trim().is_empty()),
            entry.primary_image_tag.as_deref().filter(|s| !s.trim().is_empty()),
        ) {
            (Some(person_id), Some(tag)) => Some(format!(
                "{server_base}/emby/Items/{person_id}/Images/Primary?tag={tag}&quality=90&maxWidth=1920"
            )),
            (Some(person_id), None) => Some(format!(
                "{server_base}/emby/Items/{person_id}/Images/Primary?quality=90&maxWidth=1920"
            )),
            _ => None,
        };

        // provider_ids：合并远端 ProviderIds（含 Tmdb/Imdb/Tvdb）+ 远端 marker（用于回源头像）
        let mut merged_provider_ids = entry.provider_ids.clone();
        if !matches!(merged_provider_ids, Value::Object(_)) {
            merged_provider_ids = Value::Object(serde_json::Map::new());
        }
        if let (Some(map), Some(person_id)) = (
            merged_provider_ids.as_object_mut(),
            entry.id.as_deref().filter(|s| !s.trim().is_empty()),
        ) {
            map.insert(
                "RemoteEmbyId".to_string(),
                Value::String(person_id.to_string()),
            );
            map.insert(
                "RemoteEmbySourceId".to_string(),
                Value::String(source.id.to_string()),
            );
        }

        match repository::upsert_person_reference(
            pool,
            name,
            merged_provider_ids,
            primary_image_url.as_deref(),
            None,
        )
        .await
        {
            Ok(person_id) => {
                if let Err(error) = repository::upsert_person_role(
                    pool,
                    person_id,
                    media_item_id,
                    role_type.as_str(),
                    entry.role.as_deref(),
                    sort_order,
                )
                .await
                {
                    tracing::warn!(
                        media_item_id = %media_item_id,
                        person = %name,
                        role_type = %role_type,
                        error = %error,
                        "PB31-1：写入远端 person_role 失败"
                    );
                }
            }
            Err(error) => {
                tracing::warn!(
                    media_item_id = %media_item_id,
                    person = %name,
                    error = %error,
                    "PB31-1：写入远端 persons 失败"
                );
            }
        }
        sort_order = sort_order.saturating_add(1);
    }
    Ok(())
}

/// PB31-2：拉取一个远端 Series 的详情并覆盖本地 series 行。
///
/// 远端列表接口在 `IncludeItemTypes=Movie,Episode` 时不会返回 Series，因此本地 series 行
/// 只能通过 episode 反推占位。本函数补齐 series 自身的 overview / studios / genres /
/// status / end_date / taglines / production_locations / air_days / air_time / 主图 /
/// People（主演阵容），让详情页一开始就能展示完整元数据，并写 tvshow.nfo 做 NFO 落盘。
///
/// 调用方负责通过 `series_detail_synced` 集合做去重（同一同步任务每个 series_id 只跑一次），
/// 同时本函数内部不持有同步任务的 cancel 信号，调用方应在调用前自行检查 cancel。
async fn fetch_and_upsert_series_detail(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    remote_series_id: &str,
    series_db_id: Uuid,
    library_id: Uuid,
    parent_id: Option<Uuid>,
    series_dir: &Path,
    view_id: &str,
) -> Result<(), AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items/{remote_series_id}");
    let query = vec![(
        "Fields".to_string(),
        "Overview,Genres,Studios,Tags,Status,EndDate,ProductionYear,OfficialRating,\
         CommunityRating,CriticRating,PremiereDate,ProviderIds,People,ImageTags,\
         BackdropImageTags,OriginalTitle,SortName,Taglines,ProductionLocations,\
         AirDays,AirTime,RemoteTrailers"
            .to_string(),
    )];
    let detail: RemoteBaseItem = match get_json_with_retry(pool, source, &endpoint, &query).await {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(
                source_id = %source.id,
                remote_series_id = %remote_series_id,
                error = %error,
                "PB31-2：拉取远端 Series 详情失败，沿用占位数据"
            );
            return Ok(());
        }
    };

    let series_name = detail.name.clone();
    // PB40：Series 详情同样把 7 类图 + 全部 Backdrop 一次性提取，避免详情页二次回 TMDB。
    let remote_urls = extract_remote_image_urls_full(
        source.server_url.as_str(),
        remote_series_id,
        &detail.image_tags,
        &detail.backdrop_image_tags,
    );
    let primary_path = remote_urls.primary.as_deref().map(Path::new);
    let backdrop_path = remote_urls.first_backdrop().map(Path::new);
    let logo_path = remote_urls.logo.as_deref().map(Path::new);
    let thumb_path = remote_urls.thumb.as_deref().map(Path::new);
    let banner_path = remote_urls.banner.as_deref().map(Path::new);
    let art_path = remote_urls.art.as_deref().map(Path::new);
    let disc_path = remote_urls.disc.as_deref().map(Path::new);
    let provider_ids = merge_provider_ids(
        &detail.provider_ids,
        remote_marker_provider_ids(source.id, Some(remote_series_id), Some(view_id), None),
    );
    let empty = Vec::<String>::new();
    let trailers = remote_trailers_to_vec(detail.remote_trailers.as_ref());

    if let Err(error) = repository::upsert_media_item(
        pool,
        repository::UpsertMediaItem {
            library_id,
            parent_id,
            name: series_name.as_str(),
            item_type: "Series",
            media_type: "Video",
            path: series_dir,
            container: None,
            original_title: detail.original_title.as_deref(),
            overview: detail.overview.as_deref(),
            production_year: detail.production_year,
            official_rating: detail.official_rating.as_deref(),
            community_rating: detail.community_rating,
            critic_rating: detail.critic_rating,
            runtime_ticks: None,
            premiere_date: parse_remote_premiere_date(detail.premiere_date.as_deref()),
            status: detail.status.as_deref(),
            end_date: parse_remote_premiere_date(detail.end_date.as_deref()),
            air_days: &detail.air_days,
            air_time: detail.air_time.as_deref(),
            provider_ids,
            genres: &detail.genres,
            studios: &detail.studios,
            tags: &detail.tags,
            production_locations: &detail.production_locations,
            image_primary_path: primary_path,
            backdrop_path,
            logo_path,
            thumb_path,
            art_path,
            banner_path,
            disc_path,
            backdrop_paths: &remote_urls.backdrops,
            remote_trailers: &trailers,
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
    .await
    {
        tracing::warn!(
            source_id = %source.id,
            remote_series_id = %remote_series_id,
            error = %error,
            "PB31-2：覆盖远端 Series 行失败"
        );
        return Ok(());
    }
    let _ = empty;

    if let Err(error) = upsert_remote_people_for_item(pool, source, series_db_id, &detail.people).await {
        tracing::warn!(
            source_id = %source.id,
            remote_series_id = %remote_series_id,
            error = %error,
            "PB31-2：写入远端 Series 的 People 失败"
        );
    }

    if !detail.taglines.is_empty() {
        // 直接 inline 写 taglines 列（DB 列已存在；DbMediaItem 字段绑定将在 PB32 一起补）。
        let _ = sqlx::query("UPDATE media_items SET taglines = $1, updated_at = now() WHERE id = $2")
            .bind(&detail.taglines)
            .bind(series_db_id)
            .execute(pool)
            .await;
    }

    Ok(())
}

/// 将远端 Emby 的 `RemoteTrailers` 字段（数组对象 `{ Name, Url }`）扁平为 URL 列表。
fn remote_trailers_to_vec(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                item.get("Url")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("url").and_then(Value::as_str))
                    .map(str::to_string)
            })
            .filter(|s| !s.is_empty())
            .collect(),
        _ => Vec::new(),
    }
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
    let page_size = effective_page_size(source);
    for view in views {
        let mut start_index = 0i64;
        loop {
            let page = fetch_remote_items_page_for_view(
                pool,
                source,
                user_id.as_str(),
                view.id.as_str(),
                start_index,
                page_size,
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
            start_index += page_size;

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
            // PB31-1 / PB34-1：补齐 People（演职员）+ OriginalTitle/SortName/Taglines/
            // ProductionLocations/AirDays/AirTime/RemoteTrailers 等 Emby BaseItemDto 字段，
            // 让远端同步一次性带回所有可直接展示的元数据，避免后续依赖按需 TMDB 刷新。
            "SeriesName,SeasonName,ProductionYear,ParentIndexNumber,IndexNumber,Overview,\
             OfficialRating,CommunityRating,CriticRating,PremiereDate,RunTimeTicks,\
             ProviderIds,Genres,Studios,Tags,MediaSources,MediaStreams,\
             ImageTags,BackdropImageTags,SeriesId,SeasonId,\
             SeriesPrimaryImageTag,ParentBackdropImageTags,ParentBackdropItemId,\
             ParentLogoItemId,ParentLogoImageTag,Status,EndDate,\
             People,OriginalTitle,SortName,Taglines,ProductionLocations,\
             AirDays,AirTime,RemoteTrailers"
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
        // 稳定分页排序键：Id 是 Emby 内部主键，全局唯一不重复；
        // 不指定 SortBy 时 Emby 默认按 SortName 升序，而大库里 SortName 重复 / 空串
        // 极常见，跨页时会跳过或重复条目（30 万条规模能稳定漏拉百~千条）。
        // 漏拉的 ID 会让 `delete_stale_items_for_source` 把对应本地行误判成 stale
        // 并 DELETE，是「重启后再同步媒体数量减少」灾难路径的根因之一。
        ("SortBy".to_string(), "Id".to_string()),
        ("SortOrder".to_string(), "Ascending".to_string()),
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
    progress: Option<&RemoteSyncProgress>,
) -> Result<HashSet<String>, AppError> {
    let mut all_ids = HashSet::new();
    let view_count = views.len();
    for (view_index, view) in views.iter().enumerate() {
        // SF1：进入新 view 之前先抢一次进度上报，让前端 phase=FetchingRemoteIndex 的
        // 进度条按 view 个数线性推进（4.0 → 5.0），不再一直停在 4%。
        if let Some(handle) = progress {
            handle.set_fetching_index_progress(all_ids.len() as u64, view_index, view_count);
        }
        let mut start_index = 0i64;
        loop {
            // SF1：每页之间检查 cancel —— 之前这里完全没检查，用户点中断后旧 task
            // 只能等所有 view + 所有 page 拉完才看到取消信号，对 ~40 万远端条目的
            // 大库要等 10+ 分钟。检查放在 HTTP 请求之前，最大一次「单页延迟」就退出。
            if let Some(handle) = progress {
                if handle.is_cancelled() {
                    return Err(AppError::BadRequest("同步任务已被取消".to_string()));
                }
            }
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
            // SF1：每拉完一页，把累计已扫 ID 数上报一次，前端「远端抓取」卡片
            // 就能看到 ID 数随 page 数实时增长，明确判断「在拉」还是「卡死」。
            if let Some(handle) = progress {
                handle.set_fetching_index_progress(all_ids.len() as u64, view_index, view_count);
            }
            if page.items.is_empty() || start_index >= page.total_record_count {
                break;
            }
        }
    }
    // 完成全部 view 后把进度推到 5%，下一阶段（PruningStaleItems）会接力。
    if let Some(handle) = progress {
        handle.set_fetching_index_progress(all_ids.len() as u64, view_count, view_count);
    }
    Ok(all_ids)
}

/// 删除本地 DB 中不再存在于远端的条目，并清理对应的 STRM 文件。
///
/// 内置「prune 比例安全阀」：当 `候选行数 ≥ PRUNE_GUARD_MIN_CANDIDATES` 且
/// 「待删 / 候选」超过 `PRUNE_GUARD_RATIO`（10%）时，判定 `remote_id_set`
/// 大概率不完整（典型场景：远端分页抖动 / 中途被 kill 重启再跑），整次
/// prune 直接跳过并 WARN，等下一次同步用更完整的 ID 集合重新判定，
/// 避免「重启 → 误删上千条 → 用户必须靠扫描所有媒体库恢复」灾难路径。
async fn delete_stale_items_for_source(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    _library_id: Uuid,
    remote_id_set: &HashSet<String>,
    strm_workspace: Option<&Path>,
) -> Result<u64, AppError> {
    /// 安全阀触发的最小候选量；小于此值的库视为新建/小库，prune 波动天然较大，
    /// 强行卡阈值会导致频繁误阻断，反而让小库永远没法清理真正的孤儿。
    const PRUNE_GUARD_MIN_CANDIDATES: usize = 100;
    /// 安全阀阈值：单次 prune 比例上限。10% 是经验值——
    /// 真实场景下用户在远端 Emby 一次性下架 10% 以上条目极罕见；
    /// 反过来分页抖动 / 部分视图拉空导致 ID 集合缺失 10% 以上很常见。
    const PRUNE_GUARD_RATIO_NUM: usize = 10;
    const PRUNE_GUARD_RATIO_DEN: usize = 100;
    /// PB49：单次 prune **绝对量** 上限。
    ///
    /// 比例阀只在小库上稳——10 万规模的库 9.9% = 9900 条「正好低于阈值」
    /// 但仍是不可接受的批量误删（用户报告就是「原先 10 万、现在 3938」）。
    /// 加一道绝对量保险：单次 stale 超过 5000 条就直接跳过，留给下次同步
    /// 用更完整的 ID 集合来判定。如果远端真有 >5000 条要下架，多次同步
    /// 也能渐进收敛。
    const PRUNE_GUARD_MAX_ABSOLUTE: usize = 5000;

    let source_id_str = source_id.to_string();
    struct StaleRow {
        id: Uuid,
        path: Option<String>,
        remote_id: Option<String>,
    }
    // 不再限制 library_id，支持 separate 模式下条目分散在多个库。
    // 仅检测有真实 RemoteEmbyItemId 的 Movie/Episode 节点：
    //   - Series/Season 节点的 RemoteEmbyItemId 写为空串（远端没有对应的 Movie/Episode ID），
    //     `IS NOT NULL` 不能过滤空串，必须额外用 <> '' 防御，避免把它们当作 stale 误删。
    //   - fetch_all_remote_item_ids 仅拉 Movie/Episode 类型，Series/Season 本就不在集合内。
    let rows: Vec<StaleRow> = sqlx::query(
        r#"
        SELECT id, path, provider_ids->>'RemoteEmbyItemId' AS remote_id
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND provider_ids->>'RemoteEmbyItemId' IS NOT NULL
          AND provider_ids->>'RemoteEmbyItemId' <> ''
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

    // 先两遍扫一下 stale 候选行：第一遍只算「真正会被判定 stale 的行数」用于安全阀决策，
    // 第二遍才执行物理 + DB 删除。两遍 hash 查询都是 O(N)，相比单条 DELETE 的 IO 几乎可忽略，
    // 但能确保「跨阈值就一条都不删」语义干净。
    let candidate_count = rows.len();
    let stale_count = rows
        .iter()
        .filter(|row| {
            row.remote_id
                .as_deref()
                .map(|id| !id.trim().is_empty() && !remote_id_set.contains(id))
                .unwrap_or(false)
        })
        .count();

    if candidate_count >= PRUNE_GUARD_MIN_CANDIDATES
        && stale_count * PRUNE_GUARD_RATIO_DEN > candidate_count * PRUNE_GUARD_RATIO_NUM
    {
        tracing::warn!(
            source_id = %source_id,
            candidate_count,
            stale_count,
            remote_id_set_size = remote_id_set.len(),
            ratio_threshold_pct = PRUNE_GUARD_RATIO_NUM,
            "prune 安全阀触发（比例）：本次 stale 比例超过阈值，疑似远端 ID 集合不完整（分页抖动 / 中途取消 / 进程重启等），\
             跳过本次 stale 删除，等下一次同步用更完整的 ID 集合再行判定"
        );
        return Ok(0);
    }
    // PB49：绝对量保险——比例阀对中大型库（>1 万）保护不够（9.9% 仍可能误删上千条），
    // 这里再卡一道「单次最多删 N 条」，超过就直接跳过，下次再判。
    if stale_count > PRUNE_GUARD_MAX_ABSOLUTE {
        tracing::warn!(
            source_id = %source_id,
            candidate_count,
            stale_count,
            remote_id_set_size = remote_id_set.len(),
            absolute_threshold = PRUNE_GUARD_MAX_ABSOLUTE,
            "PB49：prune 安全阀触发（绝对量）：单次 stale 数超过 {} 条上限，\
             跳过本次 stale 删除以避免大量误删；如果远端真的下架了 >{} 条，多轮同步会渐进收敛",
            PRUNE_GUARD_MAX_ABSOLUTE,
            PRUNE_GUARD_MAX_ABSOLUTE
        );
        return Ok(0);
    }

    let mut deleted = 0u64;
    for row in rows {
        let Some(ref remote_id) = row.remote_id else {
            continue;
        };
        // 兜底：SQL 已用 <> '' 过滤；这里再防御一次空串，避免误删 Series/Season。
        if remote_id.trim().is_empty() {
            continue;
        }
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
        // PB39：DeviceId 改为读 `source.spoofed_device_id`（首次 create 派生为 32 位 hex），
        // 不再用 `movie-rust-{uuid}` 这种自爆前缀。空值由 `effective_spoofed_device_id` 回落到
        // `source.id` 的 32 位 hex，仍然不带项目名前缀。
        let device_id = source.effective_spoofed_device_id();

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
        let mut used_cache = false;
        let playback_info = if let Some(cached) = get_cached_playback_info(&cache_key).await {
            used_cache = true;
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
        if status == reqwest::StatusCode::NOT_FOUND && used_cache && attempt == 0 {
            // 远端 DirectStreamUrl/TranscodingUrl 已失效（如 token rotate、媒体源 ID 变更）；
            // 清掉过期 PlaybackInfo 缓存，下一轮 attempt 会拉取 fresh PlaybackInfo 重试一次
            // （token 仍有效，无需 clear_remote_emby_source_auth_state）。
            let mut cache = PLAYBACK_INFO_CACHE.write().await;
            cache.remove(&cache_key);
            drop(cache);
            continue;
        }
        return Ok(response);
    }

    // PB35-2：所有重试用完仍未拿到可用流，记 warn 并把错误明确成「远端不可用」。
    // 之前直接抛 Unauthorized，客户端不知道是凭证还是远端宕机，这里上报更清晰的语义。
    tracing::warn!(
        source_id = %source.id,
        remote_item_id = %remote_item_id,
        "PB35-2：远端 Emby PlaybackInfo 全部 attempt 均失败，已耗尽重试"
    );
    Err(AppError::Internal(
        "远端 Emby 当前不可用（401/403 重登仍失败或所有 attempt 均超时），请稍后再试".to_string(),
    ))
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

/// 远端 Emby HTTP 拉取的最大重试次数（5xx / 网络错误 / Connection reset 等）。
/// 取 4 是因为：1 次首发 + 3 次重试，叠加退避 1s/2s/4s，最坏 7s 后放弃，
/// 既避免「797/399752 撞 502 直接失败」的雪崩，也不会让单次扫描卡死。
const REMOTE_HTTP_MAX_RETRIES: u32 = 3;

/// 重试退避基线毫秒数；实际间隔为 `BASE * 2^attempt`，attempt 从 0 起算。
const REMOTE_HTTP_BACKOFF_BASE_MS: u64 = 1000;

/// 判断状态码是否值得退避后重试。401/403 由上层 token 续登路径单独处理；
/// 其它 4xx 视为客户端错误（参数错、不存在），重试无意义直接抛错。
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::TOO_MANY_REQUESTS
            | reqwest::StatusCode::INTERNAL_SERVER_ERROR
            | reqwest::StatusCode::BAD_GATEWAY
            | reqwest::StatusCode::SERVICE_UNAVAILABLE
            | reqwest::StatusCode::GATEWAY_TIMEOUT
    ) || status.as_u16() >= 500
}

/// 判断 reqwest 网络错误是否值得重试。connect/timeout/connection reset 等暂时性错误
/// 都允许重试；只有 builder 错误等结构性问题才直接失败。
fn is_retryable_network_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request() || err.is_body()
}

async fn get_json_with_retry<T: serde::de::DeserializeOwned>(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    endpoint: &str,
    query: &[(String, String)],
) -> Result<T, AppError> {
    let client = &*crate::http_client::SHARED;
    let mut auth_retry_used = false;
    let mut last_error: Option<String> = None;

    // 重试循环：max_retries 次「网络/5xx 错误」退避重试 + 最多 1 次 401/403 续登重试。
    // 这两类重试相互独立计数：
    //   - auth_retry_used：401/403 触发的「清 token 重登」是否已用掉；
    //   - retry_count：5xx / 网络错误 触发的退避重试次数。
    let request_interval_ms = source.request_interval_ms;
    let source_id = source.id;
    for retry_count in 0..=REMOTE_HTTP_MAX_RETRIES {
        let _user_id = ensure_authenticated(pool, source, false).await?;
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
        // 拉取速率节流：实际发出请求前先按 `source.request_interval_ms` 等候足够时间，
        // 这样无论调用点是 fetch_remote_items_page_for_view（顺序循环）还是后续可能
        // 引入的并发路径，都会在网关层（per-source mutex）形成「全局最低间隔」屏障。
        throttle_remote_request(source_id, request_interval_ms).await;
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
            )
            // 远端大库（30 万+ 条）单页可达 1000 条 + 全 Fields，单次响应 body 可达数 MB；
            // 全局 SHARED 默认 30s 总超时偏紧，body 读到一半被超时砍断会抛
            // `error decoding response body`，本路径单独放宽到 120s（per-request override，
            // 不影响 TMDB / 图片 / 反代直链等其它复用 SHARED 的链路）。
            .timeout(Duration::from_secs(120));

        request = request.query(&[("api_key", token.as_str())]);

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(err) => {
                if is_retryable_network_error(&err) && retry_count < REMOTE_HTTP_MAX_RETRIES {
                    let delay_ms = REMOTE_HTTP_BACKOFF_BASE_MS << retry_count;
                    tracing::warn!(
                        endpoint,
                        attempt = retry_count + 1,
                        max_retries = REMOTE_HTTP_MAX_RETRIES,
                        delay_ms,
                        error = %err,
                        "远端 Emby 网络错误，退避后重试"
                    );
                    last_error = Some(err.to_string());
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                return Err(err.into());
            }
        };

        let status = response.status();

        // 401/403：清 token + 重登，仅允许一次（避免凭证错误时无限循环）。
        if matches!(
            status,
            reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
        ) && !auth_retry_used
        {
            auth_retry_used = true;
            repository::clear_remote_emby_source_auth_state(pool, source.id).await?;
            source.access_token = None;
            source.remote_user_id = None;
            // PB22：重登 + 清 PlaybackInfo 缓存（与 PB15 一致），下一轮 ensure_authenticated 会重新拿 token。
            invalidate_playback_info_cache_for_source(source.id).await;
            continue;
        }

        // 5xx / 429 / 408：退避后重试。
        if is_retryable_status(status) && retry_count < REMOTE_HTTP_MAX_RETRIES {
            let body_preview = response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();
            let delay_ms = REMOTE_HTTP_BACKOFF_BASE_MS << retry_count;
            tracing::warn!(
                endpoint,
                status = %status,
                attempt = retry_count + 1,
                max_retries = REMOTE_HTTP_MAX_RETRIES,
                delay_ms,
                body_preview = %body_preview,
                "远端 Emby 上游错误（5xx/429/408），退避后重试"
            );
            last_error = Some(format!("HTTP {} {}", status.as_u16(), body_preview));
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            continue;
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::Internal(format!(
                "远端 Emby 请求失败: {} {}",
                status.as_u16(),
                body
            )));
        }

        // status=200 之后的 body 读取阶段也可能瞬时失败：
        //   - 上游 / 反代 / NAT 中途掐断 keep-alive 长连接 → hyper IncompleteMessage
        //   - per-request `.timeout(120s)` 烧到 body 读阶段触发
        //   - 解压 / chunked 流自身错误
        // 这些在 reqwest 里都标记为 `is_body() == true`（Display 字面量
        // 「error decoding response body」），与 send 阶段的网络错误同属可重试类。
        // 故意放进同一个 `for retry_count` 循环里复用 `is_retryable_network_error`
        // + 退避策略，保证「拉了 11% 撞一次连接重置 → 整个同步任务直接 Failed」
        // 这条 1605s 大库灾难路径（status=200 + body decode err）也会按
        // 1s/2s/4s 退避重试，不再一击毙命。
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("-")
            .to_string();
        let bytes = match response.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => {
                if is_retryable_network_error(&err) && retry_count < REMOTE_HTTP_MAX_RETRIES {
                    let delay_ms = REMOTE_HTTP_BACKOFF_BASE_MS << retry_count;
                    tracing::warn!(
                        endpoint,
                        status = %status,
                        attempt = retry_count + 1,
                        max_retries = REMOTE_HTTP_MAX_RETRIES,
                        delay_ms,
                        content_type = %content_type,
                        error = %err,
                        "远端 Emby 响应 body 读取失败，退避后重试"
                    );
                    last_error =
                        Some(format!("body read err (status={}): {err}", status.as_u16()));
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                return Err(AppError::Internal(format!(
                    "远端响应读取失败: endpoint={endpoint}, status={}, content_type={content_type}, error={err}",
                    status.as_u16()
                )));
            }
        };
        return serde_json::from_slice::<T>(bytes.as_ref()).map_err(|error| {
            let preview = String::from_utf8_lossy(bytes.as_ref())
                .chars()
                .take(280)
                .collect::<String>();
            AppError::Internal(format!(
                "远端JSON解析失败: endpoint={endpoint}, status={}, content_type={content_type}, error={error}, body预览={preview}",
                status.as_u16()
            ))
        });
    }

    Err(AppError::Internal(format!(
        "远端 Emby 请求多次重试仍失败 endpoint={} 最近错误: {}",
        endpoint,
        last_error.unwrap_or_else(|| "未记录".to_string())
    )))
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

#[allow(dead_code)] // PB42 之后 sync 路径不再调用；保留给未来"按需刷新 playback 元数据"使用。
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

/// PB42：用分页请求里 `Fields=MediaSources,MediaStreams` 已经带回的数据，
/// 在**不发额外 HTTP**的前提下重建 `MediaAnalysisResult`。
///
/// 之前同步阶段每条目都会再发一次 `POST /Items/{id}/PlaybackInfo`：
/// - 每条 +1 次远端 RTT（典型 150-300ms）
/// - 在 32 万级别的库上累计 ~22 小时纯网络等待
/// - 带回的字段（MediaSources/MediaStreams/Chapters）几乎全部已经在分页响应里了
///
/// `BaseItemDto.media_sources` 在 Emby 标准响应里会附带完整的 `MediaSource` 结构（含
/// `MediaStreams[]` 全字段），所以**完全够构造 analysis**。仅有 `Chapters` 在 BaseItemDto 里
/// 不强制返回——但同步阶段的 `update_media_item_metadata` 不依赖 chapters，所以即使为空也
/// 不影响落库结果。真正需要 chapters 的播放路径仍旧走 `fetch_remote_playback_analysis`。
fn synthesize_analysis_from_base_item(
    item: &RemoteBaseItem,
    media_source_id: Option<&str>,
) -> Option<MediaAnalysisResult> {
    let sources = item.media_sources.as_ref()?;
    if sources.is_empty() {
        return None;
    }
    let media_source = if let Some(want) = media_source_id.filter(|s| !s.trim().is_empty()) {
        sources
            .iter()
            .find(|ms| ms.id.as_deref().map(str::trim) == Some(want.trim()))
            .or_else(|| sources.first())
    } else {
        sources.first()
    }?;
    remote_playback_source_to_analysis(item.id.as_str(), media_source)
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
        codec_type: stream.stream_type.trim().to_ascii_lowercase(),
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
    let token_changed = source.access_token.as_deref() != Some(login.access_token.as_str());
    repository::update_remote_emby_source_auth_state(
        pool,
        source.id,
        login.user.id.as_str(),
        login.access_token.as_str(),
    )
    .await?;
    source.remote_user_id = Some(login.user.id.clone());
    source.access_token = Some(login.access_token.clone());
    // PB15：force_refresh 路径或首次 token 变更后清掉本源的 PlaybackInfo 缓存，
    // 避免跨 token 复用旧直链。仅在 token 真的换了时清，避免无谓抖动。
    if token_changed {
        invalidate_playback_info_cache_for_source(source.id).await;
    }
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

/// Series 目录显示名兜底逻辑。统一供 `build_relative_strm_path`、
/// `ensure_remote_series_folder`、`ensure_remote_season_folder` 共用，
/// 防止三者因为兜底不一致而把 series sidecar 与 episode strm 拆到两个目录。
fn remote_series_display_name(item: &RemoteSyncItem) -> &str {
    item.item
        .series_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Unknown Series")
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
        let series_name = sanitize_segment(remote_series_display_name(item));
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

/// STRM：`{输出根}/{SanitizedSourceName}/`，各视图再建 `{SanitizedViewName}/`（与 [`try_strm_workspace_for_source`]、`sync_source_inner` 一致）。
/// `strm_output_path` 未配置或非空裁剪后为空 → `None`，供 watcher/扫描跳过。
pub fn try_strm_workspace_for_source(source: &DbRemoteEmbySource) -> Option<PathBuf> {
    let raw = source
        .strm_output_path
        .as_deref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())?;
    Some(Path::new(raw).join(sanitize_segment(source.name.as_str())))
}

fn strm_workspace_for_source(source: &DbRemoteEmbySource) -> Result<PathBuf, AppError> {
    try_strm_workspace_for_source(source).ok_or_else(|| {
        AppError::BadRequest(format!(
            "远端 Emby 源「{}」未配置 STRM 输出根目录，请先在编辑表单中补填后再同步",
            source.name
        ))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteViewBrief {
    id: String,
    name: String,
}

fn view_folder_segment_for_watch(source: &DbRemoteEmbySource, view_map_key: &str) -> String {
    let want = view_map_key.trim();
    let want_l = want.to_ascii_lowercase();
    if let Ok(views) = serde_json::from_value::<Vec<RemoteViewBrief>>(source.remote_views.clone()) {
        if let Some(found) = views.iter().find(|v| v.id.trim().to_ascii_lowercase() == want_l) {
            if !found.name.trim().is_empty() {
                return sanitize_segment(found.name.trim());
            }
        }
    }
    sanitize_segment(want)
}

/// Hybrid 库的 file watcher / 本地扫描需覆盖「写入 strm 与同目录侧车」的物理目录，
/// 即 `{输出根}/{源名}/{远端视图名}/`（与 `sync_source_inner` 的 `view_strm_workspace` 一致）。
///
/// `view_library_map` 若在同步后仍为完整 `view_id → library_id`，对每个映射到目标库的视图各返回一条；
/// map 为空时若 `target_library_id` 与该库相等，则递归监控 `{输出根}/{源名}/` 整棵树上所有视图子目录。
pub fn strm_watch_directories_for_sources(
    sources: &[DbRemoteEmbySource],
    library_id: Uuid,
) -> Vec<PathBuf> {
    use std::collections::HashSet;
    let mut out: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let lib_str = library_id.to_string();

    for source in sources {
        let Some(workspace) = try_strm_workspace_for_source(source) else {
            continue;
        };

        match source.view_library_map.as_object() {
            Some(map) if !map.is_empty() => {
                for (view_key, mapped) in map {
                    let Some(mapped_raw) = mapped.as_str().map(str::trim) else {
                        continue;
                    };
                    if !mapped_raw.eq_ignore_ascii_case(lib_str.as_str()) {
                        continue;
                    }
                    let sub = workspace.join(view_folder_segment_for_watch(source, view_key));
                    let canon = sub.to_string_lossy().to_string();
                    if seen.insert(canon) {
                        out.push(sub);
                    }
                }
            }
            _ => {
                if source.target_library_id != library_id {
                    continue;
                }
                let canon = workspace.to_string_lossy().to_string();
                if seen.insert(canon) {
                    out.push(workspace.clone());
                }
            }
        }
    }
    out
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
) -> Option<(&'a str, &'a [RemotePlaybackMediaStream])> {
    let sources = item.media_sources.as_ref()?;
    if let Some(want) = preferred_msid.filter(|s| !s.trim().is_empty()) {
        for ms in sources {
            if let Some(id) = ms.id.as_deref().map(str::trim) {
                if id == want.trim() && !id.is_empty() {
                    return Some((id, ms.media_streams.as_slice()));
                }
            }
        }
    }
    sources.first().and_then(|ms| {
        let id = ms.id.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
        Some((id, ms.media_streams.as_slice()))
    })
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
    // PB43：tvshow.nfo 每个 Series 目录只写一次的去重集合。DashSet 让并发任务能 lock-free
    // 检查 + insert（DashSet::insert 返回 bool 表示是否新插入，与 HashSet 语义一致）。
    tvshow_written: &DashSet<PathBuf>,
    // true = 增量「改」场景：远端在 last_sync_at 之后被修改，sidecar 需要覆盖以同步最新元数据。
    // false = 首次/恢复同步：尊重已存在文件（用户手动 Refresh / 旧版本沉淀），不覆盖。
    force_refresh: bool,
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
    // PB16：内容未变就跳过磁盘写。`stream_line` 由 `source.id + item_id + media_source_id +
    // source_secret` 决定，与远端是否变更无关；也就是说同一条目反复同步时绝大多数情况下
    // 内容会完全一致，直接 `tokio::fs::write` 等于无脑覆盖文件 mtime + 触发 IO。
    // 改为先 read → 比对 → 不一致才写，省下一次 SSD 写以及避免触发其它工具基于 mtime 的
    // 误判（比如挂在 STRM 目录上的 inotify/同步工具）。
    let new_content = format!("{}\n", stream_line.trim());
    let need_write = match tokio::fs::read(&strm_path).await {
        Ok(existing) => existing.as_slice() != new_content.as_bytes(),
        Err(_) => true,
    };
    if need_write {
        tokio::fs::write(&strm_path, new_content.as_bytes())
            .await
            .map_err(|e| AppError::Internal(format!("写入 STRM 失败: {e}")))?;
    }

    let mut local_poster: Option<PathBuf> = None;
    let mut local_backdrop: Option<PathBuf> = None;
    let mut local_logo: Option<PathBuf> = None;

    let sidecar_dir = strm_path
        .parent()
        .ok_or_else(|| AppError::Internal("STRM 缺父目录".into()))?;

    // PB42：Movie / Episode 的"封面 / 背景 / Logo"侧车文件名按 strm 路径推导，
    // 与后台 sidecar_image_download_loop 的命名规则完全一致——保证后台 worker
    // 把 jpg 落到这里之后，下次同步前台跑 sidecar_exists_nonempty 检查就能命中
    // 已下载文件，把 DB 里的远端 URL 替换成本地 path。
    let (poster_filename, backdrop_filename, logo_filename) =
        sidecar_image_filenames_for_strm(&strm_path, &item.item.item_type);

    if source.sync_metadata {
        // PB42：图片下载从前台拆出，前台只做"已下载文件检测 + 命名规划 + NFO 写入"。
        // 真正的远端图片下载交给 sidecar_image_download_loop 后台 worker 异步完成：
        //   - 主同步循环每条目省下 3 张图（~1.2 秒）+ 字幕 N 张的远端 RTT
        //   - DB 里 image_primary_path/backdrop_path/logo_path 先存远端 URL，
        //     `routes/images.rs` 已识别 http(s) 前缀做代理回源，前端立刻可见图片
        //   - worker 落盘成功后通过 update_media_item_image_path 改成本地绝对路径
        //   - 跨进程重启天然续传：worker 启动时就直接 `WHERE *_path LIKE 'http%'` 过滤
        //
        // force_refresh = true（增量「改」）：远端 ImageTag 变了，本地缓存的 jpg 已过期。
        // 直接物理删除老 jpg，让本轮 upsert 写回远端 URL，worker 后续按新 tag 重下。
        // 缺失或 force_refresh 都会把 slot 留 None，DB 自动 fallback 到 remote_urls 中的远端 URL。
        for (filename, slot) in [
            (poster_filename.as_str(), &mut local_poster),
            (backdrop_filename.as_str(), &mut local_backdrop),
            (logo_filename.as_str(), &mut local_logo),
        ] {
            let path = sidecar_dir.join(filename);
            if force_refresh {
                let _ = tokio::fs::remove_file(&path).await;
                continue;
            }
            if sidecar_exists_nonempty(&path).await {
                *slot = Some(path);
            }
        }

        // NFO 覆盖策略：
        //   - force_refresh = true（远端在水位线后修改过，增量「改」）→ 覆盖以同步最新 NFO 字段。
        //   - force_refresh = false（首次/恢复同步）→ 已存在则保留（兼容用户手动 Refresh 写入）。
        let nfo_path = strm_path.with_extension("nfo");
        let nfo_exists = sidecar_exists_nonempty(&nfo_path).await;
        if !nfo_exists || force_refresh {
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
                    let tvshow_exists = sidecar_exists_nonempty(&tvshow_path).await;
                    if !tvshow_exists || force_refresh {
                        let show_body = build_tvshow_nfo_xml_from_episode(item);
                        let _ = tokio::fs::write(&tvshow_path, show_body).await;
                    }
                }
            }
        }
    }

    if source.sync_subtitles {
        if let Some((ms_id, streams)) = media_source_row(&item.item, media_source_id) {
            // PB42：外挂字幕仍在前台下载（典型大小 KB 级，且只对带外挂字幕的条目触发，
            // 总体成本远小于图片）。如果未来确认仍是瓶颈，可以同样下沉到 worker。
            let base = normalize_server_url(&source.server_url);
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
                // 已存在的字幕在非强刷场景下保留（兼容用户手动维护的字幕）；
                // 增量「改」时覆盖以反映远端最新外挂字幕。
                if sidecar_exists_nonempty(&sub_path).await && !force_refresh {
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

/// PB42：根据 strm 物理路径 + item_type 推导侧车图片文件名。
///
/// - Movie：每部电影独占一个目录 → `poster.jpg / backdrop.jpg / logo.png`
/// - Episode：同一季所有集共享 `Season XX/` 目录 → 必须用 strm 文件名做前缀防互覆
///   （`{stem}-thumb.jpg / {stem}-fanart.jpg / {stem}-clearlogo.png`）
///
/// 该函数同时被前台 sync 路径（用于"已下载文件存在性检测"）和后台 sidecar 下载 worker 调用，
/// 只要双方使用同一份命名规则就能保证 worker 落盘后下次同步前台直接命中本地文件。
pub(crate) fn sidecar_image_filenames_for_strm(
    strm_path: &Path,
    item_type: &str,
) -> (String, String, String) {
    let is_episode = item_type.eq_ignore_ascii_case("Episode");
    let stem = strm_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("media");
    if is_episode {
        (
            format!("{stem}-thumb.jpg"),
            format!("{stem}-fanart.jpg"),
            format!("{stem}-clearlogo.png"),
        )
    } else {
        (
            "poster.jpg".to_string(),
            "backdrop.jpg".to_string(),
            "logo.png".to_string(),
        )
    }
}

/// 定期强制刷新远端 Emby 登录令牌，确保代理鉴权不过期。
/// STRM 文件内存的是本地代理 URL（不含远端 token），无需重写文件内容。
async fn refresh_single_remote_token(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
) -> Result<(), AppError> {
    ensure_authenticated(pool, source, true).await?;
    repository::update_remote_emby_source_last_token_refresh(pool, source.id).await?;
    // PB15：token 刷新成功后清掉这个源的 PlaybackInfo 缓存，避免下游继续命中带旧
    // api_key 的直链导致 401/403 浪费一次重试。
    invalidate_playback_info_cache_for_source(source.id).await;
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

/// PB42：单次扫描的批量大小。每轮捞 200 条「path 列仍是远端 URL」的条目下载，
/// 跑完一轮就 sleep 一会再捞下一批，让其他 SQL 流量有空隙。
const SIDECAR_DOWNLOAD_BATCH_SIZE: i64 = 200;
/// PB42：worker 池并发度。受限于：
/// - 远端 Emby 单源 QPS（`request_interval_ms` 节流叠加）
/// - 本地磁盘随机写带宽
/// - sqlx 连接池容量
/// 4 是相对保守的折中，留余量给同步主流程。
const SIDECAR_DOWNLOAD_CONCURRENCY: usize = 4;
/// PB42：批次完成后两次扫描之间的间隔。`Duration::from_secs(15)` 让"刚同步进来 → 几十秒
/// 内开始下图"的体感不超过半分钟，又不会在没有任务时空转浪费 SQL。
const SIDECAR_DOWNLOAD_IDLE_INTERVAL: Duration = Duration::from_secs(15);

/// PB42：sidecar 图片下载后台 worker。
///
/// 设计要点：
/// 1. **DB 即队列，无需额外表**：SELECT 出"image_primary_path/backdrop_path/logo_path 仍指向
///    http(s)://" 的 media_items 行——这是同步主流程刚写入的"占位 URL"。任何下载完成后
///    都会被 `update_media_item_image_path` 替换为本地绝对路径，下次扫描自然就过滤掉了。
/// 2. **跨进程崩溃安全**：DB 已经持久化"待下载状态"（URL 字段），worker 重启就直接续上。
///    不需要内存任务队列。
/// 3. **失败自愈**：单条下载失败不写 DB（保留 URL），下一轮自动重试。
/// 4. **远端 token 自动跟随**：`emby_download_bytes` 走的就是 source 当前 access_token；
///    token 刷新由 `remote_emby_token_refresh_loop` 负责，worker 这边自动透传新 token。
/// 5. **温和限速**：`SIDECAR_DOWNLOAD_CONCURRENCY=4` + per-source `request_interval_ms`
///    让远端 Emby 不会被瞬间打爆；如果同步主循环也在跑，主循环优先（共享 throttle slot）。
pub async fn remote_emby_sidecar_download_loop(pool: sqlx::PgPool) {
    loop {
        let processed = match run_remote_emby_sidecar_download_pass(&pool).await {
            Ok(n) => n,
            Err(error) => {
                tracing::warn!(error = %error, "PB42 sidecar 图片下载 worker 批次失败");
                0
            }
        };
        if processed == 0 {
            tokio::time::sleep(SIDECAR_DOWNLOAD_IDLE_INTERVAL).await;
        } else {
            // 还有任务时只 sleep 短暂时间避免空转 / 让 DB CPU 喘口气。
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
}

/// 单轮扫描 + 并发下载。返回本轮处理的 item 数；为 0 时 worker 进入长 sleep。
async fn run_remote_emby_sidecar_download_pass(pool: &sqlx::PgPool) -> Result<usize, AppError> {
    let pending = repository::find_pending_remote_image_downloads(
        pool,
        SIDECAR_DOWNLOAD_BATCH_SIZE,
    )
    .await?;
    if pending.is_empty() {
        return Ok(0);
    }
    let total = pending.len();
    tracing::debug!(
        pending_count = total,
        "PB42：开始下载远端 sidecar 图片（poster/backdrop/logo）"
    );

    // PB42：source_id → DbRemoteEmbySource 缓存，避免每个 item 都查一次 sources 表。
    // 用 Mutex 包 HashMap，让 buffer_unordered 任务并发读写。
    let source_cache: Arc<tokio::sync::Mutex<HashMap<Uuid, Arc<DbRemoteEmbySource>>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    let pool_arc = pool.clone();
    use futures::stream::{self, StreamExt};
    let download_results = stream::iter(pending.into_iter().map(|task| {
        let pool = pool_arc.clone();
        let cache = Arc::clone(&source_cache);
        async move { process_one_pending_image(&pool, task, cache).await }
    }))
    .buffer_unordered(SIDECAR_DOWNLOAD_CONCURRENCY)
    .collect::<Vec<_>>()
    .await;

    let success = download_results
        .iter()
        .filter(|r| matches!(r, Ok(true)))
        .count();
    tracing::info!(
        total = total,
        success = success,
        "PB42：sidecar 图片下载批次完成"
    );
    Ok(total)
}

/// 单条任务处理：尝试下载该 item 的 1-3 个仍是远端 URL 的图片字段。
///
/// 返回 Ok(true) 表示至少有一张图成功落盘，Ok(false) 表示都失败 / 都跳过，Err 表示底层错误。
/// 任意一张图失败都不会让别的张失败：每张图独立 try。
async fn process_one_pending_image(
    pool: &sqlx::PgPool,
    task: repository::PendingRemoteImageDownload,
    source_cache: Arc<tokio::sync::Mutex<HashMap<Uuid, Arc<DbRemoteEmbySource>>>>,
) -> Result<bool, AppError> {
    // 解析 sidecar 目录与 3 个目标文件名。
    let strm_path = std::path::Path::new(task.item_path.as_str());
    let Some(sidecar_dir) = strm_path.parent() else {
        tracing::debug!(
            item_id = %task.item_id,
            path = %task.item_path,
            "PB42：item path 无父目录，跳过 sidecar 下载"
        );
        return Ok(false);
    };
    let (poster_filename, backdrop_filename, logo_filename) =
        sidecar_image_filenames_for_strm(strm_path, task.item_type.as_str());

    // 取出（或加载）source。失败说明源被删，直接跳过——下次 scan 这条 item 也会被
    // CASCADE 清掉，不会无限堆积。
    let source = {
        let mut cache = source_cache.lock().await;
        if let Some(s) = cache.get(&task.source_id) {
            Arc::clone(s)
        } else {
            let Some(loaded) = repository::get_remote_emby_source(pool, task.source_id).await?
            else {
                tracing::debug!(
                    source_id = %task.source_id,
                    "PB42：远端源不存在或已删除，sidecar 任务忽略"
                );
                return Ok(false);
            };
            if !loaded.enabled {
                tracing::debug!(
                    source_id = %task.source_id,
                    "PB42：远端源已禁用，sidecar 任务忽略"
                );
                return Ok(false);
            }
            let arc = Arc::new(loaded);
            cache.insert(task.source_id, Arc::clone(&arc));
            arc
        }
    };
    let token = match source.access_token.as_deref() {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            tracing::debug!(
                source_id = %task.source_id,
                "PB42：远端源缺 access_token，跳过本轮 sidecar 下载（等下次 token 刷新）"
            );
            return Ok(false);
        }
    };

    // 确保 sidecar 目录存在（增量同步早就创过；防御性 mkdir）
    if let Err(error) = tokio::fs::create_dir_all(sidecar_dir).await {
        tracing::warn!(
            sidecar_dir = %sidecar_dir.to_string_lossy(),
            error = %error,
            "PB42：sidecar 目录创建失败，跳过本条"
        );
        return Ok(false);
    }

    let mut any_success = false;

    // 三种类型走完全相同的"下载→落盘→UPDATE DB"循环。
    let plans: [(&str, &Option<String>, &str); 3] = [
        ("primary", &task.remote_primary_url, poster_filename.as_str()),
        ("backdrop", &task.remote_backdrop_url, backdrop_filename.as_str()),
        ("logo", &task.remote_logo_url, logo_filename.as_str()),
    ];
    for (image_type, url_opt, filename) in plans {
        let Some(url) = url_opt.as_deref().filter(|u| !u.trim().is_empty()) else {
            continue;
        };
        let dest = sidecar_dir.join(filename);
        // 已经落过盘但 DB 还没追上（比如崩溃在 write 和 update 之间）：也走 UPDATE 一次。
        let already_local = sidecar_exists_nonempty(&dest).await;
        let bytes_result = if already_local {
            Ok(Vec::<u8>::new())
        } else {
            // 远端 URL 在 sync 阶段已带过 api_key 参数（来自 extract_remote_image_urls_full
            // 拼出来的纯 URL）— 但为了 token 刷新后旧 URL 仍能续命，这里也再追加一次最新 token。
            let url_with_token = append_remote_api_key_param(url, token);
            emby_download_bytes(source.as_ref(), token, url_with_token.as_str()).await
        };
        match bytes_result {
            Ok(bytes) => {
                // 已存在场景：bytes 是空 Vec，跳过写盘；否则写一份原子的 .tmp 再 rename
                if !already_local {
                    if bytes.is_empty() {
                        tracing::debug!(
                            item_id = %task.item_id,
                            image_type,
                            "PB42：远端返回 0 字节，跳过"
                        );
                        continue;
                    }
                    let tmp = dest.with_extension("tmp");
                    if let Err(error) = tokio::fs::write(&tmp, &bytes).await {
                        tracing::warn!(
                            item_id = %task.item_id,
                            image_type,
                            error = %error,
                            "PB42：sidecar 落盘失败"
                        );
                        continue;
                    }
                    if let Err(error) = tokio::fs::rename(&tmp, &dest).await {
                        let _ = tokio::fs::remove_file(&tmp).await;
                        tracing::warn!(
                            item_id = %task.item_id,
                            image_type,
                            error = %error,
                            "PB42：sidecar 重命名失败"
                        );
                        continue;
                    }
                }
                let local_path_str = dest.to_string_lossy().to_string();
                if let Err(error) = repository::update_media_item_image_path(
                    pool,
                    task.item_id,
                    image_type,
                    Some(local_path_str.as_str()),
                    None,
                )
                .await
                {
                    tracing::warn!(
                        item_id = %task.item_id,
                        image_type,
                        error = %error,
                        "PB42：UPDATE media_items 图片路径失败"
                    );
                    continue;
                }
                any_success = true;
            }
            Err(error) => {
                tracing::debug!(
                    item_id = %task.item_id,
                    image_type,
                    error = %error,
                    url,
                    "PB42：远端 sidecar 图片下载失败（保留 URL，下轮重试）"
                );
            }
        }
    }
    Ok(any_success)
}

/// 远端 Emby 源「定时增量同步」循环：每 60 秒检查一次每个源，
/// 当 `auto_sync_interval_minutes > 0` 且 `now() >= last_sync_at + interval` 时，
/// 自动触发该源的增量同步（增 / 改 / 删，由 `sync_source_with_progress` 内部统一处理）。
///
/// 与 `remote_library_monitor_loop` 的差异：
/// - 监控循环依赖 library `EnableRealtimeMonitor` 选项 + 5 分钟硬编码间隔；
/// - 本循环按 **源粒度** 配置间隔，独立于 library 监控开关。
pub async fn remote_emby_auto_sync_loop(state: crate::state::AppState) {
    let mut ticker = tokio::time::interval(Duration::from_secs(60));
    ticker.tick().await; // 跳过启动瞬间立即触发
    loop {
        ticker.tick().await;
        if let Err(err) = run_remote_emby_auto_sync_pass(&state).await {
            tracing::warn!(error = %err, "远端 Emby 自动增量同步轮询失败");
        }
    }
}

/// 防止 auto_sync 自身在同一个源上并发触发（用户手动按钮 + auto loop 的并发由
/// `sync_source_with_progress` 顶层去重保证：同一时间一个源最多一个 spawn 任务）。
fn auto_sync_in_flight() -> &'static tokio::sync::Mutex<std::collections::HashSet<Uuid>> {
    static SET: OnceLock<tokio::sync::Mutex<std::collections::HashSet<Uuid>>> = OnceLock::new();
    SET.get_or_init(|| tokio::sync::Mutex::new(std::collections::HashSet::new()))
}

async fn run_remote_emby_auto_sync_pass(
    state: &crate::state::AppState,
) -> Result<(), AppError> {
    let sources = repository::list_remote_emby_sources(&state.pool).await?;
    let now = Utc::now();
    for source in sources {
        if !source.enabled {
            continue;
        }
        let interval_min = source.auto_sync_interval_minutes;
        if interval_min <= 0 {
            continue;
        }
        // 没有 last_sync_at 时使用 created_at 作为基准，确保新源也会按周期触发首次同步。
        let baseline = source.last_sync_at.unwrap_or(source.created_at);
        let elapsed_min = now
            .signed_duration_since(baseline)
            .num_minutes()
            .max(0);
        if elapsed_min < i64::from(interval_min) {
            continue;
        }
        // 抢占去重锁：本轮已经在跑同一个源就跳过；正常退出时移除。
        {
            let mut guard = auto_sync_in_flight().lock().await;
            if !guard.insert(source.id) {
                continue;
            }
        }
        tracing::info!(
            source_id = %source.id,
            source_name = %source.name,
            interval_min,
            elapsed_min,
            "远端 Emby 自动增量同步：触发"
        );
        match sync_source_with_progress(state, source.id, None).await {
            Ok(result) => {
                tracing::info!(
                    source_id = %source.id,
                    written = result.written_files,
                    "远端 Emby 自动增量同步：完成"
                );
            }
            Err(err) => {
                tracing::warn!(
                    source_id = %source.id,
                    error = %err,
                    "远端 Emby 自动增量同步：失败"
                );
            }
        }
        auto_sync_in_flight().lock().await.remove(&source.id);
    }
    Ok(())
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
        .and_then(|source| source.id.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// PB39：从 source 取出"已伪装的"四元组（Client / Device / DeviceId / Version），
/// 让远端 Devices 表里这一行不再带 `MovieRustTransit / MovieRustProxy / movie-rust-{uuid}`
/// 等自爆字符串。所有调用点（Static URL、PlaybackInfo、items 列表拉取、登录预览）
/// 统一走这里，确保**同一个 source 永远展示同一台"设备"**，符合真人客户端长期使用的画像。
fn emby_auth_header(source: &DbRemoteEmbySource, token: Option<&str>) -> String {
    emby_auth_header_for_identity(
        source.effective_spoofed_client(),
        source.effective_spoofed_device_name(),
        source.effective_spoofed_device_id().as_str(),
        source.effective_spoofed_app_version(),
        token,
    )
}

/// 不带 source 上下文（preview / 一次性预登录场景）时的伪装头构造。
/// 调用方负责传入随机 device_id 并选好 client / device 名。
fn emby_auth_header_for_device(device_id: &str, token: Option<&str>) -> String {
    emby_auth_header_for_identity("Infuse-Direct", "Apple TV", device_id, "8.2.4", token)
}

fn emby_auth_header_for_identity(
    client: &str,
    device: &str,
    device_id: &str,
    version: &str,
    token: Option<&str>,
) -> String {
    if let Some(token) = token.filter(|value| !value.trim().is_empty()) {
        format!(
            "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\", Token=\"{}\"",
            client.trim(),
            device.trim(),
            device_id.trim(),
            version.trim(),
            token.trim()
        )
    } else {
        format!(
            "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\"",
            client.trim(),
            device.trim(),
            device_id.trim(),
            version.trim()
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

/// PB40：管理员诊断接口的支撑函数。直接命中 `/Users/{uid}/Items?ParentId=...&Fields=<fields>`
/// 并把远端 **原始 JSON 响应** 透传回来——让前端 / 用户能在浏览器里实地核对：
/// - 远端到底返回了哪些字段？（ImageTags / BackdropImageTags 是否覆盖 7 类图？）
/// - 我们 sync 时丢了哪些（之前只取 Primary + 第一张 Backdrop）？
/// - 是否还有 TMDB 才能补的缺口？
///
/// 返回 `serde_json::Value`：保留远端 PascalCase / 嵌套结构原状。
pub async fn diagnostic_fetch_sample_items(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    parent_id: Option<&str>,
    fields: Option<&str>,
    limit: i64,
) -> Result<serde_json::Value, AppError> {
    let mut source = repository::get_remote_emby_source(pool, source_id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))?;
    let user_id = ensure_authenticated(pool, &mut source, false).await?;
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    // 默认请求所有可能影响 sync 的字段；用户也可显式覆盖
    let default_fields = "SeriesName,SeasonName,ProductionYear,ParentIndexNumber,IndexNumber,Overview,\
        OfficialRating,CommunityRating,CriticRating,PremiereDate,RunTimeTicks,\
        ProviderIds,Genres,Studios,Tags,MediaSources,MediaStreams,\
        ImageTags,BackdropImageTags,SeriesId,SeasonId,\
        SeriesPrimaryImageTag,ParentBackdropImageTags,ParentBackdropItemId,\
        ParentLogoItemId,ParentLogoImageTag,ParentThumbItemId,ParentThumbImageTag,\
        ParentArtItemId,ParentArtImageTag,Status,EndDate,People,\
        OriginalTitle,SortName,Taglines,ProductionLocations,\
        AirDays,AirTime,RemoteTrailers,DateCreated,Path,Container,\
        UserData,DisplayPreferencesId,Studios,GenreItems,TagItems,Chapters";
    let mut query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("IncludeItemTypes".to_string(), "Movie,Episode,Series".to_string()),
        ("Fields".to_string(), fields.unwrap_or(default_fields).to_string()),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("EnableImageTypes".to_string(), "Primary,Backdrop,Logo,Thumb,Banner,Art,Disc,Box,BoxRear,Menu".to_string()),
        ("ImageTypeLimit".to_string(), "10".to_string()),
        ("StartIndex".to_string(), "0".to_string()),
        ("Limit".to_string(), limit.clamp(1, 50).to_string()),
    ];
    if let Some(pid) = parent_id.filter(|p| !p.trim().is_empty()) {
        query.push(("ParentId".to_string(), pid.trim().to_string()));
    }
    get_json_with_retry::<serde_json::Value>(pool, &mut source, &endpoint, &query).await
}
