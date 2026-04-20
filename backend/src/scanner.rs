use crate::{
    error::AppError,
    models::{DbLibrary, ScanSummary},
    naming,
    repository::{self, UpsertMediaItem},
};
use chrono::NaiveDate;
use regex::Regex;
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use walkdir::WalkDir;

const TICKS_PER_SECOND: i64 = 10_000_000;

#[derive(Debug, Default, Clone)]
struct NfoMetadata {
    title: Option<String>,
    overview: Option<String>,
    production_year: Option<i32>,
    runtime_ticks: Option<i64>,
    premiere_date: Option<NaiveDate>,
    series_name: Option<String>,
    season_number: Option<i32>,
    episode_number: Option<i32>,
    episode_number_end: Option<i32>,
    genres: Vec<String>,
    primary_image: Option<PathBuf>,
    backdrop_image: Option<PathBuf>,
}

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
    let nfo = read_video_nfo(file).unwrap_or_default();
    let container = file
        .extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase);
    let poster = nfo
        .primary_image
        .clone()
        .or_else(|| naming::find_sidecar_image(file));
    let backdrop = nfo
        .backdrop_image
        .clone()
        .or_else(|| file.parent().and_then(naming::find_backdrop_image));
    let name = nfo.title.as_deref().unwrap_or(&parsed.title);

    repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: None,
            name,
            item_type: "Movie",
            media_type: "Video",
            path: file,
            container: container.as_deref(),
            overview: nfo.overview.as_deref(),
            production_year: nfo.production_year.or(parsed.production_year),
            runtime_ticks: nfo.runtime_ticks,
            premiere_date: nfo.premiere_date.or(parsed.premiere_date),
            genres: &nfo.genres,
            image_primary_path: poster.as_deref(),
            backdrop_path: backdrop.as_deref(),
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
    let episode_nfo = read_video_nfo(file).unwrap_or_default();

    let preliminary_series_name = episode_nfo
        .series_name
        .as_deref()
        .or(parsed.series_name.as_deref());
    let preliminary_series_name = series_name_for_file(file, preliminary_series_name);
    let preliminary_series_path = series_virtual_path(library, file, &preliminary_series_name);
    let series_nfo = read_nfo_file(&preliminary_series_path.join("tvshow.nfo")).unwrap_or_default();

    let series_name = series_nfo
        .title
        .as_deref()
        .or(episode_nfo.series_name.as_deref())
        .or(parsed.series_name.as_deref())
        .map(ToOwned::to_owned)
        .unwrap_or(preliminary_series_name);
    let series_path = series_virtual_path(library, file, &series_name);

    let season_number = episode_nfo
        .season_number
        .or(parsed.season_number)
        .or_else(|| season_number_from_file(file))
        .unwrap_or(1);
    let season_path = season_virtual_path(library, file, &series_path, season_number);
    let season_nfo = read_nfo_file(&season_path.join("season.nfo")).unwrap_or_default();
    let season_name = season_nfo
        .title
        .clone()
        .unwrap_or_else(|| format!("Season {season_number}"));

    let series_poster = series_nfo
        .primary_image
        .clone()
        .or_else(|| naming::find_folder_image(&series_path))
        .or_else(|| series_path.parent().and_then(naming::find_folder_image));
    let series_backdrop = series_nfo
        .backdrop_image
        .clone()
        .or_else(|| naming::find_backdrop_image(&series_path));
    let season_poster = season_nfo
        .primary_image
        .clone()
        .or_else(|| naming::find_folder_image(&season_path))
        .or_else(|| series_poster.clone());

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
            overview: series_nfo.overview.as_deref(),
            production_year: series_nfo.production_year.or(parsed.production_year),
            runtime_ticks: None,
            premiere_date: series_nfo.premiere_date,
            genres: &series_nfo.genres,
            image_primary_path: series_poster.as_deref(),
            backdrop_path: series_backdrop.as_deref(),
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
            overview: season_nfo.overview.as_deref(),
            production_year: season_nfo.production_year.or(series_nfo.production_year),
            runtime_ticks: None,
            premiere_date: season_nfo.premiere_date,
            genres: if season_nfo.genres.is_empty() {
                &series_nfo.genres
            } else {
                &season_nfo.genres
            },
            image_primary_path: season_poster.as_deref(),
            backdrop_path: series_backdrop.as_deref(),
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
    let poster = episode_nfo
        .primary_image
        .clone()
        .or_else(|| naming::find_sidecar_image(file))
        .or_else(|| season_poster.clone());
    let backdrop = episode_nfo
        .backdrop_image
        .clone()
        .or_else(|| series_backdrop.clone());
    let episode_name = episode_nfo.title.as_deref().unwrap_or(&parsed.title);
    let episode_number = episode_nfo
        .episode_number
        .or(parsed.episode_number)
        .or_else(|| episode_number_from_file(file));

    repository::upsert_media_item(
        pool,
        UpsertMediaItem {
            library_id: library.id,
            parent_id: Some(season_id),
            name: episode_name,
            item_type: "Episode",
            media_type: "Video",
            path: file,
            container: container.as_deref(),
            overview: episode_nfo.overview.as_deref(),
            production_year: episode_nfo
                .production_year
                .or(parsed.production_year)
                .or(series_nfo.production_year),
            runtime_ticks: episode_nfo.runtime_ticks,
            premiere_date: episode_nfo.premiere_date.or(parsed.premiere_date),
            genres: if episode_nfo.genres.is_empty() {
                &series_nfo.genres
            } else {
                &episode_nfo.genres
            },
            image_primary_path: poster.as_deref(),
            backdrop_path: backdrop.as_deref(),
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
        },
    )
    .await?;

    Ok(())
}

