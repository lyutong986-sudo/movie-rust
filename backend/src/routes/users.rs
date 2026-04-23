use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::{
        uuid_to_emby_guid, AuthenticateByNameRequest, AuthenticationResult,
        CreateUserByNameRequest, PublicUserDto, UpdateUserPasswordRequest, UserConfigurationDto,
        UserDto, UserPolicyDto,
    },
    repository, security,
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::Value;
use url::form_urlencoded;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Users/Public", get(public_users))
        .route("/users/public", get(public_users))
        .route("/Users", get(users))
        .route("/users", get(users))
        .route("/Users/New", post(create_user))
        .route("/users/new", post(create_user))
        .route("/Users/AuthenticateByName", post(authenticate_by_name))
        .route("/Users/authenticatebyname", post(authenticate_by_name))
        .route("/users/authenticatebyname", post(authenticate_by_name))
        .route("/Users/{user_id}/Authenticate", post(authenticate_by_id))
        .route("/Users/{user_id}/authenticate", post(authenticate_by_id))
        .route("/users/{user_id}/authenticate", post(authenticate_by_id))
        .route("/Users/{user_id}/Password", post(update_password))
        .route("/Users/{user_id}/password", post(update_password))
        .route("/users/{user_id}/password", post(update_password))
        .route("/Users/Me", get(me))
        .route("/users/me", get(me))
        .route("/Users/{user_id}", get(user_by_id))
        .route("/users/{user_id}", get(user_by_id))
        .route("/Users/{user_id}/Delete", post(delete_user))
        .route("/users/{user_id}/delete", post(delete_user))
        .route("/Users/{user_id}/Policy", post(update_user_policy))
        .route("/Users/{user_id}/policy", post(update_user_policy))
        .route("/users/{user_id}/policy", post(update_user_policy))
}

async fn public_users(State(state): State<AppState>) -> Result<Json<Vec<PublicUserDto>>, AppError> {
    let users = repository::list_users(&state.pool, true).await?;
    Ok(Json(
        users
            .iter()
            .map(|user| repository::user_to_public_dto(user, state.config.server_id))
            .collect(),
    ))
}

