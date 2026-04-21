use crate::{auth, error::AppError, models::{emby_id_to_uuid, VideoStreamQuery}, naming, repository, state::AppState};
use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get},
    Router,
};
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Videos/ActiveEncodings", delete(active_encodings))
        .route("/videos/ActiveEncodings", delete(active_encodings))
        .route("/Video/ActiveEncodings", delete(active_encodings))
        .route("/video/ActiveEncodings", delete(active_encodings))
        .route("/Videos/{item_id}/{*stream_path}", get(stream_video))
        .route("/videos/{item_id}/{*stream_path}", get(stream_video))
        .route("/Video/{item_id}/{*stream_path}", get(stream_video))
        .route("/video/{item_id}/{*stream_path}", get(stream_video))
        .route("/Items/{item_id}/File", get(stream_file))
        .route("/Items/{item_id}/Download", get(stream_file))
}

async fn active_encodings(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<StatusCode, AppError> {
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct VideoPath {
    item_id: String,
    stream_path: String,
}

async fn stream_video(
    State(state): State<AppState>,
    Path(path): Path<VideoPath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", path.item_id)))?;
    let stream_path = path.stream_path.trim_start_matches('/');
    if let Some(subtitle_index) = parse_subtitle_stream_index(stream_path) {
        auth::require_auth(&state, request.headers(), request.uri().query()).await?;
        return serve_subtitle(&state, item_id, subtitle_index, request).await;
    }

    if !stream_path.starts_with("stream") {
        return Err(AppError::NotFound("视频流路径不存在".to_string()));
    }

    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    
    tracing::debug!(
        item_id = %item_id,
        container = ?query.container,
        video_codec = ?query.video_codec,
        audio_codec = ?query.audio_codec,
        max_video_bitrate = ?query.max_video_bitrate,
        "视频流请求参数"
    );
    
    serve_media_item(&state, item_id, request, Some(query)).await
}

async fn stream_file(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, item_id, request, None).await
}

async fn serve_media_item(
    state: &AppState,
    item_id: Uuid,
    request: Request<Body>,
    query: Option<VideoStreamQuery>,
) -> Result<Response, AppError> {
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let path = PathBuf::from(&item.path);
    if !path.exists() {
        return Err(AppError::NotFound("媒体文件不存在".to_string()));
    }

    if naming::is_strm(&path) {
        let content = tokio::fs::read_to_string(&path).await?;
        let target = naming::strm_target_from_text(&content)
            .ok_or_else(|| AppError::BadRequest("STRM 文件没有有效的远程播放地址".to_string()))?;

        tracing::info!(
            item_id = %item.id,
            item_name = %item.name,
            target = %target,
            "代理 STRM 远程播放流"
        );
        return proxy_remote_stream(&target, request).await;
    }

    if let Some(ref q) = query {
        let has_transcoding_params = q.video_codec.is_some()
            || q.audio_codec.is_some()
            || q.max_video_bitrate.is_some()
            || q.max_audio_channels.is_some()
            || q.max_width.is_some()
            || q.max_height.is_some()
            || q.max_framerate.is_some();
        if has_transcoding_params {
            // 尝试获取用户ID和设备ID（简化实现）
            // 在实际实现中，这些信息应从认证会话中获取
            let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap(); // 默认用户ID
            let device_id = "default-device".to_string();
            
            // 检查是否启用转码
            if state.config.enable_transcoding {
                tracing::info!(
                    item_id = %item_id,
                    user_id = %user_id,
                    "视频转码请求，启动转码会话"
                );
                
                // 尝试启动转码会话
                match state.transcoder.start_transcoding(
                    item_id,
                    user_id,
                    &device_id,
                    q.clone(),
                    &path,
                ).await {
                    Ok(session) => {
                        tracing::info!(
                            session_id = %session.id,
                            protocol = %session.protocol,
                            "转码会话已启动"
                        );
                        // 对于HLS/DASH协议，应该返回播放列表文件
                        // 简化实现：返回一个指示转码已启动的响应
                        if session.protocol == "hls" {
                            let playlist_path = session.playlist_path;
                            if playlist_path.exists() {
                                return ServeFile::new(playlist_path)
                                    .oneshot(request)
                                    .await
                                    .map(IntoResponse::into_response)
                                    .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)));
                            }
                        }
                        tracing::warn!("转码会话已启动，但播放列表尚未生成，使用直接播放");
                    }
                    Err(e) => {
                        tracing::error!(
                            item_id = %item_id,
                            error = %e,
                            "启动转码会话失败，使用直接播放"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    item_id = %item_id,
                    "视频转码请求，但转码功能已禁用，使用直接播放"
                );
            }
        }
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

async fn proxy_remote_stream(url: &str, request: Request<Body>) -> Result<Response, AppError> {
    let method = request.method().clone();
    let client = Client::new();
    let mut remote_request = if method == Method::HEAD {
        client.head(url)
    } else {
        client.get(url)
    };

    for name in [
        header::RANGE,
        header::IF_RANGE,
        header::USER_AGENT,
        header::ACCEPT,
        header::ACCEPT_LANGUAGE,
    ] {
        if let Some(value) = request
            .headers()
            .get(&name)
            .and_then(|value| value.to_str().ok())
        {
            remote_request = remote_request.header(name.as_str(), value);
        }
    }

    let remote_response = remote_request
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("请求 STRM 远程流失败: {error}")))?;
    let status =
        StatusCode::from_u16(remote_response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let headers = remote_response.headers().clone();

    tracing::info!(
        url = %url,
        status = %status,
        "STRM 远程流已响应"
    );

    let mut response_builder = Response::builder().status(status);
    for (key, value) in headers.iter() {
        if is_hop_by_hop_header(key.as_str()) {
            continue;
        }

        if let Ok(value) = value.to_str() {
            response_builder = response_builder.header(key.as_str(), value);
        }
    }

    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from_stream(remote_response.bytes_stream())
    };

    response_builder
        .body(body)
        .map_err(|error| AppError::Internal(format!("构建 STRM 播放响应失败: {error}")))
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

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
    )
}
