use crate::{
    error::AppError,
    media_analyzer,
    metadata::provider::MetadataProviderManager,
    models::{DbLibrary, ScanSummary},
    naming,
    repository::{self, UpsertMediaItem},
};
use chrono::NaiveDate;
use regex::Regex;
use serde_json::{json, Map, Value};
use std::{
    collections::HashSet,
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
    backdrop_image: Option<PathBuf>,
    logo_image: Option<PathBuf>,
    thumb_image: Option<PathBuf>,
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

pub async fn scan_all_libraries(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
) -> Result<ScanSummary, AppError> {
    let libraries = repository::list_libraries(pool).await?;
    let mut scanned_files = 0_i64;
    let mut imported_items = 0_i64;
    let mut refreshed_series = HashSet::new();

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
                    import_tv_file(
                        pool,
                        metadata_manager,
                        &mut refreshed_series,
                        library,
                        &path,
                        &file,
                    )
                    .await?;
                } else {
                    import_movie_file(pool, metadata_manager, library, &file).await?;
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
    metadata_manager: Option<&MetadataProviderManager>,
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
    let logo = nfo
        .logo_image
        .clone()
        .or_else(|| find_item_image(file, &["logo", "clearlogo"]));
    let thumb = nfo
        .thumb_image
        .clone()
        .or_else(|| find_item_image(file, &["thumb", "landscape"]))
        .or_else(|| backdrop.clone());
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
        },
    )
    .await?;
    sync_nfo_people(pool, movie_id, &nfo.people).await?;
    refresh_remote_people(pool, metadata_manager, movie_id, "movie", &provider_ids).await;
    refresh_movie_remote_metadata(pool, metadata_manager, movie_id, &provider_ids).await;
    analyze_imported_media(pool, movie_id, file).await?;

    Ok(())
}

async fn import_tv_file(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    refreshed_series: &mut HashSet<uuid::Uuid>,
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
        .unwrap_or_else(|| {
            if season_number == 0 {
                "Specials".to_string()
            } else {
                format!("Season {season_number}")
            }
        });

    let series_poster = series_nfo
        .primary_image
        .clone()
        .or_else(|| naming::find_folder_image(&series_path))
        .or_else(|| series_path.parent().and_then(naming::find_folder_image));
    let series_backdrop = series_nfo
        .backdrop_image
        .clone()
        .or_else(|| naming::find_backdrop_image(&series_path));
    let series_logo = series_nfo
        .logo_image
        .clone()
        .or_else(|| find_folder_art(&series_path, &["logo", "clearlogo"]));
    let series_thumb = series_nfo
        .thumb_image
        .clone()
        .or_else(|| find_folder_art(&series_path, &["thumb", "landscape"]))
        .or_else(|| series_backdrop.clone());
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
        },
    )
    .await?;
    sync_nfo_people(pool, series_id, &series_nfo.people).await?;
    refresh_remote_people(pool, metadata_manager, series_id, "tv", &series_provider_ids).await;
    refresh_series_remote_metadata(
        pool,
        metadata_manager,
        refreshed_series,
        series_id,
        &series_provider_ids,
    )
    .await;
    refresh_series_episode_catalog(pool, metadata_manager, series_id, &series_provider_ids).await;

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
              critic_rating: season_nfo.critic_rating.or(series_nfo.critic_rating),
              runtime_ticks: None,
            premiere_date: season_nfo.premiere_date,
            status: season_nfo.status.as_deref().or(series_nfo.status.as_deref()),
            end_date: season_nfo.end_date.or(series_nfo.end_date),
            air_days: if season_nfo.air_days.is_empty() {
                &series_nfo.air_days
            } else {
                &season_nfo.air_days
            },
            air_time: season_nfo.air_time.as_deref().or(series_nfo.air_time.as_deref()),
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
            logo_path: series_logo.as_deref(),
            thumb_path: series_thumb.as_deref(),
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
    let episode_logo = episode_nfo.logo_image.clone().or_else(|| series_logo.clone());
    let episode_thumb = episode_nfo
        .thumb_image
        .clone()
        .or_else(|| find_item_image(file, &["thumb", "landscape"]))
        .or_else(|| series_thumb.clone())
        .or_else(|| backdrop.clone());
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
              critic_rating: episode_nfo.critic_rating.or(series_nfo.critic_rating),
              runtime_ticks: episode_nfo.runtime_ticks,
            premiere_date: episode_nfo.premiere_date.or(parsed.premiere_date),
            status: episode_nfo.status.as_deref().or(series_nfo.status.as_deref()),
            end_date: episode_nfo.end_date.or(series_nfo.end_date),
            air_days: if episode_nfo.air_days.is_empty() {
                &series_nfo.air_days
            } else {
                &episode_nfo.air_days
            },
            air_time: episode_nfo.air_time.as_deref().or(series_nfo.air_time.as_deref()),
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
        },
    )
    .await?;
    let episode_people = if episode_nfo.people.is_empty() {
        &series_nfo.people
    } else {
        &episode_nfo.people
    };
    sync_nfo_people(pool, episode_id, episode_people).await?;
    analyze_imported_media(pool, episode_id, file).await?;

    Ok(())
}

