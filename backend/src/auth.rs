use crate::{
    error::AppError,
    models::{AuthSessionRow, DbUser, UserPolicyDto},
    repository,
    state::AppState,
};
use axum::{
    extract::FromRequestParts,
    http::{header::AUTHORIZATION, request::Parts, HeaderMap},
};
use chrono::{Datelike, Timelike};
use std::net::IpAddr;
use url::form_urlencoded;
use url::Url;

#[derive(Debug, Clone)]
pub struct AuthSession {
    pub access_token: String,
    pub user_id: uuid::Uuid,
    pub is_admin: bool,
    pub is_api_key: bool,
}

#[derive(Debug, Clone)]
pub struct OptionalAuthSession(pub Option<AuthSession>);

impl From<AuthSessionRow> for AuthSession {
    fn from(value: AuthSessionRow) -> Self {
        Self {
            access_token: value.access_token,
            user_id: value.user_id,
            is_admin: value.is_admin,
            is_api_key: value.session_type.eq_ignore_ascii_case("ApiKey"),
        }
    }
}

impl FromRequestParts<AppState> for AuthSession {
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let token = extract_token(&parts.headers, parts.uri.query());
        let state = state.clone();

        async move {
            let token = token.ok_or_else(|| {
                tracing::debug!("未找到认证令牌");
                AppError::Unauthorized
            })?;

            if let Some(api_key) = &state.config.api_key {
                if token == *api_key {
                    return Ok(AuthSession {
                        access_token: token,
                        user_id: state.config.server_id,
                        is_admin: true,
                        is_api_key: false,
                    });
                }
            }

            let session = repository::get_session(&state.pool, &token)
                .await?
                .ok_or_else(|| {
                    tracing::debug!("令牌无效或会话不存在: {}", token);
                    AppError::Unauthorized
                })?;

            if let Some(expires_at) = session.expires_at {
                if expires_at < chrono::Utc::now() {
                    tracing::debug!("会话已过期: {}", token);
                    return Err(AppError::Unauthorized);
                }
            }

            Ok(session.into())
        }
    }
}

