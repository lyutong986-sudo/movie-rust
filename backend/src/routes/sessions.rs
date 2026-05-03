use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::{BaseItemDto, LegacyPlaybackQuery, PlaybackReport, QueryResult, SessionInfoDto},
    repository,
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Auth/Keys", get(list_auth_keys).post(create_auth_key))
        .route("/Auth/Keys/{key}", delete(delete_auth_key))
        .route("/Auth/Keys/{key}/Delete", post(delete_auth_key))
        .route("/Auth/Providers", get(auth_providers))
        .route("/Sessions", get(list_sessions))
        .route("/Sessions/PlayQueue", get(play_queue))
        .route("/Sessions/{id}/PlayQueue", get(session_play_queue))
        .route("/Sessions/{id}/Commands", get(session_commands))
        .route("/Sessions/{id}/Commands/Pending", get(session_commands))
        .route("/Sessions/{id}/Command", post(no_content_for_session))
        .route(
            "/Sessions/{id}/Command/{command}",
            post(no_content_for_session_command),
        )
        .route("/Sessions/{id}/Message", post(message_for_session))
        .route("/Sessions/{id}/Playing", post(no_content_for_session))
        .route(
            "/Sessions/{id}/Playing/{command}",
            post(no_content_for_session_command),
        )
        .route(
            "/Sessions/{id}/System/{command}",
            post(no_content_for_session_command),
        )
        .route(
            "/Sessions/{id}/Users/{user_id}",
            post(no_content_for_session_user).delete(no_content_for_session_user),
        )
        .route(
            "/Sessions/{id}/Users/{user_id}/Delete",
            post(no_content_for_session_user),
        )
        .route("/Sessions/{id}/Viewing", post(update_session_viewing))
        .route("/Sessions/Capabilities", post(update_capabilities))
        .route("/Sessions/Capabilities/Full", post(update_capabilities))
        .route("/Sessions/Logout", post(logout_session))
        .route("/Sessions/Playing", post(playback_started))
        .route("/Sessions/Playing/Ping", post(playback_ping))
        .route("/Sessions/Playing/Progress", post(playback_progress))
        .route("/Sessions/Playing/Stopped", post(playback_stopped))
        .route(
            "/PlayingItems/{item_id}",
            post(legacy_started).delete(legacy_stopped),
        )
        .route("/PlayingItems/{item_id}/Delete", post(legacy_stopped))
        .route("/PlayingItems/{item_id}/Progress", post(legacy_progress))
        .route(
            "/Users/{user_id}/PlayingItems/{item_id}",
            post(legacy_user_started).delete(legacy_user_stopped),
        )
        .route(
            "/Users/{user_id}/PlayingItems/{item_id}/Delete",
            post(legacy_user_stopped),
        )
        .route(
            "/Users/{user_id}/PlayingItems/{item_id}/Progress",
            post(legacy_user_progress),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PagingQuery {
    #[serde(default, alias = "startIndex")]
    start_index: Option<i64>,
    #[serde(default, alias = "limit")]
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CreateAuthKeyQuery {
    #[serde(default, alias = "app")]
    app: Option<String>,
    #[serde(default, alias = "appVersion")]
    app_version: Option<String>,
    #[serde(default, alias = "expiresInDays")]
    expires_in_days: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlayQueueQuery {
    #[serde(default, alias = "id")]
    id: Option<String>,
    #[serde(default, alias = "deviceId")]
    device_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SessionCommandQuery {
    #[serde(default, alias = "startIndex")]
    start_index: Option<i64>,
    #[serde(default, alias = "limit")]
    limit: Option<i64>,
    #[serde(default, alias = "consume", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    consume: Option<bool>,
}

async fn list_sessions(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionInfoDto>>, AppError> {
    auth::require_interactive_session(&session)?;
    let sessions = if session.is_admin {
        repository::list_sessions(&state.pool).await?
    } else {
        repository::list_sessions_for_user(&state.pool, session.user_id).await?
    };
    let mut items = Vec::with_capacity(sessions.len());
    for session in sessions {
        let mut dto = repository::session_to_dto(&session, state.config.server_id);
        if let Some(runtime) = repository::session_runtime_state(
            &state.pool,
            &session.access_token,
            session.user_id,
            state.config.server_id,
        )
        .await?
        {
            dto.now_playing_item = Some(runtime.now_playing_item);
            dto.play_state = Some(runtime.play_state);
            dto.now_playing_queue = runtime.now_playing_queue;
        }
        if let Some(summary) =
            repository::get_session_state_summary(&state.pool, &session.access_token).await?
        {
            merge_session_play_state(&mut dto, summary);
        }
        if let Some(capabilities) =
            repository::get_session_capabilities(&state.pool, &session.access_token).await?
        {
            apply_session_capabilities(&mut dto, &capabilities);
        }
        if let Some(viewing_item) =
            session_viewing_item(&state, &session.access_token, session.user_id).await?
        {
            dto.now_viewing_item = Some(viewing_item);
        }
        items.push(dto);
    }
    Ok(Json(items))
}

async fn list_auth_keys(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<PagingQuery>,
) -> Result<Json<QueryResult<Value>>, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }

    let all_sessions = repository::list_api_key_sessions(&state.pool).await?;
    let start_index = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(100).clamp(1, 500) as usize;
    let total_record_count = all_sessions.len() as i64;
    let items = all_sessions
        .into_iter()
        .skip(start_index)
        .take(limit)
        .map(|session| {
            json!({
                "Id": session.access_token,
                "AccessToken": session.access_token,
                "UserId": session.user_id.to_string(),
                "UserName": session.user_name,
                "AppName": session.client.unwrap_or_else(|| "Movie Rust Client".to_string()),
                "AppVersion": session.application_version.unwrap_or_else(|| "0.1.0".to_string()),
                "DeviceId": session.device_id,
                "DeviceName": session.device_name,
                "DateLastActivity": session.last_activity_at,
                "ExpirationDate": session.expires_at,
                "IsActive": session.expires_at.is_none_or(|expires_at| expires_at > Utc::now()),
                "RemoteEndPoint": session.remote_address.clone().unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index as i64),
    }))
}

async fn create_auth_key(
    session: AuthSession,
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<CreateAuthKeyQuery>,
) -> Result<Json<Value>, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let app = query.app.unwrap_or_else(|| "Api Key".to_string());
    let app_version = query.app_version.unwrap_or_else(|| "0.1.0".to_string());
    let expires_at = query
        .expires_in_days
        .filter(|days| *days > 0)
        .map(|days| Utc::now() + Duration::days(days));
    let created = repository::create_session_with_type(
        &state.pool,
        session.user_id,
        None,
        Some(app.clone()),
        Some(app),
        Some(app_version),
        auth::infer_client_ip(&headers),
        expires_at,
        "ApiKey",
    )
    .await?;
    Ok(Json(json!({
        "Id": created.access_token,
        "AccessToken": created.access_token,
        "UserId": created.user_id.to_string(),
        "UserName": created.user_name,
        "AppName": created.client,
        "AppVersion": created.application_version,
        "DeviceId": created.device_id,
        "DeviceName": created.device_name,
        "DateLastActivity": created.last_activity_at,
        "ExpirationDate": created.expires_at,
        "IsActive": created.expires_at.is_none_or(|value| value > Utc::now()),
        "RemoteEndPoint": created.remote_address.clone().unwrap_or_default(),
    })))
}

async fn delete_auth_key(
    session: AuthSession,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let existing = repository::get_api_key_session(&state.pool, &key).await?;
    if existing.is_none() {
        return Err(AppError::NotFound("API Key 不存在".to_string()));
    }
    repository::delete_session(&state.pool, &key).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn auth_providers(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }

    let rows: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT DISTINCT unnest(ARRAY[\
            COALESCE(NULLIF(TRIM(policy->>'AuthenticationProviderId'), ''), 'Default'),\
            COALESCE(NULLIF(TRIM(policy->>'PasswordResetProviderId'), ''), 'Default')\
         ]) FROM users WHERE policy IS NOT NULL AND policy != 'null'::jsonb",
    )
    .fetch_all(&state.pool)
    .await?;
    let mut provider_ids = std::collections::BTreeSet::new();
    for (val,) in rows {
        if let Some(v) = val {
            if !v.trim().is_empty() {
                provider_ids.insert(v);
            }
        }
    }
    if provider_ids.is_empty() {
        provider_ids.insert("Default".to_string());
    }

    let items = provider_ids
        .into_iter()
        .map(|provider_id| {
            json!({
                "Name": provider_id,
                "Id": provider_id,
                "Type": "Password",
                "IsEnabled": true
            })
        })
        .collect();
    Ok(Json(items))
}

async fn play_queue(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<PlayQueueQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    auth::require_interactive_session(&session)?;
    let result = repository::session_play_queue(
        &state.pool,
        query.id.as_deref(),
        query.device_id.as_deref(),
        session.user_id,
        state.config.server_id,
    )
    .await?;
    Ok(Json(result))
}

async fn session_play_queue(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<PlayQueueQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    auth::require_interactive_session(&session)?;
    ensure_session_control_access(&state, &session, &id).await?;
    // PB17：path 里的 session_id 指向「被遥控/被查询的会话」，其 NowPlayingQueue 是
    // 那个会话所属用户的播放队列；如果继续用调用者 `session.user_id` 当过滤条件，admin
    // 在 Web 控制台查看其他设备的 PlayQueue 时会恒空（query 的 `s.user_id = $1` 永远不
    // 命中）。这里改为先按 session_id 反查目标会话，拿到真实归属用户后再交给 repository。
    let target = repository::find_active_session(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound("会话不存在".to_string()))?;
    let result = repository::session_play_queue(
        &state.pool,
        Some(&id),
        query.device_id.as_deref(),
        target.user_id,
        state.config.server_id,
    )
    .await?;
    Ok(Json(result))
}

async fn session_commands(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<SessionCommandQuery>,
) -> Result<Json<QueryResult<Value>>, AppError> {
    auth::require_interactive_session(&session)?;
    ensure_session_control_access(&state, &session, &id).await?;
    let result = repository::list_session_commands(
        &state.pool,
        &id,
        session.user_id,
        session.is_admin,
        query.start_index.unwrap_or(0),
        query.limit.unwrap_or(100),
        query.consume.unwrap_or(false),
    )
    .await?;
    Ok(Json(result))
}

fn bytes_to_json(bytes: &Bytes) -> Value {
    if bytes.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(bytes).unwrap_or_else(|_| json!({}))
    }
}

