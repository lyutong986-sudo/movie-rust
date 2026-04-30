use crate::{
    auth::{AuthSession, OptionalAuthSession},
    error::AppError,
    metadata::{
        provider::{ExternalRemoteImage, MetadataProvider, MetadataProviderManager},
        tmdb::TmdbProvider,
    },
    models::{emby_id_to_uuid, ImageInfoDto, LibraryOptionsDto},
    repository,
    state::AppState,
    work_limiter::{WorkLimiterConfig, WorkLimiterKind},
};
use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, Method, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::FilterType;
use image::{ExtendedColorType, GenericImageView, ImageEncoder, ImageFormat};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::hash_map::DefaultHasher,
    collections::HashMap,
    hash::{Hash, Hasher},
    io::Cursor,
    path::PathBuf,
};
use tokio::fs;

use crate::http_client::SHARED as SHARED_HTTP_CLIENT;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Items/{item_id}/Images", get(list_item_images))
        .route("/Items/{item_id}/ThumbnailSet", get(get_item_thumbnail_set))
        .route(
            "/Items/{item_id}/RemoteImages",
            get(list_item_remote_images),
        )
        .route(
            "/Items/{item_id}/RemoteImages/Providers",
            get(list_item_remote_image_providers),
        )
        .route(
            "/Items/{item_id}/RemoteImages/Download",
            post(download_item_remote_image),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}",
            get(get_item_image)
                .head(get_item_image)
                .post(upload_item_image)
                .delete(delete_item_image),
        )
        .route(
            "/Items/{item_id}/Images/{image_type}/Delete",
            post(delete_item_image).delete(delete_item_image),
        )
        // 所有 `/Items/{id}/Images/{type}/...` 的子路径都落到这一条 wildcard
        // 上，由 tail 解析具体动作（index/Url/Index/Delete/多参数图片地址等）
        // 以避免和 axum 0.8 里的更具体路由发生冲突。
        .route(
            "/Items/{item_id}/Images/{image_type}/{*image_tail}",
            get(get_item_image_with_tail)
                .head(get_item_image_with_tail)
                .post(upload_item_image_with_tail)
                .delete(delete_item_image_with_tail),
        )
        .route(
            "/Persons/{name}/Images/{image_type}",
            get(get_person_image).head(get_person_image),
        )
        .route(
            "/Persons/{name}/Images/{image_type}/{*image_tail}",
            get(get_person_image_with_tail).head(get_person_image_with_tail),
        )
        .route(
            "/Artists/{name}/Images/{image_type}",
            get(get_person_image).head(get_person_image),
        )
        .route(
            "/Artists/{name}/Images/{image_type}/{*image_tail}",
            get(get_person_image_with_tail).head(get_person_image_with_tail),
        )
        .route(
            "/Genres/{name}/Images/{image_type}",
            get(get_genre_image).head(get_genre_image),
        )
        .route(
            "/Genres/{name}/Images/{image_type}/{*image_tail}",
            get(get_genre_image_with_tail).head(get_genre_image_with_tail),
        )
        .route(
            "/Studios/{name}/Images/{image_type}",
            get(get_genre_image).head(get_genre_image),
        )
        .route(
            "/Studios/{name}/Images/{image_type}/{*image_tail}",
            get(get_genre_image_with_tail).head(get_genre_image_with_tail),
        )
        .route(
            "/Tags/{name}/Images/{image_type}",
            get(get_genre_image).head(get_genre_image),
        )
        .route(
            "/Tags/{name}/Images/{image_type}/{*image_tail}",
            get(get_genre_image_with_tail).head(get_genre_image_with_tail),
        )
        .route(
            "/MusicGenres/{name}/Images/{image_type}",
            get(get_genre_image).head(get_genre_image),
        )
        .route(
            "/MusicGenres/{name}/Images/{image_type}/{*image_tail}",
            get(get_genre_image_with_tail).head(get_genre_image_with_tail),
        )
        .route("/Images/Remote", get(get_remote_image))
        .route(
            "/Users/{user_id}/Images/{image_type}",
            get(get_user_image)
                .head(get_user_image)
                .post(upload_user_image)
                .delete(delete_user_image),
        )
        .route(
            "/Users/{user_id}/Images/{image_type}/Delete",
            post(delete_user_image).delete(delete_user_image),
        )
        // 同上，`{*image_tail}` 会同时捕获 `{image_index}/Delete` 等路径。
        .route(
            "/Users/{user_id}/Images/{image_type}/{*image_tail}",
            get(get_user_image_with_tail)
                .head(get_user_image_with_tail)
                .post(delete_user_image_tail)
                .delete(delete_user_image_tail),
        )
}

async fn list_item_images(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Vec<ImageInfoDto>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let mut images = Vec::new();
    if let Some(item) = repository::get_media_item(&state.pool, item_id).await? {
        if let Some(path) = item.image_primary_path {
            images.push(ImageInfoDto {
                image_type: "Primary".to_string(),
                image_index: None,
                image_tag: item.date_modified.timestamp().to_string(),
                path,
            });
        }
        if let Some(path) = item.logo_path {
            images.push(ImageInfoDto {
                image_type: "Logo".to_string(),
                image_index: None,
                image_tag: item.date_modified.timestamp().to_string(),
                path,
            });
        }
        if let Some(path) = item.thumb_path {
            images.push(ImageInfoDto {
                image_type: "Thumb".to_string(),
                image_index: None,
                image_tag: item.date_modified.timestamp().to_string(),
                path,
            });
        }
        if let Some(path) = item.banner_path {
            images.push(ImageInfoDto {
                image_type: "Banner".to_string(),
                image_index: None,
                image_tag: item.date_modified.timestamp().to_string(),
                path,
            });
        }
        if let Some(path) = item.disc_path {
            images.push(ImageInfoDto {
                image_type: "Disc".to_string(),
                image_index: None,
                image_tag: item.date_modified.timestamp().to_string(),
                path,
            });
        }
        if let Some(path) = item.art_path {
            images.push(ImageInfoDto {
                image_type: "Art".to_string(),
                image_index: None,
                image_tag: item.date_modified.timestamp().to_string(),
                path,
            });
        }
        if let Some(path) = item.backdrop_path {
            images.push(ImageInfoDto {
                image_type: "Backdrop".to_string(),
                image_index: Some(0),
                image_tag: item.date_modified.timestamp().to_string(),
                path,
            });
        }
        for (i, path) in item.backdrop_paths.iter().enumerate() {
            images.push(ImageInfoDto {
                image_type: "Backdrop".to_string(),
                image_index: Some((i + 1) as i32),
                image_tag: item.date_modified.timestamp().to_string(),
                path: path.clone(),
            });
        }
    } else if let Some(person) = repository::get_person_by_uuid(&state.pool, item_id).await? {
        let image_tag = person
            .primary_image_tag
            .clone()
            .unwrap_or_else(|| "0".to_string());
        for image_type in ["Primary", "Backdrop", "Logo", "Thumb"] {
            if let Some(path) =
                repository::get_person_image_path(&state.pool, &person.id, image_type).await?
            {
                images.push(ImageInfoDto {
                    image_type: image_type.to_string(),
                    image_index: (image_type == "Backdrop").then_some(0),
                    image_tag: image_tag.clone(),
                    path,
                });
            }
        }
    } else if let Some(library) = repository::get_library(&state.pool, item_id).await? {
        if let Some(path) = library.primary_image_path.as_ref() {
            if !path.trim().is_empty() {
                let tag = library
                    .primary_image_tag
                    .clone()
                    .unwrap_or_else(|| library.created_at.timestamp().to_string());
                images.push(ImageInfoDto {
                    image_type: "Primary".to_string(),
                    image_index: None,
                    image_tag: tag,
                    path: path.clone(),
                });
            }
        }
    } else {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    }

    Ok(Json(images))
}

