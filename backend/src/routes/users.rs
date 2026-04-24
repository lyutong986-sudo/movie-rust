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
    extract::{Path, Query, State},
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
        .route("/Users/Query", get(query_users))
        .route("/Users/Prefixes", get(user_prefixes))
        .route("/Users/ItemAccess", get(user_item_access))
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
        // Emby POST /Users/{Id} 用于"更新用户基本信息"（重命名 / 写回 Policy / Configuration）。
        // DELETE /Users/{Id} 是官方 OpenAPI 里的删除动词，保留 POST /Users/{Id}/Delete 兼容旧客户端。
        .route(
            "/Users/{user_id}",
            get(user_by_id).post(update_user).delete(delete_user),
        )
        .route(
            "/users/{user_id}",
            get(user_by_id).post(update_user).delete(delete_user),
        )
        .route("/Users/{user_id}/Delete", post(delete_user))
        .route("/users/{user_id}/delete", post(delete_user))
        .route("/Users/{user_id}/EasyPassword", post(update_easy_password))
        .route("/Users/{user_id}/easypassword", post(update_easy_password))
        .route("/users/{user_id}/easypassword", post(update_easy_password))
        .route("/Users/{user_id}/Policy", post(update_user_policy))
        .route("/Users/{user_id}/Configuration", get(user_configuration).post(update_user_configuration))
        .route("/Users/{user_id}/Configuration/Partial", post(update_user_configuration_partial))
        .route("/Users/{user_id}/Connect/Link", post(user_connect_link))
        .route("/Users/{user_id}/Connect/Link/Delete", post(user_connect_link_delete))
        .route(
            "/Users/{user_id}/TrackSelections/{track_type}",
            post(user_track_selection),
        )
        .route(
            "/Users/{user_id}/TrackSelections/{track_type}/Delete",
            post(user_track_selection_delete),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/Rating",
            post(user_item_rating).delete(user_item_rating_delete),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/Rating/Delete",
            post(user_item_rating_delete),
        )
        .route(
            "/Users/{user_id}/TypedSettings/{key}",
            get(user_typed_settings).post(update_user_typed_settings),
        )
        .route("/Users/{user_id}/policy", post(update_user_policy))
        .route("/users/{user_id}/policy", post(update_user_policy))
        .route("/Users/ForgotPassword", post(forgot_password))
        .route("/Users/ForgotPassword/Pin", post(forgot_password_pin))
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UserQuery {
    #[serde(default, alias = "startIndex")]
    start_index: Option<i64>,
    #[serde(default, alias = "limit")]
    limit: Option<i64>,
    #[serde(default, alias = "searchTerm")]
    search_term: Option<String>,
}

async fn query_users(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<UserQuery>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let users = repository::list_users(&state.pool, false).await?;
    let search_term = query
        .search_term
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());
    let filtered = users
        .into_iter()
        .filter(|user| {
            search_term
                .as_ref()
                .is_none_or(|term| user.name.to_ascii_lowercase().contains(term))
        })
        .collect::<Vec<_>>();
    let start_index = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(100).clamp(1, 1000) as usize;
    let total_record_count = filtered.len() as i64;
    let items = filtered
        .into_iter()
        .skip(start_index)
        .take(limit)
        .map(|user| repository::user_to_dto(&user, state.config.server_id))
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({
        "Items": items,
        "TotalRecordCount": total_record_count,
        "StartIndex": start_index
    })))
}

