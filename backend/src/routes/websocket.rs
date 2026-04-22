use axum::{
    extract::{ws::{Message, WebSocket}, Query, State, WebSocketUpgrade},
    response::Response,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{error::AppError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct WebSocketQuery {
    #[serde(rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    api_key: Option<String>,
    #[serde(rename = "token", alias = "Token")]
    token: Option<String>,
    #[serde(rename = "deviceId", alias = "DeviceId")]
    device_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WebSocketSession {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub device_id: Option<String>,
    pub access_token: Option<String>,
}

pub type Sessions = Arc<RwLock<HashMap<Uuid, WebSocketSession>>>;

pub async fn emby_websocket_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WebSocketQuery>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let token = query
        .token
        .as_deref()
        .or(query.api_key.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let mut access_token = None;
    let user_id = if let Some(token) = token {
        if state
            .config
            .api_key
            .as_ref()
            .is_some_and(|api_key| api_key == token)
        {
            Some(state.config.server_id)
        } else {
            let auth_session = crate::repository::get_session(&state.pool, token).await?;
            if auth_session.is_some() {
                access_token = Some(token.to_string());
            }
            auth_session.map(|session| session.user_id)
        }
    } else {
        None
    };

    let session = WebSocketSession {
        id: Uuid::new_v4(),
        user_id,
        device_id: query.device_id,
        access_token,
    };

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, session, state)))
}

