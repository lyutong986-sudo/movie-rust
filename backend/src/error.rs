use axum::{http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("未授权")]
    Unauthorized,
    #[error("没有权限")]
    Forbidden,
    #[error("资源不存在: {0}")]
    NotFound(String),
    #[error("请求参数错误: {0}")]
    BadRequest(String),
    #[error("数据库错误: {0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("内部错误: {0}")]
    Internal(String),
    #[error("FFmpeg错误: {0}")]
    FfmpegError(String),
    #[error("转码功能已禁用")]
    TranscodingDisabled,
    #[error("不支持的转码协议: {0}")]
    InvalidTranscodingProtocol(String),
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ErrorBody {
    error_code: String,
    error_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stack_trace: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_code) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "Forbidden"),
            AppError::NotFound(_) => (StatusCode::NOT_FOUND, "NotFound"),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, "BadRequest"),
            AppError::Sqlx(_) => (StatusCode::INTERNAL_SERVER_ERROR, "DatabaseError"),
            AppError::Io(_) => (StatusCode::INTERNAL_SERVER_ERROR, "IOError"),
            AppError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "InternalServerError"),
            AppError::FfmpegError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "FfmpegError"),
            AppError::TranscodingDisabled => (StatusCode::BAD_REQUEST, "TranscodingDisabled"),
            AppError::InvalidTranscodingProtocol(_) => {
                (StatusCode::BAD_REQUEST, "InvalidTranscodingProtocol")
            }
        };

        if status.is_server_error() {
            tracing::error!(error = %self, "请求处理失败");
        } else {
            tracing::warn!(status = %status, error = %self, "请求未成功");
        }

        let body = Json(ErrorBody {
            error_code: error_code.to_string(),
            error_message: self.to_string(),
            stack_trace: None,
        });

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        AppError::Internal(value.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(value: reqwest::Error) -> Self {
        let mut detail = format!("HTTP请求错误: {value}");
        // 展开完整错误链，暴露真正的内层原因（TLS/DNS/connection reset等）
        let mut source = std::error::Error::source(&value);
        while let Some(inner) = source {
            detail.push_str(&format!(" -> {inner}"));
            source = std::error::Error::source(inner);
        }
        AppError::Internal(detail)
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        AppError::Internal(format!("JSON解析错误: {}", value))
    }
}
