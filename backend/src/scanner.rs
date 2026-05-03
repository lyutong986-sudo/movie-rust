use crate::{
    config::Config,
    error::AppError,
    media_analyzer,
    metadata::{
        provider::{ExternalRemoteImage, MetadataProvider, MetadataProviderManager},
        tmdb::TmdbProvider,
    },
    models::{DbLibrary, LibraryOptionsDto, ScanSummary},
    naming,
    repository::{self, UpsertMediaItem},
    work_limiter::{WorkLimiterConfig, WorkLimiterKind, WorkLimiters},
};
use chrono::{DateTime, NaiveDate, Utc};
use regex::Regex;
use serde_json::{json, Map, Value};
use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, OnceLock,
    },
};
use tokio::{
    sync::{Mutex, RwLock},
    task::JoinSet,
};
use walkdir::WalkDir;

const TICKS_PER_SECOND: i64 = 10_000_000;

/// `item.added` 对齐 Sakura `media.py`：Episode 需要 **SeriesId** 才能给「收藏该剧」的用户发更新通知。
#[derive(Clone, Copy, Default)]
struct ItemAddedWebhookExtras<'a> {
    episode_series_id: Option<uuid::Uuid>,
    season_name: Option<&'a str>,
    index_number: Option<i32>,
}

/// scanner 入库后通知 `item.added` webhook。
///
/// scanner 流程从 `Movie / Series / Season / Episode` 任意一种新建条目时调用。
/// `was_new=false`（ON CONFLICT UPDATE）的情况会**跳过**——只有真正第一次入库才推送，
/// 避免增量扫描刷库时把"已有 item"也广播一遍。
fn notify_item_added(
    pool: &sqlx::PgPool,
    config: &Config,
    item_id: uuid::Uuid,
    was_new: bool,
    item_type: &str,
    name: &str,
    series_name: Option<&str>,
    extras: ItemAddedWebhookExtras<'_>,
) {
    if !was_new {
        return;
    }
    let mut item = json!({
        "Item": {
            "Id":         crate::models::uuid_to_emby_guid(&item_id),
            "Name":       name,
            "Type":       item_type,
            "SeriesName": series_name.unwrap_or(""),
        }
    });
    if item_type == "Episode" {
        if let Some(obj) = item
            .get_mut("Item")
            .and_then(|v| v.as_object_mut())
        {
            if let Some(sid) = extras.episode_series_id {
                obj.insert(
                    "SeriesId".to_string(),
                    json!(crate::models::uuid_to_emby_guid(&sid)),
                );
            }
            if let Some(sn) = extras.season_name {
                obj.insert("SeasonName".to_string(), json!(sn));
            }
            if let Some(ix) = extras.index_number {
                obj.insert("IndexNumber".to_string(), json!(ix));
            }
        }
    }
    crate::webhooks::dispatch_raw(
        pool.clone(),
        config.server_id,
        config.server_name.clone(),
        crate::webhooks::events::ITEM_ADDED.to_owned(),
        item,
    );
}

/// PB14：在扫描入口/出口派发 `library.scan.start` / `library.scan.complete`，与 Emby
/// Webhooks plugin 行为对齐。`libraries` 为本次任务覆盖的全部库（单库扫描时长度=1，
/// 全库扫描时为已枚举到的全部库）；payload 形如：
/// ```json
/// {"Library":[{"Id":"...","Name":"..."}, ...]}
/// ```
fn dispatch_library_scan_event(
    pool: &sqlx::PgPool,
    config: &Config,
    event: &'static str,
    libraries: &[DbLibrary],
) {
    let library_payload: Vec<Value> = libraries
        .iter()
        .map(|l| {
            json!({
                "Id":   crate::models::uuid_to_emby_guid(&l.id),
                "Name": l.name,
            })
        })
        .collect();
    crate::webhooks::dispatch_raw(
        pool.clone(),
        config.server_id,
        config.server_name.clone(),
        event.to_owned(),
        json!({
            "Library": library_payload,
        }),
    );
}

/// 扫描进度快照，用于前端实时展示
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScanProgressSnapshot {
    pub phase: String,
    pub current_library: Option<String>,
    pub total_files: u64,
    pub scanned_files: u64,
    pub imported_items: u64,
    pub percent: f64,
    /// PB49 (S1)：scanner 在 Phase B 入口前因「DB 已记录 + 远端 source 管理 + 文件
    /// 不更新」而短路跳过的文件数。让前端 UI 一眼看出本次本地兜底扫描真正干了多少活。
    #[serde(default)]
    pub skipped_remote_strm: u64,
}

/// 扫描进度句柄。通过原子计数器和 RwLock 在扫描过程中并发更新，
/// 然后由外部（任务注册表）定期 snapshot 读取。
#[derive(Clone, Default)]
pub struct ScanProgress {
    total_files: Arc<AtomicU64>,
    scanned_files: Arc<AtomicU64>,
    imported_items: Arc<AtomicU64>,
    skipped_remote_strm: Arc<AtomicU64>,
    phase: Arc<RwLock<String>>,
    current_library: Arc<RwLock<Option<String>>>,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_phase(&self, phase: impl Into<String>) {
        if let Ok(mut guard) = self.phase.try_write() {
            *guard = phase.into();
        } else {
            let phase = phase.into();
            let cell = self.phase.clone();
            tokio::spawn(async move {
                *cell.write().await = phase;
            });
        }
    }

    pub fn set_current_library(&self, name: Option<String>) {
        if let Ok(mut guard) = self.current_library.try_write() {
            *guard = name;
        } else {
            let cell = self.current_library.clone();
            tokio::spawn(async move {
                *cell.write().await = name;
            });
        }
    }

    pub fn add_total_files(&self, count: u64) {
        self.total_files.fetch_add(count, Ordering::Relaxed);
    }