async fn item_image_url_response(
    state: &AppState,
    item_id_str: &str,
    image_type: &str,
    image_index: Option<i32>,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let normalized = normalized_item_image_type(image_type);
    let idx = image_index.unwrap_or(0);
    let has_image = if let Some(item) = repository::get_media_item(&state.pool, item_id).await? {
        match normalized.as_str() {
            "Backdrop" if idx == 0 => item.backdrop_path.is_some(),
            "Backdrop" if idx > 0 => item.backdrop_paths.get((idx - 1) as usize).is_some(),
            "Logo" => item.logo_path.is_some(),
            "Thumb" => item.thumb_path.is_some(),
            "Banner" => item.banner_path.is_some(),
            "Disc" => item.disc_path.is_some(),
            "Art" => item.art_path.is_some(),
            _ => item.image_primary_path.is_some(),
        }
    } else if let Some(person) = repository::get_person_by_uuid(&state.pool, item_id).await? {
        repository::get_person_image_path(&state.pool, &person.id, &normalized)
            .await?
            .is_some()
    } else {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    };
    if !has_image {
        return Err(AppError::NotFound("图片不存在".to_string()));
    }
    let mut url = format!("/Items/{item_id_str}/Images/{normalized}");
    if let Some(index) = image_index {
        url.push('/');
        url.push_str(&index.to_string());
    }
    Ok(Json(json!({ "Url": url })))
}

async fn list_item_remote_images(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<RemoteImagesQuery>,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let images = remote_images_for_item(
        &state,
        &item,
        query.image_type.as_deref(),
        query.provider_name.as_deref(),
        query.include_all_languages.unwrap_or(false),
        query.language.as_deref(),
    )
    .await?;
    let total_record_count = images.len();
    let start = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(100).clamp(1, 500) as usize;
    let items = images
        .into_iter()
        .skip(start)
        .take(limit)
        .map(remote_image_to_json)
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "Images": items,
        "TotalRecordCount": total_record_count,
        "StartIndex": start,
        "Providers": remote_image_provider_names(&state, &item)
    })))
}

async fn list_item_remote_image_providers(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Vec<String>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    Ok(Json(remote_image_provider_names(&state, &item)))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteImagesQuery {
    #[serde(default, rename = "Type", alias = "type")]
    image_type: Option<String>,
    #[serde(default, alias = "providerName")]
    provider_name: Option<String>,
    #[serde(default, alias = "includeAllLanguages", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    include_all_languages: Option<bool>,
    #[serde(default, alias = "Language", alias = "language")]
    language: Option<String>,
    #[serde(default, alias = "enableSeriesImages", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    enable_series_images: Option<bool>,
    #[serde(default, alias = "StartIndex", alias = "startIndex")]
    start_index: Option<i32>,
    #[serde(default, alias = "Limit", alias = "limit")]
    limit: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RemoteImageDownloadQuery {
    #[serde(default, alias = "imageUrl")]
    image_url: Option<String>,
    #[serde(default, rename = "Type", alias = "type")]
    image_type: Option<String>,
}

async fn download_item_remote_image(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<RemoteImageDownloadQuery>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let image_url = query
        .image_url
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 ImageUrl 参数".to_string()))?;
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    work_limiter_config(&state).await?;
    let tmdb_permit = state
        .work_limiters
        .acquire(WorkLimiterKind::TmdbMetadata)
        .await;
    let response = SHARED_HTTP_CLIENT
        .get(image_url)
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("下载远程图片失败: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::NotFound(format!(
            "远程图片不存在: {}",
            response.status()
        )));
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Internal(format!("读取远程图片失败: {error}")))?;
    drop(tmdb_permit);
    let mut headers = HeaderMap::new();
    if let Ok(value) = content_type.parse() {
        headers.insert(header::CONTENT_TYPE, value);
    }
    let _write_permit = state
        .work_limiters
        .acquire(WorkLimiterKind::LibraryScan)
        .await;
    upload_item_image_impl(
        session,
        state,
        item_id_str,
        query.image_type.unwrap_or_else(|| "Primary".to_string()),
        headers,
        bytes,
        None,
    )
    .await
}

async fn remote_images_for_item(
    state: &AppState,
    item: &crate::models::DbMediaItem,
    image_type: Option<&str>,
    provider_name: Option<&str>,
    include_all_languages: bool,
    language: Option<&str>,
) -> Result<Vec<ExternalRemoteImage>, AppError> {
    let mut images = Vec::new();
    push_existing_remote_image(&mut images, item.image_primary_path.as_deref(), "Primary");
    push_existing_remote_image(&mut images, item.logo_path.as_deref(), "Logo");
    push_existing_remote_image(&mut images, item.thumb_path.as_deref(), "Thumb");
    push_existing_remote_image(&mut images, item.banner_path.as_deref(), "Banner");
    push_existing_remote_image(&mut images, item.disc_path.as_deref(), "Disc");
    push_existing_remote_image(&mut images, item.art_path.as_deref(), "Art");
    push_existing_remote_image(&mut images, item.backdrop_path.as_deref(), "Backdrop");
    for path in &item.backdrop_paths {
        push_existing_remote_image(&mut images, Some(path.as_str()), "Backdrop");
    }

    if tmdb_remote_images_supported(item) {
        let library_options = item_library_options(state, item.id).await?;
        if let (Some(manager), Some((tmdb_id, season_number, episode_number, remote_media_type))) = (
            state.metadata_manager.as_ref(),
            tmdb_remote_image_context(state, item).await,
        ) {
            if let Some(provider) = item_tmdb_provider(state, manager, &library_options) {
                work_limiter_config(state).await?;
                let tmdb_permit = state
                    .work_limiters
                    .acquire(WorkLimiterKind::TmdbMetadata)
                    .await;
                let mut provider_images = if season_number.is_some() || episode_number.is_some() {
                    provider
                        .get_remote_images_for_child(
                            remote_media_type,
                            &tmdb_id,
                            season_number,
                            episode_number,
                        )
                        .await
                        .unwrap_or_default()
                } else {
                    provider
                        .get_remote_images(remote_media_type, &tmdb_id)
                        .await
                        .unwrap_or_default()
                };
                drop(tmdb_permit);
                images.append(&mut provider_images);
            }
        }
    }

    if let Some(image_type) = image_type.filter(|value| !value.trim().is_empty()) {
        let normalized = normalized_item_image_type(image_type);
        images.retain(|image| image.image_type.eq_ignore_ascii_case(&normalized));
    }
    if let Some(provider_name) = provider_name.filter(|value| !value.trim().is_empty()) {
        images.retain(|image| image.provider_name.eq_ignore_ascii_case(provider_name));
    }
    if let Some(language) = language
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalized_language_filter)
    {
        images.retain(|image| {
            image
                .language
                .as_deref()
                .is_none_or(|image_language| normalized_language_filter(image_language) == language)
        });
    } else if !include_all_languages {
        images.retain(|image| {
            image.language.as_deref().is_none_or(|language| {
                let language = normalized_language_filter(language);
                language == "zh" || language == "en"
            })
        });
    }

    images.sort_by(|left, right| {
        left.image_type
            .cmp(&right.image_type)
            .then_with(|| {
                right
                    .community_rating
                    .partial_cmp(&left.community_rating)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.provider_name.cmp(&right.provider_name))
            .then_with(|| left.url.cmp(&right.url))
    });
    images.dedup_by(|left, right| left.url == right.url && left.image_type == right.image_type);
    Ok(images)
}