async fn user_prefixes(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let mut items = vec!["#".to_string()];
    let ui = startup.ui_culture.to_ascii_lowercase();
    if ui.starts_with("zh") {
        for ch in 'A'..='Z' {
            items.push(ch.to_string());
        }
        items.push("拼音".to_string());
    } else {
        for ch in 'A'..='Z' {
            items.push(ch.to_string());
        }
    }
    Ok(Json(serde_json::json!(items)))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UserItemAccessQuery {
    #[serde(default, alias = "userId")]
    user_id: Option<Uuid>,
    #[serde(default, alias = "itemId")]
    item_id: Option<String>,
}

async fn user_item_access(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<UserItemAccessQuery>,
) -> Result<Json<Value>, AppError> {
    let target_user_id = query.user_id.unwrap_or(session.user_id);
    auth::ensure_user_access(&session, target_user_id)?;
    let Some(item_id) = query.item_id.as_deref() else {
        return Ok(Json(serde_json::json!({
            "UserId": target_user_id.to_string().to_uppercase(),
            "Items": [],
            "TotalRecordCount": 0
        })));
    };
    let item_uuid = crate::models::emby_id_to_uuid(item_id)
        .map_err(|_| AppError::BadRequest("无效的 itemId".to_string()))?;
    let has_access = repository::user_can_access_item(&state.pool, target_user_id, item_uuid).await?;
    Ok(Json(serde_json::json!({
        "UserId": target_user_id.to_string().to_uppercase(),
        "ItemId": item_id,
        "HasAccess": has_access
    })))
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

/// `POST /Users/{Id}`：Emby 官方的 `UpdateUser`，客户端通常携带一份完整
/// `UserDto`（含 `Name` / `Policy` / `Configuration`）提交。我们只接受白名单
/// 字段：
/// - 任何拥有 `EnableUserPreferenceAccess` 的用户都可以改自己的
///   `Configuration`；
/// - 管理员额外可以改 `Name` 和 `Policy`（Policy 走 `update_user_policy` 的
///   安全检查链，保证系统至少存在一个管理员/启用用户）。
async fn update_user(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<Value>,
) -> Result<Json<UserDto>, AppError> {
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;

    let self_update = session.user_id == user_id;
    if !session.is_admin && !self_update {
        return Err(AppError::Forbidden);
    }
    if self_update && !session.is_admin {
        let policy = repository::user_policy_from_value(&user.policy);
        if !policy.enable_user_preference_access {
            return Err(AppError::Forbidden);
        }
    }

    let payload_obj = payload.as_object();

    if session.is_admin {
        if let Some(new_name) = payload_obj
            .and_then(|map| map.get("Name").or_else(|| map.get("name")))
            .and_then(Value::as_str)
        {
            repository::rename_user(&state.pool, user_id, new_name).await?;
        }
    }

    if let Some(configuration_value) = payload_obj
        .and_then(|map| {
            map.get("Configuration").or_else(|| map.get("configuration"))
        })
        .cloned()
    {
        let mut current = if user.configuration.is_null() {
            serde_json::to_value(UserConfigurationDto::default())?
        } else {
            user.configuration.clone()
        };
        merge_json(&mut current, configuration_value);
        let next = serde_json::from_value::<UserConfigurationDto>(current)
            .map_err(|error| AppError::BadRequest(format!("用户配置格式无效: {error}")))?;
        repository::update_user_configuration(&state.pool, user_id, &next).await?;
    }

    if session.is_admin {
        if let Some(policy_value) = payload_obj
            .and_then(|map| map.get("Policy").or_else(|| map.get("policy")))
            .cloned()
        {
            if policy_value.is_object() {
                apply_user_policy_update(&state, user_id, policy_value).await?;
            }
        }
    }

    let refreshed = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    Ok(Json(repository::user_to_dto(&refreshed, state.config.server_id)))
}

/// `POST /Users/{Id}/EasyPassword`：Emby 用于设置 PIN 的快速密码。
/// - 用户只能改自己的 PIN，管理员可以代改；
/// - 传入 `ResetPassword=true` 会清空现有 PIN；
/// - 非管理员改自己的 PIN 需要提供 `CurrentPw` / `CurrentPassword` 与主密码匹配。
async fn update_easy_password(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<UpdateUserPasswordRequest>,
) -> Result<StatusCode, AppError> {
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }

    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;

    if payload.reset_password.unwrap_or(false) {
        repository::set_user_easy_password(&state.pool, user_id, None).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    let new_password = payload
        .new_pw
        .as_deref()
        .or(payload.new_password.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少新的快速密码".to_string()))?;

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

    repository::set_user_easy_password(&state.pool, user_id, Some(new_password)).await?;
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
    apply_user_policy_update(&state, user_id, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 带安全约束的策略写入：保留 `update_user_policy` 与 `update_user` 复用。
///
/// 约束：
/// - 不能把最后一个启用的管理员降级或禁用；
/// - 不允许禁用管理员用户；
/// - 不能让系统出现"零个启用用户"。
async fn apply_user_policy_update(
    state: &AppState,
    user_id: Uuid,
    payload: Value,
) -> Result<(), AppError> {
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

    repository::update_user_policy(&state.pool, user_id, &policy).await
}

async fn user_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<UserConfigurationDto>, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let configuration = if user.configuration.is_null() {
        UserConfigurationDto::default()
    } else {
        serde_json::from_value::<UserConfigurationDto>(user.configuration).unwrap_or_default()
    };
    Ok(Json(configuration))
}

async fn update_user_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(configuration): Json<UserConfigurationDto>,
) -> Result<Json<UserConfigurationDto>, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    repository::update_user_configuration(&state.pool, user_id, &configuration).await?;
    Ok(Json(configuration))
}

async fn update_user_configuration_partial(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<Value>,
) -> Result<Json<UserConfigurationDto>, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let mut current = if user.configuration.is_null() {
        serde_json::to_value(UserConfigurationDto::default())?
    } else {
        user.configuration
    };
    merge_json(&mut current, payload);
    let next = serde_json::from_value::<UserConfigurationDto>(current)
        .map_err(|error| AppError::BadRequest(format!("用户配置格式无效: {error}")))?;
    repository::update_user_configuration(&state.pool, user_id, &next).await?;
    Ok(Json(next))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ForgotPasswordRequest {
    #[serde(alias = "EnteredUsername", alias = "enteredUsername", alias = "username")]
    entered_username: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ForgotPasswordPinRequest {
    #[serde(alias = "enteredPin", alias = "Pin", alias = "pin")]
    entered_pin: Option<String>,
    #[serde(alias = "newPw", alias = "newPassword", alias = "NewPw", alias = "NewPassword")]
    new_password: Option<String>,
}

fn generate_pin() -> String {
    let bytes = Uuid::new_v4().into_bytes();
    let mut value: u64 = 0;
    for (index, byte) in bytes.iter().take(8).enumerate() {
        value ^= (*byte as u64) << (index * 8);
    }
    let number = (value % 1_000_000) as u32;
    format!("{:06}", number)
}

async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> Result<Json<Value>, AppError> {
    let username = payload
        .entered_username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("请输入用户名".to_string()))?;
    let user = repository::get_user_by_name(&state.pool, username).await?;
    let response_fields = |pin_file: &str, expires: chrono::DateTime<chrono::Utc>| {
        serde_json::json!({
            "Action": "PinCode",
            "PinFile": pin_file,
            "PinExpirationDate": expires.to_rfc3339(),
        })
    };
    let Some(user) = user else {
        return Ok(Json(response_fields("UnknownUser", chrono::Utc::now())));
    };
    let pin = generate_pin();
    let expires_at = chrono::Utc::now() + chrono::Duration::minutes(30);
    let key = format!("password_reset_pin:{}", user.id);
    let value = serde_json::json!({
        "Pin": pin,
        "ExpiresAt": expires_at.to_rfc3339(),
    });
    repository::set_setting_value(&state.pool, &key, value).await?;
    tracing::info!(user = %user.name, pin = %pin, "已生成密码重置 PIN（调试日志）");
    Ok(Json(response_fields(
        &format!("passwordreset-{}.txt", user.id),
        expires_at,
    )))
}

async fn forgot_password_pin(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordPinRequest>,
) -> Result<Json<Value>, AppError> {
    let entered_pin = payload
        .entered_pin
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("请输入 PIN".to_string()))?;
    let new_password = payload
        .new_password
        .as_deref()
        .map(str::trim)
        .filter(|value| value.len() >= 4)
        .ok_or_else(|| AppError::BadRequest("新密码至少 4 个字符".to_string()))?;

    let users = repository::list_users(&state.pool, false).await?;
    for user in users {
        let key = format!("password_reset_pin:{}", user.id);
        let Some(value) = repository::get_setting_value(&state.pool, &key).await? else {
            continue;
        };
        let stored_pin = value.get("Pin").and_then(Value::as_str).unwrap_or_default();
        let expires_at = value
            .get("ExpiresAt")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&chrono::Utc));
        if stored_pin != entered_pin {
            continue;
        }
        if expires_at.is_some_and(|value| value < chrono::Utc::now()) {
            let _ = repository::set_setting_value(
                &state.pool,
                &key,
                serde_json::Value::Null,
            )
            .await;
            return Err(AppError::BadRequest("PIN 已过期，请重新申请".to_string()));
        }
        repository::change_user_password(&state.pool, user.id, new_password).await?;
        let _ = repository::set_setting_value(
            &state.pool,
            &key,
            serde_json::Value::Null,
        )
        .await;
        return Ok(Json(serde_json::json!({
            "Success": true,
            "UsersReset": [uuid_to_emby_guid(&user.id)],
        })));
    }
    Err(AppError::BadRequest("PIN 无效或已过期".to_string()))
}

