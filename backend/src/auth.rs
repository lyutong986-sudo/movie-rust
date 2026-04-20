use crate::{error::AppError, models::AuthSessionRow, repository, state::AppState};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, HeaderMap},
};
use url::form_urlencoded;

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub access_token: String,
    pub user_id: uuid::Uuid,
    pub is_admin: bool,
}

impl From<AuthSessionRow> for AuthSession {
    fn from(value: AuthSessionRow) -> Self {
        Self {
            access_token: value.access_token,
            user_id: value.user_id,
            is_admin: value.is_admin,
        }
    }
}

impl FromRequestParts<AppState> for AuthSession {
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let headers = parts.headers.clone();
        let query = parts.uri.query().map(ToOwned::to_owned);
        let state = state.clone();

        async move { require_auth(&state, &headers, query.as_deref()).await }
    }
}

pub async fn require_auth(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<AuthSession, AppError> {
    let token = extract_token(headers, query).ok_or(AppError::Unauthorized)?;
    let session = repository::get_session(&state.pool, &token)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(session.into())
}

pub fn require_admin(session: &AuthSession) -> Result<(), AppError> {
    if session.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub fn extract_token(headers: &HeaderMap, query: Option<&str>) -> Option<String> {
    header_value(headers, "X-Emby-Token")
        .or_else(|| header_value(headers, "X-MediaBrowser-Token"))
        .or_else(|| bearer_token(headers))
        .or_else(|| media_browser_token(headers))
        .or_else(|| query_token(query))
}

pub fn client_value(headers: &HeaderMap, key: &str) -> Option<String> {
    authorization_header(headers).and_then(|value| parse_kv_header(value, key))
}

fn header_value(headers: &HeaderMap, key: &str) -> Option<String> {
    headers
        .get(key)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?.trim();
    value
        .strip_prefix("Bearer ")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
}

fn media_browser_token(headers: &HeaderMap) -> Option<String> {
    let value = authorization_header(headers)?;
    parse_kv_header(value, "Token")
}

fn query_token(query: Option<&str>) -> Option<String> {
    let query = query?;
    form_urlencoded::parse(query.as_bytes())
        .find_map(|(key, value)| match key.as_ref() {
            "api_key" | "apiKey" | "ApiKey" | "X-Emby-Token" | "X-MediaBrowser-Token" => {
                Some(value.into_owned())
            }
            _ => None,
        })
        .filter(|value| !value.trim().is_empty())
}

fn parse_kv_header(value: &str, key: &str) -> Option<String> {
    value.split(',').find_map(|part| {
        let normalized = part
            .trim()
            .strip_prefix("MediaBrowser ")
            .or_else(|| part.trim().strip_prefix("Emby "))
            .unwrap_or_else(|| part.trim());
        let mut segments = normalized.splitn(2, '=');
        let name = segments.next()?.trim();
        let raw_value = segments.next()?.trim();

        if name.eq_ignore_ascii_case(key) {
            Some(raw_value.trim_matches('"').to_string())
        } else {
            None
        }
    })
}

fn authorization_header(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("X-Emby-Authorization")
        .or_else(|| headers.get(AUTHORIZATION))
        .and_then(|value| value.to_str().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn extracts_token_from_x_emby_authorization() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "X-Emby-Authorization",
            HeaderValue::from_static(
                "MediaBrowser Client=\"Emby\", Device=\"Player\", DeviceId=\"device-1\", Version=\"4.0\", Token=\"abc123\"",
            ),
        );

        assert_eq!(extract_token(&headers, None).as_deref(), Some("abc123"));
        assert_eq!(
            client_value(&headers, "DeviceId").as_deref(),
            Some("device-1")
        );
    }

    #[test]
    fn extracts_token_from_emby_query_aliases() {
        let headers = HeaderMap::new();

        assert_eq!(
            extract_token(&headers, Some("api_key=abc123")).as_deref(),
            Some("abc123")
        );
    }
}
