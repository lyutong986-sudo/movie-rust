use axum::{
    extract::{ws::WebSocket, Query, State, WebSocketUpgrade},
    response::Response,
};
use serde::Deserialize;
use std::time::Duration;
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
        .filter(|value| !value.is_empty())
        .ok_or(AppError::Unauthorized)?;

    let mut access_token = None;
    let user_id = if state
        .config
        .api_key
        .as_ref()
        .is_some_and(|api_key| api_key == token)
    {
        // 服务端配置的 X-Emby-Token API Key：允许 WS 连接，但仅限管理员场景；
        // 这里把 user_id 标成 server_id 以便 list_session_commands 能匹配。
        state.config.server_id
    } else {
        let auth_session = crate::repository::get_session(&state.pool, token)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if auth_session.session_type.eq_ignore_ascii_case("ApiKey") {
            // Emby 官方语义：API Key 不属于"交互式"会话，禁止建立 WebSocket。
            return Err(AppError::Forbidden);
        }
        access_token = Some(token.to_string());
        auth_session.user_id
    };

    let session = WebSocketSession {
        id: Uuid::new_v4(),
        user_id: Some(user_id),
        device_id: query.device_id,
        access_token,
    };

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, session, state)))
}

async fn handle_socket(mut socket: WebSocket, session: WebSocketSession, state: AppState) {
    let session_id = session.id;
    let sessions = state.websocket_sessions.clone();
    sessions.insert(session_id, session.clone());

    tracing::info!(
        session_id = %session_id,
        user_id = ?session.user_id,
        device_id = ?session.device_id,
        "WebSocket connection opened"
    );

    let mut close_reason = None;

    loop {
        match tokio::time::timeout(Duration::from_secs(1), socket.recv()).await {
            Ok(Some(Ok(message))) => match message {
                axum::extract::ws::Message::Text(text) => {
                    tracing::trace!(session_id = %session_id, "received WebSocket message");
                    let payload = format!(r#"{{"MessageType":"KeepAlive","Data":{}}}"#, serde_json::to_string(&*text).unwrap_or_else(|_| "null".to_string()));
                    if socket
                        .send(axum::extract::ws::Message::Text(payload.into()))
                        .await
                        .is_err()
                    {
                        close_reason = Some("send failed".to_string());
                        break;
                    }
                }
                axum::extract::ws::Message::Ping(bytes) => {
                    if socket
                        .send(axum::extract::ws::Message::Pong(bytes))
                        .await
                        .is_err()
                    {
                        close_reason = Some("pong failed".to_string());
                        break;
                    }
                }
                axum::extract::ws::Message::Close(frame) => {
                    close_reason = frame.map(|frame| frame.reason.to_string());
                    break;
                }
                _ => {}
            },
            Ok(Some(Err(error))) => {
                tracing::error!(session_id = %session_id, error = %error, "WebSocket receive failed");
                close_reason = Some(error.to_string());
                break;
            }
            Ok(None) => break,
            Err(_) => {}
        }

        if let (Some(access_token), Some(user_id)) = (&session.access_token, session.user_id) {
            match crate::repository::get_session(&state.pool, access_token).await {
                Ok(Some(_)) => {}
                Ok(None) => {
                    close_reason = Some("session invalidated".to_string());
                    break;
                }
                Err(error) => {
                    tracing::warn!(
                        session_id = %session_id,
                        error = %error,
                        "failed to revalidate websocket session"
                    );
                    close_reason = Some("session revalidation failed".to_string());
                    break;
                }
            }

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
                        if socket
                            .send(axum::extract::ws::Message::Text(payload.to_string().into()))
                            .await
                            .is_err()
                        {
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

    sessions.remove(&session_id);
    tracing::info!(session_id = %session_id, reason = ?close_reason, "WebSocket connection closed");
}
