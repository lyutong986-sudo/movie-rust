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

/// per-source 最大并发 HTTP 请求信号量。
/// 防止瞬间大量并发请求触发远端服务器的限流/WAF 保护。
/// 所有对远端 Emby 的 HTTP 请求（JSON API + 图片下载）都必须先获取 permit。
const REMOTE_HTTP_MAX_CONCURRENT_PER_SOURCE: usize = 2;
static REMOTE_HTTP_CONCURRENCY: std::sync::LazyLock<
    RwLock<HashMap<Uuid, Arc<tokio::sync::Semaphore>>>,
> = std::sync::LazyLock::new(|| RwLock::new(HashMap::new()));

/// 获取给定 source 的 HTTP 并发许可。限制同一时间对同一远端最多 N 个在途请求。
async fn acquire_http_permit(source_id: Uuid) -> tokio::sync::OwnedSemaphorePermit {
    let sem = {
        let read = REMOTE_HTTP_CONCURRENCY.read().await;
        read.get(&source_id).cloned()
    };
    let sem = if let Some(s) = sem {
        s
    } else {
        let mut write = REMOTE_HTTP_CONCURRENCY.write().await;
        write
            .entry(source_id)
            .or_insert_with(|| Arc::new(tokio::sync::Semaphore::new(REMOTE_HTTP_MAX_CONCURRENT_PER_SOURCE)))
            .clone()
    };
    sem.acquire_owned().await.expect("HTTP concurrency semaphore closed")
}

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
/// 在层级删除检测里把这条父行 cascade-delete 掉，
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
    pub written_files: u64,
    pub progress: f64,
    #[serde(default)]
    pub skipped_existing: u64,
    #[serde(default)]
    pub strm_missing_reprocessed: u64,
    /// 层级同步：当前正在处理的 Series 名称
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub current_series: String,
    /// 层级同步：因 RecursiveItemCount 未变化而跳过的 Series 数量
    #[serde(default)]
    pub skipped_unchanged_series: u64,
    /// 层级同步：因 ChildCount 未变化而跳过的 Season 数量
    #[serde(default)]
    pub skipped_unchanged_seasons: u64,
    /// 层级同步：已处理的 Series 数量
    #[serde(default)]
    pub processed_series: u64,
    /// 层级同步：Series 总数
    #[serde(default)]
    pub total_series: u64,
}

// PB49 (D1)：snapshot 用 std::sync::Mutex 而非 tokio::sync::RwLock。
//
// 旧实现里所有 set_* 方法都是「try_write 拿到就写、拿不到就 tokio::spawn 一个 task
// 异步去写」的模式——这个模式在主循环高频写（每条 episode 一次 set_streaming_progress）、
// 同时 poller 每秒 read 一次的场景下，会出现：
//   1. try_write 失败的瞬时争用 → spawn 一个永久存活的 task 占内核 stack（>=2KB）；
//   2. spawn 出去的写按 tokio scheduler 顺序执行，可能被晚到的 set 覆盖（写顺序错乱）；
//   3. 每次 set_* 都要做一次 Arc::clone snapshot 准备「可能 spawn」用——分配冗余。
//
// snapshot 数据本身是几个 u64 + 一个短 String，临界区 < 1us。换 std::sync::Mutex
// 的 lock() 是同步的、never-await，写入立刻完成，没有 spawn、没有竞态、没有顺序错乱。
// 在 tokio 异步上下文里同步锁 < 1us 完全可以接受（不会触发 work-stealing 失衡）。
#[derive(Clone, Default)]
pub struct RemoteSyncProgress {
    snapshot: Arc<std::sync::Mutex<RemoteSyncProgressSnapshot>>,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
}

impl RemoteSyncProgress {
    pub fn new() -> Self {
        Self::default()
    }

    /// `async` 签名保留是为了向后兼容调用点（poller 用 `.await`）；
    /// 实现已不再需要 await，但改签名会触发太多改动，得不偿失。
    pub async fn snapshot(&self) -> RemoteSyncProgressSnapshot {
        self.snapshot.lock().expect("RemoteSyncProgress mutex poisoned").clone()
    }

    fn with_snapshot<F: FnOnce(&mut RemoteSyncProgressSnapshot)>(&self, f: F) {
        // mutex poisoned 只可能在 panic-while-holding-lock 时发生；这里 panic 比静默
        // 跳过 UI 更新好——poison 意味着已有更深层 bug，让它表面化。
        let mut guard = self.snapshot.lock().expect("RemoteSyncProgress mutex poisoned");
        f(&mut guard);
    }

    pub fn set_phase(&self, phase: impl Into<String>, progress: f64) {
        let value = phase.into();
        let progress = progress.clamp(0.0, 100.0);
        self.with_snapshot(|guard| {
            guard.phase = value;
            guard.progress = progress;
        });
    }