fn normalized_language_filter(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or(language)
        .trim()
        .to_ascii_lowercase()
}

fn push_existing_remote_image(
    images: &mut Vec<ExternalRemoteImage>,
    url: Option<&str>,
    image_type: &str,
) {
    let Some(url) = url.filter(|value| {
        let value = value.trim();
        value.starts_with("http://") || value.starts_with("https://")
    }) else {
        return;
    };
    images.push(ExternalRemoteImage {
        provider_name: "LocalMetadata".to_string(),
        url: url.to_string(),
        thumbnail_url: Some(url.to_string()),
        image_type: image_type.to_string(),
        language: None,
        width: None,
        height: None,
        community_rating: None,
        vote_count: None,
    });
}

fn remote_image_to_json(image: ExternalRemoteImage) -> Value {
    json!({
        "ProviderName": image.provider_name,
        "Url": image.url,
        "ThumbnailUrl": image.thumbnail_url,
        "Type": image.image_type,
        "Language": image.language,
        "Width": image.width,
        "Height": image.height,
        "CommunityRating": image.community_rating,
        "VoteCount": image.vote_count,
        "RatingType": "Score"
    })
}

fn remote_image_provider_names(state: &AppState, item: &crate::models::DbMediaItem) -> Vec<String> {
    let mut providers = Vec::new();
    if item
        .image_primary_path
        .as_deref()
        .or(item.backdrop_path.as_deref())
        .or(item.logo_path.as_deref())
        .or(item.thumb_path.as_deref())
        .or(item.banner_path.as_deref())
        .or(item.disc_path.as_deref())
        .or(item.art_path.as_deref())
        .or(item.backdrop_paths.first().map(String::as_str))
        .is_some_and(|value| value.starts_with("http://") || value.starts_with("https://"))
    {
        providers.push("LocalMetadata".to_string());
    }
    if tmdb_remote_images_supported(item)
        && state
            .metadata_manager
            .as_ref()
            .and_then(|manager| manager.get_provider("tmdb"))
            .is_some()
    {
        providers.push("TheMovieDb".to_string());
    }
    providers
}

fn tmdb_id_for_item(item: &crate::models::DbMediaItem) -> Option<String> {
    item.provider_ids.as_object().and_then(|providers| {
        ["Tmdb", "TMDb", "tmdb", "TheMovieDb"]
            .iter()
            .find_map(|key| providers.get(*key).and_then(Value::as_str))
            .map(ToOwned::to_owned)
    })
}

async fn tmdb_remote_image_context(
    state: &AppState,
    item: &crate::models::DbMediaItem,
) -> Option<(String, Option<i32>, Option<i32>, &'static str)> {
    if item.item_type.eq_ignore_ascii_case("Movie") {
        return tmdb_id_for_item(item).map(|id| (id, None, None, "Movie"));
    }
    if item.item_type.eq_ignore_ascii_case("Series") {
        return tmdb_id_for_item(item).map(|id| (id, None, None, "Series"));
    }
    if item.item_type.eq_ignore_ascii_case("Season") {
        let tmdb_id = if let Some(id) = tmdb_id_for_item(item) {
            Some(id)
        } else {
            let parent_id = item.parent_id?;
            let parent = repository::get_media_item(&state.pool, parent_id)
                .await
                .ok()
                .flatten()?;
            tmdb_id_for_item(&parent)
        };
        return tmdb_id.map(|id| (id, item.index_number, None, "Season"));
    }
    if item.item_type.eq_ignore_ascii_case("Episode") {
        let season_number = item.parent_index_number;
        let episode_number = item.index_number;
        let parent_id = item.parent_id?;
        let season = repository::get_media_item(&state.pool, parent_id)
            .await
            .ok()
            .flatten()?;
        let tmdb_id = if let Some(id) = tmdb_id_for_item(&season) {
            Some(id)
        } else {
            let series_id = season.parent_id?;
            let series = repository::get_media_item(&state.pool, series_id)
                .await
                .ok()
                .flatten()?;
            tmdb_id_for_item(&series)
        };
        return tmdb_id.map(|id| (id, season_number, episode_number, "Episode"));
    }
    None
}

fn tmdb_remote_images_supported(item: &crate::models::DbMediaItem) -> bool {
    item.item_type.eq_ignore_ascii_case("Movie")
        || item.item_type.eq_ignore_ascii_case("Series")
        || item.item_type.eq_ignore_ascii_case("Season")
        || item.item_type.eq_ignore_ascii_case("Episode")
}

async fn item_library_options(
    state: &AppState,
    item_id: uuid::Uuid,
) -> Result<LibraryOptionsDto, AppError> {
    Ok(repository::get_library_for_media_item(&state.pool, item_id)
        .await?
        .map(|library| repository::library_options(&library))
        .unwrap_or_default())
}

fn item_tmdb_provider<'a>(
    state: &'a AppState,
    metadata_manager: &'a MetadataProviderManager,
    library_options: &'a LibraryOptionsDto,
) -> Option<Box<dyn MetadataProvider + 'a>> {
    if let Some(api_key) = &state.config.tmdb_api_key {
        let preferred_metadata_language = library_options
            .preferred_metadata_language
            .as_deref()
            .unwrap_or(&state.config.preferred_metadata_language);
        let metadata_country_code = library_options
            .metadata_country_code
            .as_deref()
            .unwrap_or(&state.config.metadata_country_code);
        return Some(Box::new(TmdbProvider::new_with_preferences(
            api_key.clone(),
            preferred_metadata_language,
            metadata_country_code,
        )));
    }

    metadata_manager
        .get_provider("tmdb")
        .map(|provider| Box::new(RouteProviderRef { inner: provider }) as Box<dyn MetadataProvider>)
}

