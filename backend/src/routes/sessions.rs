use crate::{
    auth::AuthSession,
    error::AppError,
    models::{LegacyPlaybackQuery, PlaybackReport, SessionInfoDto},
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Sessions", get(list_sessions))
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

async fn list_sessions(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<SessionInfoDto>>, AppError> {
    let sessions = repository::list_sessions(&state.pool).await?;
    Ok(Json(
        sessions.iter().map(repository::session_to_dto).collect(),
    ))
}

async fn no_content(_session: AuthSession) -> StatusCode {
    StatusCode::NO_CONTENT
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
            .play_session_id
            .as_deref()
            .or(report.session_id.as_deref()),
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
            .or(Some(session.access_token.as_str())),
        event_type,
        query.position_ticks,
        query.is_paused,
        Some(played_to_completion),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}
