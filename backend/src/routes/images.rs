use crate::{
    auth::{AuthSession, OptionalAuthSession}, error::AppError, models::ImageInfoDto, repository, state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use reqwest::Client;
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Items/{item_id}/Images", get(list_item_images))
        .route("/Items/{item_id}/Images/{image_type}", get(get_item_image))
        .route(
            "/Items/{item_id}/Images/{image_type}/{*image_tail}",
            get(get_item_image_with_tail),
        )
        .route("/Images/Remote", get(get_remote_image))
        .route(
            "/Users/{user_id}/Images/{image_type}",
            get(user_image_placeholder),
        )
        .route(
            "/Users/{user_id}/Images/{image_type}/{*image_tail}",
            get(user_image_placeholder),
        )
}

async fn list_item_images(
    _session: OptionalAuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
) -> Result<Json<Vec<ImageInfoDto>>, AppError> {
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

    Ok(Json(images))
}

async fn get_item_image(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_item_image(session.0, state, item_id, image_type, request).await
}

async fn get_item_image_with_tail(
    session: OptionalAuthSession,
    State(state): State<AppState>,
    Path((item_id, image_type, _image_tail)): Path<(Uuid, String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_item_image(session.0, state, item_id, image_type, request).await
}

async fn serve_item_image(
    _session: Option<AuthSession>,
    state: AppState,
    item_id: Uuid,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let path = if image_type.eq_ignore_ascii_case("Backdrop") {
        item.backdrop_path
    } else {
        item.image_primary_path
    }
    .ok_or_else(|| AppError::NotFound("图片不存在".to_string()))?;

    serve_image(&path, request).await
}

async fn get_remote_image(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let image_url = params
        .get("ImageUrl")
        .ok_or_else(|| AppError::BadRequest("Missing ImageUrl parameter".to_string()))?;

    serve_remote_image(image_url, request).await
}

async fn user_image_placeholder() -> StatusCode {
    StatusCode::NOT_FOUND
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

    ServeFile::new(path)
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
        .map_err(|e| AppError::Internal(format!("Failed to fetch remote image: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::NotFound(format!(
            "Remote image not found: {}",
            status
        )));
    }

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v: &header::HeaderValue| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("Failed to read remote image body: {}", e)))?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400") // 缓存24小时
        .body(Body::from(body_bytes))
        .map_err(|e| AppError::Internal(format!("Failed to build response: {}", e)))?;

    Ok(response)
}