    pub fn inc_scanned(&self) {
        self.scanned_files.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_imported(&self) {
        self.imported_items.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_skipped_remote_strm(&self) {
        self.skipped_remote_strm.fetch_add(1, Ordering::Relaxed);
    }

    pub async fn snapshot(&self) -> ScanProgressSnapshot {
        let total = self.total_files.load(Ordering::Relaxed);
        let scanned = self.scanned_files.load(Ordering::Relaxed);
        let imported = self.imported_items.load(Ordering::Relaxed);
        let skipped_remote_strm = self.skipped_remote_strm.load(Ordering::Relaxed);
        // 96% 主扫描 + 4% 后扫描，对齐 Jellyfin ValidateMediaLibraryInternal 的分配。
        let percent = if total == 0 {
            0.0
        } else {
            (scanned as f64 / total as f64 * 96.0).min(96.0)
        };
        let phase = self.phase.read().await.clone();
        let current_library = self.current_library.read().await.clone();
        ScanProgressSnapshot {
            phase,
            current_library,
            total_files: total,
            scanned_files: scanned,
            imported_items: imported,
            percent,
            skipped_remote_strm,
        }
    }

    pub fn mark_post_scan(&self, percent_within_post: f64) {
        // 后扫描阶段：96% + 4% 内按 percent_within_post（0..1）线性分配
        let pct = percent_within_post.clamp(0.0, 1.0);
        let total = 96.0 + pct * 4.0;
        self.scanned_files.store(
            ((total / 100.0) * self.total_files.load(Ordering::Relaxed) as f64) as u64,
            Ordering::Relaxed,
        );
    }
}

#[derive(Clone)]
struct ScanRuntime {
    work_limiters: WorkLimiters,
    refreshed_series: Arc<Mutex<HashSet<uuid::Uuid>>>,
    db_semaphore: Option<Arc<tokio::sync::Semaphore>>,
}

impl ScanRuntime {
    fn new(work_limiters: WorkLimiters) -> Self {
        Self {
            work_limiters,
            refreshed_series: Arc::new(Mutex::new(HashSet::new())),
            db_semaphore: None,
        }
    }

    fn with_db_semaphore(mut self, sem: Arc<tokio::sync::Semaphore>) -> Self {
        self.db_semaphore = Some(sem);
        self
    }

    async fn acquire(&self, kind: WorkLimiterKind) -> crate::work_limiter::WorkPermit {
        self.work_limiters.acquire(kind).await
    }

    async fn acquire_db_permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        match &self.db_semaphore {
            Some(sem) => Some(sem.clone().acquire_owned().await.ok()?),
            None => None,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct NfoMetadata {
    title: Option<String>,
    original_title: Option<String>,
    overview: Option<String>,
    production_year: Option<i32>,
    official_rating: Option<String>,
    community_rating: Option<f64>,
    critic_rating: Option<f64>,
    runtime_ticks: Option<i64>,
    premiere_date: Option<NaiveDate>,
    status: Option<String>,
    end_date: Option<NaiveDate>,
    air_days: Vec<String>,
    air_time: Option<String>,
    series_name: Option<String>,
    season_number: Option<i32>,
    episode_number: Option<i32>,
    episode_number_end: Option<i32>,
    provider_ids: Value,
    genres: Vec<String>,
    studios: Vec<String>,
    tags: Vec<String>,
    production_locations: Vec<String>,
    people: Vec<NfoPerson>,
    primary_image: Option<PathBuf>,
    /// NFO 中按顺序出现的 fanart/backdrop（可多张，首张对应 `backdrop_path`）
    backdrop_images: Vec<PathBuf>,
    logo_image: Option<PathBuf>,
    thumb_image: Option<PathBuf>,
    banner_image: Option<PathBuf>,
    disc_image: Option<PathBuf>,
    art_image: Option<PathBuf>,
    remote_trailers: Vec<String>,
}

#[derive(Debug, Default, Clone)]
struct NfoPerson {
    name: String,
    role_type: String,
    role: Option<String>,
    sort_order: i32,
    provider_ids: Value,
    primary_image: Option<PathBuf>,
}

#[allow(dead_code)]
pub async fn scan_all_libraries(
    pool: &sqlx::PgPool,
    metadata_manager: Option<Arc<MetadataProviderManager>>,
    config: &Config,
    work_limiters: WorkLimiters,
) -> Result<ScanSummary, AppError> {
    scan_all_libraries_with_db_semaphore(pool, metadata_manager, config, work_limiters, None, None).await
}

pub async fn scan_all_libraries_with_db_semaphore(
    pool: &sqlx::PgPool,
    metadata_manager: Option<Arc<MetadataProviderManager>>,
    config: &Config,
    work_limiters: WorkLimiters,
    progress: Option<ScanProgress>,
    db_semaphore: Option<Arc<tokio::sync::Semaphore>>,
) -> Result<ScanSummary, AppError> {
    let libraries = repository::list_libraries(pool).await?;
    scan_libraries(pool, metadata_manager, config, work_limiters, libraries, progress, db_semaphore).await
}

#[allow(dead_code)]
pub async fn scan_single_library_with_progress(
    pool: &sqlx::PgPool,
    metadata_manager: Option<Arc<MetadataProviderManager>>,
    config: &Config,
    work_limiters: WorkLimiters,
    library_id: uuid::Uuid,
    progress: Option<ScanProgress>,
) -> Result<ScanSummary, AppError> {
    scan_single_library_with_db_semaphore(pool, metadata_manager, config, work_limiters, library_id, progress, None).await
}

#[allow(dead_code)]
pub async fn scan_single_library(
    pool: &sqlx::PgPool,
    metadata_manager: Option<Arc<MetadataProviderManager>>,
    config: &Config,
    work_limiters: WorkLimiters,
    library_id: uuid::Uuid,
) -> Result<ScanSummary, AppError> {
    scan_single_library_with_db_semaphore(pool, metadata_manager, config, work_limiters, library_id, None, None).await
}

pub async fn scan_single_library_with_db_semaphore(
    pool: &sqlx::PgPool,
    metadata_manager: Option<Arc<MetadataProviderManager>>,
    config: &Config,
    work_limiters: WorkLimiters,
    library_id: uuid::Uuid,
    progress: Option<ScanProgress>,
    db_semaphore: Option<Arc<tokio::sync::Semaphore>>,
) -> Result<ScanSummary, AppError> {
    let library = repository::get_library(pool, library_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;
    scan_libraries(
        pool,
        metadata_manager,
        config,
        work_limiters,
        vec![library],
        progress,
        db_semaphore,
    )
    .await
}

async fn scan_libraries(
    pool: &sqlx::PgPool,
    metadata_manager: Option<Arc<MetadataProviderManager>>,
    config: &Config,
    work_limiters: WorkLimiters,
    libraries: Vec<DbLibrary>,
    progress: Option<ScanProgress>,
    db_semaphore: Option<Arc<tokio::sync::Semaphore>>,
) -> Result<ScanSummary, AppError> {
    // PB14：在扫描入口派发 library.scan.start。即使 libraries 为空也派发（让下游知道有
    // 一次扫描请求），与 Emby Webhooks plugin 的「扫描事件总是成对出现」语义一致。
    dispatch_library_scan_event(
        pool,
        config,
        crate::webhooks::events::LIBRARY_SCAN_START,
        &libraries,
    );
    let startup = repository::startup_configuration(pool, config).await?;
    let limits = WorkLimiterConfig {
        library_scan_limit: startup.library_scan_thread_count.max(1) as u32,
        media_analysis_limit: startup.strm_analysis_thread_count.max(1) as u32,
        tmdb_metadata_limit: startup.tmdb_metadata_thread_count.max(1) as u32,
    };
    work_limiters.configure(limits).await;
    let runtime = match db_semaphore {
        Some(sem) => ScanRuntime::new(work_limiters).with_db_semaphore(sem),
        None => ScanRuntime::new(work_limiters),
    };
    let mut scanned_files = 0_i64;
    let mut imported_items = 0_i64;

    // ---- Phase A：收集文件（Jellyfin 的 ValidateTopLibraryFolders 等价物）
    if let Some(p) = &progress {
        p.set_phase("CollectingFiles");
    }

    let mut library_files: Vec<(DbLibrary, LibraryOptionsDto, PathBuf, Vec<PathBuf>)> = Vec::new();
    for library in &libraries {
        let library_options = repository::library_options(library);
        if let Some(p) = &progress {
            p.set_current_library(Some(library.name.clone()));
        }
        let scan_paths = repository::library_scan_paths_union_remote_strm(pool, library).await?;
        for library_path in scan_paths {
            let path = PathBuf::from(&library_path);
            if !path.exists() {
                tracing::warn!("媒体库路径不存在: {}", library_path);
                continue;
            }
            let files = collect_video_files(path.clone()).await?;
            if let Some(p) = &progress {
                p.add_total_files(files.len() as u64);
            }
            scanned_files += files.len() as i64;
            library_files.push((library.clone(), library_options.clone(), path, files));
        }
    }

    // ---- Phase A.5（PB49 S1）：远端 STRM 短路过滤
    //
    // 远端 sync 刚刚把 N 万个 STRM 文件 + 对应 media_items 行写完后，scanner 默认会
    // 把同一个 STRM 工作区目录扫一遍——而过去对每个 STRM 都要走完整 import 链路
    // (read NFO + 3× upsert_media_item + TMDB/OpenSubtitles HTTP + analyze + trickplay
    // 路径检查)，对 25 万条目可能多花 30-60 分钟纯重复劳动。
    //
    // 这里在 Phase B 入口前，把所有收集到的 file path 一次性丢给 DB 反查：
    //   - 如果 path 在 media_items 里 + 由远端 source 管理（provider_ids 含 RemoteEmbySourceId）
    //     + 文件 mtime <= DB updated_at（DB 比磁盘新或同步），就直接 skip。
    //   - 否则（首次扫描的本地文件 / 用户手动改过 STRM token / 物理文件被替换 等）落到
    //     原 import 链路。
    //
    // 计数器：跳过的也照常 inc_scanned（让 UI 上的「已扫文件」看到真正的总数），
    // 但 inc_imported 只算真正入库 / 更新的，再单独打一个 skipped_remote_strm 给 UI。
    let total_collected: usize = library_files.iter().map(|(_, _, _, files)| files.len()).sum();
    let remote_managed_paths = if total_collected > 0 {
        let all_paths: Vec<String> = library_files
            .iter()
            .flat_map(|(_, _, _, files)| files.iter().map(|p| p.to_string_lossy().into_owned()))
            .collect();
        match repository::lookup_remote_managed_paths(pool, &all_paths).await {
            Ok(map) => {
                if !map.is_empty() {
                    tracing::info!(
                        candidate_files = total_collected,
                        remote_managed_in_db = map.len(),
                        "PB49 (S1)：scanner 短路预查命中数（满足 mtime <= updated_at 的部分将跳过）"
                    );
                }
                map
            }
            Err(error) => {
                tracing::warn!(
                    error = %error,
                    "PB49 (S1)：scanner 短路预查失败，回退为完整扫描（不影响正确性）"
                );
                std::collections::HashMap::new()
            }
        }
    } else {
        std::collections::HashMap::new()
    };

    let mut skipped_remote_strm: i64 = 0;

    // ---- Phase B：入库（Jellyfin 的 RootFolder.ValidateChildren 等价物）
    // 跨库并行：所有库的文件统一进入 JoinSet，由 work_limiter 控制并发上限。
    if let Some(p) = &progress {
        p.set_phase("Importing");
    }

    let mut tasks = JoinSet::new();
    for (library, library_options, path, files) in library_files {
        for file in files {
            // PB49 (S1)：在 spawn 之前先做短路检查。命中条件：
            //   1. 该 path 出现在 lookup_remote_managed_paths 返回集合中
            //   2. 文件 mtime <= DB updated_at（说明 DB 是最新或同步的）
            // 任意一条不满足就走原 import 流程。
            let file_path_str = file.to_string_lossy().into_owned();
            if let Some(db_updated_at) = remote_managed_paths.get(&file_path_str) {
                let mtime_ok = match tokio::fs::metadata(&file).await {
                    Ok(meta) => match meta.modified() {
                        Ok(sys_time) => {
                            let mtime: DateTime<Utc> = sys_time.into();
                            mtime <= *db_updated_at
                        }
                        // 拿不到 mtime（极少见，比如某些 FUSE 文件系统），保守地按
                        // 「不能确认最新」处理 → 走完整 import。
                        Err(_) => false,
                    },
                    Err(_) => false,
                };
                if mtime_ok {
                    skipped_remote_strm += 1;
                    if let Some(p) = &progress {
                        p.inc_scanned();
                        p.inc_skipped_remote_strm();
                    }
                    continue;
                }
            }
            while tasks.len() >= limits.library_scan_limit as usize {
                match tasks.join_next().await {
                    Some(joined) => match joined {
                        Ok(Ok(())) => {
                            imported_items += 1;
                            if let Some(p) = &progress {
                                p.inc_scanned();
                                p.inc_imported();
                            }
                        }
                        Ok(Err(error)) => {
                            tracing::error!("文件扫描失败（跳过继续）: {error}");
                            if let Some(p) = &progress {
                                p.inc_scanned();
                            }
                        }
                        Err(error) => {
                            tracing::error!("扫描任务 panic: {error}");
                            if let Some(p) = &progress {
                                p.inc_scanned();
                            }
                        }
                    },
                    None => break,
                }
            }

            let pool = pool.clone();
            let metadata_manager = metadata_manager.clone();
            let config = config.clone();
            let runtime = runtime.clone();
            let library = library.clone();
            let library_options = library_options.clone();
            let path = path.clone();

            tasks.spawn(async move {
                let _scan_permit = runtime.acquire(WorkLimiterKind::LibraryScan).await;
                let _db_permit = runtime.acquire_db_permit().await;
                if library.collection_type.eq_ignore_ascii_case("tvshows") {
                    import_tv_file(
                        &pool,
                        metadata_manager.as_deref(),
                        &config,
                        &runtime,
                        &library,
                        &library_options,
                        &path,
                        &file,
                    )
                    .await?;
                } else {
                    import_movie_file(
                        &pool,
                        metadata_manager.as_deref(),
                        &config,
                        &runtime,
                        &library,
                        &library_options,
                        &file,
                    )
                    .await?;
                }

                Ok::<(), AppError>(())
            });
        }
    }

    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok(Ok(())) => {
                imported_items += 1;
                if let Some(p) = &progress {
                    p.inc_scanned();
                    p.inc_imported();
                }
            }
            Ok(Err(error)) => {
                tracing::error!("文件扫描失败（跳过继续）: {error}");
                if let Some(p) = &progress {
                    p.inc_scanned();
                }
            }
            Err(error) => {
                tracing::error!("扫描任务 panic: {error}");
                if let Some(p) = &progress {
                    p.inc_scanned();
                }
            }
        }
    }

    // ---- Phase C：后扫描（对齐 Jellyfin RunPostScanTasks 的 4%）
    if let Some(p) = &progress {
        p.set_phase("PostProcessing");
        p.set_current_library(None);
    }

    // Trickplay + MediaSegments 后台生成
    let pool_post = pool.clone();
    let ffmpeg = config.ffmpeg_path.clone();
    tokio::spawn(async move {
        // Trickplay: 查找没有 trickplay 的视频项
        let items_without_trickplay: Vec<(uuid::Uuid, String)> = sqlx::query_as(
            r#"SELECT mi.id, mi.path FROM media_items mi
               WHERE mi.item_type IN ('Movie', 'Episode')
                 AND mi.path != ''
                 AND NOT EXISTS (SELECT 1 FROM trickplay_info ti WHERE ti.item_id = mi.id)
               LIMIT 50"#,
        )
        .fetch_all(&pool_post)
        .await
        .unwrap_or_default();

        for (item_id, path) in &items_without_trickplay {
            if path.starts_with("http://") || path.starts_with("https://") || path.ends_with(".strm") {
                continue;
            }
            if let Err(e) = crate::routes::trickplay::generate_trickplay(
                &pool_post, *item_id, path, &ffmpeg,
            ).await {
                tracing::warn!("Trickplay 生成失败 ({}): {e}", item_id);
            }
        }

        // MediaSegments: 检测没有分段的剧集片头/片尾
        let items_without_segments: Vec<(uuid::Uuid, String, Option<i64>)> = sqlx::query_as(
            r#"SELECT mi.id, mi.path, mi.runtime_ticks FROM media_items mi
               WHERE mi.item_type = 'Episode'
                 AND mi.path != ''
                 AND mi.runtime_ticks IS NOT NULL
                 AND NOT EXISTS (SELECT 1 FROM media_segments ms WHERE ms.item_id = mi.id)
               LIMIT 50"#,
        )
        .fetch_all(&pool_post)
        .await
        .unwrap_or_default();

        for (item_id, path, runtime) in items_without_segments {
            if path.starts_with("http://") || path.starts_with("https://") || path.ends_with(".strm") {
                continue;
            }
            let Some(runtime_ticks) = runtime else { continue };
            if let Some(segments) = crate::routes::media_segments::detect_segments(
                &ffmpeg, &path, runtime_ticks,
            ).await {
                if !segments.is_empty() {
                    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
                        "INSERT INTO media_segments (item_id, segment_type, start_ticks, end_ticks) "
                    );
                    qb.push_values(&segments, |mut b, (seg_type, start, end)| {
                        b.push_bind(item_id)
                            .push_bind(seg_type)
                            .push_bind(*start)
                            .push_bind(*end);
                    });
                    let _ = qb.build().execute(&pool_post).await;
                }
            }
        }
    });

    if let Some(p) = &progress {
        p.mark_post_scan(1.0);
    }

    // PB14：扫描出口派发 library.scan.complete。注意 Phase C 的后扫描任务（trickplay 等）
    // 是 spawn 出去的后台任务，不阻塞主流程；与 Emby plugin 一致——scan.complete 在主流
    // 程入库结束时即派发，不等待延迟生成的衍生资产。
    dispatch_library_scan_event(
        pool,
        config,
        crate::webhooks::events::LIBRARY_SCAN_COMPLETE,
        &libraries,
    );

    if skipped_remote_strm > 0 {
        tracing::info!(
            skipped_remote_strm,
            scanned_files,
            imported_items,
            "PB49 (S1)：scanner 完成，远端 STRM 短路跳过 / 总扫 / 入库 数据"
        );
    }

    Ok(ScanSummary {
        libraries: libraries.len() as i64,
        scanned_files,
        imported_items,
    })
}

async fn collect_video_files(root: PathBuf) -> Result<Vec<PathBuf>, AppError> {
    tokio::task::spawn_blocking(move || {
        let mut files = Vec::new();

        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if naming::is_video(entry.path()) {
                files.push(entry.path().to_path_buf());
            }
        }

        files
    })
    .await
    .map_err(|error| AppError::Internal(format!("扫描任务失败: {error}")))
}

/// 合并 NFO 与片夹中的额外壁纸路径（排除主 `backdrop_path`），保持顺序并去重。
fn merge_extra_backdrop_paths(
    primary: &Option<PathBuf>,
    extras: impl Iterator<Item = PathBuf>,
) -> Vec<String> {
    let primary_s = primary.as_ref().map(|p| p.to_string_lossy().into_owned());
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for p in extras {
        let s = p.to_string_lossy().into_owned();
        if primary_s.as_ref() == Some(&s) {
            continue;
        }
        if seen.insert(s.clone()) {
            out.push(s);
        }
    }
    out
}

async fn import_movie_file(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    config: &Config,
    runtime: &ScanRuntime,
    library: &DbLibrary,
    library_options: &LibraryOptionsDto,
    file: &Path,
) -> Result<(), AppError> {
    let parsed = naming::parse_media_path(file);
    let nfo = read_movie_nfo(file).unwrap_or_default();
    let container = file
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase);
    let movie_folder = file.parent().unwrap_or_else(|| Path::new("."));
    let folder_imgs = naming::discover_folder_images(movie_folder);
    let poster = nfo
        .primary_image
        .clone()
        .or_else(|| naming::find_sidecar_image(file))
        .or_else(|| folder_imgs.primary.clone());
    let backdrop = nfo
        .backdrop_images
        .first()
        .cloned()
        .or_else(|| folder_imgs.backdrop.clone())
        .or_else(|| file.parent().and_then(naming::find_backdrop_image));
    let backdrop_paths: Vec<String> = merge_extra_backdrop_paths(
        &backdrop,
        nfo.backdrop_images
            .iter()
            .skip(1)
            .cloned()
            .chain(folder_imgs.backdrops_extra.clone().into_iter()),
    );
    let logo = nfo
        .logo_image
        .clone()
        .or_else(|| folder_imgs.logo.clone())
        .or_else(|| find_item_image(file, &["logo", "clearlogo"]));
    let thumb = nfo
        .thumb_image
        .clone()
        .or_else(|| folder_imgs.thumb.clone())
        .or_else(|| find_item_image(file, &["thumb", "landscape"]))
        .or_else(|| backdrop.clone());
    let banner = nfo
        .banner_image
        .clone()
        .or_else(|| folder_imgs.banner.clone());
    let disc = nfo.disc_image.clone().or_else(|| folder_imgs.disc.clone());
    let art = nfo.art_image.clone().or_else(|| folder_imgs.art.clone());
    let name = nfo.title.as_deref().unwrap_or(&parsed.title);
    let provider_ids = merge_provider_ids(nfo.provider_ids.clone(), provider_ids_from_path(file));

