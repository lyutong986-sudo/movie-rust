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

/// 把客户端传过来的 itemId 字符串解析成本地 UUID。
///
/// 1. 先按标准 Emby GUID 解析（`emby_id_to_uuid` 已支持去掉 `mediasource_` 前缀、无连字符 hex 等）。
/// 2. 解析失败时，把它当成"上游 Emby 原始 itemId"再查一遍 `provider_ids`：
///    - `RemoteEmbyItemId`（Episode/Movie 同步路径写入）
///    - `RemoteEmbySeriesId`（Series 详情同步路径写入）
///
/// 这样 Hills / 网页端等客户端如果还缓存着上游 Emby 的数字 itemId（比如刚从
/// `https://emby.example.com` 切到我们后端时 UI cache 没失效），就不会一律 400，
/// 而是命中本地条目走通常的图片代理路径。
async fn resolve_item_id_with_remote_fallback(
    pool: &sqlx::PgPool,
    item_id_str: &str,
) -> Result<uuid::Uuid, AppError> {
    if let Ok(uuid) = emby_id_to_uuid(item_id_str) {
        return Ok(uuid);
    }
    if let Some(uuid) = repository::find_item_id_by_remote_emby_id(pool, item_id_str).await? {
        tracing::debug!(
            remote_id = %item_id_str,
            local_id = %uuid,
            "通过 RemoteEmbyItemId/RemoteEmbySeriesId 把上游 itemId 映射到本地 UUID"
        );
        return Ok(uuid);
    }
    Err(AppError::BadRequest(format!(
        "无效的项目ID格式: {item_id_str}"
    )))
}

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

fn build_image_info(image_type: &str, image_index: Option<i32>, tag: &str, path: String) -> ImageInfoDto {
    let filename = std::path::Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();
    let (width, height, size) = if !path.starts_with("http://") && !path.starts_with("https://") {
        let p = std::path::Path::new(&path);
        let dims = image::image_dimensions(p).unwrap_or((0, 0));
        let file_size = p.metadata().map(|m| m.len()).unwrap_or(0);
        (dims.0, dims.1, file_size)
    } else {
        (0, 0, 0)
    };
    ImageInfoDto {
        image_type: image_type.to_string(),
        image_index,
        image_tag: tag.to_string(),
        path,
        filename,
        width,
        height,
        size,
    }
}