async fn work_limiter_config(state: &AppState) -> Result<WorkLimiterConfig, AppError> {
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let config = WorkLimiterConfig {
        library_scan_limit: startup.library_scan_thread_count.max(1) as u32,
        media_analysis_limit: startup.strm_analysis_thread_count.max(1) as u32,
        tmdb_metadata_limit: startup.tmdb_metadata_thread_count.max(1) as u32,
    };
    state.work_limiters.configure(config).await;
    Ok(config)
}

pub fn item_image_storage_path_pub(
    state: &AppState,
    item: &crate::models::DbMediaItem,
    library_options: &LibraryOptionsDto,
    image_type: &str,
    backdrop_index: Option<i32>,
    extension: &str,
) -> PathBuf {
    item_image_storage_path(state, item, library_options, image_type, backdrop_index, extension)
}

fn item_image_storage_path(
    state: &AppState,
    item: &crate::models::DbMediaItem,
    library_options: &LibraryOptionsDto,
    image_type: &str,
    backdrop_index: Option<i32>,
    extension: &str,
) -> PathBuf {
    if library_options.save_local_metadata {
        let item_path = PathBuf::from(&item.path);
        if let Some(path) =
            item_image_target_path(item, &item_path, image_type, backdrop_index, extension)
        {
            return path;
        }
    }

    let mut stem = format!("{}-{}", item.id, image_type.to_ascii_lowercase());
    if image_type.eq_ignore_ascii_case("Backdrop") {
        let idx = backdrop_index.unwrap_or(0);
        if idx > 0 {
            stem.push_str(&format!("-{idx}"));
        }
    }
    state
        .config
        .static_dir
        .join("item-images")
        .join(format!("{stem}.{extension}"))
}

fn item_image_target_path(
    item: &crate::models::DbMediaItem,
    item_path: &PathBuf,
    image_type: &str,
    backdrop_index: Option<i32>,
    extension: &str,
) -> Option<PathBuf> {
    let folder = item_path.parent().unwrap_or(item_path.as_path());
    let stem = item_path.file_stem()?.to_string_lossy();
    let normalized = image_type.to_ascii_lowercase();
    let backdrop_idx = backdrop_index.unwrap_or(0);

    if item.item_type.eq_ignore_ascii_case("Episode") {
        return match normalized.as_str() {
            "primary" | "thumb" => Some(folder.join(format!("{stem}-thumb.{extension}"))),
            "backdrop" => {
                if backdrop_idx == 0 {
                    Some(folder.join(format!("{stem}-fanart.{extension}")))
                } else {
                    Some(folder.join(format!("{stem}-fanart{backdrop_idx}.{extension}")))
                }
            }
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
        return match normalized.as_str() {
            "primary" => Some(folder.join(format!("{prefix}-poster.{extension}"))),
            "backdrop" => {
                if backdrop_idx == 0 {
                    Some(folder.join(format!("{prefix}-fanart.{extension}")))
                } else {
                    Some(folder.join(format!("{prefix}-fanart{backdrop_idx}.{extension}")))
                }
            }
            "logo" => Some(folder.join(format!("{prefix}-logo.{extension}"))),
            "thumb" => Some(folder.join(format!("{prefix}-landscape.{extension}"))),
            "banner" => Some(folder.join(format!("{prefix}-banner.{extension}"))),
            "disc" => Some(folder.join(format!("{prefix}-disc.{extension}"))),
            "art" => Some(folder.join(format!("{prefix}-clearart.{extension}"))),
            _ => None,
        };
    }

    let filename: String = match normalized.as_str() {
        "primary" => "poster".to_string(),
        "backdrop" => {
            if backdrop_idx == 0 {
                "fanart".to_string()
            } else {
                format!("fanart{backdrop_idx}")
            }
        }
        "logo" => "logo".to_string(),
        "thumb" => "landscape".to_string(),
        "banner" => "banner".to_string(),
        "disc" => "disc".to_string(),
        "art" => "clearart".to_string(),
        _ => return None,
    };

    if item_path.is_dir() {
        Some(folder.join(format!("{filename}.{extension}")))
    } else {
        Some(folder.join(format!("{stem}-{filename}.{extension}")))
    }
}

struct RouteProviderRef<'a> {
    inner: &'a dyn MetadataProvider,
}

#[async_trait::async_trait]
impl MetadataProvider for RouteProviderRef<'_> {
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

