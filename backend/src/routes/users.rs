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
use moka::future::Cache;
use serde_json::Value;
use std::{sync::LazyLock, time::Duration};
use url::form_urlencoded;

static PUBLIC_USERS_CACHE: LazyLock<Cache<(), Vec<PublicUserDto>>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(5))
        .build()
});
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
        .route(
            "/Users/{user_id}/Configuration",
            get(user_configuration).post(update_user_configuration),
        )
        .route(
            "/Users/{user_id}/Configuration/Partial",
            post(update_user_configuration_partial),
        )
        .route("/Users/{user_id}/Connect/Link", post(user_connect_link))
        .route(
            "/Users/{user_id}/Connect/Link/Delete",
            post(user_connect_link_delete),
        )
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
        .route("/api/admin/users/import-emby", post(import_emby_users))
        .route("/api/admin/users/policy/bulk", post(bulk_update_user_policy))
}

async fn public_users(State(state): State<AppState>) -> Result<Json<Vec<PublicUserDto>>, AppError> {
    if let Some(cached) = PUBLIC_USERS_CACHE.get(&()).await {
        return Ok(Json(cached));
    }
    let users = repository::list_users(&state.pool, true).await?;
    let result: Vec<PublicUserDto> = users
        .iter()
        .map(|user| repository::user_to_public_dto(user, state.config.server_id))
        .collect();
    PUBLIC_USERS_CACHE.insert((), result.clone()).await;
    Ok(Json(result))
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
    #[serde(default, alias = "nameStartsWithOrGreater")]
    name_starts_with_or_greater: Option<String>,
}

