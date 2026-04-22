use crate::{
    auth::{AuthSession, OptionalAuthSession},
    error::AppError,
    models::{emby_id_to_uuid, ImageInfoDto},
    repository,
    state::AppState,
};
use axum::{
    body::{Body, Bytes},
    extract::{Path, Query, Request, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use reqwest::Client;
use serde::Serialize;
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
            "/Items/{item_id}/Images/{image_type}",
            get(get_item_image)
                .head(get_item_image)
                .post(upload_item_image)
                .delete(delete_item_image),
        )
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
            "/Users/{user_id}/Images/{image_type}/{*image_tail}",
            get(get_user_image_with_tail).head(get_user_image_with_tail),
        )
}

async fn list_item_images(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Vec<ImageInfoDto>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let mut images = Vec::new();
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

    Ok(Json(images))
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
    let image_index = image_tail
        .split('/')
        .next()
        .and_then(|value| value.parse::<i32>().ok());
    serve_item_image(session.0, state, item_id_str, image_type, image_index, request).await
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
    Path((item_id_str, image_type, _image_tail)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    upload_item_image_impl(session, state, item_id_str, image_type, headers, body).await
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
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((user_id, image_type)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_user_image(state, user_id, image_type, request).await
}

async fn get_user_image_with_tail(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((user_id, image_type, _image_tail)): Path<(String, String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_user_image(state, user_id, image_type, request).await
}

async fn serve_person_image(
    state: AppState,
    name: String,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let path = repository::get_person_image_path(&state.pool, &name, &image_type)
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
    repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let image_type = normalized_item_image_type(&image_type);
    let extension = image_extension_from_headers(&headers);
    let dir = state.config.static_dir.join("item-images");
    fs::create_dir_all(&dir).await.map_err(AppError::Io)?;
    let filename = format!(
        "{}-{}.{}",
        item_id,
        image_type.to_ascii_lowercase(),
        extension
    );
    let path = dir.join(filename);
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
        if image_type.eq_ignore_ascii_case("Primary") {
            if let Some(person) = repository::get_person_by_uuid(&state.pool, item_id).await? {
                if let Some(path) = repository::get_person_image_path(&state.pool, &person.id, &image_type).await? {
                    return serve_image(&path, request).await;
                }
            }
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
    state: AppState,
    user_id: String,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let user_id = emby_id_to_uuid(&user_id)
        .map_err(|_| AppError::BadRequest("无效的用户ID格式".to_string()))?;
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
    match headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
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