async fn list_item_images(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Vec<ImageInfoDto>>, AppError> {
    let item_id = resolve_item_id_with_remote_fallback(&state.pool, &item_id_str).await?;
    let mut images = Vec::new();
    if let Some(item) = repository::get_media_item(&state.pool, item_id).await? {
        let tag = item.date_modified.timestamp().to_string();
        if let Some(path) = item.image_primary_path {
            images.push(build_image_info("Primary", None, &tag, path));
        }
        if let Some(path) = item.logo_path {
            images.push(build_image_info("Logo", None, &tag, path));
        }
        if let Some(path) = item.thumb_path {
            images.push(build_image_info("Thumb", None, &tag, path));
        }
        if let Some(path) = item.banner_path {
            images.push(build_image_info("Banner", None, &tag, path));
        }
        if let Some(path) = item.disc_path {
            images.push(build_image_info("Disc", None, &tag, path));
        }
        if let Some(path) = item.art_path {
            images.push(build_image_info("Art", None, &tag, path));
        }
        if let Some(ref bd_path) = item.backdrop_path {
            images.push(build_image_info("Backdrop", Some(0), &tag, bd_path.clone()));
        }
        for (i, path) in item.backdrop_paths.iter().enumerate() {
            if item.backdrop_path.as_deref() == Some(path.as_str()) {
                continue;
            }
            let idx = images.iter().filter(|img| img.image_type == "Backdrop").count() as i32;
            images.push(build_image_info("Backdrop", Some(idx), &tag, path.clone()));
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
                let idx = (image_type == "Backdrop").then_some(0);
                images.push(build_image_info(image_type, idx, &image_tag, path));
            }
        }
    } else if let Some(library) = repository::get_library(&state.pool, item_id).await? {
        if let Some(path) = library.primary_image_path.as_ref() {
            if !path.trim().is_empty() {
                let tag = library
                    .primary_image_tag
                    .clone()
                    .unwrap_or_else(|| library.created_at.timestamp().to_string());
                images.push(build_image_info("Primary", None, &tag, path.clone()));
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
    let item_id = resolve_item_id_with_remote_fallback(&state.pool, item_id_str).await?;
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
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<RemoteImagesQuery>,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    // PB19：远端图列表暴露的是 TMDb/Fanart 等外部图候选，受限用户即便能拿到 itemId 也不
    // 应该能枚举出隐藏库条目的候选海报；这里加 user_can_access_item，admin 豁免。
    if !session.is_admin
        && !repository::user_can_access_item(&state.pool, session.user_id, item_id).await?
    {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    }
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
    #[serde(default, alias = "StartIndex", alias = "startIndex", deserialize_with = "crate::models::deserialize_option_i32_lenient")]
    start_index: Option<i32>,
    #[serde(default, alias = "Limit", alias = "limit", deserialize_with = "crate::models::deserialize_option_i32_lenient")]
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
    // 来自远端 Emby 源的图片 URL（落库时不带 token）需要走伪装鉴权链路；
    // 否则严格 WAF 上游会一律 401/403/404，picker 点同一张图一直「远程图片不存在」。
    let authenticated_source =
        crate::remote_emby::find_remote_emby_source_for_url(&state.pool, image_url).await;
    let response = if let Some(source) = authenticated_source.as_ref() {
        crate::remote_emby::build_authenticated_emby_image_request(
            &SHARED_HTTP_CLIENT,
            source,
            image_url,
        )
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("下载远程图片失败: {error}")))?
    } else {
        SHARED_HTTP_CLIENT
            .get(image_url)
            .send()
            .await
            .map_err(|error| AppError::Internal(format!("下载远程图片失败: {error}")))?
    };
    if !response.status().is_success() {
        let upstream_status = response.status();
        tracing::debug!(
            image_url = %image_url,
            upstream_status = %upstream_status,
            authenticated = %authenticated_source.is_some(),
            "上游图片返回非 2xx 状态，按 NotFound 处理"
        );
        return Err(AppError::NotFound(format!(
            "远程图片不存在 (HTTP {})",
            upstream_status.as_u16()
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
        translation_limit: startup.translation_thread_count.max(1) as u32,
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

    // Bug 修复 (2026-05-04 PB52-Img)：`folder = item_path.parent()`，当 `item_path`
    // 本身是目录时（典型场景：远端 Emby 同步的 Series / Movie 文件夹模式，
    // `item.path = view_workspace/<series_name>/`），错误地把 poster 落到
    // `<series_name>` 的**父目录** —— 也就是整个 view 根目录 / library 分类目录。
    // 同分类下所有 Series 都会写到同一个 `<分类>/poster.jpg`，互相覆盖；
    // 浏览器层表现为「多个媒体共享同一封面 + 刷新后所有封面同时变」。
    //
    // 修复：is_dir() 分支应当把 poster 落到 `item_path` **内部**（Emby/Jellyfin
    // 「电影文件夹 / Series 目录」NFO 结构标准用法），而不是父目录。
    if item_path.is_dir() {
        Some(item_path.join(format!("{filename}.{extension}")))
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
    let method = request.method().clone();
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    match resolve_person_image_path(&state, &name, &image_type).await {
        Ok(Some(path)) => {
            let result = serve_image(Some(&state), &path, request, &image_query).await;
            if result.is_ok() {
                return result;
            }
            placeholder_image_response(&state, &image_type, &image_query, method, if_none_match).await
        }
        _ => {
            placeholder_image_response(&state, &image_type, &image_query, method, if_none_match).await
        }
    }
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
    serve_image(Some(&state), &path, request, &image_query).await
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
    let item_id = resolve_item_id_with_remote_fallback(&state.pool, &item_id_str).await?;

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
        return serve_image(Some(&state), &path, request, &image_query).await;
    }

    if let Some(path) =
        repository::get_missing_episode_image_path(&state.pool, item_id, &image_type).await?
    {
        return serve_image(Some(&state), &path, request, &image_query).await;
    }

    let Some(item) = repository::get_media_item(&state.pool, item_id).await? else {
        if let Some(person) = repository::get_person_by_uuid(&state.pool, item_id).await? {
            let method = request.method().clone();
            let if_none_match = request
                .headers()
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            if let Ok(Some(path)) = resolve_person_image_path(&state, &person.id, &image_type).await {
                let result = serve_image(Some(&state), &path, request, &image_query).await;
                if result.is_ok() {
                    return result;
                }
            }
            return placeholder_image_response(&state, &image_type, &image_query, method, if_none_match).await;
        }
        if let Some(library) = repository::get_library(&state.pool, item_id).await? {
            let normalized = normalized_item_image_type(&image_type);
            if normalized.eq_ignore_ascii_case("Primary") {
                if let Some(path) = library.primary_image_path.as_ref() {
                    if !path.trim().is_empty() {
                        return serve_image(Some(&state), path, request, &image_query).await;
                    }
                }
                if let Some((_, child_path, _)) =
                    repository::first_library_child_image(&state.pool, library.id).await?
                {
                    return serve_image(Some(&state), &child_path, request, &image_query).await;
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
        // 远端 Emby 条目的图片 URL（无 token）：动态追加 access_token。
        // 同时把 source 透传给 serve_image_with_source，省掉 serve_remote_image 内部的
        // list_remote_emby_sources 全表扫（审计日志里每张图都打两次 remote_emby_sources，
        // 一次按 id 一次按 name，这里就是其中一次）。
        let path_is_remote = path.starts_with("http://") || path.starts_with("https://");
        let remote_source = if path_is_remote {
            let source_id_opt = item
                .provider_ids
                .get("RemoteEmbySourceId")
                .and_then(|v| v.as_str())
                .and_then(|s| uuid::Uuid::parse_str(s).ok());
            match source_id_opt {
                Some(source_id) => repository::get_remote_emby_source(&state.pool, source_id)
                    .await
                    .ok()
                    .flatten(),
                None => None,
            }
        } else {
            None
        };

        let effective_path = if path_is_remote {
            if let Some(token) = remote_source
                .as_ref()
                .and_then(|s| s.access_token.as_ref())
                .filter(|t| !t.trim().is_empty())
            {
                let sep = if path.contains('?') { '&' } else { '?' };
                format!("{path}{sep}api_key={token}")
            } else {
                path.clone()
            }
        } else {
            path.clone()
        };

        // 远端 Emby 图片：本次请求继续走远程拉 + transform，但同时启动后台任务
        // 把原图（无 transform）下载到本地 + UPDATE DB image_*_path。下次相同 item
        // 再来请求时直接走本地 serve_local_path，不再触达远端，也不再依赖 PB42-IC 缓存。
        if path_is_remote {
            if let Some(source) = remote_source.clone() {
                let backdrop_idx = if normalized.eq_ignore_ascii_case("Backdrop") {
                    Some(idx)
                } else {
                    None
                };
                spawn_remote_image_persist(
                    &state,
                    &item,
                    &normalized,
                    backdrop_idx,
                    source,
                    path.clone(),
                );
            }
        }

        let method = request.method().clone();
        let if_none_match = request
            .headers()
            .get(header::IF_NONE_MATCH)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        let result = serve_image_with_source(
            Some(&state),
            &effective_path,
            request,
            &image_query,
            remote_source.as_ref(),
        )
        .await;
        let should_fallback = matches!(&result, Err(AppError::NotFound(_)) | Err(AppError::Internal(_)));
        if should_fallback {
            if let Some(fallback_url) =
                find_tmdb_image_fallback(&state.pool, &item, &normalized, state.config.tmdb_api_key.as_deref()).await
            {
                let from_remote = path.starts_with("http://") || path.starts_with("https://");
                tracing::debug!(
                    item_id = %item.id,
                    image_type = %normalized,
                    from_remote,
                    "原图加载失败，回退到 TMDB 远程代理"
                );
                spawn_image_fallback_refresh(&state, item.id);
                let fallback_req = Request::builder()
                    .method(method.clone())
                    .body(Body::empty())
                    .unwrap_or_default();
                let fb = serve_remote_image(Some(&state), &fallback_url, &image_query, fallback_req, None).await;
                if let Err(AppError::NotFound(_)) = &fb {
                    return placeholder_image_response(&state, &normalized, &image_query, method, if_none_match).await;
                }
                return fb;
            }
            spawn_image_fallback_refresh(&state, item.id);
            return placeholder_image_response(&state, &normalized, &image_query, method, if_none_match).await;
        }
        return result;
    }

    let method = request.method().clone();
    let if_none_match = request
        .headers()
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    if let Some(fallback_url) = find_tmdb_image_fallback(&state.pool, &item, &normalized, state.config.tmdb_api_key.as_deref()).await {
        tracing::debug!(
            item_id = %item.id,
            image_type = %normalized,
            "数据库未配置图片路径，回退到 TMDB 远程代理"
        );
        spawn_image_fallback_refresh(&state, item.id);
        let fallback_req = Request::builder()
            .method(method.clone())
            .body(Body::empty())
            .unwrap_or_default();
        let fb = serve_remote_image(Some(&state), &fallback_url, &image_query, fallback_req, None).await;
        if let Err(AppError::NotFound(_)) = &fb {
            return placeholder_image_response(&state, &normalized, &image_query, method, if_none_match).await;
        }
        return fb;
    }

    spawn_image_fallback_refresh(&state, item.id);
    placeholder_image_response(&state, &normalized, &image_query, method, if_none_match).await
}

/// 图片 404 时异步触发 TMDB 元数据/图片补全。
/// 使用 refresh_queue 去重，同一条目不会重复触发。
fn spawn_image_fallback_refresh(state: &AppState, item_id: uuid::Uuid) {
    if !crate::refresh_queue::try_begin_refresh(item_id) {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        tracing::info!(
            item_id = %item_id,
            "图片缺失兜底：后台触发 TMDB 元数据/图片补全"
        );
        if let Err(e) = super::items::do_refresh_item_metadata(&state, item_id).await {
            tracing::warn!(
                item_id = %item_id,
                error = %e,
                "图片缺失兜底：后台元数据补全失败"
            );
        } else {
            tracing::info!(
                item_id = %item_id,
                "图片缺失兜底：后台元数据/图片补全完成"
            );
        }
        crate::refresh_queue::end_refresh(item_id);
    });
}

/// 把"远端 Emby 条目的图片 URL"异步下载到本地，下载完成后把 DB 中的 `image_*_path`
/// 由远程 URL 改成本地文件路径。下次同一条 item 请求图片就会直接走 `serve_local_path`，
/// 不再触达远端、不再依赖 PB42-IC 的 cache_dir TTL（默认 7 天会被 evict）。
///
/// 用 `(item_id, image_type, backdrop_index)` 三元组在 `refresh_queue::IMAGE_PERSISTING`
/// 里去重。当前请求**不阻塞**，仍然走 `serve_remote_image` 拿到带 transform 的图返回，
/// 同时后台默默把原图（无 transform）写盘 + update DB。
fn spawn_remote_image_persist(
    state: &AppState,
    item: &crate::models::DbMediaItem,
    image_type: &str,
    backdrop_index: Option<i32>,
    source: crate::models::DbRemoteEmbySource,
    remote_url: String,
) {
    if !crate::refresh_queue::try_begin_image_persist(item.id, image_type, backdrop_index) {
        return;
    }
    let state = state.clone();
    let item = item.clone();
    let image_type = image_type.to_string();
    tokio::spawn(async move {
        match persist_remote_emby_image_to_local(
            &state,
            &item,
            &image_type,
            backdrop_index,
            &source,
            &remote_url,
        )
        .await
        {
            Ok(local_path) => {
                tracing::info!(
                    item_id = %item.id,
                    image_type = %image_type,
                    backdrop_index = backdrop_index.unwrap_or(-1),
                    local_path = %local_path,
                    "远端 Emby 图片已落盘到本地，DB 路径已切换"
                );
            }
            Err(e) => {
                let is_not_found = matches!(&e, AppError::NotFound(_));
                tracing::warn!(
                    item_id = %item.id,
                    image_type = %image_type,
                    backdrop_index = backdrop_index.unwrap_or(-1),
                    error = %e,
                    clear_dead_url = is_not_found,
                    "远端 Emby 图片落盘失败"
                );
                if is_not_found {
                    if let Err(clear_err) = repository::update_media_item_image_path(
                        &state.pool,
                        item.id,
                        &image_type,
                        None,
                        backdrop_index,
                    )
                    .await
                    {
                        tracing::warn!(
                            item_id = %item.id,
                            image_type = %image_type,
                            error = %clear_err,
                            "清除死链 image_path 失败"
                        );
                    } else {
                        tracing::info!(
                            item_id = %item.id,
                            image_type = %image_type,
                            "已清除远端 404 死链，下次请求将走 TMDB/占位图回退"
                        );
                    }
                }
            }
        }
        crate::refresh_queue::end_image_persist(item.id, &image_type, backdrop_index);
    });
}

async fn persist_remote_emby_image_to_local(
    state: &AppState,
    item: &crate::models::DbMediaItem,
    image_type: &str,
    backdrop_index: Option<i32>,
    source: &crate::models::DbRemoteEmbySource,
    remote_url: &str,
) -> Result<String, AppError> {
    let library_options = item_library_options(state, item.id).await?;
    let response = crate::remote_emby::build_authenticated_emby_image_request(
        &SHARED_HTTP_CLIENT,
        source,
        remote_url,
    )
    .send()
    .await
    .map_err(|e| AppError::Internal(format!("远端图片请求失败: {e}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::NotFound(format!(
            "远端图片不存在 (HTTP {})",
            status.as_u16()
        )));
    }

    // 优先用 URL 后缀，落不到再看 Content-Type，最后兜 jpg。
    let extension = crate::naming::extension_from_url(remote_url)
        .filter(|ext| {
            crate::naming::IMAGE_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        })
        .or_else(|| {
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .and_then(extension_from_content_type)
        })
        .unwrap_or_else(|| "jpg".to_string());

    let target_path = item_image_storage_path_pub(
        state,
        item,
        &library_options,
        image_type,
        backdrop_index,
        &extension,
    );

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("读取远端图片失败: {e}")))?;
    if body_bytes.is_empty() {
        return Err(AppError::Internal("远端图片内容为空".to_string()));
    }

    if let Some(parent) = target_path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(AppError::Io)?;
    }
    tokio::fs::write(&target_path, &body_bytes)
        .await
        .map_err(AppError::Io)?;

    let path_str = target_path.to_string_lossy().to_string();
    repository::update_media_item_image_path(
        &state.pool,
        item.id,
        image_type,
        Some(&path_str),
        backdrop_index,
    )
    .await?;
    Ok(path_str)
}

fn extension_from_content_type(content_type: &str) -> Option<String> {
    let lower = content_type.to_ascii_lowercase();
    let mime = lower.split(';').next().unwrap_or("").trim();
    Some(match mime {
        "image/jpeg" | "image/jpg" => "jpg".to_string(),
        "image/png" => "png".to_string(),
        "image/webp" => "webp".to_string(),
        _ => return None,
    })
}

/// 程序生成的占位图。当媒体真没有图片字段、TMDB 也找不到回退时，与其返回 404 让
/// 客户端显示破碎图标，不如返回一张程序合成的灰底海报，前端立刻有视觉反馈，后台
/// `spawn_image_fallback_refresh` 拉到真图后下次请求自然替换（占位图带 5 分钟短缓存）。
///
/// 按 image_type 选不同长宽比，命中文件缓存（`static_dir/placeholders/{type}-{w}x{h}.jpg`）
/// 后续直接读盘，避免每个 404 都重画一遍。
///
/// 实现细节：所有 `image::ImageBuffer` / `JpegEncoder` 操作都放在 `spawn_blocking`
/// 里完成，绝不跨 axum 的 await 持有 image crate 内部类型，避免 handler future 的
/// `Send` bound 被这些类型污染。
async fn placeholder_image_response(
    state: &AppState,
    image_type: &str,
    image_query: &ItemImageQuery,
    method: Method,
    if_none_match: Option<String>,
) -> Result<Response, AppError> {
    let normalized = normalized_item_image_type(image_type);
    let (width, height) = placeholder_dimensions(&normalized);

    let placeholders_dir = state.config.static_dir.join("placeholders");
    let cache_path =
        placeholders_dir.join(format!("{}-{}x{}.jpg", normalized.to_ascii_lowercase(), width, height));

    let raw_bytes: Vec<u8> = match tokio::fs::read(&cache_path).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let normalized_owned = normalized.clone();
            let bytes = tokio::task::spawn_blocking(move || {
                generate_placeholder_jpeg(width, height, &normalized_owned)
            })
            .await
            .map_err(|e| AppError::Internal(format!("占位图生成线程异常: {e}")))??;
            if let Some(parent) = cache_path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let _ = tokio::fs::write(&cache_path, &bytes).await;
            bytes
        }
    };

    // 占位图也支持客户端附带的 transform（缩放 / 质量），不然窄屏拉一张大图浪费流量。
    let image_query_owned = image_query.clone();
    let (final_bytes, content_type) = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, String), AppError> {
        match apply_item_image_transform(&raw_bytes, &image_query_owned)? {
            Some((b, ct)) => Ok((b, ct)),
            None => Ok((raw_bytes, "image/jpeg".to_string())),
        }
    })
    .await
    .map_err(|e| AppError::Internal(format!("占位图变换线程异常: {e}")))??;

    let etag = format!(
        "\"placeholder-{}-{w}x{h}-{q}\"",
        normalized.to_ascii_lowercase(),
        w = width,
        h = height,
        q = image_query.etag_suffix(),
    );

    if let Some(val) = if_none_match {
        if val == etag || val == format!("W/{}", etag) || val == "*" {
            return Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header(header::ETAG, &etag)
                .header(header::CACHE_CONTROL, "public, max-age=300")
                .body(Body::empty())
                .map_err(|e| AppError::Internal(format!("构建占位图响应失败: {e}")));
        }
    }

    let is_head = method == Method::HEAD;
    let len = final_bytes.len();
    let body = if is_head { Body::empty() } else { Body::from(final_bytes) };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ETAG, etag)
        // 5 分钟短缓存：后台 refresh_queue 抓到真图后，下次请求最迟 5 分钟内就能看到。
        .header(header::CACHE_CONTROL, "public, max-age=300")
        .header("X-Placeholder", "1")
        .header(header::CONTENT_LENGTH, len)
        .body(body)
        .map_err(|e| AppError::Internal(format!("构建占位图响应失败: {e}")))
}