    let (movie_id, movie_was_new) = repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: None,
            name,
            item_type: "Movie",
            media_type: "Video",
            path: file,
            container: container.as_deref(),
            original_title: nfo.original_title.as_deref(),
            overview: nfo.overview.as_deref(),
            production_year: nfo.production_year.or(parsed.production_year),
            official_rating: nfo.official_rating.as_deref(),
            community_rating: nfo.community_rating,
            critic_rating: nfo.critic_rating,
            runtime_ticks: nfo.runtime_ticks,
            premiere_date: nfo.premiere_date.or(parsed.premiere_date),
            status: nfo.status.as_deref(),
            end_date: nfo.end_date,
            air_days: &nfo.air_days,
            air_time: nfo.air_time.as_deref(),
            provider_ids: provider_ids.clone(),
            genres: &nfo.genres,
            studios: &nfo.studios,
            tags: &nfo.tags,
            production_locations: &nfo.production_locations,
            image_primary_path: poster.as_deref(),
            backdrop_path: backdrop.as_deref(),
            logo_path: logo.as_deref(),
            thumb_path: thumb.as_deref(),
            art_path: art.as_deref(),
            banner_path: banner.as_deref(),
            disc_path: disc.as_deref(),
            backdrop_paths: &backdrop_paths,
            remote_trailers: &nfo.remote_trailers,
            series_name: None,
            season_name: None,
            index_number: None,
            index_number_end: None,
            parent_index_number: None,
            width: parsed.width,
            height: parsed.height,
            video_codec: parsed.video_codec.as_deref(),
            audio_codec: parsed.audio_codec.as_deref(),
            series_id: None,
            force_overwrite_images: false,
        },
    )
    .await?;
    notify_item_added(
        pool,
        config,
        movie_id,
        movie_was_new,
        "Movie",
        name,
        None,
        ItemAddedWebhookExtras::default(),
    );
    sync_nfo_people(pool, movie_id, &nfo.people).await?;
    if library_options.enable_internet_providers {
        refresh_remote_people(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            movie_id,
            "movie",
            &provider_ids,
        )
        .await;
        refresh_movie_remote_metadata(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            movie_id,
            &provider_ids,
        )
        .await;
    }
    if library_options.enable_internet_providers && library_options.download_images_in_advance {
        cache_remote_images_for_item(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            movie_id,
            file,
            Some(file.parent().unwrap_or(file)),
            "Movie",
        )
        .await;
    }
    analyze_imported_media(pool, runtime, movie_id, file).await?;

    if library_options.enable_chapter_image_extraction {
        extract_chapter_images(pool, config, movie_id, file).await;
    }

    Ok(())
}

