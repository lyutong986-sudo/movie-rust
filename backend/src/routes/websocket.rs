use axum::{
    extract::{ws::WebSocket, Query, State, WebSocketUpgrade},
    response::Response,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{auth, error::AppError, state::AppState};

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
}

pub type Sessions = Arc<RwLock<HashMap<Uuid, WebSocketSession>>>;

pub async fn emby_websocket_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WebSocketQuery>,
    State(state): State<AppState>,
) -> Result<Response, AppError> {
    let user_id = None;
    
    let session = WebSocketSession {
        id: Uuid::new_v4(),
        user_id,
        device_id: query.device_id,
    };

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, session, state)))
}

async fn handle_socket(mut socket: WebSocket, session: WebSocketSession, state: AppState) {
    let session_id = session.id;
    
    let sessions = state.websocket_sessions.clone();
    
    sessions.write().await.insert(session_id, session.clone());
    
    tracing::info!(session_id = %session_id, user_id = ?session.user_id, "WebSocket 连接已建立");
    
    let mut close_reason = None;
    
    while let Some(msg) = socket.recv().await {
        match msg {
            Ok(msg) => {
                match msg {
                    axum::extract::ws::Message::Text(text) => {
                        tracing::debug!(session_id = %session_id, message = %text, "收到 WebSocket 消息");
                        // 简单回显
                        let _ = socket.send(axum::extract::ws::Message::Text(text)).await;
                    }
                    axum::extract::ws::Message::Close(frame) => {
                        close_reason = frame.map(|f| f.reason.to_string());
                        break;
                    }
                    _ => {}
                }
            }
            Err(e) => {
                tracing::error!(session_id = %session_id, error = %e, "WebSocket 接收错误");
                break;
            }
        }
    }
    
    sessions.write().await.remove(&session_id);
    tracing::info!(session_id = %session_id, reason = ?close_reason, "WebSocket 连接已关闭");
}