async fn handle_socket(mut socket: WebSocket, session: WebSocketSession, state: AppState) {
    let session_id = session.id;
    let sessions = state.websocket_sessions.clone();
    let mut event_receiver = state.websocket_events.subscribe();
    let mut command_interval = tokio::time::interval(Duration::from_secs(1));
    sessions.write().await.insert(session_id, session.clone());

    tracing::info!(
        session_id = %session_id,
        user_id = ?session.user_id,
        device_id = ?session.device_id,
        "WebSocket connection opened"
    );

    let mut close_reason = None;

    loop {
        tokio::select! {
            received = socket.recv() => match received {
                Some(Ok(message)) => match message {
                Message::Text(text) => {
                    tracing::debug!(session_id = %session_id, message = %text, "received WebSocket message");
                    match handle_client_message(&mut socket, &state, &session, &text).await {
                        Ok(true) => {}
                        Ok(false) => {
                            let payload = json!({
                                "MessageType": "KeepAlive",
                                "Data": text.to_string()
                            });
                            if socket.send(Message::Text(payload.to_string().into())).await.is_err() {
                                close_reason = Some("send failed".to_string());
                                break;
                            }
                        }
                        Err(error) => {
                            tracing::warn!(session_id = %session_id, error = %error, "failed to handle WebSocket message");
                        }
                    }
                }
                Message::Ping(bytes) => {
                    if socket.send(Message::Pong(bytes)).await.is_err() {
                        close_reason = Some("pong failed".to_string());
                        break;
                    }
                }
                Message::Close(frame) => {
                    close_reason = frame.map(|frame| frame.reason.to_string());
                    break;
                }
                _ => {}
                },
                Some(Err(error)) => {
                    let error = error.to_string();
                    let error_lower = error.to_lowercase();
                    if error_lower.contains("connection reset without closing handshake")
                        || error_lower.contains("reset without closing handshake")
                    {
                        tracing::debug!(session_id = %session_id, error = %error, "WebSocket closed without close frame");
                    } else {
                        tracing::error!(session_id = %session_id, error = %error, "WebSocket receive failed");
                    }
                    close_reason = Some(error);
                    break;
                }
                None => break,
            },
            broadcast = event_receiver.recv() => match broadcast {
                Ok(payload) => {
                    if socket.send(Message::Text(payload.into())).await.is_err() {
                        close_reason = Some("broadcast push failed".to_string());
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    close_reason = Some("broadcast channel closed".to_string());
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                    tracing::warn!(session_id = %session_id, skipped, "websocket event receiver lagged");
                }
            },
            _ = command_interval.tick() => {
                if let (Some(access_token), Some(user_id)) = (&session.access_token, session.user_id) {
                    match crate::repository::list_session_commands(
                        &state.pool,
                        access_token,
                        user_id,
                        false,
                        0,
                        50,
                        true,
                    )
                    .await
                    {
                        Ok(result) => {
                            for command in result.items {
                                let payload = serde_json::json!({
                                    "MessageType": "Command",
                                    "Data": command
                                });
                                if socket.send(Message::Text(payload.to_string().into())).await.is_err() {
                                    close_reason = Some("command push failed".to_string());
                                    break;
                                }
                            }
                            if close_reason.is_some() {
                                break;
                            }
                        }
                        Err(error) => {
                            tracing::warn!(session_id = %session_id, error = %error, "failed to push session commands");
                        }
                    }
                }
            }
        }
    }

    sessions.write().await.remove(&session_id);
    tracing::info!(session_id = %session_id, reason = ?close_reason, "WebSocket connection closed");
}

async fn handle_client_message(
    socket: &mut WebSocket,
    state: &AppState,
    _session: &WebSocketSession,
    text: &str,
) -> Result<bool, AppError> {
    let payload = match serde_json::from_str::<Value>(text) {
        Ok(payload) => payload,
        Err(_) => return Ok(false),
    };

    let message_type = payload
        .get("MessageType")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let Some(message_type) = message_type else {
        return Ok(false);
    };

    match message_type {
        "KeepAlive" => {
            send_socket_message(socket, "KeepAlive", json!({})).await?;
            Ok(true)
        }
        "SessionsStart" => {
            let sessions = crate::repository::list_sessions(&state.pool).await?;
            let mut items = Vec::with_capacity(sessions.len());
            for listed in sessions {
                let mut dto = crate::repository::session_to_dto(&listed);
                if let Some(runtime) = crate::repository::session_runtime_state(
                    &state.pool,
                    &listed.access_token,
                    listed.user_id,
                    state.config.server_id,
                )
                .await?
                {
                    dto.now_playing_item = Some(runtime.now_playing_item);
                    dto.play_state = Some(runtime.play_state);
                }
                if let Some(summary) =
                    crate::repository::get_session_state_summary(&state.pool, &listed.access_token).await?
                {
                    merge_play_state_summary(&mut dto, summary);
                }
                if let Some(capabilities) =
                    crate::repository::get_session_capabilities(&state.pool, &listed.access_token).await?
                {
                    apply_capabilities(&mut dto, &capabilities);
                }
                items.push(dto);
            }
            send_socket_message(socket, "Sessions", json!(items)).await?;
            Ok(true)
        }
        "ActivityLogEntryStart" => {
            let items = crate::repository::list_activity_logs(&state.pool, 50).await?;
            send_socket_message(socket, "ActivityLogEntry", json!(items)).await?;
            Ok(true)
        }
        "ScheduledTasksInfoStart" => {
            let items = crate::routes::system::build_scheduled_tasks(&state).await?;
            send_socket_message(socket, "ScheduledTasksInfo", json!(items)).await?;
            Ok(true)
        }
        "ForceKeepAlive" => {
            send_socket_message(socket, "KeepAlive", json!({})).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn send_socket_message(
    socket: &mut WebSocket,
    message_type: &str,
    data: Value,
) -> Result<(), AppError> {
    let payload = json!({
        "MessageType": message_type,
        "Data": data
    });
    socket
        .send(Message::Text(payload.to_string().into()))
        .await
        .map_err(|error| AppError::Internal(format!("发送 WebSocket 消息失败: {error}")))?;
    Ok(())
}

pub fn broadcast_message(state: &AppState, message_type: &str, data: Value) {
    let payload = json!({
        "MessageType": message_type,
        "Data": data
    })
    .to_string();
    let _ = state.websocket_events.send(payload);
}

fn merge_play_state_summary(dto: &mut crate::models::SessionInfoDto, summary: Value) {
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

fn apply_capabilities(dto: &mut crate::models::SessionInfoDto, capabilities: &Value) {
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