async fn import_tv_file(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    config: &Config,
    runtime: &ScanRuntime,
    library: &DbLibrary,
    library_options: &LibraryOptionsDto,
    library_root: &Path,
    file: &Path,
) -> Result<(), AppError> {
    let parsed = naming::parse_media_path(file);
    let episode_nfo = read_episode_nfo(file).unwrap_or_default();

    let preliminary_series_name = episode_nfo
        .series_name
        .as_deref()
        .or(parsed.series_name.as_deref());
    let preliminary_series_name = series_name_for_file(file, preliminary_series_name);
    let preliminary_series_path = series_virtual_path(library_root, file, &preliminary_series_name);
    // Jellyfin/Kodi：从剧集文件所在目录向上查找最近的 tvshow.nfo（剧集根目录可能在任意层级），
    // 不能仅依赖 series_virtual_path（与真实文件夹不一致时会漏掉整剧元数据）。
    let walked_tvshow = read_tvshow_nfo_walking_up(library_root, file);
    let series_nfo = walked_tvshow
        .as_ref()
        .map(|(_, meta)| meta.clone())
        .or_else(|| read_nfo_file(&preliminary_series_path.join("tvshow.nfo")))
        .or_else(|| read_nfo_file(&preliminary_series_path.join("series.nfo")))
        .unwrap_or_default();

    let series_name = series_nfo
        .title
        .as_deref()
        .or(episode_nfo.series_name.as_deref())
        .or(parsed.series_name.as_deref())
        .map(ToOwned::to_owned)
        .unwrap_or(preliminary_series_name);
    // 若磁盘上已找到 tvshow.nfo，剧集根路径以该目录为准。
    let mut series_path = walked_tvshow
        .map(|(dir, _)| dir)
        .unwrap_or_else(|| series_virtual_path(library_root, file, &series_name));

    // EnableAutomaticSeriesGrouping: 跨目录同名 Series 合并
    if library_options.enable_automatic_series_grouping {
        if let Ok(existing) = repository::find_series_by_name_in_library(
            pool, library.id, &series_name,
        ).await {
            if let Some(existing_series) = existing {
                series_path = PathBuf::from(&existing_series.path);
            }
        }
    }
    let series_provider_ids = merge_provider_ids(
        series_nfo.provider_ids.clone(),
        provider_ids_from_path(&series_path),
    );
    let episode_provider_ids = merge_provider_ids(
        episode_nfo.provider_ids.clone(),
        provider_ids_from_path(file),
    );

    let season_number = episode_nfo
        .season_number
        .or(parsed.season_number)
        .or_else(|| season_number_from_file(file))
        .unwrap_or(1);
    let season_path = season_virtual_path(library_root, file, &series_path, season_number);
    // 优先读取磁盘上季文件夹内的 season.nfo（季目录名可能是 Season 05 等与虚拟路径不一致）。
    let season_nfo = file
        .parent()
        .and_then(|p| read_nfo_file(&p.join("season.nfo")))
        .or_else(|| read_nfo_file(&season_path.join("season.nfo")))
        .unwrap_or_default();
    let season_name = season_nfo.title.clone().unwrap_or_else(|| {
        if season_number == 0 {
            "Specials".to_string()
        } else {
            format!("Season {season_number}")
        }
    });

    let series_folder_imgs = naming::discover_folder_images(&series_path);
    let series_poster = series_nfo
        .primary_image
        .clone()
        .or_else(|| series_folder_imgs.primary.clone())
        .or_else(|| naming::find_folder_image(&series_path))
        .or_else(|| series_path.parent().and_then(naming::find_folder_image));
    let series_backdrop = series_nfo
        .backdrop_images
        .first()
        .cloned()
        .or_else(|| series_folder_imgs.backdrop.clone())
        .or_else(|| naming::find_backdrop_image(&series_path));
    let series_backdrop_paths: Vec<String> = merge_extra_backdrop_paths(
        &series_backdrop,
        series_nfo
            .backdrop_images
            .iter()
            .skip(1)
            .cloned()
            .chain(series_folder_imgs.backdrops_extra.clone().into_iter()),
    );
    let series_logo = series_nfo
        .logo_image
        .clone()
        .or_else(|| series_folder_imgs.logo.clone())
        .or_else(|| find_folder_art(&series_path, &["logo", "clearlogo"]));
    let series_thumb = series_nfo
        .thumb_image
        .clone()
        .or_else(|| series_folder_imgs.thumb.clone())
        .or_else(|| find_folder_art(&series_path, &["thumb", "landscape"]))
        .or_else(|| series_backdrop.clone());
    let series_banner = series_nfo
        .banner_image
        .clone()
        .or_else(|| series_folder_imgs.banner.clone());
    let series_disc_img = series_nfo
        .disc_image
        .clone()
        .or_else(|| series_folder_imgs.disc.clone());
    let series_art = series_nfo
        .art_image
        .clone()
        .or_else(|| series_folder_imgs.art.clone());
    let season_folder_imgs = naming::discover_folder_images(&season_path);
    let season_poster = season_nfo
        .primary_image
        .clone()
        .or_else(|| {
            find_season_art(
                &series_path,
                &season_path,
                season_number,
                &["poster", "folder"],
            )
        })
        .or_else(|| season_folder_imgs.primary.clone())
        .or_else(|| naming::find_folder_image(&season_path))
        .or_else(|| series_poster.clone());
    let season_backdrop = season_nfo
        .backdrop_images
        .first()
        .cloned()
        .or_else(|| {
            find_season_art(
                &series_path,
                &season_path,
                season_number,
                &["fanart", "backdrop", "background"],
            )
        })
        .or_else(|| season_folder_imgs.backdrop.clone())
        .or_else(|| series_backdrop.clone());
    let season_backdrop_paths: Vec<String> = merge_extra_backdrop_paths(
        &season_backdrop,
        season_nfo
            .backdrop_images
            .iter()
            .skip(1)
            .cloned()
            .chain(season_folder_imgs.backdrops_extra.clone().into_iter()),
    );
    let season_logo = season_nfo
        .logo_image
        .clone()
        .or_else(|| {
            find_season_art(
                &series_path,
                &season_path,
                season_number,
                &["logo", "clearlogo"],
            )
        })
        .or_else(|| season_folder_imgs.logo.clone())
        .or_else(|| series_logo.clone());
    let season_thumb = season_nfo
        .thumb_image
        .clone()
        .or_else(|| {
            find_season_art(
                &series_path,
                &season_path,
                season_number,
                &["landscape", "thumb"],
            )
        })
        .or_else(|| season_folder_imgs.thumb.clone())
        .or_else(|| series_thumb.clone());
    let season_banner = season_nfo
        .banner_image
        .clone()
        .or_else(|| season_folder_imgs.banner.clone())
        .or_else(|| series_banner.clone());
    let season_disc_img = season_nfo
        .disc_image
        .clone()
        .or_else(|| season_folder_imgs.disc.clone())
        .or_else(|| series_disc_img.clone());
    let season_art = season_nfo
        .art_image
        .clone()
        .or_else(|| season_folder_imgs.art.clone())
        .or_else(|| series_art.clone());

    let (series_id, series_was_new) = repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: None,
            name: &series_name,
            item_type: "Series",
            media_type: "Video",
            path: &series_path,
            container: None,
            original_title: series_nfo.original_title.as_deref(),
            overview: series_nfo.overview.as_deref(),
            production_year: series_nfo.production_year.or(parsed.production_year),
            official_rating: series_nfo.official_rating.as_deref(),
            community_rating: series_nfo.community_rating,
            critic_rating: series_nfo.critic_rating,
            runtime_ticks: None,
            premiere_date: series_nfo.premiere_date,
            status: series_nfo.status.as_deref(),
            end_date: series_nfo.end_date,
            air_days: &series_nfo.air_days,
            air_time: series_nfo.air_time.as_deref(),
            provider_ids: series_provider_ids.clone(),
            genres: &series_nfo.genres,
            studios: &series_nfo.studios,
            tags: &series_nfo.tags,
            production_locations: &series_nfo.production_locations,
            image_primary_path: series_poster.as_deref(),
            backdrop_path: series_backdrop.as_deref(),
            logo_path: series_logo.as_deref(),
            thumb_path: series_thumb.as_deref(),
            art_path: series_art.as_deref(),
            banner_path: series_banner.as_deref(),
            disc_path: series_disc_img.as_deref(),
            backdrop_paths: &series_backdrop_paths,
            remote_trailers: &series_nfo.remote_trailers,
            series_name: Some(&series_name),
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
    notify_item_added(
        pool,
        config,
        series_id,
        series_was_new,
        "Series",
        &series_name,
        Some(&series_name),
        ItemAddedWebhookExtras::default(),
    );
    sync_nfo_people(pool, series_id, &series_nfo.people).await?;
    if library_options.enable_internet_providers {
        refresh_remote_people(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            series_id,
            "tv",
            &series_provider_ids,
        )
        .await;
        refresh_series_remote_metadata(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            series_id,
            &series_provider_ids,
        )
        .await;
        refresh_series_episode_catalog(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            series_id,
            &series_provider_ids,
        )
        .await;
    }
    if library_options.enable_internet_providers && library_options.download_images_in_advance {
        cache_remote_images_for_item(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            series_id,
            &series_path,
            Some(&series_path),
            "Series",
        )
        .await;
    }

    let (season_id, season_was_new) = repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: Some(series_id),
            name: &season_name,
            item_type: "Season",
            media_type: "Video",
            path: &season_path,
            container: None,
            original_title: season_nfo.original_title.as_deref(),
            overview: season_nfo.overview.as_deref(),
            production_year: season_nfo.production_year.or(series_nfo.production_year),
            official_rating: season_nfo
                .official_rating
                .as_deref()
                .or(series_nfo.official_rating.as_deref()),
            community_rating: season_nfo.community_rating.or(series_nfo.community_rating),
            critic_rating: season_nfo.critic_rating.or(series_nfo.critic_rating),
            runtime_ticks: None,
            premiere_date: season_nfo.premiere_date,
            status: season_nfo
                .status
                .as_deref()
                .or(series_nfo.status.as_deref()),
            end_date: season_nfo.end_date.or(series_nfo.end_date),
            air_days: if season_nfo.air_days.is_empty() {
                &series_nfo.air_days
            } else {
                &season_nfo.air_days
            },
            air_time: season_nfo
                .air_time
                .as_deref()
                .or(series_nfo.air_time.as_deref()),
            provider_ids: if has_provider_ids(&season_nfo.provider_ids) {
                season_nfo.provider_ids.clone()
            } else {
                series_provider_ids.clone()
            },
            genres: if season_nfo.genres.is_empty() {
                &series_nfo.genres
            } else {
                &season_nfo.genres
            },
            studios: if season_nfo.studios.is_empty() {
                &series_nfo.studios
            } else {
                &season_nfo.studios
            },
            tags: if season_nfo.tags.is_empty() {
                &series_nfo.tags
            } else {
                &season_nfo.tags
            },
            production_locations: if season_nfo.production_locations.is_empty() {
                &series_nfo.production_locations
            } else {
                &season_nfo.production_locations
            },
            image_primary_path: season_poster.as_deref(),
            backdrop_path: season_backdrop.as_deref(),
            logo_path: season_logo.as_deref(),
            thumb_path: season_thumb.as_deref(),
            art_path: season_art.as_deref(),
            banner_path: season_banner.as_deref(),
            disc_path: season_disc_img.as_deref(),
            backdrop_paths: &season_backdrop_paths,
            remote_trailers: &season_nfo.remote_trailers,
            series_name: Some(&series_name),
            season_name: Some(&season_name),
            index_number: Some(season_number),
            index_number_end: None,
            parent_index_number: None,
            width: None,
            height: None,
            video_codec: None,
            audio_codec: None,
            series_id: Some(series_id),
            force_overwrite_images: false,
        },
    )
    .await?;
    notify_item_added(
        pool,
        config,
        season_id,
        season_was_new,
        "Season",
        &season_name,
        Some(&series_name),
        ItemAddedWebhookExtras::default(),
    );
    if library_options.enable_internet_providers && library_options.download_images_in_advance {
        cache_remote_images_for_item(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            season_id,
            &season_path,
            Some(&season_path),
            "Season",
        )
        .await;
    }

    let container = file
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase);
    let ep_folder = file.parent().unwrap_or_else(|| Path::new("."));
    let ep_folder_imgs = naming::discover_folder_images(ep_folder);
    let poster = episode_nfo
        .primary_image
        .clone()
        .or_else(|| naming::find_sidecar_image(file))
        .or_else(|| ep_folder_imgs.primary.clone())
        .or_else(|| season_poster.clone());
    let backdrop = episode_nfo
        .backdrop_images
        .first()
        .cloned()
        .or_else(|| find_item_image(file, &["fanart", "backdrop", "background"]))
        .or_else(|| ep_folder_imgs.backdrop.clone())
        .or_else(|| season_backdrop.clone());
    let episode_backdrop_paths: Vec<String> = merge_extra_backdrop_paths(
        &backdrop,
        episode_nfo
            .backdrop_images
            .iter()
            .skip(1)
            .cloned()
            .chain(ep_folder_imgs.backdrops_extra.clone().into_iter()),
    );
    let episode_logo = episode_nfo
        .logo_image
        .clone()
        .or_else(|| ep_folder_imgs.logo.clone())
        .or_else(|| find_item_image(file, &["logo", "clearlogo"]))
        .or_else(|| season_logo.clone());
    let episode_thumb = episode_nfo
        .thumb_image
        .clone()
        .or_else(|| ep_folder_imgs.thumb.clone())
        .or_else(|| find_item_image(file, &["thumb", "landscape"]))
        .or_else(|| season_thumb.clone())
        .or_else(|| backdrop.clone());
    let episode_banner = episode_nfo
        .banner_image
        .clone()
        .or_else(|| ep_folder_imgs.banner.clone())
        .or_else(|| season_banner.clone());
    let episode_disc = episode_nfo
        .disc_image
        .clone()
        .or_else(|| ep_folder_imgs.disc.clone())
        .or_else(|| season_disc_img.clone());
    let episode_art = episode_nfo
        .art_image
        .clone()
        .or_else(|| ep_folder_imgs.art.clone())
        .or_else(|| season_art.clone());
    let episode_name = episode_nfo.title.as_deref().unwrap_or(&parsed.title);
    let episode_number = episode_nfo
        .episode_number
        .or(parsed.episode_number)
        .or_else(|| episode_number_from_file(file));

    let (episode_id, episode_was_new) = repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: Some(season_id),
            name: episode_name,
            item_type: "Episode",
            media_type: "Video",
            path: file,
            container: container.as_deref(),
            original_title: episode_nfo.original_title.as_deref(),
            overview: episode_nfo.overview.as_deref(),
            production_year: episode_nfo
                .production_year
                .or(parsed.production_year)
                .or(series_nfo.production_year),
            official_rating: episode_nfo
                .official_rating
                .as_deref()
                .or(series_nfo.official_rating.as_deref()),
            community_rating: episode_nfo.community_rating.or(series_nfo.community_rating),
            critic_rating: episode_nfo.critic_rating.or(series_nfo.critic_rating),
            runtime_ticks: episode_nfo.runtime_ticks,
            premiere_date: episode_nfo.premiere_date.or(parsed.premiere_date),
            status: episode_nfo
                .status
                .as_deref()
                .or(series_nfo.status.as_deref()),
            end_date: episode_nfo.end_date.or(series_nfo.end_date),
            air_days: if episode_nfo.air_days.is_empty() {
                &series_nfo.air_days
            } else {
                &episode_nfo.air_days
            },
            air_time: episode_nfo
                .air_time
                .as_deref()
                .or(series_nfo.air_time.as_deref()),
            provider_ids: if has_provider_ids(&episode_provider_ids) {
                episode_provider_ids
            } else {
                series_provider_ids
            },
            genres: if episode_nfo.genres.is_empty() {
                &series_nfo.genres
            } else {
                &episode_nfo.genres
            },
            studios: if episode_nfo.studios.is_empty() {
                &series_nfo.studios
            } else {
                &episode_nfo.studios
            },
            tags: if episode_nfo.tags.is_empty() {
                &series_nfo.tags
            } else {
                &episode_nfo.tags
            },
            production_locations: if episode_nfo.production_locations.is_empty() {
                &series_nfo.production_locations
            } else {
                &episode_nfo.production_locations
            },
            image_primary_path: poster.as_deref(),
            backdrop_path: backdrop.as_deref(),
            logo_path: episode_logo.as_deref(),
            thumb_path: episode_thumb.as_deref(),
            art_path: episode_art.as_deref(),
            banner_path: episode_banner.as_deref(),
            disc_path: episode_disc.as_deref(),
            backdrop_paths: &episode_backdrop_paths,
            remote_trailers: if episode_nfo.remote_trailers.is_empty() {
                &series_nfo.remote_trailers
            } else {
                &episode_nfo.remote_trailers
            },
            series_name: Some(&series_name),
            season_name: Some(&season_name),
            index_number: episode_number,
            index_number_end: episode_nfo
                .episode_number_end
                .or(parsed.ending_episode_number),
            parent_index_number: Some(season_number),
            width: parsed.width,
            height: parsed.height,
            video_codec: parsed.video_codec.as_deref(),
            audio_codec: parsed.audio_codec.as_deref(),
            series_id: Some(series_id),
            force_overwrite_images: false,
        },
    )
    .await?;
    notify_item_added(
        pool,
        config,
        episode_id,
        episode_was_new,
        "Episode",
        episode_name,
        Some(&series_name),
        ItemAddedWebhookExtras {
            episode_series_id: Some(series_id),
            season_name: Some(season_name.as_str()),
            index_number: episode_number,
        },
    );
    if library_options.enable_internet_providers && library_options.download_images_in_advance {
        cache_remote_images_for_item(
            pool,
            metadata_manager,
            config,
            runtime,
            library_options,
            episode_id,
            file,
            file.parent(),
            "Episode",
        )
        .await;
    }
    let episode_people = if episode_nfo.people.is_empty() {
        &series_nfo.people
    } else {
        &episode_nfo.people
    };
    sync_nfo_people(pool, episode_id, episode_people).await?;
    analyze_imported_media(pool, runtime, episode_id, file).await?;

    if library_options.enable_chapter_image_extraction {
        extract_chapter_images(pool, config, episode_id, file).await;
    }

    Ok(())
}

async fn refresh_series_remote_metadata(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    config: &Config,
    runtime: &ScanRuntime,
    library_options: &LibraryOptionsDto,
    series_id: uuid::Uuid,
    provider_ids: &Value,
) {
    {
        let mut refreshed_series = runtime.refreshed_series.lock().await;
        if !refreshed_series.insert(series_id) {
            return;
        }
    }

    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = tmdb_provider_for_library(metadata_manager, config, library_options)
    else {
        return;
    };
    let _permit = runtime.acquire(WorkLimiterKind::TmdbMetadata).await;
    match provider.get_series_details(&tmdb_id).await {
        Ok(metadata) => {
            if let Err(error) =
                repository::update_media_item_series_metadata(pool, series_id, &metadata).await
            {
                tracing::warn!(
                    series_id = %series_id,
                    tmdb_id = %tmdb_id,
                    error = %error,
                    "刷新远程剧集元数据落库失败"
                );
            }
        }
        Err(error) => {
            tracing::warn!(series_id = %series_id, tmdb_id = %tmdb_id, error = %error, "刷新远程剧集元数据失败");
        }
    }
}

