use crate::{
    auth::AuthSession,
    error::AppError,
    models::{emby_id_to_uuid, uuid_to_emby_guid, BaseItemDto, LegacyPlaybackQuery, PlaybackReport, QueryResult, SessionInfoDto},
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
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
        .route("/Sessions/{id}/Command/{command}", post(no_content_for_session_command))
        .route("/Sessions/{id}/Message", post(message_for_session))
        .route("/Sessions/{id}/Playing", post(no_content_for_session))
        .route("/Sessions/{id}/Playing/{command}", post(no_content_for_session_command))
        .route("/Sessions/{id}/System/{command}", post(no_content_for_session_command))
        .route("/Sessions/{id}/Users/{user_id}", post(no_content_for_session_user).delete(no_content_for_session_user))
        .route("/Sessions/{id}/Users/{user_id}/Delete", post(no_content_for_session_user))
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
    #[serde(default, alias = "consume")]
    consume: Option<bool>,
}

async fn list_sessions(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionInfoDto>>, AppError> {
    let sessions = repository::list_sessions(&state.pool).await?;
    let mut items = Vec::with_capacity(sessions.len());
    for session in sessions {
        let mut dto = repository::session_to_dto(&session);
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
        if let Some(viewing_item) = session_viewing_item(
            &state,
            &session.access_token,
            session.user_id,
        )
        .await?
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
        return Err(AppError::Unauthorized);
    }

    let all_sessions = repository::list_all_sessions(&state.pool).await?;
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
                "UserId": uuid_to_emby_guid(&session.user_id),
                "UserName": session.user_name,
                "AppName": session.client.unwrap_or_else(|| "Movie Rust Client".to_string()),
                "AppVersion": session.application_version.unwrap_or_else(|| "0.1.0".to_string()),
                "DeviceId": session.device_id,
                "DeviceName": session.device_name,
                "DateCreated": session.created_at,
                "DateLastActivity": session.last_activity_at,
                "ExpirationDate": session.expires_at,
                "IsActive": session.expires_at.is_none_or(|expires_at| expires_at > Utc::now())
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
    Query(query): Query<CreateAuthKeyQuery>,
) -> Result<Json<Value>, AppError> {
    if !session.is_admin {
        return Err(AppError::Unauthorized);
    }
    let app = query.app.unwrap_or_else(|| "Api Key".to_string());
    let expires_at = query
        .expires_in_days
        .filter(|days| *days > 0)
        .map(|days| Utc::now() + Duration::days(days));
    let created = repository::create_session(
        &state.pool,
        session.user_id,
        None,
        Some(app.clone()),
        Some(app),
        None,
        expires_at,
    )
    .await?;
    Ok(Json(json!({
        "Id": created.access_token,
        "AccessToken": created.access_token,
        "UserId": uuid_to_emby_guid(&created.user_id),
        "UserName": created.user_name,
        "AppName": created.client,
        "AppVersion": created.application_version,
        "DeviceId": created.device_id,
        "DeviceName": created.device_name,
        "DateCreated": created.created_at,
        "DateLastActivity": created.last_activity_at,
        "ExpirationDate": created.expires_at,
        "IsActive": created.expires_at.is_none_or(|value| value > Utc::now())
    })))
}

async fn delete_auth_key(
    session: AuthSession,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin && session.access_token != key {
        return Err(AppError::Unauthorized);
    }
    repository::delete_session(&state.pool, &key).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn auth_providers(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    let users = repository::list_users(&state.pool, false).await?;
    let mut provider_ids = std::collections::BTreeSet::new();
    for user in users {
        let policy = repository::user_to_dto(&user, state.config.server_id).policy;
        let auth_id = policy.authentication_provider_id.trim();
        let reset_id = policy.password_reset_provider_id.trim();
        provider_ids.insert(if auth_id.is_empty() { "Default".to_string() } else { auth_id.to_string() });
        provider_ids.insert(if reset_id.is_empty() { "Default".to_string() } else { reset_id.to_string() });
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
    let result = repository::session_play_queue(
        &state.pool,
        Some(&id),
        query.device_id.as_deref(),
        session.user_id,
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

async fn update_capabilities(
    session: AuthSession,
    State(state): State<AppState>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    let payload = body.map(|Json(value)| value).unwrap_or_else(|| json!({}));
    repository::set_session_capabilities(&state.pool, &session.access_token, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn logout_session(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<StatusCode, AppError> {
    repository::delete_session(&state.pool, &session.access_token).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    let payload = body.map(|Json(value)| value).unwrap_or_else(|| json!({}));
    let command_name = payload
        .get("Name")
        .or_else(|| payload.get("Command"))
        .or_else(|| payload.get("name"))
        .or_else(|| payload.get("command"))
        .and_then(Value::as_str)
        .unwrap_or("Command")
        .to_string();
    repository::record_session_command(
        &state.pool,
        &id,
        &command_name,
        payload.clone(),
    )
    .await?;
    repository::apply_session_command_state(&state.pool, &id, &command_name, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_session_viewing(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    let payload = body.map(|Json(value)| value).unwrap_or_else(|| json!({}));
    repository::set_session_viewing(&state.pool, &id, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn message_for_session(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    let payload = body.map(|Json(value)| value).unwrap_or_else(|| json!({}));
    repository::record_session_command(&state.pool, &id, "DisplayMessage", payload.clone()).await?;
    repository::apply_session_command_state(&state.pool, &id, "DisplayMessage", &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session_command(
    _session: AuthSession,
    State(state): State<AppState>,
    Path((id, command)): Path<(String, String)>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    let payload = body.map(|Json(value)| value).unwrap_or_else(|| json!({}));
    repository::record_session_command(
        &state.pool,
        &id,
        &command,
        payload.clone(),
    )
    .await?;
    repository::apply_session_command_state(&state.pool, &id, &command, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session_user(
    _session: AuthSession,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let payload = json!({ "UserId": user_id });
    repository::record_session_command(
        &state.pool,
        &id,
        "SetAdditionalUser",
        payload.clone(),
    )
    .await?;
    repository::apply_session_command_state(&state.pool, &id, "SetAdditionalUser", &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn playback_started(
    session: AuthSession,
    State(state): State<AppState>,
    Json(report): Json<PlaybackReport>,
) -> Result<StatusCode, AppError> {
    record_report(&state, &session, "Started", report).await
}

async fn playback_ping(_session: AuthSession) -> StatusCode {
    StatusCode::NO_CONTENT
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
    Path(item_id): Path<String>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    let item_id = parse_emby_uuid_path(&item_id, "item id")?;
    record_legacy(&state, &session, item_id, "Started", query).await
}

async fn legacy_progress(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    let item_id = parse_emby_uuid_path(&item_id, "item id")?;
    record_legacy(&state, &session, item_id, "Progress", query).await
}

async fn legacy_stopped(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<String>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    let item_id = parse_emby_uuid_path(&item_id, "item id")?;
    record_legacy(&state, &session, item_id, "Stopped", query).await
}

async fn legacy_user_started(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    let user_id = parse_emby_uuid_path(&user_id, "用户ID")?;
    let item_id = parse_emby_uuid_path(&item_id, "项目ID")?;
    record_legacy_for_user(&state, &session, user_id, item_id, "Started", query).await
}

async fn legacy_user_progress(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    let user_id = parse_emby_uuid_path(&user_id, "用户ID")?;
    let item_id = parse_emby_uuid_path(&item_id, "项目ID")?;
    record_legacy_for_user(&state, &session, user_id, item_id, "Progress", query).await
}

async fn legacy_user_stopped(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(String, String)>,
    Query(query): Query<LegacyPlaybackQuery>,
) -> Result<StatusCode, AppError> {
    let user_id = parse_emby_uuid_path(&user_id, "用户ID")?;
    let item_id = parse_emby_uuid_path(&item_id, "项目ID")?;
    record_legacy_for_user(&state, &session, user_id, item_id, "Stopped", query).await
}

async fn record_report(
    state: &AppState,
    session: &AuthSession,
    event_type: &str,
    report: PlaybackReport,
) -> Result<StatusCode, AppError> {
    let user_id = report.user_id.unwrap_or(session.user_id);
    let item_id = report.item_id;
    let session_id = report
        .session_id
        .clone()
        .unwrap_or_else(|| session.access_token.clone());
    repository::record_playback_event(
        &state.pool,
        user_id,
        item_id,
        report
            .session_id
            .as_deref()
            .or(Some(session.access_token.as_str())),
        event_type,
        report.position_ticks,
        report.is_paused,
        report.played_to_completion,
    )
    .await?;
    broadcast_playback_updates(state, user_id, item_id, &session_id, event_type).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn record_legacy(
    state: &AppState,
    session: &AuthSession,
    item_id: Uuid,
    event_type: &str,
    query: LegacyPlaybackQuery,
) -> Result<StatusCode, AppError> {
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
    let played_to_completion = if event_type == "Stopped" {
        query.position_ticks.is_none_or(|ticks| ticks > 0)
    } else {
        false
    };

    repository::record_playback_event(
        &state.pool,
        user_id,
        Some(item_id),
        query
            .play_session_id
            .as_deref()
            .filter(|value| value == &session.access_token)
            .or(Some(session.access_token.as_str())),
        event_type,
        query.position_ticks,
        query.is_paused,
        Some(played_to_completion),
    )
    .await?;
    broadcast_playback_updates(state, user_id, Some(item_id), &session.access_token, event_type).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn broadcast_playback_updates(
    state: &AppState,
    user_id: Uuid,
    item_id: Option<Uuid>,
    session_id: &str,
    event_type: &str,
) -> Result<(), AppError> {
    if let Some(item_id) = item_id {
        let user_data =
            crate::routes::items::collect_related_user_data(state, user_id, item_id).await?;
        crate::routes::items::broadcast_user_data_changed(state, user_data.iter().collect());
    }

    let sessions = repository::list_sessions(&state.pool).await?;
    let mut items = Vec::with_capacity(sessions.len());
    for session in sessions {
        let mut dto = repository::session_to_dto(&session);
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
            session_viewing_item(state, &session.access_token, session.user_id).await?
        {
            dto.now_viewing_item = Some(viewing_item);
        }
        items.push(dto);
    }

    crate::routes::websocket::broadcast_message(state, "Sessions", json!(items));
    crate::routes::websocket::broadcast_message(
        state,
        "PlaybackProgress",
        json!({
            "SessionId": session_id,
            "UserId": uuid_to_emby_guid(&user_id),
            "ItemId": item_id.map(|item_id| uuid_to_emby_guid(&item_id)),
            "EventName": event_type
        }),
    );
    Ok(())
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

fn parse_emby_uuid_path(value: &str, label: &str) -> Result<Uuid, AppError> {
    emby_id_to_uuid(value)
        .map_err(|_| AppError::BadRequest(format!("invalid {label} format: {value}")))
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
