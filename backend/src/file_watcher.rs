use crate::{repository, state::AppState};
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::PathBuf;
use tokio::sync::mpsc;
use uuid::Uuid;

struct WatchedLibrary {
    id: Uuid,
    paths: Vec<PathBuf>,
}

pub async fn file_watcher_loop(state: AppState) {
    loop {
        if let Err(err) = run_file_watcher(&state).await {
            tracing::warn!(error = %err, "文件监控循环异常，30 秒后重试");
        }
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    }
}

async fn run_file_watcher(state: &AppState) -> Result<(), crate::error::AppError> {
    let (tx, mut rx) = mpsc::channel::<Event>(256);

    let mut watcher = RecommendedWatcher::new(
        move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.blocking_send(event);
            }
        },
        Config::default().with_poll_interval(std::time::Duration::from_secs(5)),
    )
    .map_err(|e| crate::error::AppError::Internal(format!("创建文件监控器失败: {e}")))?;

    let mut watched_paths: HashSet<PathBuf> = HashSet::new();

    let libraries = collect_monitored_libraries(state).await?;
    for lib in &libraries {
        for path in &lib.paths {
            if path.exists() && watched_paths.insert(path.clone()) {
                if let Err(err) = watcher.watch(path.as_path(), RecursiveMode::Recursive) {
                    tracing::warn!(path = %path.display(), error = %err, "无法监控路径");
                } else {
                    tracing::info!(
                        library_id = %lib.id,
                        path = %path.display(),
                        "文件监控：已开始监控路径"
                    );
                }
            }
        }
    }

    if watched_paths.is_empty() {
        tracing::debug!("没有需要监控的本地媒体库路径，文件监控休眠");
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        return Ok(());
    }

    let mut dirty_libraries: HashSet<Uuid> = HashSet::new();
    let mut debounce_timer = tokio::time::interval(std::time::Duration::from_secs(15));
    let mut refresh_config = tokio::time::interval(std::time::Duration::from_secs(300));

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                for path in &event.paths {
                    for lib in &libraries {
                        if lib.paths.iter().any(|lp| path.starts_with(lp)) {
                            dirty_libraries.insert(lib.id);
                        }
                    }
                }
            }
            _ = debounce_timer.tick() => {
                if !dirty_libraries.is_empty() {
                    let lib_ids: Vec<Uuid> = dirty_libraries.drain().collect();
                    for lib_id in lib_ids {
                        tracing::info!(
                            library_id = %lib_id,
                            "文件监控：检测到文件变更，触发库扫描"
                        );
                        // 不阻塞当前循环，直接 fire-and-forget
                        let state_clone = state.clone();
                        tokio::spawn(async move {
                            use crate::scanner;
                            if let Err(err) = scanner::scan_single_library_with_db_semaphore(
                                &state_clone.pool,
                                state_clone.metadata_manager.clone(),
                                &state_clone.config,
                                state_clone.work_limiters.clone(),
                                lib_id,
                                None,
                                Some(state_clone.scan_db_semaphore.clone()),
                            ).await {
                                tracing::warn!(
                                    library_id = %lib_id,
                                    error = %err,
                                    "文件监控触发的库扫描失败"
                                );
                            }
                        });
                    }
                }
            }
            _ = refresh_config.tick() => {
                // 定期刷新监控列表（新增/移除库路径）
                break;
            }
        }
    }
    Ok(())
}

async fn collect_monitored_libraries(
    state: &AppState,
) -> Result<Vec<WatchedLibrary>, crate::error::AppError> {
    let libs = repository::list_libraries(&state.pool).await?;
    let mut result = Vec::new();
    for lib in libs {
        let opts = repository::library_options(&lib);
        if !opts.enable_realtime_monitor {
            continue;
        }
        let path_str = lib.path.as_str();

        let remote_sources = repository::find_remote_sources_for_library(&state.pool, lib.id)
            .await
            .unwrap_or_default();

        let mut paths = Vec::new();
        for info in &opts.path_infos {
            if info.path.starts_with("__remote") {
                continue;
            }
            let p = PathBuf::from(&info.path);
            if p.exists() {
                paths.push(p);
            }
        }
        // 远端 STRM 物理目录：`{输出根}/{源名}/{远端视图}/`，用户手工删 strm/侧车时也需能被监控到
        for pb in crate::remote_emby::strm_watch_directories_for_sources(&remote_sources, lib.id)
        {
            if pb.exists() {
                paths.push(pb);
            }
        }
        if paths.is_empty() {
            if !path_str.starts_with("__remote_view_") && !path_str.starts_with("__remote_transit")
            {
                let p = PathBuf::from(path_str);
                if p.exists() {
                    paths.push(p);
                }
            }
        }

        let mut seen = HashSet::new();
        paths.retain(|p| seen.insert(p.to_string_lossy().to_string()));

        if !paths.is_empty() {
            result.push(WatchedLibrary { id: lib.id, paths });
        }
    }
    Ok(result)
}
