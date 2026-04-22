use crate::{error::AppError, models::{AuthSessionRow, UserPolicyDto}, repository, state::AppState};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, HeaderMap},
};
use url::form_urlencoded;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub access_token: String,
    pub user_id: uuid::Uuid,
    pub is_admin: bool,
}

#[derive(Debug, Clone)]
pub struct OptionalAuthSession(pub Option<AuthSession>);

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
    let token = extract_token(headers, query).ok_or_else(|| {
        tracing::debug!(
            "未找到认证令牌，headers: {:?}, query: {:?}",
            headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.to_str().unwrap_or("[invalid]")))
                .collect::<Vec<_>>(),
            query
        );
        AppError::Unauthorized
    })?;
    
    if let Some(api_key) = &state.config.api_key {
        if token == *api_key {
            return Ok(AuthSession {
                access_token: token,
                user_id: state.config.server_id,
                is_admin: true,
            });
        }
    }
    
    let session = repository::get_session(&state.pool, &token)
        .await?
        .ok_or_else(|| {
            tracing::debug!("令牌无效或会话不存在: {}", token);
            AppError::Unauthorized
        })?;
    
    // 检查会话是否已过期
    if let Some(expires_at) = session.expires_at {
        if expires_at < chrono::Utc::now() {
            tracing::debug!("会话已过期: {}", token);
            return Err(AppError::Unauthorized);
        }
    }
    
    ensure_session_policy_allows(&state, &session).await?;
    Ok(session.into())
}

pub fn require_admin(session: &AuthSession) -> Result<(), AppError> {
    if session.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn user_policy(state: &AppState, session: &AuthSession) -> Result<UserPolicyDto, AppError> {
    if session.is_admin && session.user_id == state.config.server_id {
        return Ok(UserPolicyDto {
            is_administrator: true,
            ..UserPolicyDto::default()
        });
    }

    let user = repository::get_user_by_id(&state.pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let policy = repository::user_to_dto(&user, state.config.server_id).policy;

    if policy.is_disabled {
        return Err(AppError::Forbidden);
    }

    Ok(policy)
}

pub async fn policy_for_user(state: &AppState, user_id: Uuid) -> Result<UserPolicyDto, AppError> {
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(repository::user_to_dto(&user, state.config.server_id).policy)
}

pub async fn allowed_library_ids_for_user(state: &AppState, user_id: Uuid) -> Result<Option<Vec<Uuid>>, AppError> {
    let policy = policy_for_user(state, user_id).await?;
    if policy.is_administrator || policy.enable_all_folders {
        Ok(None)
    } else {
        Ok(Some(policy.enabled_folders))
    }
}

pub async fn require_media_playback(state: &AppState, session: &AuthSession, item_id: Uuid) -> Result<(), AppError> {
    let policy = user_policy(state, session).await?;
    if !policy.is_administrator && !policy.enable_media_playback {
        return Err(AppError::Forbidden);
    }
    require_library_access(state, &policy, item_id).await
}

pub async fn require_content_download(state: &AppState, session: &AuthSession, item_id: Uuid) -> Result<(), AppError> {
    let policy = user_policy(state, session).await?;
    if !policy.is_administrator && !policy.enable_content_downloading {
        return Err(AppError::Forbidden);
    }
    require_library_access(state, &policy, item_id).await
}

pub async fn require_content_deletion(state: &AppState, session: &AuthSession, item_id: Uuid) -> Result<(), AppError> {
    let policy = user_policy(state, session).await?;
    if !policy.is_administrator && !policy.enable_content_deletion {
        return Err(AppError::Forbidden);
    }
    let Some(library_id) = repository::get_media_item_library_id(&state.pool, item_id).await? else {
        return Err(AppError::NotFound("媒体项目不存在".to_string()));
    };
    if !policy.is_administrator
        && !policy.enable_content_deletion_from_folders.is_empty()
        && !policy.enable_content_deletion_from_folders.contains(&library_id)
    {
        return Err(AppError::Forbidden);
    }
    require_library_access_by_library_id(&policy, library_id)
}

pub async fn require_subtitle_download(state: &AppState, session: &AuthSession, item_id: Uuid) -> Result<(), AppError> {
    let policy = user_policy(state, session).await?;
    if !policy.is_administrator && !policy.enable_subtitle_downloading {
        return Err(AppError::Forbidden);
    }
    require_library_access(state, &policy, item_id).await
}

pub async fn require_subtitle_management(state: &AppState, session: &AuthSession, item_id: Uuid) -> Result<(), AppError> {
    let policy = user_policy(state, session).await?;
    if !policy.is_administrator && !policy.enable_subtitle_management {
        return Err(AppError::Forbidden);
    }
    require_library_access(state, &policy, item_id).await
}

async fn require_library_access(state: &AppState, policy: &UserPolicyDto, item_id: Uuid) -> Result<(), AppError> {
    let Some(library_id) = repository::get_media_item_library_id(&state.pool, item_id).await? else {
        return Err(AppError::NotFound("媒体项目不存在".to_string()));
    };
    require_library_access_by_library_id(policy, library_id)
}

fn require_library_access_by_library_id(policy: &UserPolicyDto, library_id: Uuid) -> Result<(), AppError> {
    if policy.is_administrator || policy.enable_all_folders || policy.enabled_folders.contains(&library_id) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

impl FromRequestParts<AppState> for OptionalAuthSession {
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let headers = parts.headers.clone();
        let query = parts.uri.query().map(ToOwned::to_owned);
        let state = state.clone();

        async move {
            match extract_token(&headers, query.as_deref()) {
                Some(token) => {
                    if let Some(api_key) = &state.config.api_key {
                        if token == *api_key {
                            return Ok(OptionalAuthSession(Some(AuthSession {
                                access_token: token,
                                user_id: state.config.server_id,
                                is_admin: true,
                            })));
                        }
                    }
                    
                    match repository::get_session(&state.pool, &token).await {
                        Ok(Some(session)) => {
                            ensure_session_policy_allows(&state, &session).await?;
                            Ok(OptionalAuthSession(Some(session.into())))
                        }
                        Ok(None) => Ok(OptionalAuthSession(None)),
                        Err(e) => Err(e.into()),
                    }
                }
                None => Ok(OptionalAuthSession(None)),
            }
        }
    }
}

async fn ensure_session_policy_allows(state: &AppState, session: &AuthSessionRow) -> Result<(), AppError> {
    let Some(user) = repository::get_user_by_id(&state.pool, session.user_id).await? else {
        return Err(AppError::Unauthorized);
    };
    let policy = repository::user_to_dto(&user, state.config.server_id).policy;

    if policy.is_disabled {
        return Err(AppError::Forbidden);
    }

    if !policy.is_administrator && !policy.enable_all_devices {
        let allowed = session
            .device_id
            .as_deref()
            .map(|device_id| policy.enabled_devices.iter().any(|value| value == device_id))
            .unwrap_or(false);
        if !allowed {
            return Err(AppError::Forbidden);
        }
    }

    Ok(())
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
