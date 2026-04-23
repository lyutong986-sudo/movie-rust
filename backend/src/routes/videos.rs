use crate::{
    auth::{self, AuthSession},
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
use std::{
    path::{Path as StdPath, PathBuf},
    time::Duration,
};
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
        .route("/Videos/{item_id}/hls/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/videos/{item_id}/hls/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/Video/{item_id}/hls/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/video/{item_id}/hls/{_playlist_id}/{segment_file}", get(video_hls_segment).head(video_hls_segment))
        .route("/Audio/{item_id}/master.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/audio/{item_id}/master.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/Audio/{item_id}/main.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/audio/{item_id}/main.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/Audio/{item_id}/live.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/audio/{item_id}/live.m3u8", get(audio_master_playlist).head(audio_master_playlist))
        .route("/Audio/{item_id}/hls1/{_playlist_id}/{segment_file}", get(audio_hls_segment).head(audio_hls_segment))
        .route("/audio/{item_id}/hls1/{_playlist_id}/{segment_file}", get(audio_hls_segment).head(audio_hls_segment))
        .route("/Audio/{item_id}/hls/{_playlist_id}/{segment_file}", get(audio_hls_segment).head(audio_hls_segment))
        .route("/audio/{item_id}/hls/{_playlist_id}/{segment_file}", get(audio_hls_segment).head(audio_hls_segment))
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
        .route("/Videos/{item_id}/stream.{container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/videos/{item_id}/stream.{container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/Video/{item_id}/stream.{container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/video/{item_id}/stream.{container}", get(stream_video_with_container).head(stream_video_with_container))
        .route("/Videos/{item_id}/{stream_file_name}", get(stream_video_by_file_name).head(stream_video_by_file_name))
        .route("/videos/{item_id}/{stream_file_name}", get(stream_video_by_file_name).head(stream_video_by_file_name))
        .route("/Video/{item_id}/{stream_file_name}", get(stream_video_by_file_name).head(stream_video_by_file_name))
        .route("/video/{item_id}/{stream_file_name}", get(stream_video_by_file_name).head(stream_video_by_file_name))
        .route("/Videos/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/videos/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/Video/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/video/{item_id}/{_media_source_id}/stream", get(stream_video_for_media_source).head(stream_video_for_media_source))
        .route("/Videos/{item_id}/{_media_source_id}/stream.{container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
        .route("/videos/{item_id}/{_media_source_id}/stream.{container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
        .route("/Video/{item_id}/{_media_source_id}/stream.{container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
        .route("/video/{item_id}/{_media_source_id}/stream.{container}", get(stream_video_for_media_source_with_container).head(stream_video_for_media_source_with_container))
}

async fn active_encodings(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<StatusCode, AppError> {
    let session = auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    let query = request.uri().query().map(ToOwned::to_owned);
    let device_id = request_device_id(&request);
    stop_active_encoding_request(&state, &session, query.as_deref(), device_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn active_encodings_delete(
    State(state): State<AppState>,
    request: Request<Body>,
) -> Result<StatusCode, AppError> {
    let session = auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    let query = request.uri().query().map(ToOwned::to_owned);
    let device_id = request_device_id(&request);
    stop_active_encoding_request(&state, &session, query.as_deref(), device_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop_active_encoding_request(
    state: &AppState,
    session: &AuthSession,
    query: Option<&str>,
    device_id: Option<String>,
) -> Result<(), AppError> {
    if let Some(session_id) = query_value(
        query,
        &[
            "TranscodingJobId",
            "transcodingJobId",
            "EncodingJobId",
            "encodingJobId",
            "PlaySessionId",
            "playSessionId",
        ],
    )
    .and_then(|value| parse_transcoding_session_id(&value))
    {
        state.transcoder.stop_transcoding(session_id).await?;
    }

    let stopped = state
        .transcoder
        .stop_transcoding_for_user_device(session.user_id, device_id.as_deref())
        .await;
    tracing::debug!(
        user_id = %session.user_id,
        device_id = ?device_id,
        stopped,
        "ActiveEncodings cleanup completed"
    );
    Ok(())
}

fn parse_transcoding_session_id(value: &str) -> Option<Uuid> {
    let trimmed = value.trim();
    Uuid::parse_str(trimmed).ok().or_else(|| {
        if trimmed.len() == 32 {
            Uuid::parse_str(&format!(
                "{}-{}-{}-{}-{}",
                &trimmed[0..8],
                &trimmed[8..12],
                &trimmed[12..16],
                &trimmed[16..20],
                &trimmed[20..32]
            ))
            .ok()
        } else {
            None
        }
    })
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
struct VideoStreamFilePath {
    item_id: String,
    stream_file_name: String,
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
    stream_video_request(
        &state,
        &path.item_id,
        None,
        Some(path.container),
        query,
        request,
    )
    .await
}

async fn stream_video_by_file_name(
    State(state): State<AppState>,
    Path(path): Path<VideoStreamFilePath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let container = path
        .stream_file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_string());
    stream_video_request(&state, &path.item_id, None, container, query, request).await
}

async fn stream_video_for_media_source(
    State(state): State<AppState>,
    Path(path): Path<VideoMediaSourcePath>,
    Query(query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    stream_video_request(
        &state,
        &path.item_id,
        Some(path._media_source_id),
        None,
        query,
        request,
    )
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
    let session = auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    let device_id = request_device_id(&request);
    serve_media_item(&state, item_id, request, None, &session, device_id).await
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

    let playlist =
        format!("#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:0,\n{subtitle_url}\n#EXT-X-ENDLIST\n");
    playlist_response(request.method(), playlist)
}

async fn serve_media_item(
    state: &AppState,
    item_id: Uuid,
    request: Request<Body>,
    query: Option<VideoStreamQuery>,
    session: &AuthSession,
    device_id: Option<String>,
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
            || effective_max_video_bitrate(q).is_some()
            || q.max_audio_channels.is_some()
            || q.max_width.is_some()
            || q.max_height.is_some()
            || q.max_framerate.is_some()
            || q.transcoding_protocol.is_some()
            || q.segment_container.is_some()
            || q.segment_length.is_some()
            || q.min_segments.is_some()
            || q.break_on_non_key_frames.is_some()
            || q.video_stream_index.is_some();
        if has_transcoding_params {
            let mut transcoding_query = q.clone();
            if transcoding_query.max_video_bitrate.is_none() {
                transcoding_query.max_video_bitrate = q.max_streaming_bitrate.or(q.video_bitrate);
            }
            let user_id = session.user_id;
            let device_id = device_id
                .clone()
                .unwrap_or_else(|| "unknown-device".to_string());

            let encoding_options = repository::encoding_options(&state.pool, &state.config).await?;
            if encoding_options.enable_transcoding {
                tracing::info!(
                    item_id = %item_id,
                    user_id = %user_id,
                    "视频转码请求，启动转码会话"
                );

                match state
                    .transcoder
                    .start_transcoding(item_id, user_id, &device_id, transcoding_query, encoding_options, &path)
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
    Query(_query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_hls_segment(
        &state,
        item_id,
        &path._playlist_id,
        &path.segment_file,
        request,
    )
    .await
}

async fn audio_hls_segment(
    State(state): State<AppState>,
    Path(path): Path<HlsSegmentPath>,
    Query(_query): Query<VideoStreamQuery>,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&path.item_id)
        .map_err(|_| AppError::BadRequest(format!("无效的项目 ID 格式: {}", path.item_id)))?;
    auth::require_auth(&state, request.headers(), request.uri().query()).await?;
    serve_hls_segment(
        &state,
        item_id,
        &path._playlist_id,
        &path.segment_file,
        request,
    )
    .await
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
    let session = auth::require_auth(state, request.headers(), request.uri().query()).await?;
    let device_id = request_device_id(&request);
    if use_transcoded_hls_playlist() {
        return transcoded_hls_playlist_response(
            state,
            requested_item_id,
            query.clone(),
            request.method().clone(),
            request.uri().query(),
            &session,
            device_id,
            false,
        )
        .await;
    }

    let item = repository::get_media_item(&state.pool, requested_item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let media_source =
        repository::get_media_source_with_streams(&state.pool, &item, state.config.server_id)
            .await?;

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
            &extend_query_pairs(passthrough, video_query_pairs(&query, &media_source_id)),
        );
        format!("#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:0,\n{segment_url}\n#EXT-X-ENDLIST\n")
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
    let session = auth::require_auth(state, request.headers(), request.uri().query()).await?;
    let device_id = request_device_id(&request);
    if use_transcoded_hls_playlist() {
        return transcoded_hls_playlist_response(
            state,
            requested_item_id,
            query.clone(),
            request.method().clone(),
            request.uri().query(),
            &session,
            device_id,
            true,
        )
        .await;
    }

    let item = repository::get_media_item(&state.pool, requested_item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let media_source =
        repository::get_media_source_with_streams(&state.pool, &item, state.config.server_id)
            .await?;
    let passthrough = auth_passthrough_query(request.uri().query());
    let item_emby_id = crate::models::uuid_to_emby_guid(&item.id);
    let media_source_id = query
        .media_source_id
        .clone()
        .unwrap_or_else(|| media_source.id.clone());
    let segment_url = append_query_pairs(
        &format!("/Audio/{item_emby_id}/hls1/main/0.ts"),
        &extend_query_pairs(passthrough, video_query_pairs(&query, &media_source_id)),
    );

    let playlist =
        format!("#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:0,\n{segment_url}\n#EXT-X-ENDLIST\n");
    playlist_response(request.method(), playlist)
}

fn use_transcoded_hls_playlist() -> bool {
    true
}

async fn transcoded_hls_playlist_response(
    state: &AppState,
    item_id: Uuid,
    mut query: VideoStreamQuery,
    method: Method,
    original_query: Option<&str>,
    session: &AuthSession,
    device_id: Option<String>,
    is_audio: bool,
) -> Result<Response, AppError> {
    let encoding_options = repository::encoding_options(&state.pool, &state.config).await?;
    if !encoding_options.enable_transcoding {
        return Err(AppError::BadRequest(
            "HLS 播放需要启用真实转码输出".to_string(),
        ));
    }

    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let path = PathBuf::from(&item.path);
    if !path.exists() {
        return Err(AppError::NotFound("媒体文件不存在".to_string()));
    }
    if naming::is_strm(&path) {
        return Err(AppError::BadRequest(
            "STRM 远程流不支持服务端 HLS 分片转码".to_string(),
        ));
    }

    query.transcoding_protocol = Some("hls".to_string());
    let session = state
        .transcoder
        .start_transcoding(
            item_id,
            session.user_id,
            &device_id.unwrap_or_else(|| "unknown-device".to_string()),
            query,
            encoding_options,
            &path,
        )
        .await?;

    wait_for_file(&session.playlist_path, Duration::from_secs(3)).await?;
    let playlist = tokio::fs::read_to_string(&session.playlist_path).await?;
    let passthrough = auth_passthrough_query(original_query);
    let item_emby_id = crate::models::uuid_to_emby_guid(&item_id);
    let rewritten = rewrite_hls_playlist(
        &playlist,
        &item_emby_id,
        &session.id.to_string(),
        &passthrough,
        is_audio,
    );
    playlist_response(&method, rewritten)
}

async fn serve_hls_segment(
    state: &AppState,
    item_id: Uuid,
    playlist_id: &str,
    segment_file: &str,
    request: Request<Body>,
) -> Result<Response, AppError> {
    let session_id = Uuid::parse_str(playlist_id)
        .map_err(|_| AppError::BadRequest("无效的 HLS 会话 ID".to_string()))?;
    let session = state
        .transcoder
        .get_session(session_id)
        .await
        .ok_or_else(|| AppError::NotFound("HLS 转码会话不存在".to_string()))?;
    if session.media_item_id != item_id {
        return Err(AppError::NotFound("HLS 分片不属于当前媒体".to_string()));
    }
    if segment_file.contains(['\\', '/', ':']) || segment_file.trim().is_empty() {
        return Err(AppError::BadRequest("无效的 HLS 分片文件名".to_string()));
    }

    let path = session.output_dir.join(segment_file);
    if !path.is_file() {
        return Err(AppError::NotFound("HLS 分片尚未生成".to_string()));
    }

    ServeFile::new(path)
        .oneshot(request)
        .await
        .map(IntoResponse::into_response)
        .map_err(|error| AppError::Io(std::io::Error::new(std::io::ErrorKind::Other, error)))
}

async fn wait_for_file(path: &StdPath, timeout: Duration) -> Result<(), AppError> {
    let started = tokio::time::Instant::now();
    while started.elapsed() < timeout {
        if path.is_file() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    Err(AppError::Internal(
        "HLS 转码播放列表尚未生成，请稍后重试".to_string(),
    ))
}

fn rewrite_hls_playlist(
    playlist: &str,
    item_emby_id: &str,
    session_id: &str,
    passthrough: &[(String, String)],
    is_audio: bool,
) -> String {
    let media_prefix = if is_audio { "Audio" } else { "Videos" };
    playlist
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                return line.to_string();
            }

            let file_name = StdPath::new(trimmed)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or(trimmed);
            append_query_pairs(
                &format!("/{media_prefix}/{item_emby_id}/hls1/{session_id}/{file_name}"),
                passthrough,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
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
            && matches!(
                stream.stream_type.as_str(),
                "attachment" | "EmbeddedImage" | "embeddedimage"
            )
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

    let session = auth::require_auth(state, request.headers(), request.uri().query()).await?;
    let device_id = request_device_id(&request);

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

    serve_media_item(state, item_id, request, Some(query), &session, device_id).await
}

fn resolve_stream_item_id(
    default_item_id: Uuid,
    media_source_id: Option<&str>,
) -> Result<Uuid, AppError> {
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

fn request_device_id(request: &Request<Body>) -> Option<String> {
    auth::client_value(request.headers(), "DeviceId")
        .or_else(|| query_value(request.uri().query(), &["DeviceId", "deviceId"]))
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

fn query_value(query: Option<&str>, keys: &[&str]) -> Option<String> {
    let query = query?;
    url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
        if keys
            .iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
        {
            Some(value.into_owned()).filter(|value| !value.trim().is_empty())
        } else {
            None
        }
    })
}

fn auth_passthrough_query(query: Option<&str>) -> Vec<(String, String)> {
    let Some(query) = query else {
        return Vec::new();
    };

    url::form_urlencoded::parse(query.as_bytes())
        .filter_map(|(key, value)| {
            let key_ref = key.as_ref();
            match key_ref {
                "api_key"
                | "apiKey"
                | "ApiKey"
                | "X-Emby-Token"
                | "X-MediaBrowser-Token"
                | "PlaySessionId"
                | "playSessionId"
                | "Static"
                | "static"
                | "DeviceId"
                | "deviceId"
                | "MediaSourceId"
                | "mediaSourceId"
                | "StartTimeTicks"
                | "startTimeTicks"
                | "SubtitleStreamIndex"
                | "subtitleStreamIndex"
                | "AudioStreamIndex"
                | "audioStreamIndex" => Some((key.into_owned(), value.into_owned())),
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

fn video_query_pairs(query: &VideoStreamQuery, media_source_id: &str) -> Vec<(String, String)> {
    let effective_max_bitrate = effective_max_video_bitrate(query);
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
    if let Some(value) = effective_max_bitrate {
        pairs.push(("VideoBitRate".to_string(), value.to_string()));
        pairs.push(("MaxStreamingBitrate".to_string(), value.to_string()));
    }
    if let Some(value) = query.video_bitrate {
        pairs.push(("VideoBitrate".to_string(), value.to_string()));
    }
    if let Some(value) = query.audio_bitrate {
        pairs.push(("AudioBitrate".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_audio_channels {
        pairs.push(("MaxAudioChannels".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_framerate {
        pairs.push(("MaxFramerate".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_width {
        pairs.push(("MaxWidth".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_height {
        pairs.push(("MaxHeight".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_ref_frames {
        pairs.push(("MaxRefFrames".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_video_bit_depth {
        pairs.push(("MaxVideoBitDepth".to_string(), value.to_string()));
    }
    if let Some(value) = query.max_audio_bit_depth {
        pairs.push(("MaxAudioBitDepth".to_string(), value.to_string()));
    }
    if let Some(value) = query.audio_sample_rate {
        pairs.push(("AudioSampleRate".to_string(), value.to_string()));
    }
    if let Some(value) = query.play_session_id.clone() {
        pairs.push(("PlaySessionId".to_string(), value));
    }
    if let Some(value) = query.static_param {
        pairs.push(("Static".to_string(), value.to_string()));
    }
    if let Some(value) = &query.subtitle_method {
        pairs.push(("SubtitleMethod".to_string(), value.clone()));
    }
    if let Some(value) = query.require_avc {
        pairs.push(("RequireAvc".to_string(), value.to_string()));
    }
    if let Some(value) = query.de_interlace {
        pairs.push(("DeInterlace".to_string(), value.to_string()));
    }
    if let Some(value) = query.require_non_anamorphic {
        pairs.push(("RequireNonAnamorphic".to_string(), value.to_string()));
    }
    if let Some(value) = query.transcoding_max_audio_channels {
        pairs.push(("TranscodingMaxAudioChannels".to_string(), value.to_string()));
    }
    if let Some(value) = query.cpu_core_limit {
        pairs.push(("CPUCoreLimit".to_string(), value.to_string()));
    }
    if let Some(value) = &query.live_stream_id {
        pairs.push(("LiveStreamId".to_string(), value.clone()));
    }
    if let Some(value) = query.enable_mpegts_m2_ts_mode {
        pairs.push(("EnableMpegtsM2TsMode".to_string(), value.to_string()));
    }
    if let Some(value) = query.video_stream_index {
        pairs.push(("VideoStreamIndex".to_string(), value.to_string()));
    }
    if let Some(value) = &query.transcoding_protocol {
        pairs.push(("TranscodingProtocol".to_string(), value.clone()));
    }
    if let Some(value) = &query.segment_container {
        pairs.push(("SegmentContainer".to_string(), value.clone()));
    }
    if let Some(value) = query.segment_length {
        pairs.push(("SegmentLength".to_string(), value.to_string()));
    }
    if let Some(value) = query.min_segments {
        pairs.push(("MinSegments".to_string(), value.to_string()));
    }
    if let Some(value) = query.break_on_non_key_frames {
        pairs.push(("BreakOnNonKeyFrames".to_string(), value.to_string()));
    }
    if let Some(value) = &query.manifest_subtitles {
        pairs.push(("ManifestSubtitles".to_string(), value.clone()));
    }

    pairs
}

fn effective_max_video_bitrate(query: &VideoStreamQuery) -> Option<i64> {
    query
        .max_video_bitrate
        .or(query.max_streaming_bitrate)
        .or(query.video_bitrate)
}