async fn users(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<UserDto>>, AppError> {
    auth::require_admin(&session)?;
    let users = repository::list_users(&state.pool, false).await?;
    Ok(Json(
        users
            .iter()
            .map(|user| repository::user_to_dto(user, state.config.server_id))
            .collect(),
    ))
}

async fn user_by_id(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserDto>, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    Ok(Json(repository::user_to_dto(&user, state.config.server_id)))
}

async fn me(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<UserDto>, AppError> {
    let user = repository::get_user_by_id(&state.pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    Ok(Json(repository::user_to_dto(&user, state.config.server_id)))
}

async fn create_user(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<CreateUserByNameRequest>,
) -> Result<Json<UserDto>, AppError> {
    auth::require_admin(&session)?;
    let copy_from_user_id = payload
        .copy_from_user_id
        .as_deref()
        .map(Uuid::parse_str)
        .transpose()
        .map_err(|_| AppError::BadRequest("无效的 CopyFromUserId".to_string()))?;
    let password = payload
        .new_pw
        .as_deref()
        .or(payload.new_password.as_deref())
        .or(payload.password.as_deref());
    let user =
        repository::create_user(&state.pool, &payload.name, password, copy_from_user_id).await?;
    Ok(Json(repository::user_to_dto(&user, state.config.server_id)))
}

async fn delete_user(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    if session.user_id == user_id {
        return Err(AppError::BadRequest("不能删除当前登录用户".to_string()));
    }
    repository::delete_user(&state.pool, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn authenticate_by_name(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<AuthenticationResult>, AppError> {
    let payload = parse_authenticate_request(&headers, &body)?;
    authenticate(&state, headers, payload).await
}

async fn authenticate_by_id(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<Uuid>,
    body: Bytes,
) -> Result<Json<AuthenticationResult>, AppError> {
    let mut payload = parse_authenticate_request(&headers, &body)?;
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    payload.username = Some(user.name);
    authenticate(&state, headers, payload).await
}

async fn authenticate(
    state: &AppState,
    headers: HeaderMap,
    payload: AuthenticateByNameRequest,
) -> Result<Json<AuthenticationResult>, AppError> {
    let username = payload
        .username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少用户名".to_string()))?;
    let password = payload
        .pw
        .as_deref()
        .or(payload.password.as_deref())
        .unwrap_or_default();

    let user = repository::get_user_by_name(&state.pool, username)
        .await?
        .ok_or(AppError::Unauthorized)?;

    if user.is_disabled {
        return Err(AppError::Unauthorized);
    }

    let configuration = if user.configuration.is_null() {
        UserConfigurationDto::default()
    } else {
        serde_json::from_value::<UserConfigurationDto>(user.configuration.clone())
            .unwrap_or_default()
    };

    if !security::verify_password(&user.password_hash, password) {
        repository::record_failed_login(&state.pool, &user).await?;
        return Err(AppError::Unauthorized);
    }

    if !configuration.enable_local_password {
        return Err(AppError::Unauthorized);
    }

    let device_id = auth::client_value(&headers, "DeviceId").or(payload.device_id);
    let device_name = auth::client_value(&headers, "Device").or(payload.device_name);
    let client = auth::client_value(&headers, "Client").or(payload.client);
    let application_version = auth::client_value(&headers, "Version");

    auth::ensure_login_policy(state, &headers, &user, device_id.as_deref()).await?;
    repository::clear_failed_login_count(&state.pool, &user).await?;

    let session = repository::create_session(
        &state.pool,
        user.id,
        device_id,
        device_name,
        client,
        application_version,
        None,
    )
    .await?;

    Ok(Json(AuthenticationResult {
        user: repository::user_to_dto(&user, state.config.server_id),
        session_info: repository::session_to_dto(&session),
        access_token: session.access_token,
        server_id: uuid_to_emby_guid(&state.config.server_id),
    }))
}

async fn update_password(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateUserPasswordRequest>,
) -> Result<StatusCode, AppError> {
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }

    if payload.reset_password.unwrap_or(false) {
        return Err(AppError::BadRequest(
            "当前版本暂不支持无密码重置，请直接设置新密码".to_string(),
        ));
    }

    let new_password = payload
        .new_pw
        .as_deref()
        .or(payload.new_password.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少新密码".to_string()))?;

    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;

    if session.user_id == user_id && !session.is_admin {
        let current_password = payload
            .current_pw
            .as_deref()
            .or(payload.current_password.as_deref())
            .unwrap_or_default();

        if !security::verify_password(&user.password_hash, current_password) {
            return Err(AppError::Unauthorized);
        }
    }

    repository::change_user_password(&state.pool, user_id, new_password).await?;
    repository::delete_sessions_for_user(&state.pool, user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_authenticate_request(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<AuthenticateByNameRequest, AppError> {
    if body.is_empty() {
        return Ok(AuthenticateByNameRequest {
            username: None,
            pw: None,
            password: None,
            device_id: None,
            device_name: None,
            client: None,
        });
    }

    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if content_type.contains("application/x-www-form-urlencoded") {
        return Ok(parse_authenticate_form(body));
    }

    if content_type.contains("application/json") {
        return serde_json::from_slice(body)
            .map_err(|error| AppError::BadRequest(format!("登录请求 JSON 无效: {error}")));
    }

    match serde_json::from_slice(body) {
        Ok(payload) => Ok(payload),
        Err(_) => Ok(parse_authenticate_form(body)),
    }
}

fn parse_authenticate_form(body: &[u8]) -> AuthenticateByNameRequest {
    let values: Vec<(String, String)> = form_urlencoded::parse(body)
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();

    AuthenticateByNameRequest {
        username: form_value(&values, &["Username", "UserName", "Name", "username"]),
        pw: form_value(&values, &["Pw", "pw"]),
        password: form_value(&values, &["Password", "password"]),
        device_id: form_value(&values, &["DeviceId", "deviceId"]),
        device_name: form_value(&values, &["Device", "DeviceName", "deviceName"]),
        client: form_value(&values, &["Client", "client"]),
    }
}

fn form_value(values: &[(String, String)], names: &[&str]) -> Option<String> {
    values.iter().find_map(|(key, value)| {
        if names.iter().any(|name| key.eq_ignore_ascii_case(name)) {
            Some(value.trim().to_string()).filter(|value| !value.is_empty())
        } else {
            None
        }
    })
}

async fn update_user_policy(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Unauthorized);
    }

    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;

    let mut merged_policy = if user.policy.is_null() {
        serde_json::to_value(UserPolicyDto::default())?
    } else {
        user.policy.clone()
    };
    merge_json(&mut merged_policy, payload);
    let policy: UserPolicyDto = serde_json::from_value(merged_policy)
        .map_err(|error| AppError::BadRequest(format!("用户策略格式无效: {error}")))?;

    if !policy.is_administrator && user.is_admin {
        let admin_count = repository::count_admin_users(&state.pool).await?;
        if admin_count <= 1 {
            return Err(AppError::BadRequest(
                "系统中必须至少有一个管理员用户".to_string(),
            ));
        }
    }

    if policy.is_disabled && (user.is_admin || policy.is_administrator) {
        return Err(AppError::BadRequest("管理员用户不能被禁用".to_string()));
    }

    if policy.is_disabled && !user.is_disabled {
        let enabled_count = repository::count_enabled_users(&state.pool).await?;
        if enabled_count <= 1 {
            return Err(AppError::BadRequest(
                "系统中必须至少有一个启用用户".to_string(),
            ));
        }
    }

    repository::update_user_policy(&state.pool, user_id, &policy).await?;

    Ok(StatusCode::NO_CONTENT)
}

fn merge_json(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match target_map.get_mut(&key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        target_map.insert(key, value);
                    }
                }
            }
        }
        (slot, value) => *slot = value,
    }
}