async fn get_item_image(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((item_id_str, image_type)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_item_image(session.0, state, item_id_str, image_type, None, request).await
}

async fn get_item_image_with_tail(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((item_id_str, image_type, image_tail)): Path<(String, String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    if image_tail
        .split('/')
        .last()
        .is_some_and(|segment| segment.eq_ignore_ascii_case("Url"))
    {
        let image_index = image_tail
            .split('/')
            .next()
            .filter(|segment| !segment.eq_ignore_ascii_case("Url"))
            .and_then(|value| value.parse::<i32>().ok());
        let Json(value) =
            item_image_url_response(&state, &item_id_str, &image_type, image_index).await?;
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(value.to_string()))
            .map_err(|error| AppError::Internal(format!("构建图片URL响应失败: {error}")));
    }
    let image_index = image_tail
        .split('/')
        .next()
        .and_then(|value| value.parse::<i32>().ok());
    serve_item_image(
        session.0,
        state,
        item_id_str,
        image_type,
        image_index,
        request,
    )
    .await
}

async fn upload_item_image(
    session: AuthSession,
    State(state): State<AppState>,
    Path((item_id_str, image_type)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    upload_item_image_impl(session, state, item_id_str, image_type, headers, body, None).await
}

async fn upload_item_image_with_tail(
    session: AuthSession,
    State(state): State<AppState>,
    Path((item_id_str, image_type, image_tail)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    // 统一将 POST `/{image_index}/Delete`、`/Delete` 视作删除动作，
    // 与 Emby Web Dashboard 的行为保持一致；而 `/Index` 结尾的 POST
    // 仅表示调整显示顺序，目前按管理员校验后返回 NO_CONTENT。
    match last_tail_segment(&image_tail).to_ascii_lowercase().as_str() {
        "delete" => {
            let backdrop_index = tail_leading_backdrop_index(&image_type, &image_tail);
            return delete_item_image_impl(session, state, item_id_str, image_type, backdrop_index)
                .await;
        }
        "index" => {
            if !session.is_admin {
                return Err(AppError::Forbidden);
            }
            return Ok(StatusCode::NO_CONTENT);
        }
        _ => {}
    }
    let backdrop_index = tail_leading_backdrop_index(&image_type, &image_tail);
    upload_item_image_impl(
        session,
        state,
        item_id_str,
        image_type,
        headers,
        body,
        backdrop_index,
    )
    .await
}

fn last_tail_segment(tail: &str) -> &str {
    tail.rsplit('/')
        .find(|segment| !segment.is_empty())
        .unwrap_or("")
}

async fn delete_item_image(
    session: AuthSession,
    State(state): State<AppState>,
    Path((item_id_str, image_type)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    delete_item_image_impl(session, state, item_id_str, image_type, None).await
}

async fn delete_item_image_with_tail(
    session: AuthSession,
    State(state): State<AppState>,
    Path((item_id_str, image_type, image_tail)): Path<(String, String, String)>,
) -> Result<StatusCode, AppError> {
    let backdrop_index = tail_leading_backdrop_index(&image_type, &image_tail);
    delete_item_image_impl(session, state, item_id_str, image_type, backdrop_index).await
}

async fn get_person_image(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((name, image_type)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_person_image(state, name, image_type, request).await
}

async fn get_person_image_with_tail(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((name, image_type, _image_tail)): Path<(String, String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_person_image(state, name, image_type, request).await
}

async fn get_genre_image(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((name, image_type)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_genre_image(state, name, image_type, request).await
}

async fn get_genre_image_with_tail(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((name, image_type, _image_tail)): Path<(String, String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_genre_image(state, name, image_type, request).await
}

async fn get_user_image(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((user_id, image_type)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_user_image(session.0, state, user_id, image_type, request).await
}

async fn get_user_image_with_tail(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((user_id, image_type, _image_tail)): Path<(String, String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_user_image(session.0, state, user_id, image_type, request).await
}

async fn delete_user_image_tail(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id_str, image_type, _image_tail)): Path<(String, String, String)>,
) -> Result<StatusCode, AppError> {
    delete_user_image(session, State(state), Path((user_id_str, image_type))).await
}

async fn serve_person_image(
    state: AppState,
    name: String,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let image_query = parse_item_image_query(request.uri());
    let path = resolve_person_image_path(&state, &name, &image_type)
        .await?
        .ok_or_else(|| AppError::NotFound("图片不存在".to_string()))?;
    serve_image(&path, request, &image_query).await
}

async fn upload_item_image_impl(
    session: AuthSession,
    state: AppState,
    item_id_str: String,
    image_type: String,
    headers: HeaderMap,
    body: Bytes,
    backdrop_index: Option<i32>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    if body.is_empty() {
        return Err(AppError::BadRequest("图片内容不能为空".to_string()));
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = match repository::get_media_item(&state.pool, item_id).await? {
        Some(item) => item,
        None => {
            if let Some(library) =
                repository::get_library(&state.pool, item_id).await?
            {
                let normalized = normalized_item_image_type(&image_type);
                if !normalized.eq_ignore_ascii_case("Primary") {
                    return Err(AppError::BadRequest(format!(
                        "媒体库仅支持 Primary 封面，收到 {normalized}"
                    )));
                }
                let extension = image_extension_from_headers(&headers);
                let dir = state.config.static_dir.join("library-images");
                fs::create_dir_all(&dir).await.map_err(AppError::Io)?;
                let path = dir.join(format!("{}-primary.{}", library.id, extension));
                fs::write(&path, &body).await.map_err(AppError::Io)?;
                repository::update_library_image_path(
                    &state.pool,
                    library.id,
                    Some(&path.to_string_lossy()),
                )
                .await?;
                return Ok(StatusCode::NO_CONTENT);
            }
            return Err(AppError::NotFound("媒体条目不存在".to_string()));
        }
    };

    let image_type = normalized_item_image_type(&image_type);
    let extension = image_extension_from_headers(&headers);
    let library_options = item_library_options(&state, item_id).await?;
    let path = item_image_storage_path(
        &state,
        &item,
        &library_options,
        &image_type,
        backdrop_index,
        &extension,
    );
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(AppError::Io)?;
    }
    fs::write(&path, &body).await.map_err(AppError::Io)?;

    repository::update_media_item_image_path(
        &state.pool,
        item_id,
        &image_type,
        Some(&path.to_string_lossy()),
        backdrop_index,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_item_image_impl(
    session: AuthSession,
    state: AppState,
    item_id_str: String,
    image_type: String,
    backdrop_index: Option<i32>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = match repository::get_media_item(&state.pool, item_id).await? {
        Some(item) => item,
        None => {
            if let Some(library) =
                repository::get_library(&state.pool, item_id).await?
            {
                let normalized = normalized_item_image_type(&image_type);
                if !normalized.eq_ignore_ascii_case("Primary") {
                    return Err(AppError::BadRequest(format!(
                        "媒体库仅支持 Primary 封面，收到 {normalized}"
                    )));
                }
                if let Some(path) = library.primary_image_path.as_ref() {
                    let path_buf = PathBuf::from(path);
                    if path_buf.exists()
                        && path_buf.starts_with(state.config.static_dir.join("library-images"))
                    {
                        let _ = fs::remove_file(path_buf).await;
                    }
                }
                repository::update_library_image_path(&state.pool, library.id, None).await?;
                return Ok(StatusCode::NO_CONTENT);
            }
            return Err(AppError::NotFound("媒体条目不存在".to_string()));
        }
    };
    let image_type = normalized_item_image_type(&image_type);
    let idx = backdrop_index.unwrap_or(0);
    let current_path = match image_type.as_str() {
        "Backdrop" if idx > 0 => item.backdrop_paths.get((idx - 1) as usize).cloned(),
        "Backdrop" => item.backdrop_path,
        "Logo" => item.logo_path,
        "Thumb" => item.thumb_path,
        "Banner" => item.banner_path,
        "Disc" => item.disc_path,
        "Art" => item.art_path,
        _ => item.image_primary_path,
    };
    if let Some(path) = current_path {
        let path_buf = PathBuf::from(&path);
        if path_buf.exists() && path_buf.starts_with(state.config.static_dir.join("item-images")) {
            let _ = fs::remove_file(path_buf).await;
        }
    }
    repository::update_media_item_image_path(
        &state.pool,
        item_id,
        &image_type,
        None,
        backdrop_index,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn serve_genre_image(
    state: AppState,
    name: String,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let image_query = parse_item_image_query(request.uri());
    let path = repository::get_genre_image_path(&state.pool, &name, &image_type)
        .await?
        .ok_or_else(|| AppError::NotFound("图片不存在".to_string()))?;
    serve_image(&path, request, &image_query).await
}

async fn serve_item_image(
    _session: Option<AuthSession>,
    state: AppState,
    item_id_str: String,
    image_type: String,
    image_index: Option<i32>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let image_query = parse_item_image_query(request.uri());
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;

    if image_type.eq_ignore_ascii_case("Chapter") {
        let chapter_index = image_index.unwrap_or(0);
        let chapters = repository::get_media_chapters(&state.pool, item_id).await?;
        let chapter = chapters
            .into_iter()
            .find(|chapter| chapter.chapter_index == chapter_index)
            .ok_or_else(|| AppError::NotFound("章节图片不存在".to_string()))?;
        let path = chapter
            .image_path
            .ok_or_else(|| AppError::NotFound("章节图片不存在".to_string()))?;
        return serve_image(&path, request, &image_query).await;
    }

    if let Some(path) =
        repository::get_missing_episode_image_path(&state.pool, item_id, &image_type).await?
    {
        return serve_image(&path, request, &image_query).await;
    }

    let Some(item) = repository::get_media_item(&state.pool, item_id).await? else {
        if let Some(person) = repository::get_person_by_uuid(&state.pool, item_id).await? {
            if let Some(path) = resolve_person_image_path(&state, &person.id, &image_type).await? {
                return serve_image(&path, request, &image_query).await;
            }
            return Err(AppError::NotFound("图片不存在".to_string()));
        }
        if let Some(library) = repository::get_library(&state.pool, item_id).await? {
            let normalized = normalized_item_image_type(&image_type);
            if normalized.eq_ignore_ascii_case("Primary") {
                if let Some(path) = library.primary_image_path.as_ref() {
                    if !path.trim().is_empty() {
                        return serve_image(path, request, &image_query).await;
                    }
                }
                if let Some((_, child_path, _)) =
                    repository::first_library_child_image(&state.pool, library.id).await?
                {
                    return serve_image(&child_path, request, &image_query).await;
                }
            }
            return Err(AppError::NotFound("图片不存在".to_string()));
        }
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    };

    let normalized = normalized_item_image_type(&image_type);
    let idx = image_index.unwrap_or(0);
    // 空字符串与未配置路径同等对待（DB 中为 NULL / 空白时仍尝试 TMDB 等回退）
    let path_opt = match normalized.as_str() {
        "Backdrop" if idx == 0 => item.backdrop_path.clone(),
        "Backdrop" if idx > 0 => item.backdrop_paths.get((idx - 1) as usize).cloned(),
        "Logo" => item.logo_path.clone(),
        "Thumb" => item.thumb_path.clone().or(item.backdrop_path.clone()),
        "Banner" => item.banner_path.clone(),
        "Disc" => item.disc_path.clone(),
        "Art" => item.art_path.clone(),
        _ => item.image_primary_path.clone(),
    }
    .filter(|p| !p.trim().is_empty());

    if let Some(path) = path_opt {
        let method = request.method().clone();
        let result = serve_image(&path, request, &image_query).await;
        if let Err(AppError::NotFound(_)) = &result {
            if !path.starts_with("http://") && !path.starts_with("https://") {
                if let Some(fallback_url) =
                    find_tmdb_image_fallback(&state.pool, &item, &normalized, state.config.tmdb_api_key.as_deref()).await
                {
                    tracing::debug!(
                        item_id = %item.id,
                        image_type = %normalized,
                        "本地图片文件不存在，回退到 TMDB 远程代理"
                    );
                    let fallback_req = Request::builder()
                        .method(method)
                        .body(Body::empty())
                        .unwrap_or_default();
                    return serve_remote_image(&fallback_url, &image_query, fallback_req).await;
                }
            }
        }
        return result;
    }

    if let Some(fallback_url) = find_tmdb_image_fallback(&state.pool, &item, &normalized, state.config.tmdb_api_key.as_deref()).await {
        tracing::debug!(
            item_id = %item.id,
            image_type = %normalized,
            "数据库未配置图片路径，回退到 TMDB 远程代理"
        );
        let fallback_req = Request::builder()
            .method(request.method().clone())
            .body(Body::empty())
            .unwrap_or_default();
        return serve_remote_image(&fallback_url, &image_query, fallback_req).await;
    }

    Err(AppError::NotFound("图片不存在".to_string()))
}

/// 数据库无路径 / 空白路径时，以及本地文件缺失时，尝试从 series_episode_catalog 或 TMDB API 构建回退 URL。
async fn find_tmdb_image_fallback(
    pool: &sqlx::PgPool,
    item: &crate::models::DbMediaItem,
    image_type: &str,
    tmdb_api_key: Option<&str>,
) -> Option<String> {
    let kind = item.item_type.as_str();

    if kind.eq_ignore_ascii_case("Episode") {
        if image_type.eq_ignore_ascii_case("Primary") || image_type.eq_ignore_ascii_case("Thumb") {
            let season_no = item.parent_index_number?;
            let ep_no = item.index_number?;
            // Episode -> parent_id=Season -> parent_id=Series
            let season_id = item.parent_id?;
            let row: Option<(Option<uuid::Uuid>,)> = sqlx::query_as(
                "SELECT parent_id FROM media_items WHERE id = $1",
            )
            .bind(season_id)
            .fetch_optional(pool)
            .await
            .ok()?;
            let series_id = row?.0?;
            let catalog_row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT image_path FROM series_episode_catalog \
                 WHERE series_id = $1 AND season_number = $2 AND episode_number = $3 LIMIT 1",
            )
            .bind(series_id)
            .bind(season_no)
            .bind(ep_no)
            .fetch_optional(pool)
            .await
            .ok()?;
            return catalog_row?.0;
        }
    }

    if kind.eq_ignore_ascii_case("Season") {
        let series_id = item.parent_id?;
        let season_no = item.index_number.unwrap_or(1);
        let row: Option<(serde_json::Value,)> = sqlx::query_as(
            "SELECT provider_ids FROM media_items WHERE id = $1",
        )
        .bind(series_id)
        .fetch_optional(pool)
        .await
        .ok()?;
        let series_providers = row?.0;
        let tmdb_id = series_providers.get("Tmdb")?.as_str()?;
        let key = tmdb_api_key?;
        let api_url = format!(
            "https://api.themoviedb.org/3/tv/{tmdb_id}/season/{season_no}?api_key={key}"
        );
        let resp = SHARED_HTTP_CLIENT.get(&api_url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let json: serde_json::Value = resp.json().await.ok()?;
        let poster = json.get("poster_path")?.as_str()?;
        return Some(format!("https://image.tmdb.org/t/p/original{poster}"));
    }

    if kind.eq_ignore_ascii_case("Series") || kind.eq_ignore_ascii_case("Movie") {
        let tmdb_id = item.provider_ids.get("Tmdb")?.as_str()?;
        let key = tmdb_api_key?;
        let media_type = if kind.eq_ignore_ascii_case("Series") { "tv" } else { "movie" };
        let api_url = format!(
            "https://api.themoviedb.org/3/{media_type}/{tmdb_id}?api_key={key}"
        );
        let resp = SHARED_HTTP_CLIENT.get(&api_url).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let json: serde_json::Value = resp.json().await.ok()?;
        let path_field = match image_type.to_ascii_lowercase().as_str() {
            "backdrop" => "backdrop_path",
            "logo" => return None,
            _ => "poster_path",
        };
        let img_path = json.get(path_field)?.as_str()?;
        return Some(format!("https://image.tmdb.org/t/p/original{img_path}"));
    }

    None
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ThumbnailSetResponse {
    thumbnails: Vec<ThumbnailSetItem>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ThumbnailSetItem {
    position_ticks: i64,
    image_tag: String,
}

async fn get_item_thumbnail_set(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<ThumbnailSetResponse>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let chapters = repository::get_media_chapters(&state.pool, item_id).await?;
    let thumbnails = chapters
        .into_iter()
        .filter_map(|chapter| {
            chapter.image_path.as_ref()?;
            Some(ThumbnailSetItem {
                position_ticks: chapter.start_position_ticks,
                image_tag: chapter.updated_at.timestamp().to_string(),
            })
        })
        .collect();

    Ok(Json(ThumbnailSetResponse { thumbnails }))
}

async fn serve_user_image(
    session: Option<AuthSession>,
    state: AppState,
    user_id: String,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let user_id = emby_id_to_uuid(&user_id)
        .map_err(|_| AppError::BadRequest("无效的用户ID格式".to_string()))?;
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let can_view = match session {
        Some(session) => {
            session.is_admin || session.user_id == user_id || (!user.is_hidden && !user.is_disabled)
        }
        None => !user.is_hidden && !user.is_disabled,
    };
    if !can_view {
        return Err(AppError::NotFound("图片不存在".to_string()));
    }
    let path = repository::get_user_image_path(&state.pool, user_id, &image_type)
        .await?
        .ok_or_else(|| AppError::NotFound("图片不存在".to_string()))?;

    let image_query = parse_item_image_query(request.uri());
    serve_image(&path, request, &image_query).await
}

async fn upload_user_image(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, image_type)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    let user_id = emby_id_to_uuid(&user_id)
        .map_err(|_| AppError::BadRequest("无效的用户ID格式".to_string()))?;
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    if body.is_empty() {
        return Err(AppError::BadRequest("图片内容不能为空".to_string()));
    }

    let image_type = normalized_user_image_type(&image_type);
    let extension = image_extension_from_headers(&headers);
    let dir = state.config.static_dir.join("user-images");
    fs::create_dir_all(&dir).await.map_err(AppError::Io)?;

    let filename = format!(
        "{}-{}.{}",
        user_id,
        image_type.to_ascii_lowercase(),
        extension
    );
    let path = dir.join(filename);
    fs::write(&path, &body).await.map_err(AppError::Io)?;

    repository::update_user_image_path(
        &state.pool,
        user_id,
        &image_type,
        Some(&path.to_string_lossy()),
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user_image(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, image_type)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let user_id = emby_id_to_uuid(&user_id)
        .map_err(|_| AppError::BadRequest("无效的用户ID格式".to_string()))?;
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }

    let image_type = normalized_user_image_type(&image_type);
    if let Some(path) = repository::get_user_image_path(&state.pool, user_id, &image_type).await? {
        let path_buf = PathBuf::from(&path);
        if path_buf.exists() && path_buf.starts_with(state.config.static_dir.join("user-images")) {
            let _ = fs::remove_file(&path_buf).await;
        }
    }

    repository::update_user_image_path(&state.pool, user_id, &image_type, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_remote_image(
    Query(params): Query<HashMap<String, String>>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let image_url = params
        .get("ImageUrl")
        .ok_or_else(|| AppError::BadRequest("缺少 ImageUrl 参数".to_string()))?;

    let image_query = parse_item_image_query(request.uri());
    serve_remote_image(image_url, &image_query, request).await
}

fn normalized_user_image_type(image_type: &str) -> String {
    match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => "Backdrop".to_string(),
        "logo" => "Logo".to_string(),
        _ => "Primary".to_string(),
    }
}

fn normalized_item_image_type(image_type: &str) -> String {
    match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => "Backdrop".to_string(),
        "logo" => "Logo".to_string(),
        "thumb" => "Thumb".to_string(),
        "banner" => "Banner".to_string(),
        "disc" => "Disc".to_string(),
        "art" => "Art".to_string(),
        _ => "Primary".to_string(),
    }
}

/// Emby 风格：`/Images/Backdrop/1/...` 的首段为索引。
fn tail_leading_backdrop_index(image_type: &str, tail: &str) -> Option<i32> {
    if !image_type.eq_ignore_ascii_case("Backdrop") {
        return None;
    }
    tail.split('/')
        .find(|segment| !segment.is_empty())
        .and_then(|segment| segment.parse().ok())
}

fn image_extension_from_headers(headers: &HeaderMap) -> &'static str {
    image_extension_from_content_type(
        headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default(),
    )
}

fn image_extension_from_content_type(content_type: &str) -> &'static str {
    match content_type.to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
}

async fn resolve_person_image_path(
    state: &AppState,
    person_id_or_name: &str,
    image_type: &str,
) -> Result<Option<String>, AppError> {
    let Some(path) =
        repository::get_person_image_path(&state.pool, person_id_or_name, image_type).await?
    else {
        return Ok(None);
    };

    if !path.starts_with("http://") && !path.starts_with("https://") {
        return Ok(Some(path));
    }

    let person = if let Ok(person_id) = emby_id_to_uuid(person_id_or_name) {
        repository::get_person_by_uuid(&state.pool, person_id).await?
    } else {
        Some(repository::get_person_by_name(&state.pool, person_id_or_name).await?)
    }
    .ok_or_else(|| AppError::NotFound("人物不存在".to_string()))?;

    let normalized = normalized_item_image_type(image_type);
    let response = SHARED_HTTP_CLIENT
        .get(&path)
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("下载人物图片失败: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::NotFound(format!(
            "远程人物图片不存在: {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let extension = image_extension_from_content_type(&content_type);
    let bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Internal(format!("读取人物图片失败: {error}")))?;

    let dir = state.config.static_dir.join("person-images");
    fs::create_dir_all(&dir).await.map_err(AppError::Io)?;
    let filename = format!(
        "{}-{}.{}",
        person.id,
        normalized.to_ascii_lowercase(),
        extension
    );
    let local_path = dir.join(filename);
    fs::write(&local_path, &bytes).await.map_err(AppError::Io)?;

    let person_uuid = emby_id_to_uuid(&person.id)
        .map_err(|_| AppError::Internal("人物ID格式无效".to_string()))?;
    let local_path_text = local_path.to_string_lossy().to_string();
    repository::update_person_image_path(
        &state.pool,
        person_uuid,
        &normalized,
        Some(&local_path_text),
    )
    .await?;

    Ok(Some(local_path_text))
}

/// Emby / Jellyfin 风格图片查询：`maxWidth`、`maxHeight`、`quality`、`format` 等。
#[derive(Debug, Default, Clone)]
struct ItemImageQuery {
    max_width: Option<u32>,
    max_height: Option<u32>,
    width: Option<u32>,
    height: Option<u32>,
    quality: Option<u8>,
    format: Option<String>,
}

fn parse_item_image_query(uri: &axum::http::Uri) -> ItemImageQuery {
    let mut q = ItemImageQuery::default();
    let Some(query_str) = uri.query() else {
        return q;
    };
    for (key, value) in url::form_urlencoded::parse(query_str.as_bytes()) {
        let k = key.to_ascii_lowercase();
        match k.as_str() {
            "maxwidth" => q.max_width = value.parse().ok(),
            "maxheight" => q.max_height = value.parse().ok(),
            "width" => q.width = value.parse().ok(),
            "height" => q.height = value.parse().ok(),
            "quality" => q.quality = value.parse().ok(),
            "format" => {
                let s = value.into_owned();
                if !s.trim().is_empty() {
                    q.format = Some(s);
                }
            }
            _ => {}
        }
    }
    q
}

impl ItemImageQuery {
    fn needs_processing(&self) -> bool {
        if self.max_width.is_some()
            || self.max_height.is_some()
            || self.width.is_some()
            || self.height.is_some()
        {
            return true;
        }
        if let Some(quality) = self.quality {
            if quality < 100 {
                return true;
            }
        }
        if let Some(ref f) = self.format {
            let f = f.trim().to_ascii_lowercase();
            if matches!(f.as_str(), "png" | "webp" | "jpg" | "jpeg") {
                return true;
            }
        }
        false
    }

    fn etag_suffix(&self) -> String {
        if !self.needs_processing() {
            return String::new();
        }
        let mut h = DefaultHasher::new();
        self.max_width.hash(&mut h);
        self.max_height.hash(&mut h);
        self.width.hash(&mut h);
        self.height.hash(&mut h);
        self.quality.hash(&mut h);
        if let Some(ref f) = self.format {
            f.to_ascii_lowercase().hash(&mut h);
        }
        format!("-t{:x}", h.finish())
    }
}

fn scale_to_fit(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w <= max_w && h <= max_h {
        return (w, h);
    }
    let rw = max_w as f64 / w.max(1) as f64;
    let rh = max_h as f64 / h.max(1) as f64;
    let r = rw.min(rh).min(1.0);
    let nw = ((w as f64 * r).round() as u32).max(1);
    let nh = ((h as f64 * r).round() as u32).max(1);
    (nw, nh)
}

fn resolve_target_size(w: u32, h: u32, query: &ItemImageQuery) -> Option<(u32, u32)> {
    let max_w = query.max_width.or(query.width);
    let max_h = query.max_height.or(query.height);
    if max_w.is_none() && max_h.is_none() {
        return None;
    }
    let max_w = max_w.unwrap_or(u32::MAX);
    let max_h = max_h.unwrap_or(u32::MAX);
    Some(scale_to_fit(w, h, max_w.max(1), max_h.max(1)))
}

fn apply_item_image_transform(
    bytes: &[u8],
    query: &ItemImageQuery,
) -> Result<Option<(Vec<u8>, String)>, AppError> {
    if !query.needs_processing() {
        return Ok(None);
    }
    let img = image::load_from_memory(bytes).map_err(|_| {
        AppError::Internal("图片解码失败，无法按 Emby 参数缩放或重编码".to_string())
    })?;
    let (w, h) = img.dimensions();
    let img = if let Some((tw, th)) = resolve_target_size(w, h, query) {
        if tw != w || th != h {
            img.resize(tw, th, FilterType::Lanczos3)
        } else {
            img
        }
    } else {
        img
    };

    let fmt = query
        .format
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase());
    let want_png = matches!(fmt.as_deref(), Some("png"));

    if want_png {
        let mut out = Vec::new();
        let mut cursor = Cursor::new(&mut out);
        img.write_to(&mut cursor, ImageFormat::Png)
            .map_err(|e| AppError::Internal(format!("PNG 编码失败: {e}")))?;
        return Ok(Some((out, "image/png".to_string())));
    }

    let quality = query.quality.unwrap_or(90).clamp(1, 100);
    let mut out = Vec::new();
    let rgb = img.to_rgb8();
    let encoder = JpegEncoder::new_with_quality(&mut out, quality);
    encoder
        .write_image(rgb.as_raw(), rgb.width(), rgb.height(), ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Internal(format!("JPEG 编码失败: {e}")))?;
    Ok(Some((out, "image/jpeg".to_string())))
}

async fn serve_image(
    path: &str,
    request: Request<Body>,
    image_query: &ItemImageQuery,
) -> Result<Response, AppError> {
    if path.starts_with("http://") || path.starts_with("https://") {
        serve_remote_image(path, image_query, request).await
    } else {
        serve_local_path(PathBuf::from(path), request, image_query).await
    }
}

async fn serve_local_path(
    path: PathBuf,
    request: Request<Body>,
    image_query: &ItemImageQuery,
) -> Result<Response, AppError> {
    let metadata = tokio::fs::metadata(&path).await.map_err(|_| {
        AppError::NotFound("文件不存在".to_string())
    })?;

    let mtime = metadata
        .modified()
        .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
    let size = metadata.len();
    let mtime_secs = mtime
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let etag_raw = format!("{:x}-{:x}{}", mtime_secs, size, image_query.etag_suffix());
    let etag = format!("\"{}\"", etag_raw);

    if let Some(if_none_match) = request.headers().get(header::IF_NONE_MATCH) {
        if let Ok(val) = if_none_match.to_str() {
            if val == etag || val == format!("W/{}", etag) || val == "*" {
                return Ok(Response::builder()
                    .status(StatusCode::NOT_MODIFIED)
                    .header(header::ETAG, &etag)
                    .header(header::CACHE_CONTROL, "public, max-age=604800, immutable")
                    .body(Body::empty())
                    .unwrap());
            }
        }
    }

    let declared_type = mime_guess::from_path(&path)
        .first_or_octet_stream()
        .to_string();
    let body_bytes = tokio::fs::read(&path).await.map_err(|e| {
        AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
    })?;

    let (final_bytes, content_type) =
        match apply_item_image_transform(&body_bytes, image_query)? {
            Some((b, ct)) => (b, ct),
            None => (body_bytes, declared_type),
        };

    let is_head = request.method() == Method::HEAD;
    let final_len = final_bytes.len();
    let body = if is_head {
        Body::empty()
    } else {
        Body::from(final_bytes)
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ETAG, &etag)
        .header(header::CACHE_CONTROL, "public, max-age=604800, immutable")
        .header(header::CONTENT_LENGTH, final_len)
        .body(body)
        .unwrap())
}

async fn serve_remote_image(
    url: &str,
    image_query: &ItemImageQuery,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let response = SHARED_HTTP_CLIENT
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("获取远程图片失败: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::NotFound(format!("远程图片不存在: {status}")));
    }

    let declared_content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("读取远程图片失败: {e}")))?;
    let slice = body_bytes.as_ref();

    let (final_bytes, content_type) =
        match apply_item_image_transform(slice, image_query)? {
            Some((b, ct)) => (b, ct),
            None => (body_bytes.to_vec(), declared_content_type),
        };

    let is_head = request.method() == Method::HEAD;
    let final_len = final_bytes.len();
    let body = if is_head {
        Body::empty()
    } else {
        Body::from(final_bytes)
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .header(header::CONTENT_LENGTH, final_len)
        .body(body)
        .map_err(|e| AppError::Internal(format!("构建图片响应失败: {e}")))?)
}
