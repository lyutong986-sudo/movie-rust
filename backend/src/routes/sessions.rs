use crate::{
    auth::AuthSession,
    error::AppError,
    models::{BaseItemDto, LegacyPlaybackQuery, PlaybackReport, QueryResult, SessionInfoDto},
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
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
        .route("/Sessions/{id}/Command", post(no_content_for_session))
        .route("/Sessions/{id}/Command/{command}", post(no_content_for_session_command))
        .route("/Sessions/{id}/Message", post(no_content_for_session))
        .route("/Sessions/{id}/Playing", post(no_content_for_session))
        .route("/Sessions/{id}/Playing/{command}", post(no_content_for_session_command))
        .route("/Sessions/{id}/System/{command}", post(no_content_for_session_command))
        .route("/Sessions/{id}/Users/{user_id}", post(no_content_for_session_user).delete(no_content_for_session_user))
        .route("/Sessions/{id}/Users/{user_id}/Delete", post(no_content_for_session_user))
        .route("/Sessions/{id}/Viewing", post(no_content_for_session))
        .route("/Sessions/Capabilities", post(no_content))
        .route("/Sessions/Capabilities/Full", post(no_content))
        .route("/Sessions/Logout", post(no_content))
        .route("/Sessions/Playing", post(playback_started))
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
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PlayQueueQuery {
    #[serde(default, alias = "id")]
    id: Option<String>,
    #[serde(default, alias = "deviceId")]
    device_id: Option<String>,
}

async fn list_sessions(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionInfoDto>>, AppError> {
    let sessions = repository::list_sessions(&state.pool).await?;
    Ok(Json(
        sessions.iter().map(repository::session_to_dto).collect(),
    ))
}

async fn list_auth_keys(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<PagingQuery>,
) -> Result<Json<QueryResult<Value>>, AppError> {
    if !session.is_admin {
        return Err(AppError::Unauthorized);
    }

    let all_sessions = repository::list_sessions(&state.pool).await?;
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
                "IsActive": true
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
    let created = repository::create_session(
        &state.pool,
        session.user_id,
        None,
        Some(app.clone()),
        Some(app),
        None,
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
        "IsActive": true
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

async fn auth_providers(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![json!({
        "Name": "Default",
        "Id": "Default",
        "Type": "Password",
        "IsEnabled": true
    })])
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

async fn no_content(_session: AuthSession) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn no_content_for_session(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    repository::record_session_command(
        &state.pool,
        &id,
        "Command",
        body.map(|Json(value)| value).unwrap_or_else(|| json!({})),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session_command(
    _session: AuthSession,
    State(state): State<AppState>,
    Path((id, command)): Path<(String, String)>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    repository::record_session_command(
        &state.pool,
        &id,
        &command,
        body.map(|Json(value)| value).unwrap_or_else(|| json!({})),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn no_content_for_session_user(
    _session: AuthSession,
    State(state): State<AppState>,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    repository::record_session_command(
        &state.pool,
        &id,
        "SetAdditionalUser",
        json!({ "UserId": user_id }),
    )
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
    let user_id = report.user_id.unwrap_or(session.user_id);
    repository::record_playback_event(
        &state.pool,
        user_id,
        report.item_id,
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
    Ok(StatusCode::NO_CONTENT)
}