async fn refresh_series_remote_metadata(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    refreshed_series: &mut HashSet<uuid::Uuid>,
    series_id: uuid::Uuid,
    provider_ids: &Value,
) {
    if !refreshed_series.insert(series_id) {
        return;
    }

    let Some(metadata_manager) = metadata_manager else {
        return;
    };
    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = metadata_manager.get_provider("tmdb") else {
        return;
    };

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
            tracing::warn!(
                series_id = %series_id,
                tmdb_id = %tmdb_id,
                error = %error,
                "刷新远程剧集元数据失败"
            );
        }
    }
}

async fn refresh_movie_remote_metadata(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    movie_id: uuid::Uuid,
    provider_ids: &Value,
) {
    let Some(metadata_manager) = metadata_manager else {
        return;
    };
    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = metadata_manager.get_provider("tmdb") else {
        return;
    };

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
        }
        Err(error) => {
            tracing::warn!(
                movie_id = %movie_id,
                tmdb_id = %tmdb_id,
                error = %error,
                "刷新远程电影元数据失败"
            );
        }
    }
}

async fn refresh_series_episode_catalog(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    series_id: uuid::Uuid,
    provider_ids: &Value,
) {
    let Some(metadata_manager) = metadata_manager else {
        return;
    };
    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = metadata_manager.get_provider("tmdb") else {
        return;
    };

    match provider.get_series_episode_catalog(&tmdb_id).await {
        Ok(items) => {
            if let Err(error) = repository::replace_series_episode_catalog(pool, series_id, &items).await {
                tracing::warn!(
                    series_id = %series_id,
                    tmdb_id = %tmdb_id,
                    error = %error,
                    "同步远程剧集目录失败"
                );
            }
        }
        Err(error) => {
            tracing::warn!(
                series_id = %series_id,
                tmdb_id = %tmdb_id,
                error = %error,
                "获取远程剧集目录失败"
            );
        }
    }
}

async fn refresh_remote_people(
    pool: &sqlx::PgPool,
    metadata_manager: Option<&MetadataProviderManager>,
    media_item_id: uuid::Uuid,
    media_type: &str,
    provider_ids: &Value,
) {
    let Some(metadata_manager) = metadata_manager else {
        return;
    };
    let Some(tmdb_id) = tmdb_id_from_provider_ids(provider_ids) else {
        return;
    };
    let Some(provider) = metadata_manager.get_provider("tmdb") else {
        return;
    };

    match provider.get_item_people(media_type, &tmdb_id).await {
        Ok(people) => {
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
            tracing::warn!(
                media_item_id = %media_item_id,
                tmdb_id = %tmdb_id,
                error = %error,
                "获取远程人物信息失败"
            );
        }
    }
}

async fn analyze_imported_media(
    pool: &sqlx::PgPool,
    item_id: uuid::Uuid,
    file: &Path,
) -> Result<(), AppError> {
    if !file.exists() {
        return Ok(());
    }

    let analysis = if naming::is_strm(file) {
        match std::fs::read_to_string(file) {
            Ok(content) => {
                let Some(target_url) = naming::strm_target_from_text(&content) else {
                    tracing::debug!("扫描阶段跳过 .strm 分析，未找到有效 URL: {}", file.display());
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
                tracing::warn!("扫描阶段读取 .strm 文件失败 file={} error={}", file.display(), error);
                return Ok(());
            }
        }
    } else {
        match media_analyzer::analyze_media_file(file).await {
            Ok(analysis) => analysis,
            Err(error) => {
                tracing::warn!("扫描阶段分析媒体文件失败 file={} error={}", file.display(), error);
                return Ok(());
            }
        }
    };

    repository::update_media_item_metadata(pool, item_id, &analysis).await
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
        critic_rating: first_tag(&xml, &["criticrating", "critic_rating"])
            .and_then(|value| parse_decimal(&value)),
        runtime_ticks: first_tag(&xml, &["runtime"]).and_then(|value| parse_runtime_ticks(&value)),
        premiere_date: first_tag(&xml, &["premiered", "aired", "releasedate"])
            .and_then(|value| parse_date(&value)),
        status: parse_series_status(&xml),
        end_date: first_tag(&xml, &["enddate", "end_date", "ended", "lastaired", "last_air_date"])
            .and_then(|value| parse_date(&value)),
        air_days: parse_air_days(&xml),
        air_time: first_tag(&xml, &["airtime", "airs_time", "air_time"])
            .filter(|value| !value.trim().is_empty()),
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
        logo_image: None,
        thumb_image: None,
        remote_trailers: remote_trailer_urls(&xml),
    };

    for image in nfo_images(&xml, parent) {
        match image.kind.as_deref() {
            Some("fanart") | Some("backdrop") | Some("background") => {
                metadata.backdrop_image.get_or_insert(image.path);
            }
            Some("logo") | Some("clearlogo") => {
                metadata.logo_image.get_or_insert(image.path);
            }
            Some("thumb") | Some("landscape") => {
                metadata.thumb_image.get_or_insert(image.path);
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
            .or_else(|| object.get(*key).and_then(|value| value.as_i64().map(|id| id.to_string())))
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
    let normalized = value.trim();
    if normalized.eq_ignore_ascii_case("specials") || normalized.eq_ignore_ascii_case("extras") {
        return Some(0);
    }
    if let Some((_, season)) = parse_series_season_folder(value) {
        return Some(season);
    }
    season_folder_regex()
        .captures(value)
        .and_then(|captures| captures.name("season").or_else(|| captures.name("season_alt")))
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