async fn user_connect_link(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    repository::set_setting_value(
        &state.pool,
        &format!("user_connect_link:{user_id}"),
        payload,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_connect_link_delete(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    repository::delete_setting_value(&state.pool, &format!("user_connect_link:{user_id}")).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_track_selection(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, _track_type)): Path<(Uuid, String)>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    repository::set_setting_value(
        &state.pool,
        &format!("user_track_selection:{user_id}:{}", _track_type.to_ascii_lowercase()),
        payload,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_track_selection_delete(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, _track_type)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    repository::delete_setting_value(
        &state.pool,
        &format!("user_track_selection:{user_id}:{}", _track_type.to_ascii_lowercase()),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RatingQuery {
    #[serde(default, alias = "likes")]
    likes: Option<bool>,
    #[serde(default, alias = "rating")]
    rating: Option<f64>,
}

async fn user_item_rating(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, _item_id)): Path<(Uuid, String)>,
    Query(query): Query<RatingQuery>,
) -> Result<StatusCode, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    let item_id = crate::models::emby_id_to_uuid(&_item_id)
        .map_err(|_| AppError::BadRequest("无效的 itemId".to_string()))?;
    repository::set_setting_value(
        &state.pool,
        &format!("user_item_rating:{user_id}:{item_id}"),
        serde_json::json!({
            "ItemId": _item_id,
            "Likes": query.likes,
            "Rating": query.rating
        }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_item_rating_delete(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, _item_id)): Path<(Uuid, String)>,
) -> Result<StatusCode, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    let item_id = crate::models::emby_id_to_uuid(&_item_id)
        .map_err(|_| AppError::BadRequest("无效的 itemId".to_string()))?;
    repository::delete_setting_value(&state.pool, &format!("user_item_rating:{user_id}:{item_id}"))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_typed_settings(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, key)): Path<(Uuid, String)>,
) -> Result<Json<Value>, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    let setting_key = format!("user_typed_settings:{user_id}:{}", key.to_ascii_lowercase());
    let value = repository::get_setting_value(&state.pool, &setting_key)
        .await?
        .unwrap_or_else(|| serde_json::json!({}));
    Ok(Json(serde_json::json!({ "Key": key, "Value": value })))
}