fn read_video_nfo(file: &Path) -> Option<NfoMetadata> {
    let parent = file.parent()?;
    let stem = file.file_stem()?.to_string_lossy();
    for candidate in [parent.join(format!("{stem}.nfo")), parent.join("movie.nfo")] {
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
        overview: first_tag(&xml, &["plot", "outline", "review", "biography"]),
        production_year: first_tag(&xml, &["year"]).and_then(|value| value.parse().ok()),
        runtime_ticks: first_tag(&xml, &["runtime"]).and_then(|value| parse_runtime_ticks(&value)),
        premiere_date: first_tag(&xml, &["premiered", "aired", "releasedate"])
            .and_then(|value| parse_date(&value)),
        series_name: first_tag(&xml, &["showtitle"]),
        season_number: first_tag(&xml, &["season"]).and_then(|value| value.parse().ok()),
        episode_number: first_tag(&xml, &["episode"]).and_then(|value| value.parse().ok()),
        episode_number_end: first_tag(&xml, &["episodenumberend"])
            .and_then(|value| value.parse().ok()),
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
        primary_image: None,
        backdrop_image: None,
    };

    for image in nfo_images(&xml, parent) {
        match image.kind.as_deref() {
            Some("fanart") | Some("backdrop") | Some("background") => {
                metadata.backdrop_image.get_or_insert(image.path);
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

fn image_aspect(attrs: &str) -> Option<String> {
    let regex = Regex::new(r#"(?i)\baspect\s*=\s*["']([^"']+)["']"#).ok()?;
    regex
        .captures(attrs)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_ascii_lowercase())
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

fn parse_date(value: &str) -> Option<NaiveDate> {
    ["%Y-%m-%d", "%Y/%m/%d", "%Y.%m.%d"]
        .into_iter()
        .find_map(|format| NaiveDate::parse_from_str(value.trim(), format).ok())
}

fn parse_runtime_ticks(value: &str) -> Option<i64> {
    let minutes_text = value.split_whitespace().next().unwrap_or(value).trim();
    let minutes = minutes_text.parse::<i64>().ok()?;
    Some(minutes * 60 * TICKS_PER_SECOND)
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

fn season_virtual_path(
    library: &DbLibrary,
    file: &Path,
    series_path: &Path,
    season_number: i32,
) -> PathBuf {
    let parent = file.parent().unwrap_or_else(|| Path::new(&library.path));
    if parent
        .file_name()
        .and_then(OsStr::to_str)
        .is_some_and(looks_like_season_folder)
    {
        return parent.to_path_buf();
    }

    series_path.join(format!("Season {season_number:02}"))
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
    season_folder_regex()
        .captures(value)
        .and_then(|captures| captures.name("season"))
        .and_then(|value| value.as_str().parse().ok())
}

fn season_folder_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?ix)^(?:season|s|第)?[ ._\-]*(?P<season>\d{1,4})(?:[ ._\-]*季)?$")
            .expect("valid season folder regex")
    })
}

fn simple_episode_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?ix)(?:^|[ ._\-])(?:e|ep|episode|第)?(?P<episode>\d{1,3})(?:[ ._\-]|集|$)")
            .expect("valid episode number regex")
    })
}
