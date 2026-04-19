use crate::{error::AppError, models::ScanSummary, repository};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "m4v", "mkv", "avi", "mov", "webm", "wmv", "flv", "ts", "m2ts",
];

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

pub async fn scan_all_libraries(pool: &sqlx::PgPool) -> Result<ScanSummary, AppError> {
    let libraries = repository::list_libraries(pool).await?;
    let mut scanned_files = 0_i64;
    let mut imported_items = 0_i64;

    for library in &libraries {
        let path = PathBuf::from(&library.path);
        if !path.exists() {
            tracing::warn!("媒体库路径不存在: {}", library.path);
            continue;
        }

        let files = collect_video_files(path).await?;
        scanned_files += files.len() as i64;

        for file in files {
            let name = display_name_from_path(&file);
            let container = file
                .extension()
                .and_then(OsStr::to_str)
                .map(str::to_lowercase);
            let poster = find_sidecar_image(&file);

            repository::upsert_media_item(
                pool,
                library.id,
                &name,
                &file,
                container.as_deref(),
                poster.as_deref(),
            )
            .await?;
            imported_items += 1;
        }
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
            if is_video(entry.path()) {
                files.push(entry.path().to_path_buf());
            }
        }

        files
    })
    .await
    .map_err(|error| AppError::Internal(format!("扫描任务失败: {error}")))
}

fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|extension| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
        .unwrap_or(false)
}

fn display_name_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("Untitled");

    stem.replace(['.', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn find_sidecar_image(video: &Path) -> Option<PathBuf> {
    let parent = video.parent()?;
    let stem = video.file_stem()?.to_string_lossy();

    let mut candidates = Vec::new();
    for extension in IMAGE_EXTENSIONS {
        candidates.push(parent.join(format!("{stem}.{extension}")));
    }
    for name in ["poster", "folder", "cover"] {
        for extension in IMAGE_EXTENSIONS {
            candidates.push(parent.join(format!("{name}.{extension}")));
        }
    }

    candidates.into_iter().find(|candidate| candidate.exists())
}
