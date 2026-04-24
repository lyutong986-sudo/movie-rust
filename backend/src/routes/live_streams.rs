use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::AuthSession, error::AppError, models::emby_id_to_uuid, repository, state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/LiveStreams/Open", post(open_live_stream))
        .route("/livestreams/open", post(open_live_stream))
        .route("/LiveStreams/Close", post(close_live_stream))
        .route("/livestreams/close", post(close_live_stream))
        .route("/LiveStreams/MediaInfo", get(media_info).post(media_info))
        .route("/livestreams/mediainfo", get(media_info).post(media_info))
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct OpenLiveStreamBody {
    #[serde(default, alias = "itemId", alias = "ItemId")]
    item_id: Option<String>,
    #[serde(default, alias = "mediaSourceId", alias = "MediaSourceId")]
    media_source_id: Option<String>,
    #[serde(default, alias = "openToken", alias = "OpenToken")]
    open_token: Option<String>,
    #[serde(default, alias = "maxStreamingBitrate", alias = "MaxStreamingBitrate")]
    max_streaming_bitrate: Option<i64>,
    #[serde(default, alias = "startTimeTicks", alias = "StartTimeTicks")]
    start_time_ticks: Option<i64>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct OpenLiveStreamQuery {
    #[serde(default, alias = "itemId", alias = "ItemId")]
    item_id: Option<String>,
    #[serde(default, alias = "mediaSourceId", alias = "MediaSourceId")]
    media_source_id: Option<String>,
    #[serde(default, alias = "openToken", alias = "OpenToken")]
    open_token: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct CloseLiveStreamBody {
    #[serde(default, alias = "liveStreamId", alias = "LiveStreamId")]
    live_stream_id: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct MediaInfoQuery {
    #[serde(default, alias = "liveStreamId", alias = "LiveStreamId")]
    live_stream_id: Option<String>,
    #[serde(default, alias = "itemId", alias = "ItemId")]
    item_id: Option<String>,
    #[serde(default, alias = "mediaSourceId", alias = "MediaSourceId")]
    media_source_id: Option<String>,
}

async fn open_live_stream(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<OpenLiveStreamQuery>,
    body: Option<Json<OpenLiveStreamBody>>,
) -> Result<Json<Value>, AppError> {
    let body = body.map(|Json(b)| b).unwrap_or_default();

    let item_id_raw = body
        .item_id
        .clone()
        .or(query.item_id.clone())
        .or_else(|| {
            // 部分客户端把 item_id 拼进 OpenToken： "<providerId>_<itemId>[_<mediaSourceId>]"
            body.open_token
                .as_deref()
                .or(query.open_token.as_deref())
                .and_then(parse_item_id_from_open_token)
        })
        .ok_or_else(|| AppError::BadRequest("缺少 ItemId / OpenToken".to_string()))?;

    let item_id = emby_id_to_uuid(&item_id_raw)
        .map_err(|_| AppError::BadRequest(format!("无效 ItemId: {item_id_raw}")))?;

    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    // 复用现有聚合逻辑
    let mut sources =
        repository::get_media_sources_for_item(&state.pool, &item, state.config.server_id).await?;
    if sources.is_empty() {
        sources.push(
            repository::get_media_source_with_streams(&state.pool, &item, state.config.server_id)
                .await?,
        );
    }

    let desired = body.media_source_id.or(query.media_source_id);
    let idx = desired
        .as_deref()
        .and_then(|id| sources.iter().position(|s| s.id.eq_ignore_ascii_case(id)))
        .unwrap_or(0);

    let mut source = sources.swap_remove(idx);
    let live_id = uuid::Uuid::new_v4().simple().to_string();
    source.live_stream_id = Some(live_id.clone());

    // 记录会话信息，便于 Close 对账（短期缓存）
    let _ = repository::set_setting_value(
        &state.pool,
        &format!("livestream:{live_id}"),
        json!({
            "UserId": session.user_id.to_string(),
            "ItemId": item_id.to_string(),
            "MediaSourceId": source.id,
            "OpenedAt": chrono::Utc::now().to_rfc3339(),
            "MaxStreamingBitrate": body.max_streaming_bitrate,
            "StartTimeTicks": body.start_time_ticks,
        }),
    )
    .await;

    Ok(Json(json!({
        "MediaSource": source,
        "MediaSources": [source],
    })))
}

async fn close_live_stream(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<MediaInfoQuery>,
    body: Option<Json<CloseLiveStreamBody>>,
) -> Result<StatusCode, AppError> {
    let id = body
        .and_then(|Json(b)| b.live_stream_id)
        .or(query.live_stream_id)
        .ok_or_else(|| AppError::BadRequest("缺少 LiveStreamId".to_string()))?;
    let _ = repository::delete_setting_value(&state.pool, &format!("livestream:{id}")).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn media_info(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<MediaInfoQuery>,
) -> Result<Json<Value>, AppError> {
    if let Some(id) = query.live_stream_id.as_deref() {
        let doc = repository::get_setting_value(&state.pool, &format!("livestream:{id}")).await?;
        if let Some(doc) = doc {
            // 复用 OpenLiveStream 的 MediaSource 构造，避免前端对 MediaInfo 再发一次 PlaybackInfo。
            let item_id_str = doc
                .get("ItemId")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::Internal("livestream 状态缺失 ItemId".to_string()))?;
            let item_id = emby_id_to_uuid(item_id_str)
                .map_err(|_| AppError::Internal("livestream 状态 ItemId 无效".to_string()))?;
            let item = repository::get_media_item(&state.pool, item_id)
                .await?
                .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
            let mut sources =
                repository::get_media_sources_for_item(&state.pool, &item, state.config.server_id)
                    .await?;
            if sources.is_empty() {
                sources.push(
                    repository::get_media_source_with_streams(
                        &state.pool,
                        &item,
                        state.config.server_id,
                    )
                    .await?,
                );
            }
            let desired = doc
                .get("MediaSourceId")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned);
            let idx = desired
                .as_deref()
                .and_then(|id| sources.iter().position(|s| s.id.eq_ignore_ascii_case(id)))
                .unwrap_or(0);
            let source = sources.swap_remove(idx);
            return Ok(Json(json!({
                "MediaSource": source,
                "LiveStreamId": id,
            })));
        }
    }

    // 无 live stream 记录时，如果提供了 itemId/mediaSourceId，直接计算一次。
    let item_id_raw = query
        .item_id
        .clone()
        .ok_or_else(|| AppError::BadRequest("缺少 ItemId".to_string()))?;
    let item_id = emby_id_to_uuid(&item_id_raw)
        .map_err(|_| AppError::BadRequest(format!("无效 ItemId: {item_id_raw}")))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let mut sources =
        repository::get_media_sources_for_item(&state.pool, &item, state.config.server_id).await?;
    if sources.is_empty() {
        sources.push(
            repository::get_media_source_with_streams(&state.pool, &item, state.config.server_id)
                .await?,
        );
    }
    let idx = query
        .media_source_id
        .as_deref()
        .and_then(|id| sources.iter().position(|s| s.id.eq_ignore_ascii_case(id)))
        .unwrap_or(0);
    let source = sources.swap_remove(idx);
    Ok(Json(json!({ "MediaSource": source })))
}

fn parse_item_id_from_open_token(token: &str) -> Option<String> {
    // 约定 OpenToken 格式 "<provider>_<itemId>[_<mediaSourceId>]"
    let parts: Vec<&str> = token.split('_').collect();
    if parts.len() >= 2 {
        return Some(parts[1].to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_streams_router_builds_without_conflicts() {
        let _ = router();
    }

    #[test]
    fn open_token_parsing_extracts_item_id() {
        assert_eq!(
            parse_item_id_from_open_token("tmdb_abc123_source1"),
            Some("abc123".into())
        );
        assert_eq!(
            parse_item_id_from_open_token("provider_42"),
            Some("42".into())
        );
        assert_eq!(parse_item_id_from_open_token("onlyonepart"), None);
    }
}