async fn refresh_movie_remote_metadata(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    config: &Config,
    runtime: &ScanRuntime,
    library_options: &LibraryOptionsDto,
    movie_id: uuid::Uuid,
    provider_ids: &Value,
) {
    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = tmdb_provider_for_library(metadata_manager, config, library_options)
    else {
        return;
    };
    let _permit = runtime.acquire(WorkLimiterKind::TmdbMetadata).await;
    match provider.get_movie_details(&tmdb_id).await {
        Ok(metadata) => {
            if let Err(error) =
                repository::update_media_item_movie_metadata(pool, movie_id, &metadata).await
            {
                tracing::warn!(
                    movie_id = %movie_id,
                    tmdb_id = %tmdb_id,
                    error = %error,
                    "刷新远程电影元数据落库失败"
                );
            }
            // ImportCollections: 将电影加入 TMDb collection BoxSet
            if library_options.import_collections {
                if let Some(ref coll) = metadata.collection_info {
                    if let Err(error) = repository::upsert_movie_into_collection(
                        pool,
                        movie_id,
                        coll.tmdb_collection_id,
                        &coll.name,
                    )
                    .await
                    {
                        tracing::warn!(
                            movie_id = %movie_id,
                            collection = %coll.name,
                            error = %error,
                            "将电影加入合集失败"
                        );
                    }
                }
            }
        }
        Err(error) => {
            tracing::warn!(movie_id = %movie_id, tmdb_id = %tmdb_id, error = %error, "刷新远程电影元数据失败");
        }
    }
}

async fn refresh_series_episode_catalog(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    config: &Config,
    runtime: &ScanRuntime,
    library_options: &LibraryOptionsDto,
    series_id: uuid::Uuid,
    provider_ids: &Value,
) {
    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = tmdb_provider_for_library(metadata_manager, config, library_options)
    else {
        return;
    };
    let _permit = runtime.acquire(WorkLimiterKind::TmdbMetadata).await;
    match provider.get_series_episode_catalog(&tmdb_id).await {
        Ok(items) => {
            if let Err(error) =
                repository::replace_series_episode_catalog(pool, series_id, &items).await
            {
                tracing::warn!(
                    series_id = %series_id,
                    tmdb_id = %tmdb_id,
                    error = %error,
                    "同步远程剧集目录失败"
                );
            }
            if let Err(error) =
                repository::backfill_season_episode_metadata_from_catalog(pool, series_id).await
            {
                tracing::warn!(
                    series_id = %series_id,
                    ?error,
                    "从 catalog 回写 Season/Episode 元数据失败"
                );
            }
        }
        Err(error) => {
            tracing::warn!(series_id = %series_id, tmdb_id = %tmdb_id, error = %error, "获取远程剧集目录失败");
        }
    }
}

async fn refresh_remote_people(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    config: &Config,
    runtime: &ScanRuntime,
    library_options: &LibraryOptionsDto,
    media_item_id: uuid::Uuid,
    media_type: &str,
    provider_ids: &Value,
) {
    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = tmdb_provider_for_library(metadata_manager, config, library_options)
    else {
        return;
    };
    let _permit = runtime.acquire(WorkLimiterKind::TmdbMetadata).await;
    match provider.get_item_people(media_type, &tmdb_id).await {
        Ok(people) => {
            let tmdb_person_ids = people
                .iter()
                .filter_map(|person| {
                    person
                        .provider_ids
                        .get("Tmdb")
                        .or_else(|| person.provider_ids.get("TMDb"))
                        .or_else(|| person.provider_ids.get("tmdb"))
                        .cloned()
                })
                .collect::<Vec<_>>();
            if let Err(error) =
                repository::delete_tmdb_person_roles_except(pool, media_item_id, &tmdb_person_ids)
                    .await
            {
                tracing::warn!(
                    media_item_id = %media_item_id,
                    tmdb_id = %tmdb_id,
                    error = %error,
                    "清理过期 TMDb 人物角色失败"
                );
            }
            for person in people {
                let provider_ids = serde_json::to_value(&person.provider_ids).unwrap_or_default();
                match repository::upsert_person_reference(
                    pool,
                    &person.name,
                    provider_ids,
                    person.image_url.as_deref(),
                    person.external_url.as_deref(),
                )
                .await
                {
                    Ok(person_id) => {
                        if library_options.download_images_in_advance {
                            cache_person_image(
                                pool,
                                config,
                                runtime,
                                person_id,
                                person.image_url.as_deref(),
                                "Primary",
                            )
                            .await;
                        }
                        if let Err(error) = repository::upsert_person_role(
                            pool,
                            person_id,
                            media_item_id,
                            &person.role_type,
                            person.role.as_deref(),
                            person.sort_order,
                        )
                        .await
                        {
                            tracing::warn!(
                                media_item_id = %media_item_id,
                                tmdb_id = %tmdb_id,
                                person = %person.name,
                                error = %error,
                                "同步远程人物角色失败"
                            );
                        }
                    }
                    Err(error) => {
                        tracing::warn!(
                            media_item_id = %media_item_id,
                            tmdb_id = %tmdb_id,
                            person = %person.name,
                            error = %error,
                            "同步远程人物失败"
                        );
                    }
                }
            }
        }
        Err(error) => {
            tracing::warn!(media_item_id = %media_item_id, tmdb_id = %tmdb_id, error = %error, "获取远程人物信息失败");
        }
    }
}

async fn analyze_imported_media(
    pool: &sqlx::PgPool,
    runtime: &ScanRuntime,
    item_id: uuid::Uuid,
    file: &Path,
) -> Result<(), AppError> {
    if !file.exists() {
        return Ok(());
    }

    let analysis = if naming::is_strm(file) {
        let _permit = runtime.acquire(WorkLimiterKind::MediaAnalysis).await;
        match tokio::fs::read_to_string(file).await {
            Ok(content) => {
                let Some(target_url) = naming::strm_target_from_text(&content) else {
                    tracing::debug!(
                        "扫描阶段跳过 .strm 分析，未找到有效 URL: {}",
                        file.display()
                    );
                    return Ok(());
                };

                match media_analyzer::analyze_remote_media(&target_url).await {
                    Ok(analysis) => analysis,
                    Err(error) => {
                        tracing::warn!(
                            "扫描阶段分析远程 .strm 失败 file={} url={} error={}",
                            file.display(),
                            target_url,
                            error
                        );
                        return Ok(());
                    }
                }
            }
            Err(error) => {
                tracing::warn!(
                    "扫描阶段读取 .strm 文件失败 file={} error={}",
                    file.display(),
                    error
                );
                return Ok(());
            }
        }
    } else {
        let _permit = runtime.acquire(WorkLimiterKind::MediaAnalysis).await;
        match media_analyzer::analyze_media_file(file).await {
            Ok(analysis) => analysis,
            Err(error) => {
                tracing::warn!(
                    "扫描阶段分析媒体文件失败 file={} error={}",
                    file.display(),
                    error
                );
                return Ok(());
            }
        }
    };

    repository::update_media_item_metadata(pool, item_id, &analysis).await
}

fn tmdb_provider_for_library<'a>(
    metadata_manager: Option<&'a MetadataProviderManager>,
    config: &'a Config,
    library_options: &'a LibraryOptionsDto,
) -> Option<Box<dyn MetadataProvider + 'a>> {
    if let Some(api_key) = &config.tmdb_api_key {
        let preferred_metadata_language = library_options
            .preferred_metadata_language
            .as_deref()
            .unwrap_or(&config.preferred_metadata_language);
        let metadata_country_code = library_options
            .metadata_country_code
            .as_deref()
            .unwrap_or(&config.metadata_country_code);
        let provider = TmdbProvider::new_with_preferences(
            api_key.clone(),
            preferred_metadata_language,
            metadata_country_code,
        );
        return Some(Box::new(provider));
    }

    metadata_manager
        .and_then(|manager| manager.get_provider("tmdb"))
        .map(|provider| Box::new(ProviderRef { inner: provider }) as Box<dyn MetadataProvider>)
}

struct ProviderRef<'a> {
    inner: &'a dyn MetadataProvider,
}