fn placeholder_dimensions(normalized_image_type: &str) -> (u32, u32) {
    match normalized_image_type {
        "Backdrop" | "Thumb" => (800, 450),
        "Logo" => (500, 200),
        "Banner" => (1000, 185),
        "Disc" | "Art" => (500, 500),
        _ => (400, 600), // Primary / poster 2:3
    }
}

/// 用 image crate 生成一张极简灰底 + 中心几何占位图标的 JPEG。无字体依赖。
fn generate_placeholder_jpeg(width: u32, height: u32, image_type: &str) -> Result<Vec<u8>, AppError> {
    use image::{ImageBuffer, Rgb};

    let mut img: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::new(width, height);

    // 顶部到底部的深灰渐变，避免纯色看着假。
    for y in 0..height {
        let t = y as f32 / height.max(1) as f32;
        let v = (32.0 + t * 18.0) as u8; // 32 → 50
        for x in 0..width {
            img.put_pixel(x, y, Rgb([v, v, (v as u16 + 4).min(255) as u8]));
        }
    }

    // 中心画一个圆角"画框"占位图标：外框 + 内部小圆 + 斜线（山形），
    // 没字体也能让用户一眼看出"这是占位图"。
    let cx = width as i32 / 2;
    let cy = height as i32 / 2;
    let frame_w = (width.min(height) as f32 * 0.55) as i32;
    let frame_h = (frame_w as f32 * 0.7) as i32;
    let frame_x = cx - frame_w / 2;
    let frame_y = cy - frame_h / 2;
    let stroke = (frame_w / 32).max(2);

    let stroke_color = Rgb([110u8, 110, 120]);
    let fill_color = Rgb([60u8, 60, 70]);
    let accent_color = Rgb([140u8, 140, 150]);

    // 外框
    draw_rect_outline(&mut img, frame_x, frame_y, frame_w, frame_h, stroke, stroke_color);

    // 内部填充（稍亮一点的色块）
    fill_rect(
        &mut img,
        frame_x + stroke,
        frame_y + stroke,
        frame_w - 2 * stroke,
        frame_h - 2 * stroke,
        fill_color,
    );

    // 山形斜线（左下到右上的两段折线，模拟 placeholder 图标里的山）
    let bottom = frame_y + frame_h - stroke - 1;
    let mid_x = frame_x + frame_w / 2;
    draw_thick_line(
        &mut img,
        frame_x + stroke,
        bottom,
        mid_x,
        frame_y + frame_h / 3,
        stroke,
        accent_color,
    );
    draw_thick_line(
        &mut img,
        mid_x,
        frame_y + frame_h / 3,
        frame_x + frame_w - stroke,
        bottom,
        stroke,
        accent_color,
    );

    // 右上小圆（"太阳"）
    let sun_r = (frame_w / 16).max(3);
    let sun_x = frame_x + frame_w * 3 / 4;
    let sun_y = frame_y + frame_h / 4;
    fill_circle(&mut img, sun_x, sun_y, sun_r, accent_color);

    // 底部加 image_type 提示色条（不画字，用颜色区分类型，方便排查）
    let tag_color = type_tag_color(image_type);
    let tag_h = (height / 32).max(2);
    fill_rect(
        &mut img,
        0,
        height as i32 - tag_h as i32,
        width as i32,
        tag_h as i32,
        tag_color,
    );

    let mut buf: Vec<u8> = Vec::with_capacity((width * height) as usize);
    let mut encoder = JpegEncoder::new_with_quality(&mut buf, 78);
    encoder
        .encode(img.as_raw(), width, height, ExtendedColorType::Rgb8)
        .map_err(|e| AppError::Internal(format!("占位图编码失败: {e}")))?;
    Ok(buf)
}

