use crate::{
    error::AppError,
    models::{StartupConfiguration, StartupRemoteAccessRequest, UserDto},
    repository,
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::State,
    http::{header::CONTENT_TYPE, HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use url::form_urlencoded;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/Startup/Configuration",
            get(configuration).post(update_configuration),
        )
        .route("/Startup/User", get(first_user).post(create_first_user))
        .route("/Startup/FirstUser", get(first_user))
        .route(
            "/Startup/RemoteAccess",
            get(get_remote_access).post(remote_access),
        )
        .route("/Startup/Complete", axum::routing::post(complete))
}

async fn configuration(
    State(state): State<AppState>,
) -> Result<Json<StartupConfiguration>, AppError> {
    Ok(Json(
        repository::startup_configuration(&state.pool, &state.config).await?,
    ))
}

async fn update_configuration(
    State(state): State<AppState>,
    Json(payload): Json<StartupConfiguration>,
) -> Result<StatusCode, AppError> {
    repository::update_startup_configuration(&state.pool, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn first_user(State(state): State<AppState>) -> Result<Json<Option<UserDto>>, AppError> {
    let users = repository::list_users(&state.pool, false).await?;
    let user = match users.first() {
        Some(user) => Some(
            repository::user_to_dto_with_context(&state.pool, user, state.config.server_id)
                .await?,
        ),
        None => None,
    };
    Ok(Json(user))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct StartupUserPayload {
    name: Option<String>,
    password: Option<String>,
    connect_user_name: Option<String>,
}

async fn create_first_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<Value>, AppError> {
    let payload = parse_startup_user_request(&headers, &body)?;
    let name = payload
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少用户名".to_string()))?;
    let password = payload.password.as_deref().unwrap_or_default();

    let users = repository::list_users(&state.pool, false).await?;
    let user = if let Some(existing) = users.first() {
        repository::update_user_name(&state.pool, existing.id, name).await?;
        repository::get_user_by_id(&state.pool, existing.id)
            .await?
            .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?
    } else {
        repository::create_initial_admin(&state.pool, name, password).await?
    };

    let user_link_result = if let Some(connect_user_name) = payload
        .connect_user_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let value = json!({
            "ConnectUsername": connect_user_name,
            "ConnectUserName": connect_user_name,
            "ConnectUserId": connect_user_name,
            "ConnectLinkType": "LinkedUser"
        });
        repository::set_user_connect_link(&state.pool, user.id, &value).await?;
        Some(json!({
            "IsPending": false,
            "IsNewUserInvitation": false,
            "GuestDisplayName": connect_user_name
        }))
    } else {
        None
    };

    let dto = repository::user_to_dto_with_context(&state.pool, &user, state.config.server_id).await?;
    Ok(Json(json!({
        "Name": dto.name,
        "ServerId": dto.server_id,
        "Id": dto.id,
        "HasPassword": dto.has_password,
        "HasConfiguredPassword": dto.has_configured_password,
        "HasConfiguredEasyPassword": dto.has_configured_easy_password,
        "ConnectUserName": dto.connect_user_name,
        "ConnectUserId": dto.connect_user_id,
        "ConnectLinkType": dto.connect_link_type,
        "PrimaryImageTag": dto.primary_image_tag,
        "LastActivityDate": dto.last_activity_date,
        "Policy": dto.policy,
        "Configuration": dto.configuration,
        "UserLinkResult": user_link_result
    })))
}

async fn remote_access(
    State(state): State<AppState>,
    Json(payload): Json<StartupRemoteAccessRequest>,
) -> Result<StatusCode, AppError> {
    repository::update_remote_access(&state.pool, &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_remote_access(
    State(state): State<AppState>,
) -> Result<Json<StartupRemoteAccessRequest>, AppError> {
    Ok(Json(
        repository::startup_remote_access(&state.pool, &state.config).await?,
    ))
}

async fn complete(State(state): State<AppState>) -> Result<StatusCode, AppError> {
    repository::complete_startup_wizard(&state.pool).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn parse_startup_user_request(
    headers: &HeaderMap,
    body: &[u8],
) -> Result<StartupUserPayload, AppError> {
    if body.is_empty() {
        return Ok(StartupUserPayload {
            name: None,
            password: None,
            connect_user_name: None,
        });
    }

    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if content_type.contains("application/json") {
        return serde_json::from_slice(body)
            .map_err(|error| AppError::BadRequest(format!("Startup/User JSON 无效: {error}")));
    }

    let values: Vec<(String, String)> = form_urlencoded::parse(body)
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();

    Ok(StartupUserPayload {
        name: form_value(&values, &["Name", "name", "Username", "UserName"]),
        password: form_value(&values, &["Password", "password"]),
        connect_user_name: form_value(&values, &["ConnectUserName", "connectUserName"]),
    })
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