#[async_trait::async_trait]
impl MetadataProvider for ProviderRef<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn search_person(
        &self,
        name: &str,
    ) -> Result<Vec<crate::metadata::models::ExternalPersonSearchResult>, AppError> {
        self.inner.search_person(name).await
    }

    async fn get_person_details(
        &self,
        provider_id: &str,
    ) -> Result<crate::metadata::models::ExternalPerson, AppError> {
        self.inner.get_person_details(provider_id).await
    }

    async fn get_person_credits(
        &self,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalPersonCredit>, AppError> {
        self.inner.get_person_credits(provider_id).await
    }

    async fn get_series_details(
        &self,
        provider_id: &str,
    ) -> Result<crate::metadata::models::ExternalSeriesMetadata, AppError> {
        self.inner.get_series_details(provider_id).await
    }

    async fn get_movie_details(
        &self,
        provider_id: &str,
    ) -> Result<crate::metadata::models::ExternalMovieMetadata, AppError> {
        self.inner.get_movie_details(provider_id).await
    }

    async fn get_item_people(
        &self,
        media_type: &str,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalItemPerson>, AppError> {
        self.inner.get_item_people(media_type, provider_id).await
    }

    async fn get_series_episode_catalog(
        &self,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalEpisodeCatalogItem>, AppError> {
        self.inner.get_series_episode_catalog(provider_id).await
    }

    async fn get_remote_images(
        &self,
        media_type: &str,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalRemoteImage>, AppError> {
        self.inner.get_remote_images(media_type, provider_id).await
    }

    async fn get_remote_images_for_child(
        &self,
        media_type: &str,
        series_provider_id: &str,
        season_number: Option<i32>,
        episode_number: Option<i32>,
    ) -> Result<Vec<crate::metadata::provider::ExternalRemoteImage>, AppError> {
        self.inner
            .get_remote_images_for_child(
                media_type,
                series_provider_id,
                season_number,
                episode_number,
            )
            .await
    }
}

async fn cache_remote_images_for_item(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    config: &Config,
    runtime: &ScanRuntime,
    library_options: &LibraryOptionsDto,
    item_id: uuid::Uuid,
    item_path: &Path,
    item_folder: Option<&Path>,
    _media_type: &str,
) {
    let Some(item) = repository::get_media_item(pool, item_id)
        .await
        .ok()
        .flatten()
    else {
        return;
    };
    let mut images: Vec<ExternalRemoteImage> = Vec::new();
    for (image_type, path) in [
        ("Primary", item.image_primary_path.as_deref()),
        ("Backdrop", item.backdrop_path.as_deref()),
        ("Logo", item.logo_path.as_deref()),
        ("Thumb", item.thumb_path.as_deref()),
        ("Banner", item.banner_path.as_deref()),
        ("Disc", item.disc_path.as_deref()),
        ("Art", item.art_path.as_deref()),
    ] {
        push_current_remote_image(&mut images, image_type, path);
    }
    for extra_backdrop in &item.backdrop_paths {
        push_current_remote_image(&mut images, "Backdrop", Some(extra_backdrop.as_str()));
    }

    if let Some((tmdb_id, season_number, episode_number, remote_media_type)) =
        tmdb_remote_image_context(pool, &item).await
    {
        if let Some(provider) = tmdb_provider_for_library(metadata_manager, config, library_options)
        {
            let _permit = runtime.acquire(WorkLimiterKind::TmdbMetadata).await;
            let remote_result = if season_number.is_some() || episode_number.is_some() {
                provider
                    .get_remote_images_for_child(
                        remote_media_type,
                        &tmdb_id,
                        season_number,
                        episode_number,
                    )
                    .await
            } else {
                provider
                    .get_remote_images(remote_media_type, &tmdb_id)
                    .await
            };
            if let Ok(mut remote_images) = remote_result {
                images.append(&mut remote_images);
            }
        }
    }
    if images.is_empty() {
        return;
    }

    let image_download_plan: [(&str, &[&str]); 7] = [
        ("Primary", &["Primary"]),
        ("Backdrop", &["Backdrop"]),
        ("Logo", &["Logo"]),
        ("Thumb", &["Thumb", "Backdrop", "Primary"]),
        // Fanart.tv 常见有 Banner / Art / Disc；缺失时回退 TMDB 的通用图。
        ("Banner", &["Banner", "Backdrop"]),
        ("Art", &["Art", "Logo", "Backdrop"]),
        ("Disc", &["Disc", "Primary"]),
    ];

    for (image_type, fallback_types) in image_download_plan {
        let Some(image) = pick_remote_image_with_fallback(&images, fallback_types) else {
            continue;
        };
        let local_path = if library_options.save_local_metadata {
            image_target_path(&item, item_path, item_folder, image_type)
        } else {
            cache_target_path(
                &config.static_dir.join("item-images"),
                &item_id.to_string(),
                image_type,
                image.url.as_str(),
            )
        };
        let Some(local_path) = local_path else {
            continue;
        };
        let _permit = runtime.acquire(WorkLimiterKind::TmdbMetadata).await;
        if download_image_to_path(&local_path, &image.url)
            .await
            .is_err()
        {
            continue;
        }
        let local_path_text = local_path.to_string_lossy().to_string();
        let _ = repository::update_media_item_image_path(
            pool,
            item_id,
            image_type,
            Some(&local_path_text),
            None,
        )
        .await;
    }
}

fn push_current_remote_image(
    images: &mut Vec<ExternalRemoteImage>,
    image_type: &str,
    path: Option<&str>,
) {
    let Some(path) = path.filter(|value| {
        let value = value.trim();
        value.starts_with("http://") || value.starts_with("https://")
    }) else {
        return;
    };
    images.push(ExternalRemoteImage {
        provider_name: "CurrentPath".to_string(),
        url: path.to_string(),
        thumbnail_url: Some(path.to_string()),
        image_type: image_type.to_string(),
        language: None,
        width: None,
        height: None,
        community_rating: None,
        vote_count: None,
    });
}

fn remote_image_provider_priority(image: &ExternalRemoteImage) -> i32 {
    let provider = image.provider_name.to_ascii_lowercase();
    let url = image.url.to_ascii_lowercase();
    if provider.contains("currentpath") {
        return -10;
    }
    if provider.contains("fanart") || url.contains("fanart.tv") {
        return 0;
    }
    if provider.contains("localmetadata") {
        return 5;
    }
    if provider.contains("tmdb")
        || provider.contains("themoviedb")
        || url.contains("image.tmdb.org")
    {
        return 10;
    }
    20
}

fn pick_remote_image_with_fallback<'a>(
    images: &'a [ExternalRemoteImage],
    fallback_types: &[&str],
) -> Option<&'a ExternalRemoteImage> {
    let mut best: Option<(usize, i32, f64, &ExternalRemoteImage)> = None;
    for image in images {
        let Some(type_rank) = fallback_types
            .iter()
            .position(|kind| image.image_type.eq_ignore_ascii_case(kind))
        else {
            continue;
        };
        let provider_rank = remote_image_provider_priority(image);
        let rating = image.community_rating.unwrap_or(-1.0);
        let should_replace =
            if let Some((best_type_rank, best_provider_rank, best_rating, _)) = best {
                type_rank < best_type_rank
                    || (type_rank == best_type_rank && provider_rank < best_provider_rank)
                    || (type_rank == best_type_rank
                        && provider_rank == best_provider_rank
                        && rating > best_rating)
            } else {
                true
            };
        if should_replace {
            best = Some((type_rank, provider_rank, rating, image));
        }
    }
    best.map(|(_, _, _, image)| image)
}

async fn cache_person_image(
    pool: &sqlx::PgPool,
    config: &Config,
    runtime: &ScanRuntime,
    person_id: uuid::Uuid,
    image_url: Option<&str>,
    image_type: &str,
) {
    let Some(image_url) = image_url.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    let Some(local_path) = cache_target_path(
        &config.static_dir.join("person-images"),
        &person_id.to_string(),
        image_type,
        image_url,
    ) else {
        return;
    };
    let _permit = runtime.acquire(WorkLimiterKind::TmdbMetadata).await;
    if download_image_to_path(&local_path, image_url)
        .await
        .is_err()
    {
        return;
    }
    let local_path_text = local_path.to_string_lossy().to_string();
    let _ =
        repository::update_person_image_path(pool, person_id, image_type, Some(&local_path_text))
            .await;
}

async fn tmdb_remote_image_context(
    pool: &sqlx::PgPool,
    item: &crate::models::DbMediaItem,
) -> Option<(String, Option<i32>, Option<i32>, &'static str)> {
    if item.item_type.eq_ignore_ascii_case("Movie") {
        return tmdb_id_from_provider_ids(&item.provider_ids).map(|id| (id, None, None, "Movie"));
    }

    if item.item_type.eq_ignore_ascii_case("Series") {
        return tmdb_id_from_provider_ids(&item.provider_ids).map(|id| (id, None, None, "Series"));
    }

    if item.item_type.eq_ignore_ascii_case("Season") {
        let tmdb_id = if let Some(id) = tmdb_id_from_provider_ids(&item.provider_ids) {
            Some(id)
        } else {
            let parent_id = item.parent_id?;
            let parent = repository::get_media_item(pool, parent_id)
                .await
                .ok()
                .flatten()?;
            tmdb_id_from_provider_ids(&parent.provider_ids)
        };
        return tmdb_id.map(|id| (id, item.index_number, None, "Season"));
    }

    if item.item_type.eq_ignore_ascii_case("Episode") {
        let season_number = item.parent_index_number;
        let episode_number = item.index_number;
        let tmdb_id = if let Some(parent_id) = item.parent_id {
            if let Some(season) = repository::get_media_item(pool, parent_id)
                .await
                .ok()
                .flatten()
            {
                if let Some(id) = tmdb_id_from_provider_ids(&season.provider_ids) {
                    Some(id)
                } else {
                    let series_id = season.parent_id?;
                    let series = repository::get_media_item(pool, series_id)
                        .await
                        .ok()
                        .flatten()?;
                    tmdb_id_from_provider_ids(&series.provider_ids)
                }
            } else {
                None
            }
        } else {
            None
        };
        return tmdb_id.map(|id| (id, season_number, episode_number, "Episode"));
    }

    None
}

fn image_target_path(
    item: &crate::models::DbMediaItem,
    item_path: &Path,
    item_folder: Option<&Path>,
    image_type: &str,
) -> Option<PathBuf> {
    let extension = "jpg";
    let folder = item_folder.or_else(|| item_path.parent())?;
    let stem = item_path.file_stem()?.to_string_lossy();
    let image_type = image_type.to_ascii_lowercase();

    if item.item_type.eq_ignore_ascii_case("Episode") {
        return match image_type.as_str() {
            "primary" | "thumb" => Some(folder.join(format!("{stem}-thumb.{extension}"))),
            "backdrop" => Some(folder.join(format!("{stem}-fanart.{extension}"))),
            "logo" => Some(folder.join(format!("{stem}-logo.{extension}"))),
            "banner" => Some(folder.join(format!("{stem}-banner.{extension}"))),
            "disc" => Some(folder.join(format!("{stem}-disc.{extension}"))),
            "art" => Some(folder.join(format!("{stem}-clearart.{extension}"))),
            _ => None,
        };
    }

    if item.item_type.eq_ignore_ascii_case("Season") {
        let season_number = item.index_number.unwrap_or_default();
        let marker = if season_number == 0 {
            "-specials".to_string()
        } else {
            format!("{season_number:02}")
        };
        let prefix = format!("season{marker}");
        return match image_type.as_str() {
            "primary" => Some(folder.join(format!("{prefix}-poster.{extension}"))),
            "backdrop" => Some(folder.join(format!("{prefix}-fanart.{extension}"))),
            "logo" => Some(folder.join(format!("{prefix}-logo.{extension}"))),
            "thumb" => Some(folder.join(format!("{prefix}-landscape.{extension}"))),
            "banner" => Some(folder.join(format!("{prefix}-banner.{extension}"))),
            "disc" => Some(folder.join(format!("{prefix}-disc.{extension}"))),
            "art" => Some(folder.join(format!("{prefix}-clearart.{extension}"))),
            _ => None,
        };
    }

    let filename = match image_type.as_str() {
        "primary" => "poster",
        "backdrop" => "fanart",
        "logo" => "logo",
        "thumb" => "landscape",
        "banner" => "banner",
        "disc" => "disc",
        "art" => "clearart",
        _ => return None,
    };

    if item_path.is_dir() {
        Some(folder.join(format!("{filename}.{extension}")))
    } else {
        Some(folder.join(format!("{stem}-{filename}.{extension}")))
    }
}

fn cache_target_path(dir: &Path, stem: &str, image_type: &str, image_url: &str) -> Option<PathBuf> {
    let extension = naming::extension_from_url(image_url)
        .filter(|ext| {
            naming::IMAGE_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        })
        .unwrap_or_else(|| "jpg".to_string());
    Some(dir.join(format!(
        "{}-{}.{}",
        stem,
        image_type.to_ascii_lowercase(),
        extension
    )))
}

async fn download_image_to_path(path: &Path, image_url: &str) -> Result<(), AppError> {
    let bytes = crate::http_client::download_image_bytes(image_url).await?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(AppError::Io)?;
    }
    tokio::fs::write(path, &bytes).await.map_err(AppError::Io)?;
    Ok(())
}

fn read_movie_nfo(file: &Path) -> Option<NfoMetadata> {
    let parent = file.parent()?;
    let stem = file.file_stem()?.to_string_lossy();
    for candidate in [parent.join(format!("{stem}.nfo")), parent.join("movie.nfo")] {
        if let Some(metadata) = read_nfo_file(&candidate) {
            return Some(metadata);
        }
    }

    None
}

/// 自视频文件父目录起向上查找 `tvshow.nfo` / `series.nfo`，直到到达媒体库根目录或路径耗尽。
/// 与 Jellyfin `LocalSeriesProvider` / Kodi 行为一致：剧集根可在 Season 之上任意层级。
/// 返回 `(包含 NFO 的目录, 解析结果)`，便于 `series_path` 与磁盘结构对齐。
fn read_tvshow_nfo_walking_up(library_root: &Path, file: &Path) -> Option<(PathBuf, NfoMetadata)> {
    let mut dir = file.parent()?;
    const MAX_DEPTH: usize = 48;
    for _ in 0..MAX_DEPTH {
        for name in ["tvshow.nfo", "series.nfo"] {
            let candidate = dir.join(name);
            if let Some(metadata) = read_nfo_file(&candidate) {
                return Some((dir.to_path_buf(), metadata));
            }
        }
        if dir == library_root {
            break;
        }
        dir = dir.parent()?;
    }
    None
}

fn read_episode_nfo(file: &Path) -> Option<NfoMetadata> {
    let parent = file.parent()?;
    let stem = file.file_stem()?.to_string_lossy();
    for candidate in [
        parent.join(format!("{stem}.nfo")),
        parent.join("episodedetails.nfo"),
        parent.join("episode.nfo"),
    ] {
        if let Some(metadata) = read_nfo_file(&candidate) {
            return Some(metadata);
        }
    }

    None
}