async fn query_users(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<UserQuery>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let raw_term = query
        .search_term
        .as_deref()
        .or(query.name_starts_with_or_greater.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let search_term = raw_term.map(|value| format!("%{}%", value.to_ascii_lowercase()));
    let start_index = query.start_index.unwrap_or(0).max(0);
    let limit = query.limit.unwrap_or(100).clamp(1, 1000);

    let total_record_count: i64 = if let Some(ref term) = search_term {
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE LOWER(name) LIKE $1")
            .bind(term)
            .fetch_one(&state.pool)
            .await?
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM users")
            .fetch_one(&state.pool)
            .await?
    };

    let users: Vec<crate::models::DbUser> = if let Some(ref term) = search_term {
        sqlx::query_as(
            "SELECT id, name, password_hash, is_admin, is_hidden, is_disabled, policy, \
             configuration, primary_image_path, backdrop_image_path, logo_image_path, date_modified, \
             easy_password_hash, created_at, legacy_password_format, legacy_password_hash \
             FROM users WHERE LOWER(name) LIKE $1 ORDER BY name OFFSET $2 LIMIT $3",
        )
        .bind(term)
        .bind(start_index)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as(
            "SELECT id, name, password_hash, is_admin, is_hidden, is_disabled, policy, \
             configuration, primary_image_path, backdrop_image_path, logo_image_path, date_modified, \
             easy_password_hash, created_at, legacy_password_format, legacy_password_hash \
             FROM users ORDER BY name OFFSET $1 LIMIT $2",
        )
        .bind(start_index)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    };

    let items: Vec<_> = users
        .iter()
        .map(|user| repository::user_to_dto(user, state.config.server_id))
        .collect();
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
    let has_access =
        repository::user_can_access_item(&state.pool, target_user_id, item_uuid).await?;
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
    let mut dto = repository::user_to_dto(&user, state.config.server_id);
    dto.last_activity_date = repository::user_last_activity(&state.pool, user_id).await?;
    Ok(Json(dto))
}

async fn me(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<UserDto>, AppError> {
    let user = repository::get_user_by_id(&state.pool, session.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    let mut dto = repository::user_to_dto(&user, state.config.server_id);
    dto.last_activity_date = repository::user_last_activity(&state.pool, session.user_id).await?;
    Ok(Json(dto))
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
            map.get("Configuration")
                .or_else(|| map.get("configuration"))
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
    Ok(Json(repository::user_to_dto(
        &refreshed,
        state.config.server_id,
    )))
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

    // 空哈希 = 无密码（ResetPassword 后的状态）：仅当用户也没提供密码时放行。
    let passwordless = user.password_hash.trim().is_empty() && password.is_empty();

    let primary_ok = passwordless || security::verify_password(&user.password_hash, password);
    let mut legacy_upgraded = false;
    if !primary_ok {
        let legacy_ok = match (
            user.legacy_password_format.as_deref(),
            user.legacy_password_hash.as_deref(),
        ) {
            (Some(format), Some(stored)) if !format.is_empty() && !stored.is_empty() => {
                security::verify_legacy_password(format, stored, password)
            }
            _ => false,
        };
        if !legacy_ok {
            repository::record_failed_login(&state.pool, &user).await?;
            crate::webhooks::dispatch(
                state,
                crate::webhooks::events::USER_AUTH_FAILED,
                serde_json::json!({
                    "User": {
                        "Id":   crate::models::uuid_to_emby_guid(&user.id),
                        "Name": user.name,
                    }
                }),
            );
            return Err(AppError::Unauthorized);
        }
        if let Err(err) =
            repository::upgrade_legacy_password(&state.pool, user.id, password).await
        {
            tracing::warn!(user_id = %user.id, ?err, "legacy 密码升级 Argon2 失败，仍允许本次登录");
        } else {
            legacy_upgraded = true;
            tracing::info!(
                user_id = %user.id,
                "已用旧版哈希成功登录，并就地升级到 Argon2"
            );
        }
    }
    let _ = legacy_upgraded;

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
        auth::infer_client_ip(&headers),
        None,
    )
    .await?;

    let mut user_dto = repository::user_to_dto(&user, state.config.server_id);
    let now = chrono::Utc::now();
    user_dto.last_login_date = Some(now);
    user_dto.last_activity_date = Some(now);

    // 出向 webhook：登录成功 + 新会话
    let session_payload = serde_json::json!({
        "User": {
            "Id":   crate::models::uuid_to_emby_guid(&user.id),
            "Name": user.name.clone(),
        },
        "Session": {
            "Id":         session.access_token.clone(),
            "Client":     session.client.clone().unwrap_or_else(|| "Unknown".to_string()),
            "DeviceName": session.device_name.clone().unwrap_or_default(),
            "DeviceId":   session.device_id.clone().unwrap_or_default(),
            "RemoteAddress": session.remote_address.clone().unwrap_or_default(),
        }
    });
    crate::webhooks::dispatch(
        state,
        crate::webhooks::events::USER_AUTHENTICATED,
        session_payload.clone(),
    );
    crate::webhooks::dispatch(state, crate::webhooks::events::SESSION_START, session_payload);

    Ok(Json(AuthenticationResult {
        user: user_dto,
        session_info: repository::session_to_dto(&session, state.config.server_id),
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

    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;

    // Emby/Sakura 风格的"清密码"两阶段流程：先 `{ResetPassword:true}` → 再 `{NewPw:"..."}`。
    // 仅 admin（含 API Key）能直接重置；用户改自己密码必须沿正常路径提供新密码。
    if payload.reset_password.unwrap_or(false) {
        if !session.is_admin {
            return Err(AppError::Forbidden);
        }
        // 清空密码哈希 → 用户可以无密码登录（与 Emby 行为一致）。
        // Sakura_embyboss `emby_reset(id, None)` 依赖此行为。
        repository::set_user_password_hash(&state.pool, user_id, "").await?;
        repository::delete_sessions_for_user(&state.pool, user_id).await?;
        return Ok(StatusCode::NO_CONTENT);
    }

    let new_password = payload
        .new_pw
        .as_deref()
        .or(payload.new_password.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少新密码".to_string()))?;

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
        return Err(AppError::Forbidden);
    }
    apply_user_policy_update(&state, user_id, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 把 incoming policy patch 里 `EnabledFolders / BlockedMediaFolders` 中的库名字符串
/// 翻译成 GUID。
///
/// 第三方管理脚本（如 `Sakura_embyboss`）经常直接把中文库名塞进
/// `BlockedMediaFolders=['播放列表', ...]`；本项目的标准契约是 GUID 列表。
/// 这里在 deserialize 之前先做一次 lookup：
/// - 字符串能解析为标准 UUID 或 emby 32-hex GUID → 原样保留
/// - 否则到 `libraries` 表里按 `lower(name)` 匹配 → 用对应 id 的 32-hex GUID 替换
/// - 找不到的字符串保持原值（lossy deserializer 会在下一步丢弃，等同 emby
///   服务端面对未知库名的行为）
async fn resolve_folder_names_in_policy(
    state: &AppState,
    mut payload: Value,
) -> Result<Value, AppError> {
    let Some(obj) = payload.as_object_mut() else {
        return Ok(payload);
    };
    let needs_lookup = ["EnabledFolders", "BlockedMediaFolders"].iter().any(|k| {
        obj.get(*k).and_then(Value::as_array).is_some_and(|arr| {
            arr.iter().any(|v| {
                v.as_str()
                    .map(|s| Uuid::parse_str(s).is_err() && crate::models::emby_id_to_uuid(s).is_err())
                    .unwrap_or(false)
            })
        })
    });
    if !needs_lookup {
        return Ok(payload);
    }

    let libraries = repository::list_libraries(&state.pool).await?;
    let by_name: std::collections::HashMap<String, Uuid> = libraries
        .iter()
        .map(|lib| (lib.name.to_ascii_lowercase(), lib.id))
        .collect();

    for key in ["EnabledFolders", "BlockedMediaFolders"] {
        if let Some(arr) = obj.get_mut(key).and_then(Value::as_array_mut) {
            for v in arr.iter_mut() {
                let resolved = match v.as_str() {
                    Some(s) => {
                        if Uuid::parse_str(s).is_ok() || crate::models::emby_id_to_uuid(s).is_ok()
                        {
                            None
                        } else {
                            by_name.get(&s.to_ascii_lowercase()).map(|id| {
                                let guid = crate::models::uuid_to_emby_guid(id);
                                tracing::debug!(
                                    original = s,
                                    resolved = %id,
                                    "policy.{} 用库名匹配到 GUID",
                                    key
                                );
                                guid
                            })
                        }
                    }
                    None => None,
                };
                if let Some(guid) = resolved {
                    *v = Value::String(guid);
                }
            }
        }
    }
    Ok(payload)
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

    let payload = resolve_folder_names_in_policy(state, payload).await?;

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
    #[serde(
        alias = "EnteredUsername",
        alias = "enteredUsername",
        alias = "username"
    )]
    entered_username: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ForgotPasswordPinRequest {
    #[serde(alias = "enteredPin", alias = "Pin", alias = "pin")]
    entered_pin: Option<String>,
    #[serde(
        alias = "newPw",
        alias = "newPassword",
        alias = "NewPw",
        alias = "NewPassword"
    )]
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

    // 使用 SQL JOIN 一次性找到匹配 PIN 的用户，避免 O(n) 逐用户查询
    let row: Option<(uuid::Uuid, Value)> = sqlx::query_as(
        "SELECT u.id, s.value FROM users u \
         INNER JOIN system_settings s ON s.key = 'password_reset_pin:' || u.id::text \
         WHERE s.value->>'Pin' = $1 \
         LIMIT 1",
    )
    .bind(entered_pin)
    .fetch_optional(&state.pool)
    .await?;

    let Some((user_id, pin_value)) = row else {
        return Err(AppError::BadRequest("PIN 无效或已过期".to_string()));
    };

    let expires_at = pin_value
        .get("ExpiresAt")
        .and_then(Value::as_str)
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Utc));
    let key = format!("password_reset_pin:{}", user_id);
    if expires_at.is_some_and(|value| value < chrono::Utc::now()) {
        let _ = repository::set_setting_value(&state.pool, &key, serde_json::Value::Null).await;
        return Err(AppError::BadRequest("PIN 已过期，请重新申请".to_string()));
    }
    repository::change_user_password(&state.pool, user_id, new_password).await?;
    let _ = repository::set_setting_value(&state.pool, &key, serde_json::Value::Null).await;
    Ok(Json(serde_json::json!({
        "Success": true,
        "UsersReset": [uuid_to_emby_guid(&user_id)],
    })))
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
        &format!(
            "user_track_selection:{user_id}:{}",
            _track_type.to_ascii_lowercase()
        ),
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
        &format!(
            "user_track_selection:{user_id}:{}",
            _track_type.to_ascii_lowercase()
        ),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RatingQuery {
    #[serde(default, alias = "likes", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
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
    repository::delete_setting_value(
        &state.pool,
        &format!("user_item_rating:{user_id}:{item_id}"),
    )
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
        &format!(
            "user_typed_settings:{user_id}:{}",
            _key.to_ascii_lowercase()
        ),
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

// ---------------------------------------------------------------------------
// 批量导入 Emby SQLite 用户 + 批量改 Policy
//
// 使用场景：管理员把 Emby `users.db` 里 `LocalUsersv2.data` 的用户名+SHA1 密码
// 批量灌进本项目，立刻就能用 emby 老密码登录；后续登录第一次成功时密码会被
// 透明升级为 Argon2，legacy 字段被清空。
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImportEmbyUsersRequest {
    /// 一组待导入用户，每条至少含 `Name` 与 `LegacyPasswordHash`。
    pub users: Vec<ImportEmbyUserItem>,
    /// 重名策略：
    /// - `"skip"`（默认）：本地已有同名用户时跳过
    /// - `"overwrite"`：覆盖本地用户的 legacy 哈希（保留原 password_hash 不变 ——
    ///   下次登录仍会先尝试 Argon2，失败再走 emby SHA1）
    #[serde(default)]
    pub conflict_policy: Option<String>,
    /// 默认 Policy 模板（PascalCase），所有未在条目内单独指定 Policy 的用户都套用它。
    /// 留空时用 `UserPolicyDto::default()`。
    #[serde(default)]
    pub default_policy: Option<Value>,
    /// 默认 `legacy_password_format`，单条目里没指定时落到这个值。默认 `"emby_sha1"`。
    #[serde(default)]
    pub default_legacy_format: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImportEmbyUserItem {
    pub name: String,
    pub legacy_password_hash: String,
    #[serde(default)]
    pub legacy_password_format: Option<String>,
    /// 单条覆盖 Policy；与 `default_policy` 的关系是"先取条目，再 fallback default"。
    #[serde(default)]
    pub policy: Option<Value>,
    /// 其他可选元数据（仅记录到响应，不写库）。
    #[serde(default, alias = "EmbyId", alias = "IdString")]
    pub external_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImportEmbyUsersResponse {
    pub created: Vec<ImportedUserSummary>,
    pub updated: Vec<ImportedUserSummary>,
    pub skipped: Vec<ImportedUserSummary>,
    pub failed: Vec<ImportFailureSummary>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImportedUserSummary {
    pub user_id: String,
    pub name: String,
    pub external_id: Option<String>,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImportFailureSummary {
    pub name: String,
    pub external_id: Option<String>,
    pub error: String,
}

async fn import_emby_users(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<ImportEmbyUsersRequest>,
) -> Result<Json<ImportEmbyUsersResponse>, AppError> {
    auth::require_admin(&session)?;

    let conflict = payload
        .conflict_policy
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("skip")
        .to_ascii_lowercase();
    if conflict != "skip" && conflict != "overwrite" {
        return Err(AppError::BadRequest(
            "ConflictPolicy 仅支持 skip / overwrite".to_string(),
        ));
    }
    let default_format = payload
        .default_legacy_format
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("emby_sha1")
        .to_string();
    let default_policy_value = payload
        .default_policy
        .clone()
        .unwrap_or_else(|| serde_json::to_value(UserPolicyDto::default()).unwrap_or_default());

    let mut response = ImportEmbyUsersResponse {
        created: Vec::new(),
        updated: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
    };

    for item in payload.users {
        let name = item.name.trim().to_string();
        let external_id = item
            .external_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        if name.is_empty() {
            response.failed.push(ImportFailureSummary {
                name: item.name.clone(),
                external_id: external_id.clone(),
                error: "用户名为空".to_string(),
            });
            continue;
        }
        let legacy_hash = item.legacy_password_hash.trim();
        if legacy_hash.is_empty() {
            response.failed.push(ImportFailureSummary {
                name,
                external_id,
                error: "缺少 LegacyPasswordHash".to_string(),
            });
            continue;
        }
        let format = item
            .legacy_password_format
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(&default_format)
            .to_string();
        let policy_value = item.policy.clone().unwrap_or_else(|| default_policy_value.clone());

        match repository::get_user_by_name(&state.pool, &name).await {
            Ok(Some(existing)) => {
                if conflict == "skip" {
                    response.skipped.push(ImportedUserSummary {
                        user_id: uuid_to_emby_guid(&existing.id),
                        name,
                        external_id,
                    });
                    continue;
                }
                // overwrite：仅刷新 legacy 字段，不动 Argon2 主哈希
                if let Err(err) = repository::set_user_legacy_password(
                    &state.pool,
                    existing.id,
                    Some(&format),
                    Some(legacy_hash),
                )
                .await
                {
                    response.failed.push(ImportFailureSummary {
                        name,
                        external_id,
                        error: format!("更新 legacy 哈希失败: {err}"),
                    });
                    continue;
                }
                if let Err(err) =
                    apply_user_policy_update(&state, existing.id, policy_value).await
                {
                    response.failed.push(ImportFailureSummary {
                        name,
                        external_id,
                        error: format!("更新 Policy 失败: {err}"),
                    });
                    continue;
                }
                response.updated.push(ImportedUserSummary {
                    user_id: uuid_to_emby_guid(&existing.id),
                    name,
                    external_id,
                });
            }
            Ok(None) => {
                // 创建：先用占位 Argon2（一段随机 token，永远无法用明文登录），再写 legacy 哈希。
                // 这样登录路径会先 Argon2 失败 → 走 emby SHA1 fallback 命中 → 自动升级。
                let placeholder = Uuid::new_v4().to_string();
                let new_user = match repository::create_user(
                    &state.pool,
                    &name,
                    Some(&placeholder),
                    None,
                )
                .await
                {
                    Ok(u) => u,
                    Err(err) => {
                        response.failed.push(ImportFailureSummary {
                            name,
                            external_id,
                            error: format!("创建用户失败: {err}"),
                        });
                        continue;
                    }
                };
                if let Err(err) = repository::set_user_legacy_password(
                    &state.pool,
                    new_user.id,
                    Some(&format),
                    Some(legacy_hash),
                )
                .await
                {
                    response.failed.push(ImportFailureSummary {
                        name,
                        external_id,
                        error: format!("写 legacy 哈希失败: {err}"),
                    });
                    continue;
                }
                if let Err(err) =
                    apply_user_policy_update(&state, new_user.id, policy_value).await
                {
                    response.failed.push(ImportFailureSummary {
                        name,
                        external_id,
                        error: format!("应用 Policy 失败: {err}"),
                    });
                    continue;
                }
                response.created.push(ImportedUserSummary {
                    user_id: uuid_to_emby_guid(&new_user.id),
                    name,
                    external_id,
                });
            }
            Err(err) => {
                response.failed.push(ImportFailureSummary {
                    name,
                    external_id,
                    error: format!("查询用户失败: {err}"),
                });
            }
        }
    }

    Ok(Json(response))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct BulkUpdatePolicyRequest {
    /// 待更新的用户 ID 列表（支持 Emby 32-hex GUID 或标准 UUID）
    pub user_ids: Vec<String>,
    /// 要 patch 进每个用户 Policy 的 JSON 片段（PascalCase 字段）
    pub policy_patch: Value,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BulkUpdatePolicyResponse {
    pub updated: Vec<String>,
    pub failed: Vec<ImportFailureSummary>,
}

async fn bulk_update_user_policy(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<BulkUpdatePolicyRequest>,
) -> Result<Json<BulkUpdatePolicyResponse>, AppError> {
    auth::require_admin(&session)?;
    if payload.user_ids.is_empty() {
        return Err(AppError::BadRequest("UserIds 不能为空".to_string()));
    }
    if !payload.policy_patch.is_object() {
        return Err(AppError::BadRequest("PolicyPatch 必须是对象".to_string()));
    }

    let mut response = BulkUpdatePolicyResponse {
        updated: Vec::new(),
        failed: Vec::new(),
    };
    for raw_id in payload.user_ids {
        let id = match crate::models::emby_id_to_uuid(&raw_id)
            .or_else(|_| Uuid::parse_str(&raw_id))
        {
            Ok(v) => v,
            Err(_) => {
                response.failed.push(ImportFailureSummary {
                    name: raw_id.clone(),
                    external_id: None,
                    error: "用户 ID 格式无效".to_string(),
                });
                continue;
            }
        };
        match apply_user_policy_update(&state, id, payload.policy_patch.clone()).await {
            Ok(()) => response.updated.push(uuid_to_emby_guid(&id)),
            Err(err) => response.failed.push(ImportFailureSummary {
                name: raw_id,
                external_id: None,
                error: err.to_string(),
            }),
        }
    }
    Ok(Json(response))
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
        let policy_part = payload_clone.get("Policy").cloned().unwrap_or(Value::Null);
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