async fn update_capabilities(
    session: AuthSession,
    State(state): State<AppState>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(&session)?;
    let payload = bytes_to_json(&body);
    repository::set_session_capabilities(&state.pool, &session.access_token, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn logout_session(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(&session)?;
    repository::delete_session(&state.pool, &session.access_token).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(&session)?;
    ensure_session_control_access(&state, &session, &id).await?;
    let payload = bytes_to_json(&body);
    let command_name = payload
        .get("Name")
        .or_else(|| payload.get("Command"))
        .or_else(|| payload.get("name"))
        .or_else(|| payload.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("Command")
        .to_string();
    repository::record_session_command(&state.pool, &id, &command_name, payload.clone()).await?;
    repository::apply_session_command_state(&state.pool, &id, &command_name, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_session_viewing(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(&session)?;
    ensure_session_control_access(&state, &session, &id).await?;
    let payload = bytes_to_json(&body);
    repository::set_session_viewing(&state.pool, &id, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn message_for_session(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(&session)?;
    ensure_session_control_access(&state, &session, &id).await?;
    let payload = bytes_to_json(&body);
    repository::record_session_command(&state.pool, &id, "DisplayMessage", payload.clone()).await?;
    repository::apply_session_command_state(&state.pool, &id, "DisplayMessage", &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session_command(
    session: AuthSession,
    State(state): State<AppState>,
    Path((id, command)): Path<(String, String)>,
    body: Bytes,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(&session)?;
    ensure_session_control_access(&state, &session, &id).await?;
    let payload = bytes_to_json(&body);
    repository::record_session_command(&state.pool, &id, &command, payload.clone()).await?;
    repository::apply_session_command_state(&state.pool, &id, &command, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session_user(
    session: AuthSession,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    ensure_session_control_access(&state, &session, &id).await?;
    let payload = json!({ "UserId": user_id });
    repository::record_session_command(&state.pool, &id, "SetAdditionalUser", payload.clone())
        .await?;
    repository::apply_session_command_state(&state.pool, &id, "SetAdditionalUser", &payload)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn playback_started(
    session: AuthSession,
    State(state): State<AppState>,
    Json(report): Json<PlaybackReport>,
) -> Result<StatusCode, AppError> {
    record_report(&state, &session, "Started", report).await
}

async fn playback_ping(
    session: AuthSession,
    State(state): State<AppState>,
    Json(report): Json<PlaybackReport>,
) -> Result<StatusCode, AppError> {
    record_report(&state, &session, "Ping", report).await
}

async fn playback_progress(
    session: AuthSession,
    State(state): State<AppState>,
    Json(report): Json<PlaybackReport>,
) -> Result<StatusCode, AppError> {
    record_report(&state, &session, "Progress", report).await
}

async fn playback_stopped(
    session: AuthSession,
    State(state): State<AppState>,
    Json(report): Json<PlaybackReport>,
) -> Result<StatusCode, AppError> {
    record_report(&state, &session, "Stopped", report).await
}

async fn legacy_started(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    record_legacy(&state, &session, item_id, "Started", query).await
}

async fn legacy_progress(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    record_legacy(&state, &session, item_id, "Progress", query).await
}

async fn legacy_stopped(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    record_legacy(&state, &session, item_id, "Stopped", query).await
}

async fn legacy_user_started(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    record_legacy_for_user(&state, &session, user_id, item_id, "Started", query).await
}

async fn legacy_user_progress(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    record_legacy_for_user(&state, &session, user_id, item_id, "Progress", query).await
}

async fn legacy_user_stopped(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    record_legacy_for_user(&state, &session, user_id, item_id, "Stopped", query).await
}

async fn record_report(
    state: &AppState,
    session: &AuthSession,
    event_type: &str,
    report: PlaybackReport,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(session)?;
    let user_id = report.user_id.unwrap_or(session.user_id);
    ensure_user_access(session, user_id)?;
    if let Some(item_id) = report.item_id {
        if repository::get_media_item(&state.pool, item_id).await?.is_none() {
            return Err(AppError::NotFound("媒体条目不存在".to_string()));
        }
    }
    let session_id_for_event = repository::resolve_session_id_for_play_queue(
        &state.pool,
        session.access_token.as_str(),
        report.session_id.as_deref(),
    )
    .await?;
    let extras = repository::PlaybackEventExtras {
        audio_stream_index: report.audio_stream_index,
        subtitle_stream_index: report.subtitle_stream_index,
        play_method: report.play_method.clone(),
        media_source_id: report.media_source_id.clone(),
        volume_level: report.volume_level,
        repeat_mode: report.repeat_mode.clone(),
        playback_rate: report.playback_rate,
        // PB29：把客户端 PlaybackReport.PlaySessionId 带进 INSERT，进 playback_events.play_session_id
        play_session_id: report
            .play_session_id
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };
    repository::record_playback_event(
        &state.pool,
        user_id,
        report.item_id,
        Some(session_id_for_event.as_str()),
        event_type,
        report.position_ticks,
        report.is_paused,
        report.played_to_completion,
        &extras,
    )
    .await?;

    // UserDataChanged + SessionsChanged WebSocket push
    if let Some(item_id) = report.item_id {
        if matches!(event_type, "Started" | "Progress" | "Stopped") {
            if let Ok(Some(ud)) = repository::get_user_item_data(&state.pool, user_id, item_id).await {
                let user_data_entry = serde_json::json!({
                    "ItemId": crate::models::uuid_to_emby_guid(&item_id),
                    "PlaybackPositionTicks": ud.playback_position_ticks,
                    "PlayCount": ud.play_count,
                    "IsFavorite": ud.is_favorite,
                    "Played": ud.is_played,
                    "LastPlayedDate": ud.last_played_date,
                });
                let _ = state.event_tx.send(crate::state::ServerEvent::UserDataChanged {
                    user_id,
                    user_data_list: vec![user_data_entry],
                });
            }
        }
    }

    if matches!(event_type, "Started" | "Progress" | "Stopped") {
        let _ = state.event_tx.send(crate::state::ServerEvent::SessionsChanged);
    }

    // 出向 webhook：playback.start / playback.progress / playback.stop
    let event_name = match event_type {
        "Started" => Some(crate::webhooks::events::PLAYBACK_START),
        "Progress" => Some(crate::webhooks::events::PLAYBACK_PROGRESS),
        "Stopped" => Some(crate::webhooks::events::PLAYBACK_STOP),
        _ => None,
    };
    if let Some(event) = event_name {
        let payload = build_playback_payload(
            state,
            session,
            user_id,
            report.item_id,
            Some(session_id_for_event.as_str()),
            report.position_ticks,
            report.is_paused.unwrap_or(false),
            report.played_to_completion.unwrap_or(false),
        )
        .await;
        crate::webhooks::dispatch(state, event, payload);
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn build_playback_payload(
    state: &AppState,
    auth_session: &AuthSession,
    user_id: Uuid,
    item_id: Option<Uuid>,
    session_id_for_hook: Option<&str>,
    position_ticks: Option<i64>,
    is_paused: bool,
    played_to_completion: bool,
) -> serde_json::Value {
    let user = repository::get_user_by_id(&state.pool, user_id).await.ok().flatten();
    let user_obj = serde_json::json!({
        "Id":   crate::models::uuid_to_emby_guid(&user_id),
        "Name": user.as_ref().map(|u| u.name.clone()).unwrap_or_default(),
    });
    let item_obj = if let Some(iid) = item_id {
        let item = repository::get_media_item(&state.pool, iid).await.ok().flatten();
        serde_json::json!({
            "Id":   crate::models::uuid_to_emby_guid(&iid),
            "Name": item.as_ref().map(|m| m.name.clone()).unwrap_or_default(),
            "Type": item.as_ref().map(|m| m.item_type.clone()).unwrap_or_default(),
            "SeriesName": item.as_ref().and_then(|m| m.series_name.clone()).unwrap_or_default(),
        })
    } else {
        serde_json::Value::Null
    };

    // Sakura_embyboss `client_filter.py`：无 `Session.Client` 会直接 ignored（无法做 UA 过滤/踢线）。
    // 从当前登录会话行补齐 Client / Device*，与登录 webhook 口径一致。
    let session_row = repository::find_active_session(&state.pool, &auth_session.access_token)
        .await
        .ok()
        .flatten();
    let client = session_row
        .as_ref()
        .and_then(|r| r.client.as_deref())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown")
        .to_string();
    let device_name = session_row
        .as_ref()
        .and_then(|r| r.device_name.as_deref())
        .unwrap_or("")
        .to_string();
    let device_id = session_row
        .as_ref()
        .and_then(|r| r.device_id.as_deref())
        .unwrap_or("")
        .to_string();
    let sid = session_id_for_hook.unwrap_or(auth_session.access_token.as_str());
    let remote_address = session_row
        .as_ref()
        .and_then(|r| r.remote_address.clone())
        .unwrap_or_default();
    let session_obj = serde_json::json!({
        "Id": sid,
        "Client": client,
        "DeviceName": device_name,
        "DeviceId": device_id,
        "RemoteAddress": remote_address,
        "NowPlayingItem": item_obj.clone(),
    });

    serde_json::json!({
        "User":    user_obj,
        "Item":    item_obj,
        "Session": session_obj,
        "PlaybackPositionTicks": position_ticks.unwrap_or(0),
        "PlaybackInfo": {
            "PositionTicks":      position_ticks,
            "IsPaused":           is_paused,
            "PlayedToCompletion": played_to_completion,
        }
    })
}

fn ensure_user_access(session: &AuthSession, user_id: Uuid) -> Result<(), AppError> {
    if session.is_admin || session.user_id == user_id {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn ensure_session_control_access(
    state: &AppState,
    session: &AuthSession,
    target_session_id: &str,
) -> Result<(), AppError> {
    let target = repository::find_active_session(&state.pool, target_session_id)
        .await?
        .ok_or_else(|| AppError::NotFound("会话不存在".to_string()))?;

    if session.is_admin || target.user_id == session.user_id {
        return Ok(());
    }

    let user = repository::get_user_by_id(&state.pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let policy = repository::user_policy_from_value(&user.policy);
    if policy.enable_remote_control_of_other_users {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn record_legacy(
    state: &AppState,
    session: &AuthSession,
    item_id: Uuid,
    event_type: &str,
    query: LegacyPlaybackQuery,
) -> Result<StatusCode, AppError> {
    auth::require_interactive_session(session)?;
    record_legacy_for_user(state, session, session.user_id, item_id, event_type, query).await
}

async fn record_legacy_for_user(
    state: &AppState,
    session: &AuthSession,
    user_id: Uuid,
    item_id: Uuid,
    event_type: &str,
    query: LegacyPlaybackQuery,
) -> Result<StatusCode, AppError> {
    ensure_user_access(session, user_id)?;
    if repository::get_media_item(&state.pool, item_id).await?.is_none() {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    }
    // Legacy 接口不传 played_to_completion，交给 record_playback_event 的 90% 自动判定
    let played_to_completion = false;

    let extras = repository::PlaybackEventExtras {
        audio_stream_index: None,
        subtitle_stream_index: None,
        play_method: None,
        media_source_id: query.media_source_id.clone(),
        volume_level: None,
        repeat_mode: None,
        playback_rate: None,
        // PB29：legacy /PlayingItems 客户端通过 query 字段带 PlaySessionId，写进独立列
        play_session_id: query
            .play_session_id
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };
    // PlaySessionId 已写入 `playback_events.play_session_id`（extras）；`session_id` 列与
    // `session_play_queue` **必须**使用登录会话的 access_token，否则会触发 PB25 回归：
    // EXISTS(sessions.access_token) 对 PlaySessionId 恒为假 → 无播放队列 → Sakura/控制台
    // 「在线播放」统计恒 0。
    let session_id_for_event = session.access_token.clone();
    repository::record_playback_event(
        &state.pool,
        user_id,
        Some(item_id),
        Some(session_id_for_event.as_str()),
        event_type,
        query.position_ticks,
        query.is_paused,
        Some(played_to_completion),
        &extras,
    )
    .await?;

    // PB25：legacy /PlayingItems 路径之前完全不派发 SessionsChanged / UserDataChanged /
    // 出向 webhook —— 老 Emby 客户端走 legacy 上报时，UI 无法实时刷新「现在播放」、
    // Sakura 等下游也收不到 playback.* 事件。这里与 record_report 同口径补齐三类派发。
    if matches!(event_type, "Started" | "Progress" | "Stopped") {
        if let Ok(Some(ud)) = repository::get_user_item_data(&state.pool, user_id, item_id).await {
            let user_data_entry = serde_json::json!({
                "ItemId": crate::models::uuid_to_emby_guid(&item_id),
                "PlaybackPositionTicks": ud.playback_position_ticks,
                "PlayCount": ud.play_count,
                "IsFavorite": ud.is_favorite,
                "Played": ud.is_played,
                "LastPlayedDate": ud.last_played_date,
            });
            let _ = state.event_tx.send(crate::state::ServerEvent::UserDataChanged {
                user_id,
                user_data_list: vec![user_data_entry],
            });
        }
        let _ = state.event_tx.send(crate::state::ServerEvent::SessionsChanged);

        let event_name = match event_type {
            "Started" => Some(crate::webhooks::events::PLAYBACK_START),
            "Progress" => Some(crate::webhooks::events::PLAYBACK_PROGRESS),
            "Stopped" => Some(crate::webhooks::events::PLAYBACK_STOP),
            _ => None,
        };
        if let Some(event) = event_name {
            let payload = build_playback_payload(
                state,
                session,
                user_id,
                Some(item_id),
                Some(session_id_for_event.as_str()),
                query.position_ticks,
                query.is_paused.unwrap_or(false),
                played_to_completion,
            )
            .await;
            crate::webhooks::dispatch(state, event, payload);
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

fn apply_session_capabilities(dto: &mut SessionInfoDto, capabilities: &Value) {
    if let Some(supports_remote_control) = capabilities
        .get("SupportsRemoteControl")
        .and_then(Value::as_bool)
    {
        dto.supports_remote_control = supports_remote_control;
    }

    if let Some(remote_end_point) = capabilities
        .get("RemoteEndPoint")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        dto.remote_end_point = Some(remote_end_point);
    }

    if let Some(playable_media_types) = value_string_vec(
        capabilities
            .get("PlayableMediaTypes")
            .or_else(|| capabilities.get("SupportedMediaTypes")),
    ) {
        dto.playable_media_types = playable_media_types;
    }

    if let Some(supported_commands) = value_string_vec(
        capabilities
            .get("SupportedCommands")
            .or_else(|| capabilities.get("SupportedRemoteCommands")),
    ) {
        dto.supported_commands = supported_commands;
    }
}

fn value_string_vec(value: Option<&Value>) -> Option<Vec<String>> {
    let values = value?
        .as_array()?
        .iter()
        .filter_map(|entry| entry.as_str().map(str::trim))
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    Some(values)
}

fn merge_session_play_state(dto: &mut SessionInfoDto, summary: Value) {
    match dto.play_state.as_mut() {
        Some(Value::Object(play_state)) => {
            if let Some(summary_object) = summary.as_object() {
                for (key, value) in summary_object {
                    play_state.insert(key.clone(), value.clone());
                }
            }
        }
        _ => {
            dto.play_state = Some(summary);
        }
    }
}

async fn session_viewing_item(
    state: &AppState,
    session_id: &str,
    user_id: Uuid,
) -> Result<Option<BaseItemDto>, AppError> {
    let Some(viewing) = repository::get_session_viewing(&state.pool, session_id).await? else {
        return Ok(None);
    };

    let item_id = viewing
        .get("ItemId")
        .or_else(|| viewing.get("itemId"))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::trim)
        .map(crate::models::emby_id_to_uuid)
        .transpose()
        .map_err(|_| AppError::BadRequest("会话 Viewing 的 ItemId 非法".to_string()))?;

    let Some(item_id) = item_id else {
        return Ok(None);
    };

    let Some(item) = repository::get_media_item(&state.pool, item_id).await? else {
        return Ok(None);
    };

    Ok(Some(
        repository::media_item_to_dto(&state.pool, &item, Some(user_id), state.config.server_id)
            .await?,
    ))
}