fn read_nfo_file(path: &Path) -> Option<NfoMetadata> {
    if !path.exists() {
        return None;
    }

    let xml = std::fs::read_to_string(path).ok()?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut metadata = NfoMetadata {
        title: first_tag(&xml, &["title", "localtitle", "name"]),
        original_title: first_tag(&xml, &["originaltitle", "original_title"]),
        overview: first_tag(
            &xml,
            &["plot", "outline", "review", "biography", "overview"],
        ),
        production_year: first_tag(&xml, &["year", "productionyear", "production_year"])
            .and_then(|value| parse_i32(&value)),
        official_rating: first_tag(&xml, &["mpaa", "certification", "officialrating"]),
        community_rating: first_tag(&xml, &["rating", "communityrating", "userrating"])
            .and_then(|value| parse_decimal(&value)),
        critic_rating: first_tag(&xml, &["criticrating", "critic_rating"])
            .and_then(|value| parse_decimal(&value)),
        runtime_ticks: first_tag(&xml, &["runtime", "duration"])
            .and_then(|value| parse_runtime_ticks(&value)),
        premiere_date: first_tag(&xml, &["premiered", "aired", "releasedate", "date"])
            .and_then(|value| parse_date(&value)),
        status: parse_series_status(&xml),
        end_date: first_tag(
            &xml,
            &["enddate", "end_date", "ended", "lastaired", "last_air_date"],
        )
        .and_then(|value| parse_date(&value)),
        air_days: parse_air_days(&xml),
        air_time: first_tag(&xml, &["airtime", "airs_time", "air_time"])
            .filter(|value| !value.trim().is_empty()),
        series_name: first_tag(&xml, &["showtitle", "tvshowtitle", "seriesname", "series"]),
        season_number: first_tag(
            &xml,
            &[
                "season",
                "seasonnumber",
                "parentindexnumber",
                "parent_index_number",
            ],
        )
        .and_then(|value| parse_i32(&value)),
        episode_number: first_tag(
            &xml,
            &["episode", "episodenumber", "indexnumber", "displayepisode"],
        )
        .and_then(|value| parse_i32(&value)),
        episode_number_end: first_tag(
            &xml,
            &[
                "episodenumberend",
                "episodeend",
                "indexnumberend",
                "displayepisodeend",
            ],
        )
        .and_then(|value| parse_i32(&value)),
        provider_ids: provider_ids_from_nfo(&xml),
        genres: repeated_tags(&xml, "genre")
            .into_iter()
            .flat_map(|value| {
                value
                    .split('/')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect(),
        studios: repeated_tags(&xml, "studio"),
        tags: repeated_tags(&xml, "tag"),
        production_locations: repeated_tags(&xml, "country"),
        people: nfo_people(&xml, parent),
        primary_image: None,
        backdrop_images: Vec::new(),
        logo_image: None,
        thumb_image: None,
        banner_image: None,
        disc_image: None,
        art_image: None,
        remote_trailers: remote_trailer_urls(&xml),
    };

    for image in nfo_images(&xml, parent) {
        match image.kind.as_deref() {
            Some("fanart") | Some("backdrop") | Some("background") => {
                if !metadata.backdrop_images.contains(&image.path) {
                    metadata.backdrop_images.push(image.path);
                }
            }
            Some("logo") | Some("clearlogo") => {
                metadata.logo_image.get_or_insert(image.path);
            }
            Some("thumb") | Some("landscape") => {
                metadata.thumb_image.get_or_insert(image.path);
            }
            Some("banner") => {
                metadata.banner_image.get_or_insert(image.path);
            }
            Some("discart") | Some("cdart") | Some("disc") => {
                metadata.disc_image.get_or_insert(image.path);
            }
            Some("clearart") | Some("characterart") | Some("hdclearart") | Some("character")
            | Some("art") => {
                metadata.art_image.get_or_insert(image.path);
            }
            Some("poster") => {
                metadata.primary_image.get_or_insert(image.path);
            }
            _ => {
                metadata.primary_image.get_or_insert(image.path);
            }
        }
    }

    Some(metadata)
}

fn first_tag(xml: &str, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| repeated_tags(xml, name).into_iter().next())
}

fn provider_ids_from_nfo(xml: &str) -> Value {
    let mut ids = Map::new();

    if let Some(value) = first_tag(xml, &["imdbid"]).filter(|value| !value.trim().is_empty()) {
        ids.insert("Imdb".to_string(), json!(value));
    }
    if let Some(value) = first_tag(xml, &["tmdbid"]).filter(|value| !value.trim().is_empty()) {
        ids.insert("Tmdb".to_string(), json!(value));
    }
    if let Ok(regex) = Regex::new(r#"(?is)<uniqueid\b([^>]*)>(.*?)</uniqueid>"#) {
        for captures in regex.captures_iter(xml) {
            let attrs = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let raw_id = captures
                .get(2)
                .map(|value| decode_xml_text(value.as_str()))
                .unwrap_or_default();
            let id = strip_xml_tags(&raw_id).trim().to_string();
            if id.is_empty() {
                continue;
            }

            let provider = attr_value(attrs, "type")
                .or_else(|| attr_value(attrs, "provider"))
                .map(|value| provider_key(&value))
                .unwrap_or_else(|| "Unknown".to_string());
            ids.entry(provider).or_insert(json!(id));
        }
    }

    Value::Object(ids)
}

fn parse_series_status(xml: &str) -> Option<String> {
    first_tag(xml, &["status", "seriesstatus"])
        .and_then(|value| normalize_series_status(&value))
        .or_else(|| {
            let ended = first_tag(xml, &["ended", "isended"]).map(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes" | "ended" | "completed"
                )
            });
            ended.and_then(|value| value.then(|| "Ended".to_string()))
        })
}

fn normalize_series_status(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let normalized = match value.to_ascii_lowercase().as_str() {
        "ended" | "completed" | "complete" | "canceled" | "cancelled" => "Ended",
        "continuing" | "returning series" | "returning" | "in production" | "running" => {
            "Continuing"
        }
        _ => value,
    };
    Some(normalized.to_string())
}

fn parse_air_days(xml: &str) -> Vec<String> {
    let mut days = Vec::new();
    for tag in [
        "airday",
        "airdays",
        "air_day",
        "air_days",
        "airsdayofweek",
        "airs_dayofweek",
    ] {
        for value in repeated_tags(xml, tag) {
            for part in value.split([',', '/', '|', ';']) {
                if let Some(day) = normalize_air_day(part) {
                    if !days.iter().any(|existing| existing == &day) {
                        days.push(day);
                    }
                }
            }
        }
    }
    days
}

fn normalize_air_day(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let day = match value.to_ascii_lowercase().as_str() {
        "0" | "sun" | "sunday" | "周日" | "星期日" | "星期天" => "Sunday",
        "1" | "mon" | "monday" | "周一" | "星期一" => "Monday",
        "2" | "tue" | "tues" | "tuesday" | "周二" | "星期二" => "Tuesday",
        "3" | "wed" | "wednesday" | "周三" | "星期三" => "Wednesday",
        "4" | "thu" | "thur" | "thurs" | "thursday" | "周四" | "星期四" => "Thursday",
        "5" | "fri" | "friday" | "周五" | "星期五" => "Friday",
        "6" | "sat" | "saturday" | "周六" | "星期六" => "Saturday",
        _ => return None,
    };
    Some(day.to_string())
}

fn has_provider_ids(value: &Value) -> bool {
    value.as_object().is_some_and(|object| !object.is_empty())
}

fn tmdb_id_from_provider_ids(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    ["Tmdb", "TMDb", "tmdb"].iter().find_map(|key| {
        object
            .get(*key)
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .or_else(|| {
                object
                    .get(*key)
                    .and_then(|value| value.as_i64().map(|id| id.to_string()))
            })
    })
}

fn provider_ids_from_path(path: &Path) -> Value {
    let text = path.to_string_lossy();
    let Ok(regex) = Regex::new(r"(?i)\{(tmdbid|imdbid|tvdbid|traktid)\s*=\s*([^}]+)\}") else {
        return json!({});
    };
    let mut ids = Map::new();

    for captures in regex.captures_iter(&text) {
        let Some(raw_provider) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        let Some(raw_id) = captures.get(2).map(|value| value.as_str().trim()) else {
            continue;
        };
        if raw_id.is_empty() {
            continue;
        }

        let provider = match raw_provider.to_ascii_lowercase().as_str() {
            "tmdbid" => "Tmdb",
            "imdbid" => "Imdb",
            "tvdbid" => "Tvdb",
            "traktid" => "Trakt",
            _ => continue,
        };
        ids.insert(provider.to_string(), json!(raw_id));
    }

    Value::Object(ids)
}

