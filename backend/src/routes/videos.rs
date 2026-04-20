use crate::{auth, error::AppError, repository, state::AppState};
use axum::{
    body::Body,
    extract::{Path, Request, State},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Videos/{item_id}/{*stream_path}", get(stream_video))
        .route("/Items/{item_id}/File", get(stream_file))
        .route("/Items/{item_id}/Download", get(stream_file))
}

#[derive(Debug, Deserialize)]
struct VideoPath {
    item_id: Uuid,
    stream_path: String,
}

async fn stream_video(
    State(state): State<AppState>,
    Path(path): Path<VideoPath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let stream_path = path.stream_path.trim_start_matches('/');
    if let Some(subtitle_index) = parse_subtitle_stream_index(stream_path) {
        auth::require_auth(&state, request.headers(), request.uri().query()).await?;
        return serve_subtitle(&state, path.item_id, subtitle_index, request).await;
    }

    if !stream_path.starts_with("stream") {
        return Err(AppError::NotFound("视频流路径不存在".to_string()));
    }

    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, path.item_id, request).await
}

async fn stream_file(
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, item_id, request).await
}

async fn serve_media_item(
    state: &AppState,
    item_id: Uuid,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let path = PathBuf::from(item.path);
    if !path.exists() {
        return Err(AppError::NotFound("媒体文件不存在".to_string()));
    }

    // 检查是否是 .strm 文件
    if path.extension().and_then(|ext| ext.to_str()).map_or(false, |ext| ext.eq_ignore_ascii_case("strm")) {
        // 读取 .strm 文件内容（第一行）
        let content = tokio::fs::read_to_string(&path).await
            .map_err(|e| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
        let url = content.lines().next().map(str::trim).filter(|line| !line.is_empty());
        if let Some(url) = url {
            if url.starts_with("http://") || url.starts_with("https://") {
                // 返回 302 重定向到真实 URL
                return Ok(Redirect::to(url).into_response());
            }
        }
        // 如果不是有效的 URL，回退到普通文件服务
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

async fn serve_subtitle(
    state: &AppState,
    item_id: Uuid,
    stream_index: i32,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let path = repository::subtitle_path_for_stream_index(&item, stream_index)
        .ok_or_else(|| AppError::NotFound("字幕不存在".to_string()))?;

    if !path.exists() {
        return Err(AppError::NotFound("字幕文件不存在".to_string()));
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

fn parse_subtitle_stream_index(stream_path: &str) -> Option<i32> {
    let parts = stream_path.split('/').collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }

    if !parts.get(1)?.eq_ignore_ascii_case("Subtitles") {
        return None;
    }

    parts.get(2)?.parse().ok()
}