pub async fn require_auth(
    state: &AppState,
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<AuthSession, AppError> {
    let token = extract_token(headers, query).ok_or_else(|| {
        tracing::debug!("未找到认证令牌");
        AppError::Unauthorized
    })?;

    if let Some(api_key) = &state.config.api_key {
        if token == *api_key {
            return Ok(AuthSession {
                access_token: token,
                user_id: state.config.server_id,
                is_admin: true,
                is_api_key: false,
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

    Ok(session.into())
}

pub fn require_admin(session: &AuthSession) -> Result<(), AppError> {
    if session.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub fn require_interactive_session(session: &AuthSession) -> Result<(), AppError> {
    if session.is_api_key && !session.is_admin {
        Err(AppError::Forbidden)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MediaAccessKind {
    Playback,
    Download,
    VideoTranscode,
    AudioTranscode,
    Remux,
}

pub fn ensure_user_access(session: &AuthSession, user_id: uuid::Uuid) -> Result<(), AppError> {
    if session.user_id == user_id || session.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn ensure_item_access(
    state: &AppState,
    session: &AuthSession,
    item_id: uuid::Uuid,
    kind: MediaAccessKind,
) -> Result<UserPolicyDto, AppError> {
    let user = repository::get_user_by_id(&state.pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let policy = repository::user_policy_from_value(&user.policy);
    ensure_policy_allows_media_action(&policy, kind)?;

    if session.is_admin {
        return Ok(policy);
    }

    if !repository::user_can_access_item(&state.pool, session.user_id, item_id).await? {
        return Err(AppError::Forbidden);
    }

    Ok(policy)
}

pub async fn ensure_login_policy(
    state: &AppState,
    headers: &HeaderMap,
    user: &DbUser,
    device_id: Option<&str>,
) -> Result<(), AppError> {
    let policy = repository::user_policy_from_value(&user.policy);
    if policy.is_disabled || user.is_disabled {
        return Err(AppError::Unauthorized);
    }
    if policy.login_attempts_before_lockout > 0
        && policy.invalid_login_attempt_count >= policy.login_attempts_before_lockout
    {
        return Err(AppError::Forbidden);
    }
    if !policy.enable_remote_access && is_remote_login_request(state, headers) {
        return Err(AppError::Forbidden);
    }
    if !policy.enable_all_devices {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return Err(AppError::Forbidden);
        };
        if !policy
            .enabled_devices
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(device_id))
        {
            return Err(AppError::Forbidden);
        }
    }
    if !access_schedule_allows_now(&policy) {
        return Err(AppError::Forbidden);
    }
    if policy.max_active_sessions > 0 {
        let active = repository::active_session_count_for_user(&state.pool, user.id).await?;
        if active >= i64::from(policy.max_active_sessions) {
            return Err(AppError::Forbidden);
        }
    }
    Ok(())
}

fn is_remote_login_request(state: &AppState, headers: &HeaderMap) -> bool {
    if let Some(ip) = forwarded_client_ip(headers) {
        return !is_local_ip(ip);
    }

    if let Some(host) = request_host(headers) {
        if is_local_host(&host, &state.config.host) {
            return false;
        }

        if let Some(public_host) = configured_public_host(state) {
            if host.eq_ignore_ascii_case(&public_host) {
                return true;
            }
        }

        if let Ok(ip) = host.parse::<IpAddr>() {
            return !is_local_ip(ip);
        }

        return false;
    }

    false
}

fn forwarded_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    for header_name in ["X-Forwarded-For", "X-Real-IP"] {
        let value = headers.get(header_name)?.to_str().ok()?;
        let candidate = value.split(',').next()?.trim();
        if let Ok(ip) = candidate.parse::<IpAddr>() {
            return Some(ip);
        }
    }
    None
}

/// 写入 `sessions.remote_address`、Webhook `Session.RemoteAddress`、Emby 形态 **RemoteEndPoint**。
pub fn infer_client_ip(headers: &HeaderMap) -> Option<String> {
    forwarded_client_ip(headers).map(|ip| ip.to_string())
}

fn request_host(headers: &HeaderMap) -> Option<String> {
    for header_name in ["X-Forwarded-Host", "Host"] {
        let value = headers.get(header_name)?.to_str().ok()?.trim();
        if value.is_empty() {
            continue;
        }
        return Some(strip_port(value).to_string());
    }
    None
}

fn configured_public_host(state: &AppState) -> Option<String> {
    let public_url = state.config.public_url.as_ref()?.trim();
    if public_url.is_empty() {
        return None;
    }

    Url::parse(public_url)
        .ok()
        .and_then(|url| url.host_str().map(ToOwned::to_owned))
}

fn strip_port(host: &str) -> &str {
    let trimmed = host.trim().trim_matches('[').trim_matches(']');
    if let Some((name, _)) = trimmed.rsplit_once(':') {
        if !name.contains(':') {
            return name;
        }
    }
    trimmed
}

fn is_local_host(host: &str, configured_host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.eq_ignore_ascii_case(configured_host)
        || host.parse::<IpAddr>().ok().is_some_and(is_local_ip)
}

fn is_local_ip(ip: IpAddr) -> bool {
    ip.is_loopback() || is_private_or_link_local(ip)
}

fn is_private_or_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ipv4) => ipv4.is_private() || ipv4.is_link_local() || ipv4.octets()[0] == 127,
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || ipv6.is_unique_local()
                || ipv6.is_unicast_link_local()
                || ipv6.is_unspecified()
        }
    }
}

fn ensure_policy_allows_media_action(
    policy: &UserPolicyDto,
    kind: MediaAccessKind,
) -> Result<(), AppError> {
    if policy.is_disabled {
        return Err(AppError::Forbidden);
    }

    match kind {
        MediaAccessKind::Playback => {
            if policy.enable_media_playback {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
        MediaAccessKind::Download => {
            if policy.enable_media_playback && policy.enable_content_downloading {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
        MediaAccessKind::VideoTranscode => {
            if policy.enable_media_playback && policy.enable_video_playback_transcoding {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
        MediaAccessKind::AudioTranscode => {
            if policy.enable_media_playback && policy.enable_audio_playback_transcoding {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
        MediaAccessKind::Remux => {
            if policy.enable_media_playback && policy.enable_playback_remuxing {
                Ok(())
            } else {
                Err(AppError::Forbidden)
            }
        }
    }
}

fn access_schedule_allows_now(policy: &UserPolicyDto) -> bool {
    if policy.access_schedules.is_empty() {
        return true;
    }

    let now = chrono::Local::now();
    let day = match now.weekday() {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    };
    let hour = f64::from(now.hour()) + f64::from(now.minute()) / 60.0;
    policy.access_schedules.iter().any(|schedule| {
        if !schedule.day_of_week.eq_ignore_ascii_case(day) {
            return false;
        }
        if schedule.start_hour <= schedule.end_hour {
            hour >= schedule.start_hour && hour < schedule.end_hour
        } else {
            hour >= schedule.start_hour || hour < schedule.end_hour
        }
    })
}

impl FromRequestParts<AppState> for OptionalAuthSession {
    type Rejection = AppError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let token = extract_token(&parts.headers, parts.uri.query());
        let state = state.clone();

        async move {
            let Some(token) = token else {
                return Ok(OptionalAuthSession(None));
            };

            if let Some(api_key) = &state.config.api_key {
                if token == *api_key {
                    return Ok(OptionalAuthSession(Some(AuthSession {
                        access_token: token,
                        user_id: state.config.server_id,
                        is_admin: true,
                        is_api_key: false,
                    })));
                }
            }

            match repository::get_session(&state.pool, &token).await {
                Ok(Some(session)) => {
                    if let Some(expires_at) = session.expires_at {
                        if expires_at < chrono::Utc::now() {
                            return Ok(OptionalAuthSession(None));
                        }
                    }
                    Ok(OptionalAuthSession(Some(session.into())))
                }
                Ok(None) => Ok(OptionalAuthSession(None)),
                Err(e) => Err(e.into()),
            }
        }
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