fn merge_provider_ids(primary: Value, fallback: Value) -> Value {
    let mut merged = fallback.as_object().cloned().unwrap_or_default();
    if let Some(primary) = primary.as_object() {
        for (key, value) in primary {
            if !value.is_null() {
                merged.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(merged)
}

fn repeated_tags(xml: &str, name: &str) -> Vec<String> {
    let pattern = format!(
        r"(?is)<{}\b[^>]*>(.*?)</{}>",
        regex::escape(name),
        regex::escape(name)
    );
    let Ok(regex) = Regex::new(&pattern) else {
        return Vec::new();
    };

    regex
        .captures_iter(xml)
        .filter_map(|captures| captures.get(1))
        .map(|value| decode_xml_text(value.as_str()))
        .map(|value| strip_xml_tags(&value))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn repeated_blocks(xml: &str, name: &str) -> Vec<String> {
    let pattern = format!(
        r"(?is)<{}\b[^>]*>(.*?)</{}>",
        regex::escape(name),
        regex::escape(name)
    );
    let Ok(regex) = Regex::new(&pattern) else {
        return Vec::new();
    };

    regex
        .captures_iter(xml)
        .filter_map(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
        .collect()
}

fn nfo_people(xml: &str, base_dir: &Path) -> Vec<NfoPerson> {
    let mut people = Vec::new();

    for block in repeated_blocks(xml, "actor") {
        let Some(name) = first_tag(&block, &["name"]).filter(|value| !value.trim().is_empty())
        else {
            continue;
        };
        let role = first_tag(&block, &["role"]);
        let sort_order = first_tag(&block, &["order"])
            .and_then(|value| value.parse().ok())
            .unwrap_or(people.len() as i32);
        let primary_image = first_tag(&block, &["thumb"])
            .and_then(|value| resolve_local_nfo_path(base_dir, &value));
        people.push(NfoPerson {
            name,
            role_type: "Actor".to_string(),
            role,
            sort_order,
            provider_ids: provider_ids_from_nfo(&block),
            primary_image,
        });
    }

    for (role_type, tag_name) in [
        ("Director", "director"),
        ("Writer", "credits"),
        ("Writer", "writer"),
        ("Producer", "producer"),
    ] {
        for name in repeated_tags(xml, tag_name) {
            for part in split_people_names(&name) {
                people.push(NfoPerson {
                    name: part,
                    role_type: role_type.to_string(),
                    role: None,
                    sort_order: people.len() as i32,
                    provider_ids: json!({}),
                    primary_image: None,
                });
            }
        }
    }

    people
}

fn split_people_names(value: &str) -> Vec<String> {
    value
        .split(['/', ',', ';'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[derive(Debug)]
struct NfoImage {
    kind: Option<String>,
    path: PathBuf,
}

fn nfo_images(xml: &str, base_dir: &Path) -> Vec<NfoImage> {
    let Ok(regex) = Regex::new(r#"(?is)<thumb\b([^>]*)>(.*?)</thumb>"#) else {
        return Vec::new();
    };

    regex
        .captures_iter(xml)
        .filter_map(|captures| {
            let attrs = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let raw_path = captures.get(2)?.as_str();
            let path_text = decode_xml_text(raw_path).trim().to_string();
            let path = resolve_local_nfo_path(base_dir, &path_text)?;
            Some(NfoImage {
                kind: image_aspect(attrs),
                path,
            })
        })
        .collect()
}

fn remote_trailer_urls(xml: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for tag in ["trailer", "youtube_trailer", "remote_trailer"] {
        for value in repeated_tags(xml, tag) {
            let value = value.trim();
            if value.starts_with("http://") || value.starts_with("https://") {
                urls.push(value.to_string());
            }
        }
    }
    urls.sort();
    urls.dedup();
    urls
}

fn find_item_image(file: &Path, names: &[&str]) -> Option<PathBuf> {
    let parent = file.parent()?;
    let stem = file.file_stem()?.to_string_lossy();
    for name in names {
        for extension in naming::IMAGE_EXTENSIONS {
            for candidate in [
                parent.join(format!("{stem}-{name}.{extension}")),
                parent.join(format!("{stem}.{name}.{extension}")),
                parent.join(format!("{name}.{extension}")),
            ] {
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn find_folder_art(folder: &Path, names: &[&str]) -> Option<PathBuf> {
    for name in names {
        for extension in naming::IMAGE_EXTENSIONS {
            let candidate = folder.join(format!("{name}.{extension}"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

fn find_season_art(
    series_path: &Path,
    season_path: &Path,
    season_number: i32,
    names: &[&str],
) -> Option<PathBuf> {
    let markers = season_markers(season_number);
    for name in names {
        for marker in &markers {
            for extension in naming::IMAGE_EXTENSIONS {
                let filename = format!("season{marker}-{name}.{extension}");
                for base in [series_path, season_path] {
                    let candidate = base.join(&filename);
                    if candidate.exists() {
                        return Some(candidate);
                    }
                }
            }
        }
    }

    None
}

fn season_markers(season_number: i32) -> Vec<String> {
    if season_number == 0 {
        return vec!["-specials".to_string(), "00".to_string(), "0".to_string()];
    }

    vec![format!("{season_number:02}"), season_number.to_string()]
}

fn image_aspect(attrs: &str) -> Option<String> {
    let regex = Regex::new(r#"(?i)\baspect\s*=\s*["']([^"']+)["']"#).ok()?;
    regex
        .captures(attrs)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_ascii_lowercase())
}

fn attr_value(attrs: &str, name: &str) -> Option<String> {
    let pattern = format!(r#"(?i)\b{}\s*=\s*["']([^"']+)["']"#, regex::escape(name));
    let regex = Regex::new(&pattern).ok()?;
    regex
        .captures(attrs)
        .and_then(|captures| captures.get(1))
        .map(|value| decode_xml_text(value.as_str()))
        .filter(|value| !value.trim().is_empty())
}

fn provider_key(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "imdb" => "Imdb".to_string(),
        "tmdb" | "themoviedb" => "Tmdb".to_string(),
        "tvdb" | "thetvdb" => "Tvdb".to_string(),
        "trakt" => "Trakt".to_string(),
        other if !other.is_empty() => {
            let mut chars = other.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => "Unknown".to_string(),
            }
        }
        _ => "Unknown".to_string(),
    }
}

fn resolve_local_nfo_path(base_dir: &Path, value: &str) -> Option<PathBuf> {
    if value.starts_with("http://") || value.starts_with("https://") {
        return None;
    }

    let path = PathBuf::from(value);
    let candidate = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };

    candidate.exists().then_some(candidate)
}

async fn sync_nfo_people(
    pool: &sqlx::PgPool,
    media_item_id: uuid::Uuid,
    people: &[NfoPerson],
) -> Result<(), AppError> {
    for person in people {
        let person_id = repository::upsert_person_from_nfo(
            pool,
            &person.name,
            person.provider_ids.clone(),
            person.primary_image.as_deref(),
        )
        .await?;
        repository::upsert_person_role(
            pool,
            person_id,
            media_item_id,
            &person.role_type,
            person.role.as_deref(),
            person.sort_order,
        )
        .await?;
    }

    Ok(())
}

fn decode_xml_text(value: &str) -> String {
    let value = value
        .replace("<![CDATA[", "")
        .replace("]]>", "")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    value.trim().to_string()
}

fn strip_xml_tags(value: &str) -> String {
    tag_regex().replace_all(value, " ").to_string()
}

fn tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<[^>]+>").expect("valid XML tag regex"))
}

fn parse_decimal(value: &str) -> Option<f64> {
    value
        .trim()
        .replace(',', ".")
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    let value = value.trim();
    let date_part = value
        .split(['T', ' '])
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(value);

    ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"]
        .into_iter()
        .find_map(|format| NaiveDate::parse_from_str(date_part, format).ok())
}

fn parse_runtime_ticks(value: &str) -> Option<i64> {
    let minutes_text = value.split_whitespace().next().unwrap_or(value).trim();
    let minutes = minutes_text.parse::<i64>().ok()?;
    Some(minutes * 60 * TICKS_PER_SECOND)
}

fn parse_i32(value: &str) -> Option<i32> {
    value
        .trim()
        .split(['.', ',', ' '])
        .next()
        .and_then(|value| value.parse().ok())
}

fn series_name_for_file(file: &Path, parsed_series_name: Option<&str>) -> String {
    if let Some(series_name) = parsed_series_name.filter(|value| !looks_like_season_folder(value)) {
        return series_name.to_string();
    }

    let parent = file.parent();
    let parent_name = parent.and_then(Path::file_name).and_then(OsStr::to_str);
    if let Some((embedded_series_name, _)) = parent_name.and_then(parse_series_season_folder) {
        if let Some(series_name) = embedded_series_name
            .map(|value| naming::clean_display_name(&value))
            .filter(|value| !value.is_empty())
        {
            return series_name;
        }
    }
    if parent_name.is_some_and(looks_like_season_folder) {
        return parent
            .and_then(Path::parent)
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .map(naming::clean_display_name)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Unknown Series".to_string());
    }

    parent_name
        .map(naming::clean_display_name)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "Unknown Series".to_string())
}

fn series_virtual_path(library_root: &Path, file: &Path, series_name: &str) -> PathBuf {
    let parent = file.parent().unwrap_or(library_root);
    if let Some((embedded_series_name, _)) = parent
        .file_name()
        .and_then(OsStr::to_str)
        .and_then(parse_series_season_folder)
    {
        if let Some(series_name) = embedded_series_name
            .map(|value| naming::clean_display_name(&value))
            .filter(|value| !value.is_empty())
        {
            return parent
                .parent()
                .map(|base| base.join(&series_name))
                .unwrap_or_else(|| library_root.join(&series_name));
        }
    }
    if parent
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(looks_like_season_folder)
    {
        return parent
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| library_root.join(series_name));
    }

    if parent == library_root {
        return library_root.join(series_name);
    }

    parent.to_path_buf()
}

fn season_virtual_path(
    library_root: &Path,
    file: &Path,
    series_path: &Path,
    season_number: i32,
) -> PathBuf {
    let parent = file.parent().unwrap_or(library_root);
    if parent
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(looks_like_season_folder)
    {
        return parent.to_path_buf();
    }

    if season_number == 0 {
        series_path.join("Specials")
    } else {
        series_path.join(format!("Season {season_number}"))
    }
}

fn season_number_from_file(file: &Path) -> Option<i32> {
    file.parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .and_then(parse_season_number)
}

fn episode_number_from_file(file: &Path) -> Option<i32> {
    let stem = file.file_stem()?.to_string_lossy();
    simple_episode_regex()
        .captures(&stem)
        .and_then(|captures| captures.name("episode"))
        .and_then(|value| value.as_str().parse().ok())
}

fn looks_like_season_folder(value: &str) -> bool {
    parse_season_number(value).is_some()
}

fn parse_season_number(value: &str) -> Option<i32> {
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case("specials") || normalized.eq_ignore_ascii_case("extras") {
        return Some(0);
    }
    if let Some((_, season)) = parse_series_season_folder(value) {
        return Some(season);
    }
    season_folder_regex()
        .captures(value)
        .and_then(|captures| {
            captures
                .name("season")
                .or_else(|| captures.name("season_alt"))
        })
        .and_then(|value| value.as_str().parse().ok())
}

fn parse_series_season_folder(value: &str) -> Option<(Option<String>, i32)> {
    let captures = series_season_folder_regex().captures(value)?;
    let season = captures.name("season")?.as_str().parse().ok()?;
    let series_name = captures
        .name("series")
        .map(|value| value.as_str().trim())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    Some((series_name, season))
}

fn season_folder_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)
            ^
            (?:
                (?:
                    season|staffel|stagione|temporada|series|kausi|
                    seizoen|sezon(?:a|ul)?|s|第
                )
                [ ._\-]*
                \(?
                (?P<season>\d{1,4})
                \)?
                (?:[ ._\-]*季)?
                (?:\s*\(\d{4}\))?
                |
                (?P<season_alt>\d{1,4})
                (?:st|nd|rd|th|\.)?
                [ ._\-]*
                (?:
                    season|staffel|stagione|temporada|series|kausi|
                    seizoen|sezon(?:a|ul)?|第
                )?
                (?:[ ._\-]*季)?
                (?:\s*\(\d{4}\))?
            )
            $
            ",
        )
        .expect("valid season folder regex")
    })
}

fn series_season_folder_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)
            ^
            (?P<series>.+?)
            [ ._\-]*
            (?:
                season|staffel|stagione|temporada|series|kausi|
                seizoen|sezon(?:a|ul)?|s|第
            )
            [ ._\-]*
            \(?
            (?P<season>\d{1,4})
            \)?
            (?:[ ._\-]*季)?
            (?:\s*\(\d{4}\))?
            $
            ",
        )
        .expect("valid series season folder regex")
    })
}

fn simple_episode_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)
            (?:^|[ ._\-])
            (?:e|ep|episode|第)?
            [ ._\-]*
            (?P<episode>\d{1,3})
            (?:[ ._\-]*集)?
            (?:[ ._\-]|$)
            ",
        )
        .expect("valid episode number regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn season_zero_virtual_path_uses_specials() {
        let path = season_virtual_path(
            Path::new("C:/media/TV"),
            Path::new("C:/media/TV/Show/Episode S00E01.mkv"),
            Path::new("C:/media/TV/Show"),
            0,
        );

        assert_eq!(path, PathBuf::from("C:/media/TV/Show/Specials"));
    }

    #[test]
    fn nfo_parser_accepts_emby_tv_number_aliases() {
        let path =
            std::env::temp_dir().join(format!("movie-rust-nfo-{}.nfo", uuid::Uuid::new_v4()));
        std::fs::write(
            &path,
            r#"
            <episodedetails>
              <title>Episode Title</title>
              <showtitle>Example Show</showtitle>
              <seasonnumber>2</seasonnumber>
              <indexnumber>3</indexnumber>
              <indexnumberend>4</indexnumberend>
              <aired>2026-04-21T00:00:00.0000000Z</aired>
            </episodedetails>
            "#,
        )
        .unwrap();

        let metadata = read_nfo_file(&path).unwrap();
        let _ = std::fs::remove_file(&path);

        assert_eq!(metadata.series_name.as_deref(), Some("Example Show"));
        assert_eq!(metadata.season_number, Some(2));
        assert_eq!(metadata.episode_number, Some(3));
        assert_eq!(metadata.episode_number_end, Some(4));
        assert_eq!(metadata.premiere_date, NaiveDate::from_ymd_opt(2026, 4, 21));
    }

    #[test]
    fn find_season_art_matches_series_level_emby_names() {
        let root =
            std::env::temp_dir().join(format!("movie-rust-season-art-{}", uuid::Uuid::new_v4()));
        let series_path = root.join("Show");
        let season_path = series_path.join("Season 1");
        std::fs::create_dir_all(&season_path).unwrap();
        let poster = series_path.join("season01-poster.jpg");
        std::fs::write(&poster, b"poster").unwrap();

        let found = find_season_art(&series_path, &season_path, 1, &["poster"]);

        assert_eq!(found.as_deref(), Some(poster.as_path()));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn find_season_art_matches_specials_marker() {
        let root =
            std::env::temp_dir().join(format!("movie-rust-specials-art-{}", uuid::Uuid::new_v4()));
        let series_path = root.join("Show");
        let season_path = series_path.join("Specials");
        std::fs::create_dir_all(&season_path).unwrap();
        let fanart = series_path.join("season-specials-fanart.jpg");
        std::fs::write(&fanart, b"fanart").unwrap();

        let found = find_season_art(&series_path, &season_path, 0, &["fanart"]);

        assert_eq!(found.as_deref(), Some(fanart.as_path()));
        let _ = std::fs::remove_dir_all(&root);
    }
}

/// 提取章节缩略图：对每个章节使用 ffmpeg 截图并保存到缓存目录。
/// `.strm` 且内容为 `http/https` URL（含本机远端 Emby 代理）时对该 URL 拉流截图。
async fn extract_chapter_images(
    pool: &sqlx::PgPool,
    config: &Config,
    item_id: uuid::Uuid,
    file: &Path,
) {
    if !file.exists() {
        return;
    }

    enum FfmpegInput {
        LocalPath(std::path::PathBuf),
        RemoteUrl(String),
    }

    let input = if naming::is_strm(file) {
        let Ok(content) = tokio::fs::read_to_string(file).await else {
            return;
        };
        let Some(target) = naming::strm_target_from_text(&content) else {
            return;
        };
        if target.starts_with("http://") || target.starts_with("https://") {
            FfmpegInput::RemoteUrl(target)
        } else {
            return;
        }
    } else {
        FfmpegInput::LocalPath(file.to_path_buf())
    };

    let chapters = match repository::get_media_chapters(pool, item_id).await {
        Ok(c) => c,
        Err(_) => return,
    };
    if chapters.is_empty() {
        return;
    }
    let ffmpeg = config.ffmpeg_path.as_str();
    let cache_dir = "cache";
    let chapter_dir = PathBuf::from(cache_dir)
        .join("chapter-images")
        .join(item_id.to_string());
    if let Err(err) = tokio::fs::create_dir_all(&chapter_dir).await {
        tracing::debug!(error = %err, "创建章节图片目录失败");
        return;
    }

    for chapter in &chapters {
        if chapter.image_path.is_some() {
            continue;
        }
        let seconds = (chapter.start_position_ticks as f64) / 10_000_000.0;
        let out_path = chapter_dir.join(format!("chapter_{}.jpg", chapter.chapter_index));
        let mut cmd = tokio::process::Command::new(ffmpeg);
        cmd.args([
            "-ss",
            &format!("{seconds:.3}"),
            "-i",
        ]);
        match &input {
            FfmpegInput::LocalPath(p) => {
                cmd.arg(p.as_os_str());
            }
            FfmpegInput::RemoteUrl(url) => {
                cmd.arg(url);
            }
        }
        let result = cmd
            .args([
                "-frames:v", "1",
                "-q:v", "6",
                "-vf", "scale=480:-1",
                "-y",
            ])
            .arg(out_path.as_os_str())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
        match result {
            Ok(status) if status.success() && out_path.exists() => {
                if let Err(err) = repository::update_chapter_image_path(
                    pool,
                    chapter.id,
                    out_path.to_string_lossy().as_ref(),
                )
                .await
                {
                    tracing::debug!(
                        chapter_id = %chapter.id,
                        error = %err,
                        "更新章节图片路径失败"
                    );
                }
            }
            _ => {
                tracing::debug!(
                    item_id = %item_id,
                    chapter_index = chapter.chapter_index,
                    "章节截图提取失败"
                );
            }
        }
    }
}
