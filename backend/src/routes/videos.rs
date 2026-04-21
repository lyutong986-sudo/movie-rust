use crate::{
    auth,
    error::AppError,
    models::{emby_id_to_uuid, VideoStreamQuery},
    naming, repository,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, Request, State},
    http::{header, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Router,
};
use reqwest::Client;
use serde::Deserialize;
use std::path::{Path as StdPath, PathBuf};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Videos/ActiveEncodings", delete(active_encodings))
        .route("/videos/ActiveEncodings", delete(active_encodings))
        .route("/Video/ActiveEncodings", delete(active_encodings))
        .route("/video/ActiveEncodings", delete(active_encodings))
        .route("/Videos/ActiveEncodings/Delete", get(active_encodings_delete).post(active_encodings_delete))
        .route("/videos/ActiveEncodings/Delete", get(active_encodings_delete).post(active_encodings_delete))
        .route("/Video/ActiveEncodings/Delete", get(active_encodings_delete).post(active_encodings_delete))
        .route("/video/ActiveEncodings/Delete", get(active_encodings_delete).post(active_encodings_delete))
        .route("/Videos/{item_id}/master.m3u8", get(master_playlist).head(master_playlist))
        .route("/videos/{item_id}/master.m3u8", get(master_playlist).head(master_playlist))
        .route("/Video/{item_id}/master.m3u8", get(master_playlist).head(master_playlist))
        .route("/video/{item_id}/master.m3u8", get(master_playlist).head(master_playlist))
        .route("/Videos/{item_id}/live.m3u8", get(master_playlist).head(master_playlist))
        .route("/videos/{item_id}/live.m3u8", get(master_playlist).head(master_playlist))
        .route("/Video/{item_id}/live.m3u8", get(master_playlist).head(master_playlist))
        .route("/video/{item_id}/live.m3u8", get(master_playlist).head(master_playlist))
        .route("/Videos/{item_id}/main.m3u8", get(main_playlist).head(main_playlist))
        .route("/videos/{item_id}/main.m3u8", get(main_playlist).head(main_playlist))
        .route("/Video/{item_id}/main.m3u8", get(main_playlist).head(main_playlist))
        .route("/video/{item_id}/main.m3u8", get(main_playlist).head(main_playlist))
        .route("/Videos/{item_id}/subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/videos/{item_id}/subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/Video/{item_id}/subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/video/{item_id}/subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/Videos/{item_id}/live_subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/videos/{item_id}/live_subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/Video/{item_id}/live_subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/video/{item_id}/live_subtitles.m3u8", get(subtitles_playlist).head(subtitles_playlist))
        .route("/Videos/{item_id}/hls1/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/videos/{item_id}/hls1/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/Video/{item_id}/hls1/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/video/{item_id}/hls1/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/Audio/{item_id}/master.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/audio/{item_id}/master.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/Audio/{item_id}/main.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/audio/{item_id}/main.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/Audio/{item_id}/live.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/audio/{item_id}/live.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/Audio/{item_id}/hls1/{_playlist_id}/{segment_file}", get(audio_hls_segment).head(audio_hls_segment))
        .route("/audio/{item_id}/hls1/{_playlist_id}/{segment_file}", get(audio_hls_segment).head(audio_hls_segment))
        .route("/Videos/{item_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream_legacy).head(subtitle_stream_legacy))
        .route("/videos/{item_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream_legacy).head(subtitle_stream_legacy))
        .route("/Items/{item_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream_legacy).head(subtitle_stream_legacy))
        .route("/items/{item_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream_legacy).head(subtitle_stream_legacy))
        .route("/Videos/{item_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start_legacy).head(subtitle_stream_with_start_legacy))
        .route("/videos/{item_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start_legacy).head(subtitle_stream_with_start_legacy))
        .route("/Items/{item_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start_legacy).head(subtitle_stream_with_start_legacy))
        .route("/items/{item_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start_legacy).head(subtitle_stream_with_start_legacy))
        .route("/Videos/{item_id}/{_media_source_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream).head(subtitle_stream))
        .route("/videos/{item_id}/{_media_source_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream).head(subtitle_stream))
        .route("/Items/{item_id}/{_media_source_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream).head(subtitle_stream))
        .route("/items/{item_id}/{_media_source_id}/Subtitles/{index}/Stream.{_format}", get(subtitle_stream).head(subtitle_stream))
        .route("/Videos/{item_id}/{_media_source_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start).head(subtitle_stream_with_start))
        .route("/videos/{item_id}/{_media_source_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start).head(subtitle_stream_with_start))
        .route("/Items/{item_id}/{_media_source_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start).head(subtitle_stream_with_start))
        .route("/items/{item_id}/{_media_source_id}/Subtitles/{index}/{_start_position_ticks}/Stream.{_format}", get(subtitle_stream_with_start).head(subtitle_stream_with_start))
        .route("/Videos/{item_id}/{_media_source_id}/Attachments/{index}/Stream", get(attachment_stream).head(attachment_stream))
        .route("/videos/{item_id}/{_media_source_id}/Attachments/{index}/Stream", get(attachment_stream).head(attachment_stream))
        .route("/Items/{item_id}/File", get(stream_file).head(stream_file))
        .route("/Items/{item_id}/Download", get(stream_file).head(stream_file))
        .route("/Videos/{item_id}/stream", get(stream_video).head(stream_video))
        .route("/videos/{item_id}/stream", get(stream_video).head(stream_video))
        .route("/Video/{item_id}/stream", get(stream_video).head(stream_video))
        .route("/video/{item_id}/stream", get(stream_video).head(stream_video))
        .route("/Videos/{item_id}/stream.{_container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/videos/{item_id}/stream.{_container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/Video/{item_id}/stream.{_container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/video/{item_id}/stream.{_container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/Videos/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/videos/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/Video/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/video/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/Videos/{item_id}/{_media_source_id}/stream.{_container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
        .route("/videos/{item_id}/{_media_source_id}/stream.{_container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
        .route("/Video/{item_id}/{_media_source_id}/stream.{_container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
        .route("/video/{item_id}/{_media_source_id}/stream.{_container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
}

async fn active_encodings(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<StatusCode, AppError> {
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn active_encodings_delete(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<StatusCode, AppError> {
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct VideoItemPath {
    item_id: String,
}

#[derive(Debug, Deserialize)]
struct VideoItemContainerPath {
    item_id: String,
    container: String,
}

#[derive(Debug, Deserialize)]
struct VideoMediaSourcePath {
    item_id: String,
    _media_source_id: String,
}

#[derive(Debug, Deserialize)]
struct VideoMediaSourceContainerPath {
    item_id: String,
    _media_source_id: String,
    container: String,
}

#[derive(Debug, Deserialize)]
struct HlsSegmentPath {
    item_id: String,
    _playlist_id: String,
    segment_file: String,
}

#[derive(Debug, Deserialize)]
struct SubtitlePath {
    item_id: String,
    _media_source_id: String,
    index: i32,
    _format: String,
}

#[derive(Debug, Deserialize)]
struct SubtitleStartPath {
    item_id: String,
    _media_source_id: String,
    index: i32,
    _start_position_ticks: String,
    _format: String,
}

#[derive(Debug, Deserialize)]
struct LegacySubtitlePath {
    item_id: String,
    index: i32,
    _format: String,
}

#[derive(Debug, Deserialize)]
struct LegacySubtitleStartPath {
    item_id: String,
    index: i32,
    _start_position_ticks: String,
    _format: String,
}

#[derive(Debug, Deserialize)]
struct AttachmentPath {
    item_id: String,
    _media_source_id: String,
    index: i32,
}

async fn stream_video(
    State(state): State<AppState>,
    Path(path): Path<VideoItemPath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    stream_video_request(&state, &path.item_id, None, None, query, request).await
}

async fn stream_video_with_container(
    State(state): State<AppState>,
    Path(path): Path<VideoItemContainerPath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    stream_video_request(&state, &path.item_id, None, Some(path.container), query, request).await
}

async fn stream_video_for_media_source(
    State(state): State<AppState>,
    Path(path): Path<VideoMediaSourcePath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    stream_video_request(&state, &path.item_id, Some(path._media_source_id), None, query, request)
        .await
}

async fn stream_video_for_media_source_with_container(
    State(state): State<AppState>,
    Path(path): Path<VideoMediaSourceContainerPath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    stream_video_request(
        &state,
        &path.item_id,
        Some(path._media_source_id),
        Some(path.container),
        query,
        request,
    )
    .await
}

async fn stream_file(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", item_id_str)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, item_id, request, None).await
}

async fn master_playlist(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    hls_playlist_response(&state, &item_id_str, query, request, true).await
}

async fn main_playlist(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    hls_playlist_response(&state, &item_id_str, query, request, false).await
}

async fn audio_master_playlist(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    hls_audio_playlist_response(&state, &item_id_str, query, request).await
}

async fn subtitles_playlist(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", item_id_str)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;

    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let subtitle_index = query.subtitle_stream_index.unwrap_or(0);

    let subtitle_path = repository::subtitle_path_for_stream_index(&item, subtitle_index);
    let codec = subtitle_path
        .as_deref()
        .and_then(StdPath::extension)
        .and_then(|value| value.to_str())
        .unwrap_or("vtt");
    let passthrough = auth_passthrough_query(request.uri().query());
    let subtitle_url = append_query_pairs(
        &format!(
            "/Videos/{}/Subtitles/{}/Stream.{}",
            crate::models::uuid_to_emby_guid(&item.id),
            subtitle_index,
            codec
        ),
        &passthrough,
    );

    let playlist = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:0,\n{subtitle_url}\n#EXT-X-ENDLIST\n"
    );
    playlist_response(request.method(), playlist)
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
            let user_id = Uuid::parse_str("00000000-0000-0000-0000-000000000000").unwrap();
            let device_id = "default-device".to_string();

            if state.config.enable_transcoding {
                tracing::info!(
                    item_id = %item_id,
                    user_id = %user_id,
                    "视频转码请求，启动转码会话"
                );

                match state
                    .transcoder
                    .start_transcoding(item_id, user_id, &device_id, q.clone(), &path)
                    .await
                {
                    Ok(session) => {
                        tracing::info!(
                            session_id = %session.id,
                            protocol = %session.protocol,
                            "转码会话已启动"
                        );
                        if session.protocol == "hls" {
                            let playlist_path = session.playlist_path;
                            if playlist_path.exists() {
                                return ServeFile::new(playlist_path)
                                    .oneshot(request)
                                    .await
                                    .map(IntoResponse::into_response)
                                    .map_err(|error| {
                                        AppError::Io(std::io::Error::new(
                                            std::io::ErrorKind::Other,
                                            error,
                                        ))
                                    });
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

async fn video_hls_segment(
    State(state): State<AppState>,
    Path(path): Path<HlsSegmentPath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, item_id, request, Some(query)).await
}

async fn audio_hls_segment(
    State(state): State<AppState>,
    Path(path): Path<HlsSegmentPath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_media_item(&state, item_id, request, Some(query)).await
}

async fn subtitle_stream(
    State(state): State<AppState>,
    Path(path): Path<SubtitlePath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_subtitle(&state, item_id, path.index, request).await
}

async fn subtitle_stream_legacy(
    State(state): State<AppState>,
    Path(path): Path<LegacySubtitlePath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("鏃犳晥鐨勯」鐩?ID 鏍煎紡: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_subtitle(&state, item_id, path.index, request).await
}

async fn subtitle_stream_with_start(
    State(state): State<AppState>,
    Path(path): Path<SubtitleStartPath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_subtitle(&state, item_id, path.index, request).await
}

async fn subtitle_stream_with_start_legacy(
    State(state): State<AppState>,
    Path(path): Path<LegacySubtitleStartPath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("鏃犳晥鐨勯」鐩?ID 鏍煎紡: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_subtitle(&state, item_id, path.index, request).await
}

async fn attachment_stream(
    State(state): State<AppState>,
    Path(path): Path<AttachmentPath>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("鏃犳晥鐨勯」鐩?ID 鏍煎紡: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_attachment(&state, item_id, path.index, request).await
}

async fn hls_playlist_response(
    state: &AppState,
    item_id_str: &str,
    query: VideoStreamQuery,
    request: Request<Body>,
    is_master: bool,
) -> Result<Response, AppError> {
    let requested_item_id = emby_id_to_uuid(item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", item_id_str)))?;
    auth::require_auth(state, request.headers(), request.uri().query()).await?;

    let item = repository::get_media_item(&state.pool, requested_item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let media_source =
        repository::get_media_source_with_streams(&state.pool, &item, state.config.server_id).await?;

    let passthrough = auth_passthrough_query(request.uri().query());
    let item_emby_id = crate::models::uuid_to_emby_guid(&item.id);
    let media_source_id = query
        .media_source_id
        .clone()
        .unwrap_or_else(|| media_source.id.clone());

    let playlist = if is_master {
        let bandwidth = media_source.bitrate.unwrap_or(8_000_000).max(1);
        let resolution = media_source
            .media_streams
            .iter()
            .find(|stream| stream.stream_type == "Video")
            .and_then(|stream| Some((stream.width?, stream.height?)))
            .map(|(w, h)| format!("{w}x{h}"))
            .unwrap_or_else(|| "1920x1080".to_string());
        let main_url = append_query_pairs(
            &format!("/Videos/{item_emby_id}/main.m3u8"),
            &extend_query_pairs(
                passthrough.clone(),
                vec![
                    ("MediaSourceId".to_string(), media_source_id.clone()),
                    ("mediaSourceId".to_string(), media_source_id.clone()),
                ],
            ),
        );

        format!(
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-STREAM-INF:BANDWIDTH={bandwidth},RESOLUTION={resolution}\n{main_url}\n"
        )
    } else {
        let segment_url = append_query_pairs(
            &format!("/Videos/{item_emby_id}/hls1/main/0.ts"),
            &extend_query_pairs(
                passthrough,
                video_query_pairs(&query, &media_source_id),
            ),
        );
        format!(
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:0,\n{segment_url}\n#EXT-X-ENDLIST\n"
        )
    };

    playlist_response(request.method(), playlist)
}

async fn hls_audio_playlist_response(
    state: &AppState,
    item_id_str: &str,
    query: VideoStreamQuery,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let requested_item_id = emby_id_to_uuid(item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", item_id_str)))?;
    auth::require_auth(state, request.headers(), request.uri().query()).await?;

    let item = repository::get_media_item(&state.pool, requested_item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let media_source =
        repository::get_media_source_with_streams(&state.pool, &item, state.config.server_id).await?;
    let passthrough = auth_passthrough_query(request.uri().query());
    let item_emby_id = crate::models::uuid_to_emby_guid(&item.id);
    let media_source_id = query
        .media_source_id
        .clone()
        .unwrap_or_else(|| media_source.id.clone());
    let segment_url = append_query_pairs(
        &format!("/Audio/{item_emby_id}/hls1/main/0.ts"),
        &extend_query_pairs(
            passthrough,
            video_query_pairs(&query, &media_source_id),
        ),
    );

    let playlist = format!(
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:0,\n{segment_url}\n#EXT-X-ENDLIST\n"
    );
    playlist_response(request.method(), playlist)
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

async fn serve_attachment(
    state: &AppState,
    item_id: Uuid,
    stream_index: i32,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    let streams = repository::get_media_streams(&state.pool, item_id).await?;
    let attachment_exists = streams.iter().any(|stream| {
        stream.index == stream_index
            && matches!(stream.stream_type.as_str(), "attachment" | "EmbeddedImage" | "embeddedimage")
    });

    if !attachment_exists {
        return Err(AppError::NotFound("闄勪欢娴佷笉瀛樺湪".to_string()));
    }

    let fallback_image = item
        .image_primary_path
        .clone()
        .or(item.thumb_path.clone())
        .or(item.backdrop_path.clone())
        .ok_or_else(|| AppError::NotFound("附件流暂不可用".to_string()))?;

    let path = PathBuf::from(fallback_image);
    if !path.exists() {
        return Err(AppError::NotFound("附件文件不存在".to_string()));
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

async fn stream_video_request(
    state: &AppState,
    item_id_str: &str,
    path_media_source_id: Option<String>,
    path_container: Option<String>,
    mut query: VideoStreamQuery,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let requested_item_id = emby_id_to_uuid(item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", item_id_str)))?;

    if query.container.is_none() {
        query.container = path_container;
    }

    let effective_media_source_id = query
        .media_source_id
        .as_deref()
        .or(path_media_source_id.as_deref());
    let item_id = resolve_stream_item_id(requested_item_id, effective_media_source_id)?;

    auth::require_auth(state, request.headers(), request.uri().query()).await?;

    tracing::debug!(
        requested_item_id = %requested_item_id,
        resolved_item_id = %item_id,
        media_source_id = ?effective_media_source_id,
        container = ?query.container,
        video_codec = ?query.video_codec,
        audio_codec = ?query.audio_codec,
        max_video_bitrate = ?query.max_video_bitrate,
        "视频流请求参数"
    );

    serve_media_item(state, item_id, request, Some(query)).await
}

fn resolve_stream_item_id(default_item_id: Uuid, media_source_id: Option<&str>) -> Result<Uuid, AppError> {
    let Some(media_source_id) = media_source_id else {
        return Ok(default_item_id);
    };

    let normalized = media_source_id.trim();
    let suffix = normalized
        .strip_prefix("mediasource_")
        .or_else(|| normalized.strip_prefix("MediaSource_"))
        .unwrap_or(normalized);

    if let Ok(media_item_id) = emby_id_to_uuid(suffix) {
        return Ok(media_item_id);
    }

    Ok(default_item_id)
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

fn playlist_response(method: &Method, playlist: String) -> Result<Response, AppError> {
    let body = if *method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(playlist)
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")
        .header(header::CACHE_CONTROL, "no-cache")
        .body(body)
        .map_err(|error| AppError::Internal(format!("构建 HLS 播放列表响应失败: {error}")))
}

fn auth_passthrough_query(query: Option<&str>) -> Vec<(String, String)> {
    let Some(query) = query else {
        return Vec::new();
    };

    url::form_urlencoded::parse(query.as_bytes())
        .filter_map(|(key, value)| {
            let key_ref = key.as_ref();
            match key_ref {
                "api_key" | "apiKey" | "ApiKey" | "X-Emby-Token" | "X-MediaBrowser-Token" | "PlaySessionId" | "Static" => {
                    Some((key.into_owned(), value.into_owned()))
                }
                _ => None,
            }
        })
        .collect()
}

fn append_query_pairs(base: &str, pairs: &[(String, String)]) -> String {
    if pairs.is_empty() {
        return base.to_string();
    }

    let separator = if base.contains('?') { '&' } else { '?' };
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    for (key, value) in pairs {
        serializer.append_pair(key, value);
    }
    let encoded = serializer.finish();

    format!("{base}{separator}{encoded}")
}

fn extend_query_pairs(
    mut base: Vec<(String, String)>,
    extra: Vec<(String, String)>,
) -> Vec<(String, String)> {
    base.extend(extra);
    base
}

fn video_query_pairs(
    query: &VideoStreamQuery,
    media_source_id: &str,
) -> Vec<(String, String)> {
    let mut pairs = vec![
        ("MediaSourceId".to_string(), media_source_id.to_string()),
        ("mediaSourceId".to_string(), media_source_id.to_string()),
    ];

    if let Some(value) = &query.container {
        pairs.push(("Container".to_string(), value.clone()));
    }
    if let Some(value) = &query.audio_codec {
        pairs.push(("AudioCodec".to_string(), value.clone()));
    }
    if let Some(value) = &query.video_codec {
        pairs.push(("VideoCodec".to_string(), value.clone()));
    }
    if let Some(value) = query.audio_stream_index {
        pairs.push(("AudioStreamIndex".to_string(), value.to_string()));
    }
    if let Some(value) = query.subtitle_stream_index {
        pairs.push(("SubtitleStreamIndex".to_string(), value.to_string()));
    }
    if let Some(value) = query.start_time_ticks {
        pairs.push(("StartTimeTicks".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_video_bitrate {
        pairs.push(("VideoBitRate".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_audio_channels {
        pairs.push(("MaxAudioChannels".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_width {
        pairs.push(("MaxWidth".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_height {
        pairs.push(("MaxHeight".to_string(), value.to_string()));
    }
    if let Some(value) = query.play_session_id.clone() {
        pairs.push(("PlaySessionId".to_string(), value));
    }
    if let Some(value) = query.static_param {
        pairs.push(("Static".to_string(), value.to_string()));
    }

    pairs
}
