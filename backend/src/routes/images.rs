use crate::{
    auth::AuthSession, error::AppError, models::ImageInfoDto, repository, state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Items/{item_id}/Images", get(list_item_images))
        .route("/Items/{item_id}/Images/{image_type}", get(get_item_image))
        .route(
            "/Items/{item_id}/Images/{image_type}/{image_index}",
            get(get_item_image_by_index),
        )
        .route(
            "/Users/{user_id}/Images/{image_type}",
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
    session: AuthSession,
    State(state): State<AppState>,
    Path((item_id, image_type)): Path<(Uuid, String)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_item_image(session, state, item_id, image_type, request).await
}

async fn get_item_image_by_index(
    session: AuthSession,
    State(state): State<AppState>,
    Path((item_id, image_type, _image_index)): Path<(Uuid, String, i32)>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    serve_item_image(session, state, item_id, image_type, request).await
}

async fn serve_item_image(
    _session: AuthSession,
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

    serve_path(PathBuf::from(path), request).await
}

async fn user_image_placeholder() -> StatusCode {
    StatusCode::NOT_FOUND
}

async fn serve_path(path: PathBuf, request: Request<Body>) -> Result<Response, AppError> {
    if !path.exists() {
        return Err(AppError::NotFound("文件不存在".to_string()));
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}