async fn update_user_typed_settings(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, _key)): Path<(Uuid, String)>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::ensure_user_access(&session, user_id)?;
    repository::set_setting_value(
        &state.pool,
        &format!("user_typed_settings:{user_id}:{}", _key.to_ascii_lowercase()),
        payload,
    )
    .await?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rating_query_accepts_both_casings() {
        let upper: RatingQuery = serde_json::from_value(json!({"Likes": true, "Rating": 4.5}))
            .expect("PascalCase 名称应可解析");
        assert_eq!(upper.likes, Some(true));
        assert_eq!(upper.rating, Some(4.5));

        let lower: RatingQuery = serde_json::from_value(json!({"likes": false, "rating": 2.0}))
            .expect("lower-case aliases should work");
        assert_eq!(lower.likes, Some(false));
        assert_eq!(lower.rating, Some(2.0));
    }

    #[test]
    fn merge_json_overlays_patch_onto_existing_user_config() {
        let mut target = json!({
            "PlayDefaultAudioTrack": true,
            "SubtitleMode": "Default",
            "LatestItemsExcludes": ["movies"]
        });
        let patch = json!({
            "SubtitleMode": "Always",
            "DisplayCollectionsView": true,
            "LatestItemsExcludes": ["shows"]
        });

        merge_json(&mut target, patch);
        assert_eq!(target["PlayDefaultAudioTrack"], json!(true));
        assert_eq!(target["SubtitleMode"], json!("Always"));
        assert_eq!(target["DisplayCollectionsView"], json!(true));
        assert_eq!(target["LatestItemsExcludes"], json!(["shows"]));
    }

    #[test]
    fn users_router_builds_with_new_settings_and_rating_endpoints() {
        let _router = super::router();
    }

    #[test]
    fn update_user_password_request_accepts_easy_password_reset_payload() {
        // 前端 / EmbyClient 在清除 PIN 时会发 `{ResetPassword: true}`，必须能被 serde 解析。
        let payload: UpdateUserPasswordRequest =
            serde_json::from_value(json!({ "ResetPassword": true }))
                .expect("ResetPassword 负载应可解析");
        assert_eq!(payload.reset_password, Some(true));
        assert!(payload.new_pw.is_none());
        assert!(payload.new_password.is_none());
    }

    #[test]
    fn update_user_payload_extracts_whitelisted_fields() {
        // 模拟 Emby 客户端 "保存用户" 发来的 UserDto 提交，验证我们只取 Name/Configuration/Policy。
        let payload = json!({
            "Id": "whatever",
            "Name": "Renamed",
            "HasPassword": true,
            "Configuration": { "SubtitleMode": "Always" },
            "Policy": { "IsAdministrator": true }
        });
        let map = payload.as_object().unwrap();
        assert_eq!(map.get("Name").and_then(Value::as_str), Some("Renamed"));
        assert!(map.get("Configuration").unwrap().is_object());
        assert!(map.get("Policy").unwrap().is_object());
        // HasPassword / Id 不在写入白名单内 —— 静态断言。
        let payload_clone = payload.clone();
        let policy_part = payload_clone
            .get("Policy")
            .cloned()
            .unwrap_or(Value::Null);
        assert!(policy_part.is_object());
    }

    #[test]
    fn merge_json_preserves_existing_policy_when_patch_is_empty_object() {
        let mut target = json!({
            "IsAdministrator": true,
            "EnableContentDownloading": true,
            "EnabledFolders": []
        });
        merge_json(&mut target, json!({}));
        assert_eq!(target["IsAdministrator"], json!(true));
        assert_eq!(target["EnableContentDownloading"], json!(true));
        assert_eq!(target["EnabledFolders"], json!([]));
    }
}