    #[allow(dead_code)]
    pub fn set_fetch_progress(&self, fetched_items: u64, total_items: u64) {
        self.with_snapshot(|guard| {
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
        self.with_snapshot(|guard| {
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
    #[allow(dead_code)]
    pub fn set_fetching_index_progress(
        &self,
        scanned_ids: u64,
        view_index: usize,
        view_count: usize,
    ) {
        // 每个 view 各占 `1.0 / view_count` 个百分点，平均铺到 [4.0, 5.0)。即使 view
        // 个数 = 0（理论不会）或 page.total_record_count 为 0 也不会除零。
        let view_count_safe = view_count.max(1) as f64;
        let view_idx_clamped = (view_index as f64).min(view_count_safe);
        let progress = (4.0 + (view_idx_clamped / view_count_safe).min(1.0)).clamp(4.0, 5.0);
        self.with_snapshot(|guard| {
            guard.phase = "FetchingRemoteIndex".to_string();
            guard.fetched_items = scanned_ids;
            guard.progress = progress;
        });
    }

    /// PB49 (C3)：把 PB49 跳过 / 自愈计数推送到前端 snapshot。
    pub fn set_skipped_counters(&self, skipped_existing: u64, strm_missing_reprocessed: u64) {
        self.with_snapshot(|guard| {
            guard.skipped_existing = skipped_existing;
            guard.strm_missing_reprocessed = strm_missing_reprocessed;
        });
    }

    /// 层级同步：设置当前正在处理的 Series 及其进度
    pub fn set_series_progress(
        &self,
        current_series: &str,
        processed_series: u64,
        total_series: u64,
        skipped_unchanged: u64,
        fetched_items: u64,
        written_files: u64,
    ) {
        self.with_snapshot(|guard| {
            guard.phase = "SyncingRemoteItems".to_string();
            guard.current_series = current_series.to_string();
            guard.processed_series = processed_series;
            guard.total_series = total_series;
            guard.skipped_unchanged_series = skipped_unchanged;
            guard.fetched_items = fetched_items;
            guard.written_files = written_files;
            guard.total_items = total_series;
            let ratio = if total_series == 0 {
                1.0
            } else {
                processed_series as f64 / total_series as f64
            };
            guard.progress = (10.0 + ratio * 85.0).clamp(10.0, 95.0);
        });
    }

    /// 层级同步：设置电影库同步进度
    pub fn set_movie_sync_progress(
        &self,
        fetched_items: u64,
        written_files: u64,
        total_items: u64,
    ) {
        self.with_snapshot(|guard| {
            guard.phase = "SyncingMovies".to_string();
            guard.fetched_items = fetched_items;
            guard.written_files = written_files;
            guard.total_items = total_items;
            let ratio = if total_items == 0 {
                1.0
            } else {
                fetched_items as f64 / total_items as f64
            };
            guard.progress = (10.0 + ratio * 85.0).clamp(10.0, 95.0);
        });
    }

    pub fn set_streaming_progress(&self, fetched_items: u64, written_files: u64, total_items: u64) {
        self.with_snapshot(|guard| {
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
        let scan_percent = scan.percent.clamp(0.0, 96.0);
        let progress = (70.0 + (scan_percent / 96.0) * 30.0).clamp(70.0, 99.5);
        let phase = if scan.phase.is_empty() {
            "ScanningLibrary".to_string()
        } else {
            format!("ScanningLibrary/{}", scan.phase)
        };
        self.with_snapshot(|guard| {
            guard.phase = phase;
            guard.progress = progress;
        });
    }

    pub fn request_cancel(&self) {
        self.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn mark_completed(&self) {
        self.with_snapshot(|guard| {
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
    #[serde(default)]
    etag: Option<String>,
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

    // PB49 (Cap)：全局远端 sync 并发上限。
    //
    // per-source mutex 已经保证「同源不并发」，本 semaphore 进一步限制「跨源
    // 并发不超过 N」。多源用户（>= 5 个 source）配「自动增量同步间隔」时，
    // cron 容易在同一分钟触发所有源——每源主循环 8 + detail 4 = 12 个 PG 连接，
    // 5 源同时跑就要 60 个连接，加上日常 API 流量直接打爆默认 100 的 PG 池。
    //
    // 在这里 await 等待 permit 就好——per-source mutex 已经握在手里，重复
    // 触发会被前面的 try_lock 立即拒掉，不会堆积排队任务；前端 UI 通过
    // `WaitingForGlobalSlot` phase 看到「队列中」状态。
    //
    // 等待期间也响应 cancel：每秒醒一次检查 progress.is_cancelled。
    if let Some(handle) = &progress {
        handle.set_phase("WaitingForGlobalSlot", 0.5);
    }
    let global_sem = state.remote_sync_global_semaphore.clone();
    let _global_permit = {
        let mut acquire_fut = Box::pin(global_sem.acquire_owned());
        loop {
            tokio::select! {
                permit = &mut acquire_fut => {
                    break permit.map_err(|_| AppError::Internal(
                        "全局远端 sync semaphore 已关闭".to_string()
                    ))?;
                }
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    if let Some(handle) = &progress {
                        if handle.is_cancelled() {
                            return Err(AppError::BadRequest(
                                "同步任务在等待全局并发槽时被取消".to_string()
                            ));
                        }
                    }
                }
            }
        }
    };

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
        // PB49 (B4)：写 last_sync_at / last_sync_error 失败是「DB 元数据状态污染」级问题——
        // 下一次定时扫描会读到错的 last_sync_at 决定是「全量 / 增量」，写不进去意味着整条
        // sync 决策链已经失真。从 warn 升为 error，并且把当时是「成功 / 失败」也带上，
        // 让 Loki / 文件日志告警里能直接看到根因。
        let outcome = if result.is_ok() { "ok" } else { "err" };
        tracing::error!(
            source_id = %source.id,
            outcome,
            error = %error,
            "更新远端 Emby 同步状态失败 —— last_sync_at / last_sync_error 没能写进 DB，下次同步策略可能基于过时数据"
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
            if target_library_opt.is_none() {
                return Err(AppError::BadRequest(
                    format!("远端库「{}」未指定目标本地库且全局目标库不存在", view.name),
                ));
            }
            source.target_library_id
        } else {
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
    let updated_map: serde_json::Map<String, Value> = view_library_id_map
        .iter()
        .map(|(k, v)| (k.clone(), Value::String(v.to_string())))
        .collect();
    let updated_map_value = Value::Object(updated_map);
    source.view_library_map = updated_map_value.clone();
    repository::update_source_view_library_map(&state.pool, source.id, &updated_map_value)
        .await?;

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

    let is_incremental = source.last_sync_at.is_some();
    let force_refresh_sidecar = is_incremental;

    let playback_token = source
        .access_token
        .as_ref()
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| AppError::Internal("远端令牌为空，无法写入 STRM/同步条目".into()))?
        .clone();

    let strm_workspace = strm_workspace_for_source(source)?;
    tokio::fs::create_dir_all(&strm_workspace)
        .await
        .map_err(|e| AppError::Internal(format!("创建 STRM 工作区失败: {e}")))?;

    // 清理不在当前 view 列表的死 cursor
    {
        let live_view_ids: Vec<String> = views.iter().map(|v| v.id.clone()).collect();
        match repository::prune_source_view_progress_not_in(
            &state.pool,
            source.id,
            &live_view_ids,
        )
        .await
        {
            Ok(pruned) if pruned > 0 => {
                tracing::info!(source_id = %source.id, pruned, "清理已不存在 view 的续抓游标");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(source_id = %source.id, error = %error, "清理孤儿续抓游标失败");
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // 层级同步核心：按 Emby 标准 API 分层拉取
    //   tvshows 库: Series -> Seasons -> Episodes
    //   movies  库: Movie（直接拉取）
    //
    // 增量检测：
    //   - Series.RecursiveItemCount 变化 -> 该剧有增/删集
    //   - Season.ChildCount 变化 -> 该季有增/删集
    //   - Series 列表变化 -> 有新增/删除的剧
    //   - Movie TotalRecordCount 变化 -> 有新增/删除的电影
    // ═══════════════════════════════════════════════════════════════

    let tvshow_roots_written: Arc<DashSet<PathBuf>> = Arc::new(DashSet::new());
    let series_parent_map: Arc<DashMap<String, Uuid>> = Arc::new(DashMap::new());
    let season_parent_map: Arc<DashMap<String, Uuid>> = Arc::new(DashMap::new());

    // 预热已有的 Series/Season 缓存
    match repository::preheat_series_for_source(&state.pool, source.id).await {
        Ok(rows) => {
            for (view_id, name, db_id) in &rows {
                let key = format!("{view_id}::{}", sanitize_segment(name));
                series_parent_map.insert(key, *db_id);
            }
            if !rows.is_empty() {
                tracing::info!(source_id = %source.id, count = rows.len(), "预热 series_parent_map");
            }
        }
        Err(error) => {
            tracing::warn!(source_id = %source.id, error = %error, "预热 series_parent_map 失败");
        }
    }
    match repository::preheat_seasons_for_source(&state.pool, source.id).await {
        Ok(rows) => {
            for (series_db_id, season_number, db_id) in &rows {
                let key = format!("{series_db_id}::{season_number}");
                season_parent_map.insert(key, *db_id);
            }
            if !rows.is_empty() {
                tracing::info!(source_id = %source.id, count = rows.len(), "预热 season_parent_map");
            }
        }
        Err(error) => {
            tracing::warn!(source_id = %source.id, error = %error, "预热 season_parent_map 失败");
        }
    }

    let series_detail_synced: Arc<DashSet<String>> = Arc::new(DashSet::new());
    match repository::preheat_series_detail_synced(&state.pool, source.id).await {
        Ok(ids) => {
            for id in &ids {
                series_detail_synced.insert(id.clone());
            }
            if !ids.is_empty() {
                tracing::info!(source_id = %source.id, count = ids.len(), "预热 series_detail_synced");
            }
        }
        Err(error) => {
            tracing::warn!(source_id = %source.id, error = %error, "预热 series_detail_synced 失败");
        }
    }

    // 全量时跳过已入库条目加速
    let local_synced_ids: Arc<DashMap<String, String>> = if force_refresh_sidecar {
        Arc::new(DashMap::new())
    } else {
        let rows: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
            r#"
            SELECT
                provider_ids->>'RemoteEmbyItemId' AS remote_id,
                path
            FROM media_items
            WHERE provider_ids->>'RemoteEmbySourceId' = $1
              AND provider_ids->>'RemoteEmbyItemId' IS NOT NULL
              AND provider_ids->>'RemoteEmbyItemId' <> ''
            "#,
        )
        .bind(source.id.to_string())
        .fetch_all(&state.pool)
        .await?;
        let map: DashMap<String, String> = DashMap::with_capacity(rows.len());
        for (remote_id, path) in rows {
            if let Some(id) = remote_id {
                map.insert(id, path.unwrap_or_default());
            }
        }
        if !map.is_empty() {
            tracing::info!(
                source_id = %source.id,
                local_synced = map.len(),
                "全量同步启用「跳过已入库」加速"
            );
        }
        Arc::new(map)
    };

    let skipped_existing = Arc::new(AtomicU64::new(0));
    let strm_missing_reprocessed = Arc::new(AtomicU64::new(0));
    let fetched_count = Arc::new(AtomicU64::new(0));
    let written_files = Arc::new(AtomicU64::new(0));
    let skipped_unchanged_series = Arc::new(AtomicU64::new(0));
    let skipped_unchanged_seasons = Arc::new(AtomicU64::new(0));
    let skipped_unchanged_movie_views = Arc::new(AtomicU64::new(0));
    let source_arc: Arc<DbRemoteEmbySource> = Arc::new(source.clone());

    // 增量模式：批量预加载 Series/Season 的 remote counts（消除 N+1 查询）
    let series_counts_cache: HashMap<String, (Option<i64>, Option<i32>)> = if is_incremental {
        match batch_load_series_remote_counts(&state.pool, source.id).await {
            Ok(map) => {
                if !map.is_empty() {
                    tracing::info!(source_id = %source.id, count = map.len(), "批量预加载 Series remote counts");
                }
                map
            }
            Err(e) => {
                tracing::warn!(source_id = %source.id, error = %e, "批量预加载 Series counts 失败，降级为逐条查询");
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };
    let season_counts_cache: HashMap<String, i32> = if is_incremental {
        match batch_load_season_remote_child_counts(&state.pool, source.id).await {
            Ok(map) => {
                if !map.is_empty() {
                    tracing::info!(source_id = %source.id, count = map.len(), "批量预加载 Season remote child counts");
                }
                map
            }
            Err(e) => {
                tracing::warn!(source_id = %source.id, error = %e, "批量预加载 Season counts 失败，降级为逐条查询");
                HashMap::new()
            }
        }
    } else {
        HashMap::new()
    };

    let series_detail_semaphore = Arc::new(tokio::sync::Semaphore::new(SERIES_DETAIL_CONCURRENCY));
    let series_detail_handles: Arc<tokio::sync::Mutex<Vec<tokio::task::JoinHandle<()>>>> =
        Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let mut detail_handles_guard =
        DetailHandlesGuard::new(Arc::clone(&series_detail_handles), progress.clone());

    // 收集所有远端 Series/Movie ID 集合（用于删除检测——仅与 Series/Movie 层级比对，
    // 不再全量拉 Episode ID）。仅在 enable_auto_delete=true 时才收集和执行删除。
    let enable_auto_delete = source.enable_auto_delete;
    let mut remote_series_ids_by_view: HashMap<String, HashSet<String>> = HashMap::new();
    let mut remote_movie_ids_by_view: HashMap<String, HashSet<String>> = HashMap::new();
    let mut total_deleted = 0u64;

    if let Some(handle) = &progress {
        handle.set_phase("SyncingRemoteItems", 5.0);
    }

    let page_size = effective_page_size(source);

    for view in &views {
        if let Some(handle) = &progress {
            if handle.is_cancelled() {
                return Err(AppError::BadRequest("同步任务已被取消".to_string()));
            }
        }

        let item_library_id = *view_library_id_map.get(&view.id).ok_or_else(|| {
            AppError::Internal(format!("View「{}」未找到对应本地库映射", view.name))
        })?;

        let view_strm_workspace: PathBuf =
            strm_workspace.join(sanitize_segment(view.name.as_str()));
        if let Err(err) = tokio::fs::create_dir_all(&view_strm_workspace).await {
            tracing::warn!(view = %view.name, error = %err, "创建 View STRM 子目录失败");
        }

        let collection_type = view.collection_type.as_deref().unwrap_or("");
        let is_tvshows = collection_type.eq_ignore_ascii_case("tvshows")
            || collection_type.is_empty();
        let is_movies = collection_type.eq_ignore_ascii_case("movies");

        // ── View 级别 Etag 快速跳过（适用于所有类型库）──
        if is_incremental {
            if let Some(remote_etag) = &view.etag {
                if !remote_etag.is_empty() {
                    if let Ok(Some(cached_etag)) =
                        get_view_cached_etag(&state.pool, source.id, &view.id).await
                    {
                        if cached_etag == *remote_etag {
                            skipped_unchanged_movie_views.fetch_add(1, Ordering::Relaxed);
                            tracing::info!(
                                source_id = %source.id,
                                view = %view.name,
                                etag = %remote_etag,
                                "增量跳过：View Etag 未变化，跳过整个 View"
                            );
                            continue;
                        }
                    }
                }
            }
        }

        if is_movies {
            // ── 电影库：直接拉 Movie 列表 ──────────────────────────

            tracing::info!(
                source_id = %source.id,
                view = %view.name,
                "层级同步：开始处理电影库"
            );

            let mut movie_ids: HashSet<String> = HashSet::new();
            let mut start_index: i64 = 0;
            let mut view_movie_total: i64 = -1;

            loop {
                if let Some(handle) = &progress {
                    if handle.is_cancelled() {
                        return Err(AppError::BadRequest("同步任务已被取消".to_string()));
                    }
                }

                let page = fetch_remote_movies_page(
                    &state.pool,
                    source,
                    user_id.as_str(),
                    view.id.as_str(),
                    start_index,
                    page_size,
                )
                .await?;
                view_movie_total = page.total_record_count;

                if page.items.is_empty() {
                    break;
                }

                if let Some(handle) = &progress {
                    handle.set_movie_sync_progress(
                        start_index as u64,
                        written_files.load(Ordering::Relaxed),
                        page.total_record_count.max(0) as u64,
                    );
                }

                if enable_auto_delete {
                    for item in &page.items {
                        movie_ids.insert(item.id.clone());
                    }
                }

                // 并发处理 Movie 条目（复用已有的 process_one_remote_sync_item）
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
                    let local_synced_ids = Arc::clone(&local_synced_ids);
                    let skipped_existing = Arc::clone(&skipped_existing);
                    let strm_missing_reprocessed = Arc::clone(&strm_missing_reprocessed);
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
                            &local_synced_ids,
                            &skipped_existing,
                            &strm_missing_reprocessed,
                            page.total_record_count.max(0) as u64,
                            progress.as_ref(),
                            force_refresh_sidecar,
                        )
                        .await
                    }
                }))
                .buffer_unordered(REMOTE_SYNC_INNER_CONCURRENCY)
                .collect::<Vec<Result<(), AppError>>>()
                .await;

                for r in item_results {
                    r?;
                }

                start_index += page_size;
                if start_index >= page.total_record_count {
                    break;
                }
            }

            // 缓存 View 级别 Movie TotalRecordCount，下次增量可跳过
            if let Err(e) = upsert_view_remote_counts(
                &state.pool,
                source.id,
                &view.id,
                Some(view_movie_total),
                None,
            ).await {
                tracing::warn!(source_id = %source.id, view = %view.name, error = %e,
                    "缓存 Movie TotalRecordCount 失败");
            }

            // 缓存 View Etag，下次增量可通过 Etag 跳过整个 View
            if let Some(ref etag) = view.etag {
                if !etag.is_empty() {
                    if let Err(e) = upsert_view_etag(&state.pool, source.id, &view.id, etag).await {
                        tracing::warn!(source_id = %source.id, view = %view.name, error = %e,
                            "缓存 View Etag 失败");
                    }
                }
            }

            if enable_auto_delete {
                remote_movie_ids_by_view.insert(view.id.clone(), movie_ids);
            }
        } else if is_tvshows {
            // ── 电视剧库：层级拉取 Series -> Seasons -> Episodes ──
            tracing::info!(
                source_id = %source.id,
                view = %view.name,
                "层级同步：开始处理电视剧库"
            );

            // 1. 分页拉取 Series
            //    增量模式使用 DateModified 降序 + 早停：只拉取自上次同步以来有变化的 Series，
            //    遇到 DateModified <= last_sync_at 即停止分页，大幅减少 API 调用。
            //    删除检测独立使用 ID-only 轻量分页。
            //    首次全量：使用完整 API 以一次性获取所有元数据。
            let mut all_series: Vec<RemoteSeriesItem> = Vec::new();
            let mut start_index: i64 = 0;
            let mut _total_series_count: i64 = 0;

            if is_incremental {
                // ── 增量模式：按 DateModified 降序拉取，遇到早停条件即停 ──
                let last_sync = source.last_sync_at;
                let mut early_stopped = false;
                loop {
                    let page = fetch_remote_series_page_by_date_modified(
                        &state.pool,
                        source,
                        user_id.as_str(),
                        view.id.as_str(),
                        start_index,
                        page_size,
                    )
                    .await?;
                    _total_series_count = page.total_record_count;
                    if page.items.is_empty() {
                        break;
                    }
                    for item in page.items {
                        if let (Some(dm_str), Some(sync_at)) = (&item.date_modified, last_sync) {
                            if let Ok(dm) = chrono::DateTime::parse_from_rfc3339(dm_str) {
                                if dm.with_timezone(&Utc) < sync_at {
                                    early_stopped = true;
                                    break;
                                }
                            } else if let Ok(dm) = chrono::NaiveDateTime::parse_from_str(
                                dm_str.trim_end_matches('Z'),
                                "%Y-%m-%dT%H:%M:%S%.f",
                            ) {
                                if dm.and_utc() < sync_at {
                                    early_stopped = true;
                                    break;
                                }
                            }
                        }
                        all_series.push(item);
                    }
                    if early_stopped {
                        break;
                    }
                    start_index += page_size;
                    if start_index >= page.total_record_count {
                        break;
                    }
                }

                tracing::info!(
                    source_id = %source.id,
                    view = %view.name,
                    changed_series = all_series.len(),
                    total_remote = _total_series_count,
                    early_stopped,
                    "增量同步：DateModified 早停拉取变更 Series"
                );

                // 删除检测：仅在启用自动删除时才拉取全量 Series ID
                if enable_auto_delete {
                    let mut series_ids: HashSet<String> = HashSet::new();
                    for s in &all_series {
                        series_ids.insert(s.id.clone());
                    }
                    let mut del_start: i64 = 0;
                    loop {
                        let id_page = fetch_remote_series_ids_page(
                            &state.pool,
                            source,
                            user_id.as_str(),
                            view.id.as_str(),
                            del_start,
                            page_size,
                        )
                        .await?;
                        if id_page.items.is_empty() {
                            break;
                        }
                        for s in &id_page.items {
                            series_ids.insert(s.id.clone());
                        }
                        del_start += page_size;
                        if del_start >= id_page.total_record_count {
                            break;
                        }
                    }
                    remote_series_ids_by_view.insert(view.id.clone(), series_ids);
                }
            } else {
                // ── 全量模式：使用完整 Fields 拉取 ──
                loop {
                    let page = fetch_remote_series_page(
                        &state.pool,
                        source,
                        user_id.as_str(),
                        view.id.as_str(),
                        start_index,
                        page_size,
                    )
                    .await?;
                    _total_series_count = page.total_record_count;
                    if page.items.is_empty() {
                        break;
                    }
                    all_series.extend(page.items);
                    start_index += page_size;
                    if start_index >= page.total_record_count {
                        break;
                    }
                }
                if enable_auto_delete {
                    let mut series_ids: HashSet<String> = HashSet::new();
                    for s in &all_series {
                        series_ids.insert(s.id.clone());
                    }
                    remote_series_ids_by_view.insert(view.id.clone(), series_ids);
                }
            }

            tracing::info!(
                source_id = %source.id,
                view = %view.name,
                series_count = all_series.len(),
                total_record_count = _total_series_count,
                "层级同步：获取到 Series 列表"
            );

            // 2. 逐个 Series 处理
            let total_series = all_series.len() as u64;
            for (series_idx, remote_series) in all_series.iter().enumerate() {
                if let Some(handle) = &progress {
                    if handle.is_cancelled() {
                        return Err(AppError::BadRequest("同步任务已被取消".to_string()));
                    }
                    handle.set_series_progress(
                        &remote_series.name,
                        series_idx as u64,
                        total_series,
                        skipped_unchanged_series.load(Ordering::Relaxed),
                        fetched_count.load(Ordering::Relaxed),
                        written_files.load(Ordering::Relaxed),
                    );
                }

                // ── 增量检测：比对 RecursiveItemCount + ChildCount ──
                if is_incremental {
                    if let Some((local_recursive, local_child)) =
                        series_counts_cache.get(&remote_series.id)
                    {
                        let remote_recursive = remote_series.recursive_item_count;
                        let remote_child = remote_series.child_count;

                        if *local_recursive == remote_recursive
                            && local_child.map(|c| c as i32) == remote_child
                        {
                            skipped_unchanged_series.fetch_add(1, Ordering::Relaxed);
                            tracing::debug!(
                                source_id = %source.id,
                                series = %remote_series.name,
                                recursive_item_count = ?remote_recursive,
                                "增量跳过：Series 集数未变化"
                            );
                            continue;
                        }
                    }
                }

                // ── 确保 Series 父行存在（构造临时 RemoteSyncItem 适配已有函数签名）──
                let synthetic_item = RemoteSyncItem {
                    item: RemoteBaseItem {
                        id: remote_series.id.clone(),
                        name: remote_series.name.clone(),
                        item_type: "Series".to_string(),
                        overview: remote_series.overview.clone(),
                        production_year: remote_series.production_year,
                        official_rating: remote_series.official_rating.clone(),
                        community_rating: remote_series.community_rating,
                        critic_rating: remote_series.critic_rating,
                        premiere_date: remote_series.premiere_date.clone(),
                        run_time_ticks: remote_series.run_time_ticks,
                        status: remote_series.status.clone(),
                        end_date: remote_series.end_date.clone(),
                        series_name: Some(remote_series.name.clone()),
                        season_name: None,
                        parent_index_number: None,
                        index_number: None,
                        provider_ids: remote_series.provider_ids.clone(),
                        genres: remote_series.genres.clone(),
                        studios: remote_series.studios.clone(),
                        tags: remote_series.tags.clone(),
                        media_sources: None,
                        image_tags: remote_series.image_tags.clone(),
                        backdrop_image_tags: remote_series.backdrop_image_tags.clone(),
                        series_id: Some(remote_series.id.clone()),
                        season_id: None,
                        series_primary_image_tag: remote_series.image_tags.as_ref().and_then(|t| {
                            t.get("Primary").and_then(Value::as_str).map(String::from)
                        }),
                        parent_backdrop_image_tags: remote_series.backdrop_image_tags.clone(),
                        parent_backdrop_item_id: Some(remote_series.id.clone()),
                        parent_logo_item_id: None,
                        parent_logo_image_tag: None,
                        people: remote_series.people.clone(),
                        original_title: remote_series.original_title.clone(),
                        sort_name: remote_series.sort_name.clone(),
                        taglines: remote_series.taglines.clone(),
                        production_locations: remote_series.production_locations.clone(),
                        air_days: remote_series.air_days.clone(),
                        air_time: remote_series.air_time.clone(),
                        remote_trailers: remote_series.remote_trailers.clone(),
                    },
                    view_id: view.id.clone(),
                    view_name: view.name.clone(),
                };
                let series_db_id = ensure_remote_series_folder(
                    &state.pool,
                    source_arc.as_ref(),
                    &synthetic_item,
                    None,
                    &view.id,
                    &view_strm_workspace,
                    item_library_id,
                    &series_parent_map,
                )
                .await?;

                // 更新 Series 元数据（使用 Series 列表中带回的数据，不需要再发详情请求）
                series_detail_synced.insert(remote_series.id.clone());

                // ── 拉取 Seasons ──
                let seasons_result = fetch_remote_seasons(
                    &state.pool,
                    source,
                    user_id.as_str(),
                    &remote_series.id,
                )
                .await?;

                for remote_season in &seasons_result.items {
                    if let Some(handle) = &progress {
                        if handle.is_cancelled() {
                            return Err(AppError::BadRequest("同步任务已被取消".to_string()));
                        }
                    }

                    let season_number = remote_season.index_number.unwrap_or(1);

                    // ── 增量检测：比对 Season ChildCount（从批量缓存读取）──
                    if is_incremental {
                        if let Some(&local_child_count) =
                            season_counts_cache.get(&remote_season.id)
                        {
                            if let Some(remote_child_count) = remote_season.child_count {
                                if local_child_count == remote_child_count {
                                    skipped_unchanged_seasons.fetch_add(1, Ordering::Relaxed);
                                    tracing::debug!(
                                        source_id = %source.id,
                                        series = %remote_series.name,
                                        season = season_number,
                                        child_count = remote_child_count,
                                        "增量跳过：Season 集数未变化"
                                    );
                                    continue;
                                }
                            }
                        }
                    }

                    // ── 确保 Season 父行存在（构造临时 RemoteSyncItem 适配函数签名）──
                    let season_synthetic_item = RemoteSyncItem {
                        item: RemoteBaseItem {
                            id: remote_season.id.clone(),
                            name: remote_season.name.clone(),
                            item_type: "Season".to_string(),
                            overview: remote_season.overview.clone(),
                            production_year: remote_season.production_year,
                            official_rating: None,
                            community_rating: None,
                            critic_rating: None,
                            premiere_date: None,
                            run_time_ticks: None,
                            status: None,
                            end_date: None,
                            series_name: remote_season.series_name.clone().or_else(|| Some(remote_series.name.clone())),
                            season_name: Some(remote_season.name.clone()),
                            parent_index_number: remote_season.index_number,
                            index_number: remote_season.index_number,
                            provider_ids: remote_season.provider_ids.clone(),
                            genres: Vec::new(),
                            studios: Vec::new(),
                            tags: Vec::new(),
                            media_sources: None,
                            image_tags: remote_season.image_tags.clone(),
                            backdrop_image_tags: remote_season.backdrop_image_tags.clone(),
                            series_id: remote_season.series_id.clone().or_else(|| Some(remote_series.id.clone())),
                            season_id: Some(remote_season.id.clone()),
                            series_primary_image_tag: remote_season.series_primary_image_tag.clone(),
                            parent_backdrop_image_tags: remote_season.parent_backdrop_image_tags.clone(),
                            parent_backdrop_item_id: remote_season.parent_backdrop_item_id.clone(),
                            parent_logo_item_id: remote_season.parent_logo_item_id.clone(),
                            parent_logo_image_tag: remote_season.parent_logo_image_tag.clone(),
                            people: Vec::new(),
                            original_title: None,
                            sort_name: None,
                            taglines: Vec::new(),
                            production_locations: Vec::new(),
                            air_days: Vec::new(),
                            air_time: None,
                            remote_trailers: None,
                        },
                        view_id: view.id.clone(),
                        view_name: view.name.clone(),
                    };
                    let season_db_id = ensure_remote_season_folder(
                        &state.pool,
                        source_arc.as_ref(),
                        &season_synthetic_item,
                        series_db_id,
                        &view_strm_workspace,
                        item_library_id,
                        &season_parent_map,
                    )
                    .await?;

                    // ── 拉取该季的所有 Episode ──
                    let episodes_result = fetch_remote_episodes_for_season(
                        &state.pool,
                        source,
                        user_id.as_str(),
                        &remote_series.id,
                        &remote_season.id,
                    )
                    .await?;

                    if episodes_result.items.is_empty() {
                        continue;
                    }

                    // P8: 收集远端 Episode ID 集合，用于后续 episode-level 下架检测
                    let remote_episode_ids: HashSet<String> = episodes_result
                        .items
                        .iter()
                        .map(|ep| ep.id.clone())
                        .collect();

                    // 并发处理 Episode（复用已有的 process_one_remote_sync_item）
                    use futures::stream::{self, StreamExt};
                    let view_strm_workspace_arc = Arc::new(view_strm_workspace.clone());
                    let view_id_arc = Arc::new(view.id.clone());
                    let view_name_arc = Arc::new(view.name.clone());
                    let user_id_arc = Arc::new(user_id.clone());
                    let playback_token_arc = Arc::new(playback_token.clone());

                    let ep_count = episodes_result.items.len();
                    let item_results = stream::iter(
                        episodes_result.items.into_iter().map(|base_item| {
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
                            let local_synced_ids = Arc::clone(&local_synced_ids);
                            let skipped_existing = Arc::clone(&skipped_existing);
                            let strm_missing_reprocessed =
                                Arc::clone(&strm_missing_reprocessed);
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
                                    &local_synced_ids,
                                    &skipped_existing,
                                    &strm_missing_reprocessed,
                                    total_series,
                                    progress.as_ref(),
                                    force_refresh_sidecar,
                                )
                                .await
                            }
                        }),
                    )
                    .buffer_unordered(REMOTE_SYNC_INNER_CONCURRENCY)
                    .collect::<Vec<Result<(), AppError>>>()
                    .await;

                    for r in item_results {
                        r?;
                    }

                    // P8: Episode 级别下架检测——仅当启用自动删除时执行。
                    // 此时 remote_episode_ids 包含远端该季的全部 Episode，查本地 DB
                    // 该 Season 下的 Episode 取差集，删除已下架的条目及其 STRM 文件。
                    if enable_auto_delete {
                        if let Err(e) = delete_stale_episodes_for_season(
                            &state.pool,
                            season_db_id,
                            source.id,
                            &remote_episode_ids,
                            &view_strm_workspace,
                        )
                        .await
                        {
                            tracing::warn!(
                                source_id = %source.id,
                                series = %remote_series.name,
                                season = season_number,
                                error = %e,
                                "P8：Season 级 Episode 下架清理失败"
                            );
                        }
                    }

                    // 更新本地 Season 的 remote child_count
                    if let Some(child_count) = remote_season.child_count {
                        if let Err(e) = update_local_season_remote_child_count(
                            &state.pool,
                            source.id,
                            &remote_season.id,
                            child_count,
                        )
                        .await
                        {
                            tracing::warn!(
                                source_id = %source.id,
                                season_id = %remote_season.id,
                                error = %e,
                                "更新 Season remote child_count 失败"
                            );
                        }
                    }

                    tracing::debug!(
                        source_id = %source.id,
                        series = %remote_series.name,
                        season = season_number,
                        episodes = ep_count,
                        "层级同步：Season 处理完成"
                    );
                }

                // 更新本地 Series 的 remote counts
                if let Err(e) = update_local_series_remote_counts(
                    &state.pool,
                    source.id,
                    &remote_series.id,
                    remote_series.recursive_item_count,
                    remote_series.child_count,
                )
                .await
                {
                    tracing::warn!(
                        source_id = %source.id,
                        series = %remote_series.name,
                        error = %e,
                        "更新 Series remote counts 失败"
                    );
                }

                tracing::debug!(
                    source_id = %source.id,
                    series = %remote_series.name,
                    series_idx = series_idx + 1,
                    total_series,
                    "层级同步：Series 处理完成"
                );
            }

            // 缓存 View Etag，下次增量可通过 Etag 跳过整个 View
            if let Some(ref etag) = view.etag {
                if !etag.is_empty() {
                    if let Err(e) = upsert_view_etag(&state.pool, source.id, &view.id, etag).await {
                        tracing::warn!(source_id = %source.id, view = %view.name, error = %e,
                            "缓存 View Etag 失败");
                    }
                }
            }
        } else {
            // 未知类型库：使用旧方法 fallback（IncludeItemTypes=Movie,Episode）
            tracing::warn!(
                source_id = %source.id,
                view = %view.name,
                collection_type = collection_type,
                "未识别的 CollectionType，使用旧方法同步"
            );

            let mut start_index: i64 = 0;
            loop {
                if let Some(handle) = &progress {
                    if handle.is_cancelled() {
                        return Err(AppError::BadRequest("同步任务已被取消".to_string()));
                    }
                }

                let incremental_since = source.last_sync_at;
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
                    let local_synced_ids = Arc::clone(&local_synced_ids);
                    let skipped_existing = Arc::clone(&skipped_existing);
                    let strm_missing_reprocessed = Arc::clone(&strm_missing_reprocessed);
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
                            &local_synced_ids,
                            &skipped_existing,
                            &strm_missing_reprocessed,
                            page.total_record_count.max(0) as u64,
                            progress.as_ref(),
                            force_refresh_sidecar,
                        )
                        .await
                    }
                }))
                .buffer_unordered(REMOTE_SYNC_INNER_CONCURRENCY)
                .collect::<Vec<Result<(), AppError>>>()
                .await;

                for r in item_results {
                    r?;
                }

                start_index += page_size;
                if start_index >= page.total_record_count {
                    break;
                }
            }
        }
    }

    // ── 删除检测（层级优化版）──────────────────────────────────
    // 仅在 enable_auto_delete=true 时执行。
    // 电视剧库：比对 Series 列表（数量远少于 Episode），缺失的 Series -> 级联删除
    // 电影库：比对 Movie ID 列表
    if enable_auto_delete {
    if let Some(handle) = &progress {
        handle.set_phase("PruningStaleItems", 96.0);
    }

    for view in &views {
        let collection_type = view.collection_type.as_deref().unwrap_or("");
        let is_tvshows = collection_type.eq_ignore_ascii_case("tvshows")
            || collection_type.is_empty();
        let is_movies = collection_type.eq_ignore_ascii_case("movies");

        if is_tvshows {
            if let Some(remote_series_ids) = remote_series_ids_by_view.get(&view.id) {
                // 查找本地有但远端没有的 Series -> 级联删除 + STRM 目录清理
                let local_series: Vec<(Uuid, Option<String>, Option<String>)> = sqlx::query_as(
                    r#"
                    SELECT id, provider_ids->>'RemoteEmbySeriesId' AS remote_sid, path
                    FROM media_items
                    WHERE provider_ids->>'RemoteEmbySourceId' = $1
                      AND item_type = 'Series'
                      AND provider_ids->>'RemoteEmbyViewId' = $2
                    "#,
                )
                .bind(source.id.to_string())
                .bind(&view.id)
                .fetch_all(&state.pool)
                .await?;

                let mut stale_series_db_ids: Vec<Uuid> = Vec::new();
                let mut stale_series_paths: Vec<String> = Vec::new();
                for (db_id, remote_sid, path) in &local_series {
                    if let Some(sid) = remote_sid {
                        if !remote_series_ids.contains(sid) {
                            stale_series_db_ids.push(*db_id);
                            if let Some(p) = path {
                                if !p.is_empty() {
                                    stale_series_paths.push(p.clone());
                                }
                            }
                        }
                    }
                }

                if !stale_series_db_ids.is_empty() {
                    tracing::info!(
                        source_id = %source.id,
                        view = %view.name,
                        stale_count = stale_series_db_ids.len(),
                        "层级删除检测：发现远端已下架的 Series"
                    );
                    for batch in stale_series_db_ids.chunks(100) {
                        let deleted = sqlx::query(
                            "DELETE FROM media_items WHERE id = ANY($1)"
                        )
                        .bind(batch)
                        .execute(&state.pool)
                        .await?;
                        total_deleted += deleted.rows_affected();
                    }
                    // 清理孤儿 STRM 目录（Series 目录及其子内容）
                    for series_path in &stale_series_paths {
                        let dir = Path::new(series_path);
                        if dir.exists() {
                            if let Err(e) = tokio::fs::remove_dir_all(dir).await {
                                tracing::warn!(
                                    path = %series_path,
                                    error = %e,
                                    "清理已删除 Series 的 STRM 目录失败"
                                );
                            } else {
                                tracing::debug!(path = %series_path, "已清理已删除 Series 的 STRM 目录");
                            }
                        }
                    }
                }
            }
        } else if is_movies {
            if let Some(remote_movie_ids) = remote_movie_ids_by_view.get(&view.id) {
                // 查找本地有但远端没有的 Movie -> 删除 + STRM 文件清理
                let local_movies: Vec<(Uuid, Option<String>, Option<String>)> = sqlx::query_as(
                    r#"
                    SELECT id, provider_ids->>'RemoteEmbyItemId' AS remote_mid, path
                    FROM media_items
                    WHERE provider_ids->>'RemoteEmbySourceId' = $1
                      AND item_type = 'Movie'
                      AND provider_ids->>'RemoteEmbyItemId' IS NOT NULL
                      AND provider_ids->>'RemoteEmbyItemId' <> ''
                    "#,
                )
                .bind(source.id.to_string())
                .fetch_all(&state.pool)
                .await?;

                let mut stale_movie_db_ids: Vec<Uuid> = Vec::new();
                let mut stale_movie_paths: Vec<String> = Vec::new();
                for (db_id, remote_mid, path) in &local_movies {
                    if let Some(mid) = remote_mid {
                        if !remote_movie_ids.contains(mid) {
                            stale_movie_db_ids.push(*db_id);
                            if let Some(p) = path {
                                if !p.is_empty() {
                                    stale_movie_paths.push(p.clone());
                                }
                            }
                        }
                    }
                }

                if !stale_movie_db_ids.is_empty() {
                    tracing::info!(
                        source_id = %source.id,
                        view = %view.name,
                        stale_count = stale_movie_db_ids.len(),
                        "层级删除检测：发现远端已下架的 Movie"
                    );
                    for batch in stale_movie_db_ids.chunks(100) {
                        let deleted = sqlx::query(
                            "DELETE FROM media_items WHERE id = ANY($1)"
                        )
                        .bind(batch)
                        .execute(&state.pool)
                        .await?;
                        total_deleted += deleted.rows_affected();
                    }
                    // 清理孤儿 STRM/NFO 文件（Movie 的 STRM 文件及同名 NFO）
                    for movie_path in &stale_movie_paths {
                        let strm = Path::new(movie_path);
                        if strm.exists() {
                            if let Err(e) = tokio::fs::remove_file(strm).await {
                                tracing::warn!(path = %movie_path, error = %e, "清理已删除 Movie 的 STRM 文件失败");
                            }
                        }
                        let nfo = strm.with_extension("nfo");
                        if nfo.exists() {
                            let _ = tokio::fs::remove_file(&nfo).await;
                        }
                    }
                }
            }
        }
    }

    if total_deleted > 0 {
        tracing::info!(
            source_id = %source.id,
            deleted = total_deleted,
            "层级同步「删」：清理远端已下架条目"
        );
    }
    } else {
        tracing::info!(
            source_id = %source.id,
            "自动删除已关闭，跳过删除检测"
        );
    } // end enable_auto_delete

    // 等齐所有 series detail spawn task
    let pending_handles = std::mem::take(&mut *series_detail_handles.lock().await);
    if !pending_handles.is_empty() {
        let pending_count = pending_handles.len();
        if let Some(handle) = &progress {
            handle.set_phase("FinalizingSeriesDetails", 99.0);
        }
        for h in pending_handles {
            let _ = h.await;
        }
        tracing::info!(
            source_id = %source.id,
            count = pending_count,
            "所有 Series 详情后台同步完成"
        );
    }
    detail_handles_guard.disarm();

    if let Err(error) = repository::clear_source_view_progress(&state.pool, source.id).await {
        tracing::warn!(source_id = %source.id, error = %error, "清空续抓游标失败");
    }

    let fetched_count = fetched_count.load(Ordering::Relaxed);
    let written_files = written_files.load(Ordering::Relaxed) as usize;
    let skipped_existing_count = skipped_existing.load(Ordering::Relaxed);
    let strm_missing_reprocessed_count = strm_missing_reprocessed.load(Ordering::Relaxed);
    let skipped_series = skipped_unchanged_series.load(Ordering::Relaxed);
    let skipped_seasons = skipped_unchanged_seasons.load(Ordering::Relaxed);
    let skipped_movie_views = skipped_unchanged_movie_views.load(Ordering::Relaxed);

    let mode_label = if is_incremental {
        "增量(层级)"
    } else {
        "首次(层级)"
    };
    tracing::info!(
        source_id = %source.id,
        mode = mode_label,
        fetched = fetched_count,
        written = written_files,
        skipped_existing = skipped_existing_count,
        skipped_unchanged_series = skipped_series,
        skipped_unchanged_seasons = skipped_seasons,
        skipped_unchanged_movie_views = skipped_movie_views,
        strm_missing_reprocessed = strm_missing_reprocessed_count,
        deleted_stale = total_deleted,
        "远端 Emby 层级同步完成"
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
    // PB49：「全量同步路径上跳过已入库条目」的状态。
    // B1：从 DashSet 升级到 DashMap，value 为该 remote_id 在本地的 STRM 路径，
    //     用于跳过前的文件存在性自愈校验。
    local_synced_ids: &DashMap<String, String>,
    skipped_existing: &AtomicU64,
    strm_missing_reprocessed: &AtomicU64,
    total_items: u64,
    progress: Option<&RemoteSyncProgress>,
    force_refresh_sidecar: bool,
) -> Result<(), AppError> {
    if let Some(handle) = progress {
        if handle.is_cancelled() {
            return Err(AppError::BadRequest("同步任务已被取消".to_string()));
        }
    }

    let mut is_strm_self_heal = false;

    // PB49：full sync 路径上，主循环命中本地已入库的条目就直接跳过。
    //
    // 之前即使配合 B-7 续抓游标，每个已落库的 Movie/Episode 仍要走完
    // 「ensure_series_folder → ensure_season_folder → write_strm_bundle →
    //  upsert_media_item → save_media_streams → upsert_people」全套流程，
    // 单条 ~30-100ms，10 万条全量重过一遍可观地拖慢「失败后重试」体验。
    //
    // 这里的早退**不影响正确性**：
    //   - local_synced_ids 仅在 force_refresh_sidecar=false（last_sync_at IS NULL）时
    //     被填充；增量路径 (last_sync_at=Some) 下集合为空，不会命中。
    //   - 层级删除检测（Series/Movie 级）仍照常跑，
    //     所以「远端已下架的本地条目」仍会被正常清理（不依赖主循环遍历）。
    //   - 命中跳过时仍 bump fetched_count + written_files，让 UI 看到的
    //     「入库条目」反映本地真实存量，避免又出现「明明 11 万在库却显示 400」的
    //     反直觉。
    if !force_refresh_sidecar {
        if let Some(local_path_entry) = local_synced_ids.get(base_item.id.as_str()) {
            let local_path = local_path_entry.value().clone();
            drop(local_path_entry);
            // PB49 (B1)：跳过前先做一次廉价的 STRM 文件存在性校验。
            // 用户手工 rm 了某个 strm 文件想强制重生，这里如果不校验就会被
            // 静默吃掉，永远长不回来。stat(2) 在本地盘 ~10us，对全量
            // 256k 条目最多 ~2.5s 开销；远比一次完整 upsert 链条便宜。
            //
            // 注意 path 可能为空（旧版兼容）或不是 .strm 后缀（Series/Season），
            // 这两种情况都按「无 STRM 可校验」直接走原跳过路径，不影响正确性。
            let should_skip = if local_path.is_empty() || !local_path.ends_with(".strm") {
                true
            } else {
                match tokio::fs::try_exists(&local_path).await {
                    Ok(exists) => exists,
                    Err(io_err) => {
                        tracing::warn!(
                            source_id = %source.id,
                            remote_id = %base_item.id,
                            path = %local_path,
                            error = %io_err,
                            "PB49 (B1)：STRM 存在性检查 I/O 错误，视为缺失触发自愈"
                        );
                        false
                    }
                }
            };
            if should_skip {
                let s = skipped_existing.fetch_add(1, Ordering::Relaxed) + 1;
                let f = fetched_count.fetch_add(1, Ordering::Relaxed) + 1;
                let w = written_files.fetch_add(1, Ordering::Relaxed) + 1;
                if let Some(handle) = progress {
                    handle.set_streaming_progress(f, w, total_items);
                    // PB49 (C3)：跳过计数也推到 UI，让用户看到「绝大多数条目走的是
                    // 跳过-已入库 fast path」而不是真的在重写——这条信息能直接平息
                    // 「为什么数字飞涨」「是不是又重新写库」之类的疑问。
                    handle.set_skipped_counters(s, strm_missing_reprocessed.load(Ordering::Relaxed));
                }
                return Ok(());
            }
            // STRM 不存在 → 落到下面的完整路径重写文件 + 重 upsert，并计一笔
            // 自愈数量到 strm_missing_reprocessed，方便用户通过日志验证。
            let r = strm_missing_reprocessed.fetch_add(1, Ordering::Relaxed) + 1;
            if let Some(handle) = progress {
                handle.set_skipped_counters(skipped_existing.load(Ordering::Relaxed), r);
            }
            tracing::warn!(
                source_id = %source.id,
                remote_id = %base_item.id,
                missing_path = %local_path,
                "PB49 (B1)：本地 STRM 文件丢失，触发条目重写自愈"
            );
            is_strm_self_heal = true;
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
                    match fetch_and_upsert_series_detail(
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
                        Ok(_) => {
                            // PB49 (B2)：成功落库才写入持久化标记。失败路径完全不写，
                            // 让下次 sync 自动重试这部剧（in-memory DashSet 已防本轮重试）。
                            if let Err(mark_err) = repository::mark_series_detail_synced(
                                &pool_owned,
                                source_id_copy,
                                remote_sid_owned.as_str(),
                            )
                            .await
                            {
                                tracing::warn!(
                                    source_id = %source_id_copy,
                                    remote_series_id = %remote_sid_owned,
                                    error = %mark_err,
                                    "PB49 (B2)：写入 series detail 持久化标记失败（不影响内存 dedup，下次 sync 会再拉一次）"
                                );
                            }
                        }
                        Err(error) => {
                            tracing::warn!(
                                source_id = %source_id_copy,
                                remote_series_id = %remote_sid_owned,
                                error = %error,
                                "PB46：远端 Series 详情后台同步失败，忽略继续"
                            );
                        }
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
    // 自愈场景：STRM 丢失通常意味着 sidecar 图片也丢了（同目录），
    // 此时 local_poster 等为 None，upsert 会传入远端 URL。
    // force_overwrite_images=true 确保 DB 从失效的本地路径切回远端 URL，
    // 让 sidecar_image_download_loop 重新下载。如果图片文件恰好还在，
    // local_poster 就是 Some(本地路径)，写回去也完全正确。
    let overwrite_images = force_refresh_sidecar || is_strm_self_heal;
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
        overwrite_images,
    )
    .await?;
    if !is_strm_self_heal {
        if let Some(analysis) = analysis {
            repository::save_media_streams(&state.pool, upserted, &analysis).await?;
            repository::update_media_item_metadata(&state.pool, upserted, &analysis).await?;
        }
        if !item.item.people.is_empty() {
            if let Err(error) =
                upsert_remote_people_for_item(&state.pool, source, upserted, &item.item.people)
                    .await
            {
                tracing::warn!(
                    source_id = %source.id,
                    remote_item_id = %item.item.id,
                    error = %error,
                    "PB31-1：写入远端 item 的 People 失败"
                );
            }
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

    // 删除源时清掉它的拉取速率节流槽和并发信号量，避免 HashMap 累积陈旧条目
    {
        let mut throttle = REMOTE_REQUEST_THROTTLE.write().await;
        throttle.remove(&source.id);
    }
    {
        let mut concurrency = REMOTE_HTTP_CONCURRENCY.write().await;
        concurrency.remove(&source.id);
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
    // PB49 (A2)：sid-based 主 key 没命中时，再走一次 name-based fallback key。
    // preheat_series_for_source 只能填这把 key（DB 里没存 remote SeriesId），
    // 所以这里 mirror 一下：name 命中后顺手把 sid-based key 也插进去，
    // 让同剧的下一集可以走主 key 快路径。
    let name_fallback_key = format!("{view_scope}::{}", sanitize_segment(series_name.as_str()));
    if name_fallback_key != series_key {
        if let Some(existing) = series_parent_map.get(name_fallback_key.as_str()) {
            let id = *existing.value();
            series_parent_map.insert(series_key, id);
            return Ok(id);
        }
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
            provider_ids: {
                let mut pids = remote_marker_provider_ids(source.id, None, Some(&item.view_id), None);
                if let Some(sid) = item.item.series_id.as_deref().filter(|s| !s.trim().is_empty()) {
                    if let Some(obj) = pids.as_object_mut() {
                        obj.insert("RemoteEmbySeriesId".to_string(), Value::String(sid.to_string()));
                    }
                }
                pids
            },
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
            force_overwrite_images: false,
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
            provider_ids: {
                let mut pids = remote_marker_provider_ids(source.id, None, Some(&item.view_id), None);
                if let Some(obj) = pids.as_object_mut() {
                    if let Some(sid) = item.item.season_id.as_deref().filter(|s| !s.trim().is_empty()) {
                        obj.insert("RemoteEmbySeasonId".to_string(), Value::String(sid.to_string()));
                    }
                    if let Some(series_id) = item.item.series_id.as_deref().filter(|s| !s.trim().is_empty()) {
                        obj.insert("RemoteEmbySeriesId".to_string(), Value::String(series_id.to_string()));
                    }
                }
                pids
            },
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
            force_overwrite_images: false,
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
    force_overwrite_images: bool,
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
    // 额外 backdrops（跳过第一张，因为第一张已存入 backdrop_path 单值字段）；
    // media_items.backdrop_paths 存的是 index 1+ 的壁纸。
    let all_backdrops: Vec<String> = if !remote_urls.backdrops.is_empty() {
        remote_urls.backdrops.iter().skip(1).cloned().collect()
    } else {
        series_fallback_backdrops.iter().skip(1).cloned().collect()
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
            force_overwrite_images,
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
            force_overwrite_images: false,
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
            "CollectionType,ChildCount,RecursiveItemCount,Etag".to_string(),
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

// ═══════════════════════════════════════════════════════════════════════════
// 层级 Fetch 函数：按 Emby 标准 API 层级拉取
//   Views -> Series(tvshows)/Movies(movies) -> Seasons -> Episodes
// ═══════════════════════════════════════════════════════════════════════════

/// 远端 Series 条目（来自 /Users/{userId}/Items?IncludeItemTypes=Series）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteSeriesItem {
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
    status: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
    #[serde(default)]
    provider_ids: Value,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    genres: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    studios: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list_lossy")]
    tags: Vec<String>,
    #[serde(default)]
    image_tags: Option<Value>,
    #[serde(default)]
    backdrop_image_tags: Option<Value>,
    #[serde(default)]
    people: Vec<RemotePersonEntry>,
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
    /// Emby: 该 Series 下所有集的总数（含所有季）
    #[serde(default)]
    recursive_item_count: Option<i64>,
    /// Emby: 该 Series 下的季数
    #[serde(default)]
    child_count: Option<i32>,
    #[serde(default)]
    run_time_ticks: Option<i64>,
    #[serde(default)]
    date_created: Option<String>,
    #[serde(default)]
    date_modified: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteSeriesResult {
    #[serde(default)]
    items: Vec<RemoteSeriesItem>,
    total_record_count: i64,
}

/// 远端 Season 条目（来自 /Shows/{seriesId}/Seasons）
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteSeasonItem {
    id: String,
    name: String,
    #[serde(default)]
    index_number: Option<i32>,
    #[serde(default)]
    child_count: Option<i32>,
    #[serde(default)]
    production_year: Option<i32>,
    #[serde(default)]
    date_created: Option<String>,
    #[serde(default)]
    image_tags: Option<Value>,
    #[serde(default)]
    backdrop_image_tags: Option<Value>,
    #[serde(default)]
    series_name: Option<String>,
    #[serde(default)]
    series_id: Option<String>,
    #[serde(default)]
    series_primary_image_tag: Option<String>,
    #[serde(default)]
    parent_logo_item_id: Option<String>,
    #[serde(default)]
    parent_logo_image_tag: Option<String>,
    #[serde(default)]
    parent_backdrop_item_id: Option<String>,
    #[serde(default)]
    parent_backdrop_image_tags: Option<Value>,
    #[serde(default)]
    primary_image_aspect_ratio: Option<f64>,
    #[serde(rename = "Type", default)]
    item_type: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default)]
    provider_ids: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteSeasonsResult {
    #[serde(default)]
    items: Vec<RemoteSeasonItem>,
    total_record_count: i64,
}

/// 远端 Episode 条目（来自 /Shows/{seriesId}/Episodes）
/// 复用已有 RemoteBaseItem（Episode 字段完全兼容）

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteEpisodesResult {
    #[serde(default)]
    items: Vec<RemoteBaseItem>,
    total_record_count: i64,
}

/// 分页拉取某个 tvshows 库下的 Series 列表
async fn fetch_remote_series_page(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    start_index: i64,
    limit: i64,
) -> Result<RemoteSeriesResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    let query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("ParentId".to_string(), view_id.to_string()),
        ("IncludeItemTypes".to_string(), "Series".to_string()),
        (
            "Fields".to_string(),
            "BasicSyncInfo,PrimaryImageAspectRatio,ProductionYear,Status,EndDate,\
             ChildCount,RecursiveItemCount,DateCreated,DateModified,Overview,Genres,Studios,Tags,\
             ProviderIds,ImageTags,BackdropImageTags,People,OriginalTitle,SortName,\
             Taglines,ProductionLocations,AirDays,AirTime,RemoteTrailers,\
             OfficialRating,CommunityRating,CriticRating,PremiereDate"
                .to_string(),
        ),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("SortBy".to_string(), "SortName".to_string()),
        ("SortOrder".to_string(), "Ascending".to_string()),
        ("StartIndex".to_string(), start_index.to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 增量模式下拉取 Series 列表（按 DateModified 降序），用于早停优化
async fn fetch_remote_series_page_by_date_modified(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    start_index: i64,
    limit: i64,
) -> Result<RemoteSeriesResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    let query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("ParentId".to_string(), view_id.to_string()),
        ("IncludeItemTypes".to_string(), "Series".to_string()),
        (
            "Fields".to_string(),
            "BasicSyncInfo,ChildCount,RecursiveItemCount,DateModified".to_string(),
        ),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("SortBy".to_string(), "DateModified".to_string()),
        ("SortOrder".to_string(), "Descending".to_string()),
        ("StartIndex".to_string(), start_index.to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 仅拉取 Series 的 Id 列表（用于删除检测，不请求任何重量字段）
async fn fetch_remote_series_ids_page(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    start_index: i64,
    limit: i64,
) -> Result<RemoteSeriesResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    let query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("ParentId".to_string(), view_id.to_string()),
        ("IncludeItemTypes".to_string(), "Series".to_string()),
        ("Fields".to_string(), "BasicSyncInfo".to_string()),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("SortBy".to_string(), "SortName".to_string()),
        ("SortOrder".to_string(), "Ascending".to_string()),
        ("StartIndex".to_string(), start_index.to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 增量模式下拉取 Series 列表的轻量版——只请求增量检测所需的最少字段
#[allow(dead_code)]
async fn fetch_remote_series_page_lightweight(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    view_id: &str,
    start_index: i64,
    limit: i64,
) -> Result<RemoteSeriesResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Users/{user_id}/Items");
    let query = vec![
        ("Recursive".to_string(), "true".to_string()),
        ("ParentId".to_string(), view_id.to_string()),
        ("IncludeItemTypes".to_string(), "Series".to_string()),
        (
            "Fields".to_string(),
            "BasicSyncInfo,ChildCount,RecursiveItemCount".to_string(),
        ),
        ("EnableTotalRecordCount".to_string(), "true".to_string()),
        ("SortBy".to_string(), "SortName".to_string()),
        ("SortOrder".to_string(), "Ascending".to_string()),
        ("StartIndex".to_string(), start_index.to_string()),
        ("Limit".to_string(), limit.to_string()),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 分页拉取某个 movies 库下的 Movie 列表（直接使用 RemoteItemsResult）
async fn fetch_remote_movies_page(
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
        ("IncludeItemTypes".to_string(), "Movie".to_string()),
        (
            "Fields".to_string(),
            "SeriesName,SeasonName,ProductionYear,ParentIndexNumber,IndexNumber,Overview,\
             OfficialRating,CommunityRating,CriticRating,PremiereDate,RunTimeTicks,\
             ProviderIds,Genres,Studios,Tags,MediaSources,MediaStreams,\
             ImageTags,BackdropImageTags,\
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
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 拉取某个 Series 下的所有 Season（/Shows/{seriesId}/Seasons）
async fn fetch_remote_seasons(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    series_id: &str,
) -> Result<RemoteSeasonsResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Shows/{series_id}/Seasons");
    let query = vec![
        ("UserId".to_string(), user_id.to_string()),
        (
            "Fields".to_string(),
            "PrimaryImageAspectRatio,ItemCounts,ChildCount,ProductionYear,\
             DateCreated,Overview,ImageTags,BackdropImageTags,ProviderIds"
                .to_string(),
        ),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 拉取某个 Series 下某个 Season 的所有 Episode（/Shows/{seriesId}/Episodes）
async fn fetch_remote_episodes_for_season(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    series_id: &str,
    season_id: &str,
) -> Result<RemoteEpisodesResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Shows/{series_id}/Episodes");
    let query = vec![
        ("UserId".to_string(), user_id.to_string()),
        ("SeasonId".to_string(), season_id.to_string()),
        (
            "Fields".to_string(),
            "BasicSyncInfo,PrimaryImageAspectRatio,Overview,RunTimeTicks,\
             IndexNumber,ParentIndexNumber,DateCreated,PremiereDate,\
             ProviderIds,Genres,Studios,Tags,MediaSources,MediaStreams,\
             ImageTags,BackdropImageTags,SeriesId,SeasonId,\
             SeriesPrimaryImageTag,ParentBackdropImageTags,ParentBackdropItemId,\
             ParentLogoItemId,ParentLogoImageTag,\
             People,OriginalTitle,SortName,Taglines,ProductionLocations,\
             OfficialRating,CommunityRating,CriticRating,\
             SeriesName,SeasonName"
                .to_string(),
        ),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 拉取某个 Series 下所有 Episode（不限 Season，用于获取全量 Episode ID）
#[allow(dead_code)]
async fn fetch_remote_all_episodes_for_series(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    user_id: &str,
    series_id: &str,
) -> Result<RemoteEpisodesResult, AppError> {
    let server_url = normalize_server_url(&source.server_url);
    let endpoint = format!("{server_url}/Shows/{series_id}/Episodes");
    let query = vec![
        ("UserId".to_string(), user_id.to_string()),
        (
            "Fields".to_string(),
            "BasicSyncInfo,PrimaryImageAspectRatio,Overview,RunTimeTicks,\
             IndexNumber,ParentIndexNumber,DateCreated,PremiereDate,\
             ProviderIds,Genres,Studios,Tags,MediaSources,MediaStreams,\
             ImageTags,BackdropImageTags,SeriesId,SeasonId,\
             SeriesPrimaryImageTag,ParentBackdropImageTags,ParentBackdropItemId,\
             ParentLogoItemId,ParentLogoImageTag,\
             People,OriginalTitle,SortName,Taglines,ProductionLocations,\
             OfficialRating,CommunityRating,CriticRating,\
             SeriesName,SeasonName"
                .to_string(),
        ),
    ];
    get_json_with_retry(pool, source, &endpoint, &query).await
}

/// 查询本地 DB 中某个 Series 的 remote counts（逐条查询版，已被 batch_load_series_remote_counts 替代）
#[allow(dead_code)]
async fn get_local_series_remote_counts(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    remote_series_id: &str,
) -> Result<Option<(Option<i64>, Option<i32>)>, AppError> {
    let row: Option<(Value,)> = sqlx::query_as(
        r#"
        SELECT provider_ids
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND provider_ids->>'RemoteEmbySeriesId' = $2
          AND item_type = 'Series'
        LIMIT 1
        "#,
    )
    .bind(source_id.to_string())
    .bind(remote_series_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(pids,)| {
        let recursive = pids
            .get("RemoteRecursiveItemCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i64>().ok());
        let child = pids
            .get("RemoteChildCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        (recursive, child)
    }))
}

/// 更新本地 DB 中某个 Series 的 remote counts（存入 provider_ids）
async fn update_local_series_remote_counts(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    remote_series_id: &str,
    recursive_item_count: Option<i64>,
    child_count: Option<i32>,
) -> Result<(), AppError> {
    let has_recursive = recursive_item_count.is_some();
    let has_child = child_count.is_some();
    if !has_recursive && !has_child {
        return Ok(());
    }

    let expr = match (recursive_item_count, child_count) {
        (Some(r), Some(c)) => format!(
            "provider_ids = jsonb_set(\
                jsonb_set(provider_ids, '{{RemoteRecursiveItemCount}}', '\"{}\"'), \
                '{{RemoteChildCount}}', '\"{}\"')",
            r, c
        ),
        (Some(r), None) => format!(
            "provider_ids = jsonb_set(provider_ids, '{{RemoteRecursiveItemCount}}', '\"{}\"')",
            r
        ),
        (None, Some(c)) => format!(
            "provider_ids = jsonb_set(provider_ids, '{{RemoteChildCount}}', '\"{}\"')",
            c
        ),
        (None, None) => unreachable!(),
    };

    let sql = format!(
        "UPDATE media_items SET {} WHERE provider_ids->>'RemoteEmbySourceId' = $1 \
         AND provider_ids->>'RemoteEmbySeriesId' = $2 AND item_type = 'Series'",
        expr
    );
    sqlx::query(&sql)
        .bind(source_id.to_string())
        .bind(remote_series_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 获取本地 DB 中某个 Season 的 remote child_count（逐条查询版，已被 batch_load_season_remote_child_counts 替代）
#[allow(dead_code)]
async fn get_local_season_remote_child_count(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    remote_season_id: &str,
) -> Result<Option<i32>, AppError> {
    let row: Option<(Value,)> = sqlx::query_as(
        r#"
        SELECT provider_ids
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND provider_ids->>'RemoteEmbySeasonId' = $2
          AND item_type = 'Season'
        LIMIT 1
        "#,
    )
    .bind(source_id.to_string())
    .bind(remote_season_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|(pids,)| {
        pids.get("RemoteChildCount")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok())
    }))
}

/// 更新本地 DB 中某个 Season 的 remote child_count
async fn update_local_season_remote_child_count(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    remote_season_id: &str,
    child_count: i32,
) -> Result<(), AppError> {
    let sql = "UPDATE media_items SET provider_ids = jsonb_set(provider_ids, '{RemoteChildCount}', $1) \
               WHERE provider_ids->>'RemoteEmbySourceId' = $2 \
               AND provider_ids->>'RemoteEmbySeasonId' = $3 AND item_type = 'Season'";
    sqlx::query(sql)
        .bind(json!(child_count.to_string()))
        .bind(source_id.to_string())
        .bind(remote_season_id)
        .execute(pool)
        .await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// View 级别增量计数：Movie TotalRecordCount / Series TotalRecordCount
// ═══════════════════════════════════════════════════════════════════════════

#[allow(dead_code)]
async fn get_view_remote_counts(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    view_id: &str,
) -> Result<Option<(Option<i64>, Option<i64>)>, AppError> {
    let row: Option<(Option<i64>, Option<i64>)> = sqlx::query_as(
        r#"
        SELECT remote_movie_count, remote_series_count
        FROM remote_emby_source_view_progress
        WHERE source_id = $1 AND view_id = $2
        "#,
    )
    .bind(source_id)
    .bind(view_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

async fn get_view_cached_etag(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    view_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<(Option<String>,)> = sqlx::query_as(
        "SELECT remote_etag FROM remote_emby_source_view_progress WHERE source_id = $1 AND view_id = $2",
    )
    .bind(source_id)
    .bind(view_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.and_then(|(etag,)| etag))
}

async fn upsert_view_remote_counts(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    view_id: &str,
    movie_count: Option<i64>,
    series_count: Option<i64>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO remote_emby_source_view_progress
            (source_id, view_id, remote_movie_count, remote_series_count, updated_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (source_id, view_id) DO UPDATE SET
            remote_movie_count  = COALESCE($3, remote_emby_source_view_progress.remote_movie_count),
            remote_series_count = COALESCE($4, remote_emby_source_view_progress.remote_series_count),
            updated_at = now()
        "#,
    )
    .bind(source_id)
    .bind(view_id)
    .bind(movie_count)
    .bind(series_count)
    .execute(pool)
    .await?;
    Ok(())
}

async fn upsert_view_etag(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    view_id: &str,
    etag: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO remote_emby_source_view_progress (source_id, view_id, remote_etag, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (source_id, view_id) DO UPDATE SET remote_etag = $3, updated_at = now()
        "#,
    )
    .bind(source_id)
    .bind(view_id)
    .bind(etag)
    .execute(pool)
    .await?;
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════
// 批量预加载 Series/Season 的 remote counts（消除 N+1 查询）
// ═══════════════════════════════════════════════════════════════════════════

async fn batch_load_series_remote_counts(
    pool: &sqlx::PgPool,
    source_id: Uuid,
) -> Result<HashMap<String, (Option<i64>, Option<i32>)>, AppError> {
    let rows: Vec<(Option<String>, Value)> = sqlx::query_as(
        r#"
        SELECT provider_ids->>'RemoteEmbySeriesId', provider_ids
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND item_type = 'Series'
          AND provider_ids->>'RemoteEmbySeriesId' IS NOT NULL
        "#,
    )
    .bind(source_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::with_capacity(rows.len());
    for (series_id_opt, pids) in rows {
        if let Some(series_id) = series_id_opt {
            let recursive = pids
                .get("RemoteRecursiveItemCount")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i64>().ok());
            let child = pids
                .get("RemoteChildCount")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i32>().ok());
            map.insert(series_id, (recursive, child));
        }
    }
    Ok(map)
}

async fn batch_load_season_remote_child_counts(
    pool: &sqlx::PgPool,
    source_id: Uuid,
) -> Result<HashMap<String, i32>, AppError> {
    let rows: Vec<(Option<String>, Value)> = sqlx::query_as(
        r#"
        SELECT provider_ids->>'RemoteEmbySeasonId', provider_ids
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND item_type = 'Season'
          AND provider_ids->>'RemoteEmbySeasonId' IS NOT NULL
        "#,
    )
    .bind(source_id.to_string())
    .fetch_all(pool)
    .await?;

    let mut map = HashMap::with_capacity(rows.len());
    for (season_id_opt, pids) in rows {
        if let Some(season_id) = season_id_opt {
            if let Some(child_count) = pids
                .get("RemoteChildCount")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<i32>().ok())
            {
                map.insert(season_id, child_count);
            }
        }
    }
    Ok(map)
}

/// P8: 删除某 Season 下本地存在但远端已不存在的 Episode 条目及其 STRM 文件。
///
/// 仅比对 `RemoteEmbyItemId` 在 `remote_episode_ids` 中不存在的行，
/// 每个 Season 内 Episode 量通常 10~30 条，开销可忽略。
async fn delete_stale_episodes_for_season(
    pool: &sqlx::PgPool,
    season_db_id: Uuid,
    source_id: Uuid,
    remote_episode_ids: &HashSet<String>,
    strm_workspace: &Path,
) -> Result<(), AppError> {
    let source_id_str = source_id.to_string();
    struct EpRow {
        id: Uuid,
        path: Option<String>,
        remote_id: Option<String>,
    }
    let rows: Vec<EpRow> = sqlx::query(
        r#"
        SELECT id, path, provider_ids->>'RemoteEmbyItemId' AS remote_id
        FROM media_items
        WHERE parent_id = $1
          AND item_type = 'Episode'
          AND provider_ids->>'RemoteEmbySourceId' = $2
          AND provider_ids->>'RemoteEmbyItemId' IS NOT NULL
          AND provider_ids->>'RemoteEmbyItemId' <> ''
        "#,
    )
    .bind(season_db_id)
    .bind(&source_id_str)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        use sqlx::Row;
        EpRow {
            id: row.get("id"),
            path: row.get("path"),
            remote_id: row.get("remote_id"),
        }
    })
    .collect();

    let mut to_delete_ids: Vec<Uuid> = Vec::new();
    let mut strm_files_to_remove: Vec<PathBuf> = Vec::new();
    for row in rows {
        let Some(remote_id) = row.remote_id else { continue };
        if remote_id.trim().is_empty() || remote_episode_ids.contains(&remote_id) {
            continue;
        }
        to_delete_ids.push(row.id);
        if let Some(path_str) = row.path {
            let p = PathBuf::from(&path_str);
            if path_str.ends_with(".strm") && p.starts_with(strm_workspace) {
                strm_files_to_remove.push(p);
            }
        }
    }

    if to_delete_ids.is_empty() {
        return Ok(());
    }

    let deleted_count = to_delete_ids.len();

    // 先清 STRM 文件及同名旁路
    for strm_path in &strm_files_to_remove {
        if let Some(stem) = strm_path.file_stem().and_then(|s| s.to_str()) {
            if let Some(parent) = strm_path.parent() {
                if let Ok(mut entries) = tokio::fs::read_dir(parent).await {
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let fname = entry.file_name();
                        if fname.to_string_lossy().starts_with(stem) {
                            let _ = tokio::fs::remove_file(entry.path()).await;
                        }
                    }
                }
                let _ = tokio::fs::remove_dir(parent).await;
            }
        }
    }

    // 批量删 DB
    sqlx::query("DELETE FROM media_items WHERE id = ANY($1)")
        .bind(&to_delete_ids)
        .execute(pool)
        .await?;

    tracing::info!(
        source_id = %source_id,
        season_db_id = %season_db_id,
        deleted = deleted_count,
        "P8：Season 内 Episode 下架清理完成"
    );
    Ok(())
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

/// 判断是否为「Connection reset by peer」类型错误。
/// 远端 WAF/限流主动 RST 连接时 hyper 报的就是这类 I/O 错误（errno 104）。
fn is_connection_reset(err: &reqwest::Error) -> bool {
    let debug_repr = format!("{err:?}");
    debug_repr.contains("ConnectionReset")
        || debug_repr.contains("connection reset")
        || debug_repr.contains("reset by peer")
}

async fn get_json_with_retry<T: serde::de::DeserializeOwned>(
    pool: &sqlx::PgPool,
    source: &mut DbRemoteEmbySource,
    endpoint: &str,
    query: &[(String, String)],
) -> Result<T, AppError> {
    let client = &*crate::http_client::SHARED;
    let mut auth_retry_used = false;
    let mut identity_rotated = false;
    let mut last_error: Option<String> = None;

    // 重试循环：max_retries 次「网络/5xx 错误」退避重试 + 最多 1 次 401/403 续登重试
    // + 最多 1 次身份轮换后全量重试（连接 RST 场景）。
    let request_interval_ms = source.request_interval_ms;
    let source_id = source.id;
    // 身份轮换后 `continue 'identity` 重置 retry_count 重新跑一整轮
    'identity: loop {
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
        // per-source 并发限制：避免瞬间大量请求触发远端限流/WAF
        let _http_permit = acquire_http_permit(source_id).await;
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
                        is_timeout = err.is_timeout(),
                        is_connect = err.is_connect(),
                        is_request = err.is_request(),
                        is_body = err.is_body(),
                        error = %err,
                        error_debug = ?err,
                        "远端 Emby 网络错误，退避后重试"
                    );
                    last_error = Some(format!("{err:#}"));
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    continue;
                }
                // 连接被 RST 且尚未轮换过身份 → 自动更换 DeviceId 并重新认证再试一轮
                if !identity_rotated && is_connection_reset(&err) {
                    identity_rotated = true;
                    let new_device_id = Uuid::new_v4().simple().to_string();
                    tracing::warn!(
                        endpoint,
                        old_device_id = %source.effective_spoofed_device_id(),
                        new_device_id = %new_device_id,
                        "远端连接被重置，轮换设备标识后重试"
                    );
                    if let Err(e) = repository::rotate_remote_emby_source_device_id(
                        pool, source.id, &new_device_id,
                    ).await {
                        tracing::warn!(error = %e, "轮换设备标识写 DB 失败");
                    } else {
                        source.spoofed_device_id = new_device_id;
                        source.access_token = None;
                        source.remote_user_id = None;
                        auth_retry_used = false;
                        invalidate_playback_info_cache_for_source(source.id).await;
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue 'identity;
                    }
                }
                tracing::error!(
                    endpoint,
                    error_debug = ?err,
                    "远端 Emby 请求失败（不可重试或重试耗尽）"
                );
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
                        error_debug = ?err,
                        "远端 Emby 响应 body 读取失败，退避后重试"
                    );
                    last_error =
                        Some(format!("body read err (status={}): {err:#}", status.as_u16()));
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
    } // for retry_count
    break; // 正常退出 'identity 循环
    } // 'identity: loop

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
    // 先节流（不占 permit），再获取并发许可（占 permit 后立即发请求）
    throttle_remote_request(source.id, source.request_interval_ms).await;
    let _http_permit = acquire_http_permit(source.id).await;
    let resp = crate::http_client::SHARED
        .get(url)
        .header(header::USER_AGENT.as_str(), source.spoofed_user_agent.as_str())
        .header("X-Emby-Token", token)
        .header("X-Emby-Authorization", emby_auth_header(source, Some(token)))
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("下载远端资源失败: {e:#} url={url}")))?;
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
        if source.last_sync_at.is_none() {
            tracing::debug!(
                source_id = %source.id,
                source_name = %source.name,
                "自动增量同步：跳过未完成首次同步的源（请先手动触发首次同步）"
            );
            continue;
        }
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
