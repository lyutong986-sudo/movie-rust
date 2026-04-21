use crate::{
    error::AppError,
    models::{DbLibrary, ScanSummary},
    naming,
    repository::{self, UpsertMediaItem},
};
use chrono::NaiveDate;
use regex::Regex;
use serde_json::{json, Map, Value};
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
    original_title: Option<String>,
    overview: Option<String>,
    production_year: Option<i32>,
    official_rating: Option<String>,
    community_rating: Option<f64>,
    runtime_ticks: Option<i64>,
    premiere_date: Option<NaiveDate>,
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
    backdrop_image: Option<PathBuf>,
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

pub async fn scan_all_libraries(pool: &sqlx::PgPool) -> Result<ScanSummary, AppError> {
    let libraries = repository::list_libraries(pool).await?;
    let mut scanned_files = 0_i64;
    let mut imported_items = 0_i64;

    for library in &libraries {
        for library_path in repository::library_paths(library) {
            let path = PathBuf::from(&library_path);
            if !path.exists() {
                tracing::warn!("媒体库路径不存在: {}", library_path);
                continue;
            }

            let files = collect_video_files(path.clone()).await?;
            scanned_files += files.len() as i64;

            for file in files {
                if library.collection_type.eq_ignore_ascii_case("tvshows") {
                    import_tv_file(pool, library, &path, &file).await?;
                } else {
                    import_movie_file(pool, library, &file).await?;
                }
                imported_items += 1;
            }
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
    let provider_ids = merge_provider_ids(nfo.provider_ids.clone(), provider_ids_from_path(file));

    let movie_id = repository::upsert_media_item(
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
            runtime_ticks: nfo.runtime_ticks,
            premiere_date: nfo.premiere_date.or(parsed.premiere_date),
            provider_ids,
            genres: &nfo.genres,
            studios: &nfo.studios,
            tags: &nfo.tags,
            production_locations: &nfo.production_locations,
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
    sync_nfo_people(pool, movie_id, &nfo.people).await?;

    Ok(())
}

async fn import_tv_file(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
    library_root: &Path,
    file: &Path,
) -> Result<(), AppError> {
    let parsed = naming::parse_media_path(file);
    let episode_nfo = read_video_nfo(file).unwrap_or_default();

    let preliminary_series_name = episode_nfo
        .series_name
        .as_deref()
        .or(parsed.series_name.as_deref());
    let preliminary_series_name = series_name_for_file(file, preliminary_series_name);
    let preliminary_series_path = series_virtual_path(library_root, file, &preliminary_series_name);
    let series_nfo = read_nfo_file(&preliminary_series_path.join("tvshow.nfo")).unwrap_or_default();

    let series_name = series_nfo
        .title
        .as_deref()
        .or(episode_nfo.series_name.as_deref())
        .or(parsed.series_name.as_deref())
        .map(ToOwned::to_owned)
        .unwrap_or(preliminary_series_name);
    let series_path = series_virtual_path(library_root, file, &series_name);
    let series_provider_ids =
        merge_provider_ids(series_nfo.provider_ids.clone(), provider_ids_from_path(&series_path));
    let episode_provider_ids =
        merge_provider_ids(episode_nfo.provider_ids.clone(), provider_ids_from_path(file));

    let season_number = episode_nfo
        .season_number
        .or(parsed.season_number)
        .or_else(|| season_number_from_file(file))
        .unwrap_or(1);
    let season_path = season_virtual_path(library_root, file, &series_path, season_number);
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
            original_title: series_nfo.original_title.as_deref(),
            overview: series_nfo.overview.as_deref(),
            production_year: series_nfo.production_year.or(parsed.production_year),
            official_rating: series_nfo.official_rating.as_deref(),
            community_rating: series_nfo.community_rating,
            runtime_ticks: None,
            premiere_date: series_nfo.premiere_date,
            provider_ids: series_provider_ids.clone(),
            genres: &series_nfo.genres,
            studios: &series_nfo.studios,
            tags: &series_nfo.tags,
            production_locations: &series_nfo.production_locations,
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
    sync_nfo_people(pool, series_id, &series_nfo.people).await?;

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
            original_title: season_nfo.original_title.as_deref(),
            overview: season_nfo.overview.as_deref(),
            production_year: season_nfo.production_year.or(series_nfo.production_year),
            official_rating: season_nfo
                .official_rating
                .as_deref()
                .or(series_nfo.official_rating.as_deref()),
            community_rating: season_nfo.community_rating.or(series_nfo.community_rating),
            runtime_ticks: None,
            premiere_date: season_nfo.premiere_date,
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

    let episode_id = repository::upsert_media_item(
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
            runtime_ticks: episode_nfo.runtime_ticks,
            premiere_date: episode_nfo.premiere_date.or(parsed.premiere_date),
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
    let episode_people = if episode_nfo.people.is_empty() {
        &series_nfo.people
    } else {
        &episode_nfo.people
    };
    sync_nfo_people(pool, episode_id, episode_people).await?;

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
        original_title: first_tag(&xml, &["originaltitle"]),
        overview: first_tag(&xml, &["plot", "outline", "review", "biography"]),
        production_year: first_tag(&xml, &["year"]).and_then(|value| value.parse().ok()),
        official_rating: first_tag(&xml, &["mpaa", "certification", "officialrating"]),
        community_rating: first_tag(&xml, &["rating", "communityrating", "userrating"])
            .and_then(|value| parse_decimal(&value)),
        runtime_ticks: first_tag(&xml, &["runtime"]).and_then(|value| parse_runtime_ticks(&value)),
        premiere_date: first_tag(&xml, &["premiered", "aired", "releasedate"])
            .and_then(|value| parse_date(&value)),
        series_name: first_tag(&xml, &["showtitle"]),
        season_number: first_tag(&xml, &["season"]).and_then(|value| value.parse().ok()),
        episode_number: first_tag(&xml, &["episode"]).and_then(|value| value.parse().ok()),
        episode_number_end: first_tag(&xml, &["episodenumberend"])
            .and_then(|value| value.parse().ok()),
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
            let attrs = captures.get(1).map(|value| value.as_str()).unwrap_or_default();
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

fn has_provider_ids(value: &Value) -> bool {
    value.as_object().is_some_and(|object| !object.is_empty())
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

fn series_virtual_path(library_root: &Path, file: &Path, series_name: &str) -> PathBuf {
    let parent = file.parent().unwrap_or(library_root);
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
