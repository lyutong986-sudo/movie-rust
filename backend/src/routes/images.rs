use crate::{
    auth::AuthSession, error::AppError, models::ImageInfoDto, repository, state::AppState,
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

const PLACEHOLDER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="900" viewBox="0 0 600 900"><rect width="600" height="900" fill="#20242b"/><rect x="90" y="180" width="420" height="540" rx="18" fill="#2c323b"/><path d="M180 570h240l-72-96-54 66-42-48-72 78z" fill="#5b6573"/><circle cx="240" cy="360" r="42" fill="#5b6573"/></svg>"##;

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
    _session: AuthSession,
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
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_item_image(state, item_id, image_type, request).await
}

async fn get_item_image_with_tail(
    State(state): State<AppState>,
    Path((item_id, image_type, _image_tail)): Path<(Uuid, String, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_item_image(state, item_id, image_type, request).await
}

async fn serve_item_image(
    state: AppState,
    item_id: Uuid,
    image_type: String,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let Some(item) = repository::get_media_item(&state.pool, item_id).await? else {
        return placeholder_image_response();
    };

    let Some(path) = (if image_type.eq_ignore_ascii_case("Backdrop") {
        item.backdrop_path
    } else {
        item.image_primary_path
    }) else {
        return placeholder_image_response();
    };

    match serve_image(&path, request).await {
        Ok(response) => Ok(response),
        Err(_) => placeholder_image_response(),
    }
}

async fn get_remote_image(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let image_url = params
        .get("ImageUrl")
        .ok_or_else(|| AppError::BadRequest("Missing ImageUrl parameter".to_string()))?;

    match serve_remote_image(image_url, request).await {
        Ok(response) => Ok(response),
        Err(_) => placeholder_image_response(),
    }
}

async fn user_image_placeholder() -> Result<Response, AppError> {
    placeholder_image_response()
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
        .map_err(|error| AppError::Internal(format!("Failed to fetch remote image: {error}")))?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::NotFound(format!(
            "Remote image not found: {status}"
        )));
    }

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value: &header::HeaderValue| value.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let body_bytes = response
        .bytes()
        .await
        .map_err(|error| AppError::Internal(format!("Failed to read remote image body: {error}")))?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "public, max-age=86400")
        .body(Body::from(body_bytes))
        .map_err(|error| AppError::Internal(format!("Failed to build response: {error}")))
}

fn placeholder_image_response() -> Result<Response, AppError> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")
        .header(header::CACHE_CONTROL, "public, max-age=3600")
        .body(Body::from(PLACEHOLDER_SVG))
        .map_err(|error| AppError::Internal(format!("Failed to build placeholder image: {error}")))
}