fn type_tag_color(image_type: &str) -> image::Rgb<u8> {
    use image::Rgb;
    match image_type {
        "Backdrop" | "Thumb" => Rgb([78, 132, 188]),
        "Logo" => Rgb([188, 152, 78]),
        "Banner" => Rgb([162, 98, 188]),
        "Disc" | "Art" => Rgb([78, 188, 132]),
        _ => Rgb([188, 78, 98]), // Primary
    }
}

fn draw_rect_outline(
    img: &mut image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    stroke: i32,
    color: image::Rgb<u8>,
) {
    fill_rect(img, x, y, w, stroke, color);
    fill_rect(img, x, y + h - stroke, w, stroke, color);
    fill_rect(img, x, y, stroke, h, color);
    fill_rect(img, x + w - stroke, y, stroke, h, color);
}

fn fill_rect(
    img: &mut image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: image::Rgb<u8>,
) {
    let img_w = img.width() as i32;
    let img_h = img.height() as i32;
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(img_w);
    let y1 = (y + h).min(img_h);
    for yy in y0..y1 {
        for xx in x0..x1 {
            img.put_pixel(xx as u32, yy as u32, color);
        }
    }
}

fn draw_thick_line(
    img: &mut image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: image::Rgb<u8>,
) {
    // 简化的 Bresenham + 用 thickness×thickness 的方块代替单像素描点。
    let dx = (x1 - x0).abs();
    let dy = -(y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    let half = thickness / 2;
    let mut x = x0;
    let mut y = y0;
    loop {
        fill_rect(img, x - half, y - half, thickness, thickness, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn fill_circle(
    img: &mut image::ImageBuffer<image::Rgb<u8>, Vec<u8>>,
    cx: i32,
    cy: i32,
    r: i32,
    color: image::Rgb<u8>,
) {
    let r2 = r * r;
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r2 {
                let x = cx + dx;
                let y = cy + dy;
                if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                    img.put_pixel(x as u32, y as u32, color);
                }
            }
        }
    }
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
    let item_id = resolve_item_id_with_remote_fallback(&state.pool, &item_id_str).await?;
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
    serve_image(Some(&state), &path, request, &image_query).await
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
    State(state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let image_url = params
        .get("ImageUrl")
        .ok_or_else(|| AppError::BadRequest("缺少 ImageUrl 参数".to_string()))?;

    let image_query = parse_item_image_query(request.uri());
    serve_remote_image(Some(&state), image_url, &image_query, request, None).await
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
        "primary" => "Primary".to_string(),
        "backdrop" => "Backdrop".to_string(),
        "logo" => "Logo".to_string(),
        "thumb" => "Thumb".to_string(),
        "banner" => "Banner".to_string(),
        "disc" => "Disc".to_string(),
        "art" => "Art".to_string(),
        "box" => "Box".to_string(),
        "boxrear" => "BoxRear".to_string(),
        "menu" => "Menu".to_string(),
        "chapter" => "Chapter".to_string(),
        "screenshot" => "Screenshot".to_string(),
        "profile" => "Profile".to_string(),
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
    let response = match SHARED_HTTP_CLIENT.get(&path).send().await {
        Ok(resp) => resp,
        Err(error) => {
            tracing::warn!(
                person_id = %person.id,
                image_type = %image_type,
                error = %error,
                "下载人物图片失败，回退占位图"
            );
            return Ok(None);
        }
    };
    if !response.status().is_success() {
        let upstream_status = response.status();
        tracing::debug!(
            person_id = %person.id,
            image_type = %image_type,
            upstream_status = %upstream_status,
            url = %path,
            "上游人物图片返回非 2xx 状态，清除死链并回退占位图"
        );
        let person_uuid = emby_id_to_uuid(&person.id).ok();
        if let Some(pid) = person_uuid {
            let _ = repository::update_person_image_path(
                &state.pool,
                pid,
                &normalized,
                None,
            )
            .await;
        }
        return Ok(None);
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
    state: Option<&AppState>,
    path: &str,
    request: Request<Body>,
    image_query: &ItemImageQuery,
) -> Result<Response, AppError> {
    serve_image_with_source(state, path, request, image_query, None).await
}

/// 与 `serve_image` 同义，但允许调用方把已经查到的 `DbRemoteEmbySource` 传进来，
/// 避免 `serve_remote_image` 再走一次 `find_remote_emby_source_for_url`（全表 SELECT
/// `remote_emby_sources` ORDER BY name）。
///
/// 审计日志（2026-05-04）显示每张远端 Emby 图片都触发两次 `remote_emby_sources`
/// 查询：一次 `WHERE id = $1`（拿 token），一次 `ORDER BY name`（host 匹配）。
/// 把已知 source 透传下去后第二次可以省掉，单图请求只剩一次按主键的查询。
async fn serve_image_with_source(
    state: Option<&AppState>,
    path: &str,
    request: Request<Body>,
    image_query: &ItemImageQuery,
    authenticated_source: Option<&crate::models::DbRemoteEmbySource>,
) -> Result<Response, AppError> {
    if path.starts_with("http://") || path.starts_with("https://") {
        serve_remote_image(state, path, image_query, request, authenticated_source).await
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

// ---------------------------------------------------------------------------
// PB42-IC：远端图片磁盘缓存
//
// Hills 客户端打开主页瞬间会触发 100+ 张图片代理请求，每张都回源到 upstream
// Emby 既慢（远端节流 + ffmpeg 解码）又烧上游 QPS。我们在本地落一份缓存：
//
//   cache_dir/<key2>/<key>.bin   ← 实际 bytes
//   cache_dir/<key2>/<key>.ct    ← content-type 字符串
//
// `key` = sha256(stripped_url + image_query.etag_suffix())[..16] 16 位 hex；
// `key2` = key 前 2 位（避免单目录文件过多）。
//
// 缓存命中后：
//   - If-None-Match 只用 `"<key>"` 比较，命中直接 304；
//   - 否则读盘 → 直接返回（不打 upstream）。
//
// 缓存未命中：
//   - 走原有 SHARED_HTTP_CLIENT 拉取 → apply_item_image_transform → 写盘 → 返回。
//
// 旧文件由 `image_cache_eviction_loop` 周期清理（mtime 超出 TTL）。
// ---------------------------------------------------------------------------
use std::sync::OnceLock;

static IMAGE_CACHE_DIR: OnceLock<PathBuf> = OnceLock::new();
static IMAGE_CACHE_TTL: OnceLock<std::time::Duration> = OnceLock::new();

/// 由 `main.rs` 在启动时调用，固化磁盘缓存目录与 TTL。
pub fn init_image_cache(dir: PathBuf, ttl: std::time::Duration) {
    let _ = IMAGE_CACHE_DIR.set(dir);
    let _ = IMAGE_CACHE_TTL.set(ttl);
}

fn image_cache_dir() -> Option<&'static PathBuf> {
    IMAGE_CACHE_DIR.get()
}

/// 从 URL 中剥掉 api_key / X-Emby-Token 等会话级参数，让同一张图片在 token
/// 刷新前后命中同一个缓存项。
fn strip_volatile_query_params(url: &str) -> String {
    let Some(qpos) = url.find('?') else {
        return url.to_string();
    };
    let (base, query) = url.split_at(qpos);
    let query = &query[1..];
    let kept: Vec<String> = query
        .split('&')
        .filter(|kv| {
            let key = kv.split('=').next().unwrap_or("");
            !matches!(
                key.to_ascii_lowercase().as_str(),
                "api_key"
                    | "apikey"
                    | "x-emby-token"
                    | "x_emby_token"
                    | "x-mediabrowser-token"
                    | "mediabrowsertoken"
            )
        })
        .map(str::to_string)
        .collect();
    if kept.is_empty() {
        base.to_string()
    } else {
        format!("{base}?{}", kept.join("&"))
    }
}

fn remote_image_cache_key(url: &str, image_query: &ItemImageQuery) -> String {
    use sha2::{Digest, Sha256};
    let stripped = strip_volatile_query_params(url);
    let mut hasher = Sha256::new();
    hasher.update(stripped.as_bytes());
    hasher.update(image_query.etag_suffix().as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..16])
}

fn remote_image_cache_paths(key: &str) -> Option<(PathBuf, PathBuf)> {
    let base = image_cache_dir()?;
    let bucket = &key[..2.min(key.len())];
    let dir = base.join(bucket);
    let bin = dir.join(format!("{key}.bin"));
    let ct = dir.join(format!("{key}.ct"));
    Some((bin, ct))
}

async fn remote_image_cache_load(key: &str) -> Option<(Vec<u8>, String)> {
    let (bin_path, ct_path) = remote_image_cache_paths(key)?;
    let bytes = tokio::fs::read(&bin_path).await.ok()?;
    if bytes.is_empty() {
        return None;
    }
    let content_type = tokio::fs::read_to_string(&ct_path)
        .await
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Some((bytes, content_type))
}

async fn remote_image_cache_store(key: &str, bytes: &[u8], content_type: &str) {
    let Some((bin_path, ct_path)) = remote_image_cache_paths(key) else {
        return;
    };
    if bytes.is_empty() {
        return;
    }
    if let Some(parent) = bin_path.parent() {
        if let Err(error) = tokio::fs::create_dir_all(parent).await {
            tracing::debug!(
                cache_dir = %parent.to_string_lossy(),
                error = %error,
                "PB42-IC：创建图片缓存目录失败，跳过本次写入"
            );
            return;
        }
    }
    let tmp = bin_path.with_extension("bin.tmp");
    if let Err(error) = tokio::fs::write(&tmp, bytes).await {
        tracing::debug!(error = %error, "PB42-IC：写图片缓存 .tmp 失败");
        return;
    }
    if let Err(error) = tokio::fs::rename(&tmp, &bin_path).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        tracing::debug!(error = %error, "PB42-IC：rename 图片缓存 .tmp → .bin 失败");
        return;
    }
    if let Err(error) = tokio::fs::write(&ct_path, content_type.as_bytes()).await {
        tracing::debug!(error = %error, "PB42-IC：写 content-type sidecar 失败");
    }
}

/// PB42-IC：周期清理过期缓存文件。每小时跑一次。
pub async fn image_cache_eviction_loop() {
    let Some(base) = image_cache_dir().cloned() else {
        return;
    };
    let ttl = IMAGE_CACHE_TTL
        .get()
        .copied()
        .unwrap_or(std::time::Duration::from_secs(7 * 24 * 3600));
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
    ticker.tick().await; // 跳过启动瞬间
    loop {
        ticker.tick().await;
        if let Err(error) = run_image_cache_eviction_pass(&base, ttl).await {
            tracing::warn!(error = %error, "PB42-IC：图片缓存清理任务失败");
        }
    }
}

async fn run_image_cache_eviction_pass(base: &std::path::Path, ttl: std::time::Duration) -> std::io::Result<()> {
    let now = std::time::SystemTime::now();
    let mut buckets = match tokio::fs::read_dir(base).await {
        Ok(rd) => rd,
        Err(_) => return Ok(()),
    };
    let mut total_removed: u64 = 0;
    while let Some(bucket_entry) = buckets.next_entry().await? {
        let bucket_path = bucket_entry.path();
        if !bucket_path.is_dir() {
            continue;
        }
        let mut files = match tokio::fs::read_dir(&bucket_path).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Some(entry) = files.next_entry().await? {
            let path = entry.path();
            let metadata = match entry.metadata().await {
                Ok(m) => m,
                Err(_) => continue,
            };
            let modified = metadata
                .modified()
                .unwrap_or(std::time::UNIX_EPOCH);
            if let Ok(elapsed) = now.duration_since(modified) {
                if elapsed > ttl {
                    let _ = tokio::fs::remove_file(&path).await;
                    total_removed = total_removed.saturating_add(1);
                }
            }
        }
    }
    if total_removed > 0 {
        tracing::info!(removed = total_removed, "PB42-IC：图片缓存清理完成");
    }
    Ok(())
}

/// PB42-IC：基于 cache_key 直接组装 ETag。无论是否命中缓存，同一张图返回的
/// ETag 都一致 → 客户端 If-None-Match 永远能命中 304。
fn etag_for_cache_key(key: &str) -> String {
    format!("\"img-{key}\"")
}

fn build_remote_image_response(
    bytes: Vec<u8>,
    content_type: String,
    etag: &str,
    request: &Request<Body>,
) -> Result<Response, AppError> {
    let is_head = request.method() == Method::HEAD;
    let len = bytes.len();
    let body = if is_head { Body::empty() } else { Body::from(bytes) };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "public, max-age=604800, immutable")
        .header(header::CONTENT_LENGTH, len)
        .body(body)
        .map_err(|e| AppError::Internal(format!("构建图片响应失败: {e}")))
}

fn build_not_modified_response(etag: &str) -> Response {
    Response::builder()
        .status(StatusCode::NOT_MODIFIED)
        .header(header::ETAG, etag)
        .header(header::CACHE_CONTROL, "public, max-age=604800, immutable")
        .body(Body::empty())
        .unwrap()
}

fn if_none_match_hits(req: &Request<Body>, etag: &str) -> bool {
    let Some(value) = req.headers().get(header::IF_NONE_MATCH) else {
        return false;
    };
    let Ok(value) = value.to_str() else {
        return false;
    };
    if value == "*" {
        return true;
    }
    // 客户端可能上送 `"img-<key>"` 或 `W/"img-<key>"` 或多值用逗号分隔。
    let weak = format!("W/{etag}");
    value
        .split(',')
        .map(str::trim)
        .any(|v| v == etag || v == weak)
}

async fn serve_remote_image(
    state: Option<&AppState>,
    url: &str,
    image_query: &ItemImageQuery,
    request: Request<Body>,
    known_source: Option<&crate::models::DbRemoteEmbySource>,
) -> Result<Response, AppError> {
    // PB42-IC：cache_key 不依赖 token / api_key，所以 ETag 也稳定。
    // 客户端 If-None-Match 在没拿到响应字节的情况下就能直接 304。
    let cache_key = remote_image_cache_key(url, image_query);
    let etag = etag_for_cache_key(&cache_key);

    if if_none_match_hits(&request, &etag) {
        return Ok(build_not_modified_response(&etag));
    }

    if let Some((bytes, content_type)) = remote_image_cache_load(&cache_key).await {
        tracing::trace!(cache_key = %cache_key, "PB42-IC：图片缓存命中");
        return build_remote_image_response(bytes, content_type, &etag, &request);
    }

    // 命中已配置的远端 Emby 源时改用伪装鉴权头：覆盖 WAF 严格、必须带 token / 特定 UA 的上游。
    // 落库的远端图片 URL 不带 token，裸 GET 容易被反爬拦下来；找到对应 source 后注入完整身份。
    //
    // 调用方若已根据 item.provider_ids.RemoteEmbySourceId 拿到 source，就直接复用，
    // 避免再走 list_remote_emby_sources 的全表扫（审计日志里这条会和 `WHERE id = $1`
    // 成对出现，每张图等于打两次表）。
    let lookup_source = if known_source.is_some() {
        None
    } else {
        match state {
            Some(state) => {
                crate::remote_emby::find_remote_emby_source_for_url(&state.pool, url).await
            }
            None => None,
        }
    };
    let authenticated_source = known_source.or(lookup_source.as_ref());
    let response = if let Some(source) = authenticated_source {
        crate::remote_emby::build_authenticated_emby_image_request(
            &SHARED_HTTP_CLIENT,
            source,
            url,
        )
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("获取远程图片失败: {e}")))?
    } else {
        SHARED_HTTP_CLIENT
            .get(url)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("获取远程图片失败: {e}")))?
    };

    let status = response.status();
    if !status.is_success() {
        tracing::debug!(
            url = %url,
            upstream_status = %status,
            "上游图片返回非 2xx 状态，按 NotFound 处理"
        );
        // 把 HTTP status 透出到错误体里：401/403 → 远端鉴权失效需要刷 token；
        // 404 → 上游把图片删了 / 上游条目变更；429/5xx → 限流或上游故障。
        return Err(AppError::NotFound(format!(
            "远程图片不存在 (HTTP {})",
            status.as_u16()
        )));
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

    let (final_bytes, content_type) = match apply_item_image_transform(slice, image_query)? {
        Some((b, ct)) => (b, ct),
        None => (body_bytes.to_vec(), declared_content_type),
    };

    remote_image_cache_store(&cache_key, &final_bytes, &content_type).await;

    build_remote_image_response(final_bytes, content_type, &etag, &request)
}
