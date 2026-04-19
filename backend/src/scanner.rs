use crate::{
    error::AppError,
    models::{DbLibrary, ScanSummary},
    naming,
    repository::{self, UpsertMediaItem},
};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

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
            if library.collection_type.eq_ignore_ascii_case("tvshows") {
                import_tv_file(pool, library, &file).await?;
            } else {
                import_movie_file(pool, library, &file).await?;
            }
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
            if naming::is_video(entry.path()) {
                files.push(entry.path().to_path_buf());
            }
        }

        files
    })
    .await
    .map_err(|error| AppError::Internal(format!("扫描任务失败: {error}")))
}

async fn import_movie_file(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
    file: &Path,
) -> Result<(), AppError> {
    let parsed = naming::parse_media_path(file);
    let container = file
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase);
    let poster = naming::find_sidecar_image(file);

    repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: None,
            name: &parsed.title,
            item_type: "Movie",
            media_type: "Video",
            path: file,
            container: container.as_deref(),
            overview: None,
            production_year: parsed.production_year,
            runtime_ticks: None,
            premiere_date: parsed.premiere_date,
            image_primary_path: poster.as_deref(),
            backdrop_path: None,
            series_name: None,
            season_name: None,
            index_number: None,
            index_number_end: None,
            parent_index_number: None,
            width: parsed.width,
            height: parsed.height,
            video_codec: parsed.video_codec.as_deref(),
            audio_codec: parsed.audio_codec.as_deref(),
        },
    )
    .await?;

    Ok(())
}

async fn import_tv_file(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
    file: &Path,
) -> Result<(), AppError> {
    let parsed = naming::parse_media_path(file);

    if parsed.episode_number.is_none() || parsed.season_number.is_none() {
        return import_movie_file(pool, library, file).await;
    }

    let series_name = series_name_for_file(file, parsed.series_name.as_deref());
    let season_number = parsed.season_number.unwrap_or(1);
    let season_name = format!("Season {season_number}");
    let series_path = series_virtual_path(library, file, &series_name);
    let season_path = series_path.join(format!("Season {season_number:02}"));
    let series_poster = naming::find_folder_image(&series_path)
        .or_else(|| series_path.parent().and_then(naming::find_folder_image));
    let season_poster =
        naming::find_folder_image(file.parent().unwrap_or_else(|| Path::new(&library.path)));

    let series_id = repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: None,
            name: &series_name,
            item_type: "Series",
            media_type: "Video",
            path: &series_path,
            container: None,
            overview: None,
            production_year: parsed.production_year,
            runtime_ticks: None,
            premiere_date: None,
            image_primary_path: series_poster.as_deref(),
            backdrop_path: None,
            series_name: Some(&series_name),
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

    let season_id = repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: Some(series_id),
            name: &season_name,
            item_type: "Season",
            media_type: "Video",
            path: &season_path,
            container: None,
            overview: None,
            production_year: parsed.production_year,
            runtime_ticks: None,
            premiere_date: None,
            image_primary_path: season_poster.as_deref(),
            backdrop_path: None,
            series_name: Some(&series_name),
            season_name: Some(&season_name),
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

    let container = file
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase);
    let poster = naming::find_sidecar_image(file);

    repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: Some(season_id),
            name: &parsed.title,
            item_type: "Episode",
            media_type: "Video",
            path: file,
            container: container.as_deref(),
            overview: None,
            production_year: parsed.production_year,
            runtime_ticks: None,
            premiere_date: parsed.premiere_date,
            image_primary_path: poster.as_deref(),
            backdrop_path: None,
            series_name: Some(&series_name),
            season_name: Some(&season_name),
            index_number: parsed.episode_number,
            index_number_end: parsed.ending_episode_number,
            parent_index_number: Some(season_number),
            width: parsed.width,
            height: parsed.height,
            video_codec: parsed.video_codec.as_deref(),
            audio_codec: parsed.audio_codec.as_deref(),
        },
    )
    .await?;

    Ok(())
}

fn series_name_for_file(file: &Path, parsed_series_name: Option<&str>) -> String {
    if let Some(series_name) = parsed_series_name.filter(|value| !looks_like_season_folder(value)) {
        return series_name.to_string();
    }

    let parent = file.parent();
    let parent_name = parent.and_then(Path::file_name).and_then(OsStr::to_str);
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

fn series_virtual_path(library: &DbLibrary, file: &Path, series_name: &str) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new(&library.path));
    let library_root = Path::new(&library.path);
    if parent
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(looks_like_season_folder)
    {
        return parent
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(&library.path).join(series_name));
    }

    if parent == library_root {
        return library_root.join(series_name);
    }

    parent.to_path_buf()
}

fn looks_like_season_folder(value: &str) -> bool {
    let lower = value.to_lowercase();
    lower.starts_with("season ")
        || lower.starts_with("season.")
        || lower.starts_with("season_")
        || lower.starts_with('s') && lower[1..].chars().all(|value| value.is_ascii_digit())
}
