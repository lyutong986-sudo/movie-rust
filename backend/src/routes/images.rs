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
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path as StdPath, PathBuf},
};
use tokio::fs;
use tower::ServiceExt;
use tower_http::services::ServeFile;

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
        if let Some(path) = item.backdrop_path {
            images.push(ImageInfoDto {
                image_type: "Backdrop".to_string(),
                image_index: Some(0),
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
    let has_image = if let Some(item) = repository::get_media_item(&state.pool, item_id).await? {
        match normalized.as_str() {
            "Backdrop" => item.backdrop_path.is_some(),
            "Logo" => item.logo_path.is_some(),
            "Thumb" => item.thumb_path.is_some(),
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

#[derive(Debug, Deserialize)]
struct ItemImageIndexPath {
    item_id: String,
    image_type: String,
    image_index: i32,
}

#[derive(Debug, Deserialize)]
struct ItemImageExtendedPath {
    item_id: String,
    image_type: String,
    image_index: i32,
    tag: String,
    format: String,
    max_width: i32,
    max_height: i32,
    percent_played: i32,
    unplayed_count: i32,
}

#[derive(Debug, Deserialize)]
struct UserImageIndexPath {
    user_id: String,
    image_type: String,
    image_index: i32,
}

async fn get_item_image_index_url(
    State(state): State<AppState>,
    Path(path): Path<ItemImageIndexPath>,
) -> Result<Json<Value>, AppError> {
    item_image_url_response(
        &state,
        &path.item_id,
        &path.image_type,
        Some(path.image_index),
    )
    .await
}

async fn delete_item_image_index(
    session: AuthSession,
    State(state): State<AppState>,
    Path(path): Path<ItemImageIndexPath>,
) -> Result<StatusCode, AppError> {
    delete_item_image_with_tail(
        session,
        State(state),
        Path((path.item_id, path.image_type, path.image_index.to_string())),
    )
    .await
}

async fn set_item_image_index(
    session: AuthSession,
    State(_state): State<AppState>,
    Path(_path): Path<ItemImageIndexPath>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn get_item_image_extended(
    State(state): State<AppState>,
    Path(path): Path<ItemImageExtendedPath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let _ = (
        &path.tag,
        &path.format,
        path.max_width,
        path.max_height,
        path.percent_played,
        path.unplayed_count,
    );
    get_item_image_with_tail(
        OptionalAuthSession(None),
        State(state),
        Path((
            path.item_id,
            path.image_type,
            path.image_index.to_string(),
        )),
        request,
    )
    .await
}

async fn delete_user_image_index(
    session: AuthSession,
    State(state): State<AppState>,
    Path(path): Path<UserImageIndexPath>,
) -> Result<StatusCode, AppError> {
    let _ = path.image_index;
    delete_user_image(session, State(state), Path((path.user_id, path.image_type))).await
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
    #[serde(default, alias = "includeAllLanguages")]
    include_all_languages: Option<bool>,
    #[serde(default, alias = "Language", alias = "language")]
    language: Option<String>,
    #[serde(default, alias = "enableSeriesImages")]
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
    let response = Client::new()
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
    push_existing_remote_image(&mut images, item.backdrop_path.as_deref(), "Backdrop");
    push_existing_remote_image(&mut images, item.logo_path.as_deref(), "Logo");
    push_existing_remote_image(&mut images, item.thumb_path.as_deref(), "Thumb");

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

fn item_image_storage_path(
    state: &AppState,
    item: &crate::models::DbMediaItem,
    library_options: &LibraryOptionsDto,
    image_type: &str,
    extension: &str,
) -> PathBuf {
    if library_options.save_local_metadata {
        let item_path = PathBuf::from(&item.path);
        if let Some(path) = item_image_target_path(item, &item_path, image_type, extension) {
            return path;
        }
    }

    state.config.static_dir.join("item-images").join(format!(
        "{}-{}.{}",
        item.id,
        image_type.to_ascii_lowercase(),
        extension
    ))
}

fn item_image_target_path(
    item: &crate::models::DbMediaItem,
    item_path: &PathBuf,
    image_type: &str,
    extension: &str,
) -> Option<PathBuf> {
    let folder = item_path.parent().unwrap_or(item_path.as_path());
    let stem = item_path.file_stem()?.to_string_lossy();
    let normalized = image_type.to_ascii_lowercase();

    if item.item_type.eq_ignore_ascii_case("Episode") {
        return match normalized.as_str() {
            "primary" | "thumb" => Some(folder.join(format!("{stem}-thumb.{extension}"))),
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
            "backdrop" => Some(folder.join(format!("{prefix}-fanart.{extension}"))),
            "logo" => Some(folder.join(format!("{prefix}-logo.{extension}"))),
            "thumb" => Some(folder.join(format!("{prefix}-landscape.{extension}"))),
            _ => None,
        };
    }

    let filename = match normalized.as_str() {
        "primary" => "poster",
        "backdrop" => "fanart",
        "logo" => "logo",
        "thumb" => "landscape",
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
    upload_item_image_impl(session, state, item_id_str, image_type, headers, body).await
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
            return delete_item_image_impl(session, state, item_id_str, image_type).await;
        }
        "index" => {
            if !session.is_admin {
                return Err(AppError::Forbidden);
            }
            return Ok(StatusCode::NO_CONTENT);
        }
        _ => {}
    }
    upload_item_image_impl(session, state, item_id_str, image_type, headers, body).await
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
    delete_item_image_impl(session, state, item_id_str, image_type).await
}

async fn delete_item_image_with_tail(
    session: AuthSession,
    State(state): State<AppState>,
    Path((item_id_str, image_type, _image_tail)): Path<(String, String, String)>,
) -> Result<StatusCode, AppError> {
    delete_item_image_impl(session, state, item_id_str, image_type).await
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
    let path = resolve_person_image_path(&state, &name, &image_type)
        .await?
        .ok_or_else(|| AppError::NotFound("图片不存在".to_string()))?;
    serve_image(&path, request).await
}

async fn upload_item_image_impl(
    session: AuthSession,
    state: AppState,
    item_id_str: String,
    image_type: String,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    if body.is_empty() {
        return Err(AppError::BadRequest("图片内容不能为空".to_string()));
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let image_type = normalized_item_image_type(&image_type);
    let extension = image_extension_from_headers(&headers);
    let library_options = item_library_options(&state, item_id).await?;
    let path = item_image_storage_path(&state, &item, &library_options, &image_type, &extension);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(AppError::Io)?;
    }
    fs::write(&path, &body).await.map_err(AppError::Io)?;

    repository::update_media_item_image_path(
        &state.pool,
        item_id,
        &image_type,
        Some(&path.to_string_lossy()),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_item_image_impl(
    session: AuthSession,
    state: AppState,
    item_id_str: String,
    image_type: String,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let image_type = normalized_item_image_type(&image_type);
    let current_path = match image_type.as_str() {
        "Backdrop" => item.backdrop_path,
        "Logo" => item.logo_path,
        "Thumb" => item.thumb_path,
        _ => item.image_primary_path,
    };
    if let Some(path) = current_path {
        let path_buf = PathBuf::from(&path);
        if path_buf.exists() && path_buf.starts_with(state.config.static_dir.join("item-images")) {
            let _ = fs::remove_file(path_buf).await;
        }
    }
    repository::update_media_item_image_path(&state.pool, item_id, &image_type, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn serve_genre_image(
    state: AppState,
    name: String,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let path = repository::get_genre_image_path(&state.pool, &name, &image_type)
        .await?
        .ok_or_else(|| AppError::NotFound("图片不存在".to_string()))?;
    serve_image(&path, request).await
}

async fn serve_item_image(
    _session: Option<AuthSession>,
    state: AppState,
    item_id_str: String,
    image_type: String,
    image_index: Option<i32>,
    request: Request<Body>,
) -> Result<Response, AppError> {
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
        return serve_image(&path, request).await;
    }

    if let Some(path) =
        repository::get_missing_episode_image_path(&state.pool, item_id, &image_type).await?
    {
        return serve_image(&path, request).await;
    }

    let Some(item) = repository::get_media_item(&state.pool, item_id).await? else {
        if let Some(person) = repository::get_person_by_uuid(&state.pool, item_id).await? {
            if let Some(path) = resolve_person_image_path(&state, &person.id, &image_type).await? {
                return serve_image(&path, request).await;
            }
            return Err(AppError::NotFound("图片不存在".to_string()));
        }
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    };

    let path = match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => item.backdrop_path,
        "logo" => item.logo_path,
        "thumb" => item.thumb_path.or(item.backdrop_path),
        _ => item.image_primary_path,
    }
    .ok_or_else(|| AppError::NotFound("图片不存在".to_string()))?;

    serve_image(&path, request).await
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

    serve_image(&path, request).await
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

    serve_remote_image(image_url, request).await
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
        _ => "Primary".to_string(),
    }
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
    let client = Client::new();
    let response = client
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

async fn serve_image(path: &str, request: Request<Body>) -> Result<Response, AppError> {
    if path.starts_with("http://") || path.starts_with("https://") {
        serve_remote_image(path, request).await
    } else {
        serve_local_path(PathBuf::from(path), request).await
    }
}

async fn serve_local_path(path: PathBuf, request: Request<Body>) -> Result<Response, AppError> {
    if !path.exists() {
        return Err(AppError::NotFound("文件不存在".to_string()));
    }

    ServeFile::new(StdPath::new(&path))
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

async fn serve_remote_image(url: &str, _request: Request<Body>) -> Result<Response, AppError> {
    let client = Client::new();
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("获取远程图片失败: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::NotFound(format!("远程图片不存在: {status}")));
    }

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("读取远程图片失败: {e}")))?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(body_bytes))
        .map_err(|e| AppError::Internal(format!("构建图片响应失败: {e}")))?;

    Ok(response)
}
