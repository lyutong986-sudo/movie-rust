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
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ErrorBody {
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::Forbidden => StatusCode::FORBIDDEN,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
            AppError::Sqlx(_) | AppError::Io(_) | AppError::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        if status.is_server_error() {
            tracing::error!(error = %self, "请求处理失败");
        } else {
            tracing::warn!(status = %status, error = %self, "请求未成功");
        }

        let body = Json(ErrorBody {
            message: self.to_string(),
        });

        (status, body).into_response()
    }
}

impl From<anyhow::Error> for AppError {
    fn from(value: anyhow::Error) -> Self {
        AppError::Internal(value.to_string())
    }
}
