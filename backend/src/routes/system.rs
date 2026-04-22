use crate::{
    auth::AuthSession,
    models::{
        uuid_to_emby_guid, ActivityLogQuery, BrandingConfiguration, EndpointInfo, LogFileDto, PublicSystemInfo,
        QueryResult, SystemInfo,
    },
    repository,
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};
use std::{net::IpAddr, path::Path as FsPath};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/System/Info/Public", get(public_info))
        .route("/system/info/public", get(public_info))
        .route("/System/Info", get(system_info))
        .route("/system/info", get(system_info))
        .route("/System/Endpoint", get(endpoint_info))
        .route("/system/endpoint", get(endpoint_info))
        .route("/System/Ext/ServerDomains", get(server_domains))
        .route("/system/ext/serverdomains", get(server_domains))
        .route("/System/Ping", get(ping).post(ping))
        .route("/system/ping", get(ping).post(ping))
        .route("/Branding/Configuration", get(branding_configuration))
        .route("/branding/configuration", get(branding_configuration))
        .route("/Branding/Css", get(branding_css))
        .route("/Branding/Css.css", get(branding_css))
        .route("/branding/css", get(branding_css))
        .route("/branding/css.css", get(branding_css))
        .route("/System/Configuration", get(system_configuration).post(update_system_configuration))
        .route("/System/Configuration/{key}", get(named_configuration).post(update_named_configuration))
        .route("/System/Logs", get(server_logs))
        .route("/System/Logs/Query", get(server_logs_query))
        .route("/System/Logs/Log", get(server_log_content_by_query))
        .route("/System/Logs/{name}", get(server_log_content))
        .route("/System/Logs/{name}/Lines", get(server_log_lines))
        .route("/system/logs", get(server_logs))
        .route("/System/ActivityLog/Entries", get(activity_log_entries))
        .route("/system/activitylog/entries", get(activity_log_entries))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LogQuery {
    #[serde(default, alias = "startIndex")]
    start_index: Option<i64>,
    #[serde(default, alias = "limit")]
    limit: Option<i64>,
    #[serde(default, alias = "searchTerm")]
    search_term: Option<String>,
    #[serde(default, alias = "name")]
    name: Option<String>,
}

async fn public_info(
    State(state): State<AppState>,
) -> Result<Json<PublicSystemInfo>, crate::error::AppError> {
    let startup_wizard_completed = repository::startup_wizard_completed(&state.pool).await?;
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;

    Ok(Json(PublicSystemInfo {
        local_address: format!("http://{}:{}", state.config.host, state.config.port),
        server_name: startup.server_name,
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Movie Rust".to_string(),
        operating_system: std::env::consts::OS.to_string(),
        id: uuid_to_emby_guid(&state.config.server_id),
        startup_wizard_completed,
    }))
}

async fn system_info(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<SystemInfo>, crate::error::AppError> {
    let startup_wizard_completed = repository::startup_wizard_completed(&state.pool).await?;
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;

    Ok(Json(SystemInfo {
        local_address: format!("http://{}:{}", state.config.host, state.config.port),
        server_name: startup.server_name,
        version: env!("CARGO_PKG_VERSION").to_string(),
        product_name: "Movie Rust".to_string(),
        operating_system: std::env::consts::OS.to_string(),
        id: uuid_to_emby_guid(&state.config.server_id),
        startup_wizard_completed,
        can_self_restart: false,
    }))
}

async fn endpoint_info(_session: AuthSession, State(state): State<AppState>) -> Json<EndpointInfo> {
    let host = state.config.host.trim().to_ascii_lowercase();
    let parsed_ip = state.config.host.parse::<IpAddr>().ok();
    let is_local = host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || parsed_ip.is_some_and(|ip| ip.is_loopback());
    let is_in_network = is_local || parsed_ip.is_some_and(is_private_or_link_local);
    Json(EndpointInfo {
        is_local,
        is_in_network,
    })
}

async fn server_domains(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let remote_access = repository::startup_remote_access(&state.pool, &state.config).await?;
    let local_address = format!("http://{}:{}", state.config.host, state.config.port);
    let mut data = vec![json!({
        "name": startup.server_name,
        "url": local_address,
        "isLocal": true,
        "isRemote": false
    })];

    if remote_access.enable_remote_access {
        if let Some(public_url) = state.config.public_url.as_ref() {
            let normalized = public_url.trim().trim_end_matches('/').to_string();
            if !normalized.is_empty()
                && data.iter().all(|entry| entry.get("url").and_then(Value::as_str) != Some(normalized.as_str()))
            {
                data.push(json!({
                    "name": startup.server_name,
                    "url": normalized,
                    "isLocal": false,
                    "isRemote": true
                }));
            }
        }
    }

    Ok(Json(json!({ "data": data })))
}

async fn branding_configuration(
    State(state): State<AppState>,
) -> Result<Json<BrandingConfiguration>, crate::error::AppError> {
    Ok(Json(
        repository::branding_configuration(&state.pool, &state.config).await?,
    ))
}

async fn branding_css(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let css = repository::branding_css(&state.pool, &state.config).await?;
    Ok(([(CONTENT_TYPE, "text/css; charset=utf-8")], css))
}

async fn system_configuration(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    Ok(Json(
        repository::server_configuration_value(&state.pool, &state.config).await?,
    ))
}

async fn update_system_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, crate::error::AppError> {
    if !session.is_admin {
        return Err(crate::error::AppError::Unauthorized);
    }
    repository::update_server_configuration_value(&state.pool, &state.config, payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn named_configuration(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, crate::error::AppError> {
    let key = key.trim().to_ascii_lowercase();
    if key == "branding" {
        return Ok(Json(json!(
            repository::branding_configuration(&state.pool, &state.config).await?
        )));
    }

    Ok(Json(json!({})))
}

async fn update_named_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Path(key): Path<String>,
    body: Bytes,
) -> Result<StatusCode, crate::error::AppError> {
    if !session.is_admin {
        return Err(crate::error::AppError::Unauthorized);
    }

    let key = key.trim().to_ascii_lowercase();
    if key == "branding" {
        let payload = parse_named_configuration_body(&body)?;
        let current = repository::branding_configuration(&state.pool, &state.config).await?;
        let next = BrandingConfiguration {
            login_disclaimer: payload
                .get("LoginDisclaimer")
                .and_then(Value::as_str)
                .unwrap_or(current.login_disclaimer.as_str())
                .to_string(),
            custom_css: payload
                .get("CustomCss")
                .and_then(Value::as_str)
                .unwrap_or(current.custom_css.as_str())
                .to_string(),
            splashscreen_enabled: payload
                .get("SplashscreenEnabled")
                .and_then(Value::as_bool)
                .unwrap_or(current.splashscreen_enabled),
        };
        repository::update_branding_configuration(&state.pool, &next).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn server_logs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<LogFileDto>>, crate::error::AppError> {
    Ok(Json(repository::list_server_logs(&state.config.log_dir).await?))
}

async fn server_logs_query(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
) -> Result<Json<QueryResult<LogFileDto>>, crate::error::AppError> {
    let logs = repository::list_server_logs(&state.config.log_dir).await?;
    let search_term = query
        .search_term
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    let filtered = logs
        .into_iter()
        .filter(|log| {
            search_term
                .as_ref()
                .is_none_or(|term| log.name.to_ascii_lowercase().contains(term))
        })
        .collect::<Vec<_>>();

    let start_index = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(50).clamp(1, 500) as usize;
    let total_record_count = filtered.len() as i64;
    let items = filtered.into_iter().skip(start_index).take(limit).collect();

    Ok(Json(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index as i64),
    }))
}

async fn server_log_content(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let path = log_file_path(&state.config.log_dir, &name)?;
    let content = tokio::fs::read_to_string(path).await?;
    Ok(([(CONTENT_TYPE, "text/plain; charset=utf-8")], content))
}

async fn server_log_content_by_query(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LogQuery>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let name = query
        .name
        .as_deref()
        .ok_or_else(|| crate::error::AppError::BadRequest("Log name is required".to_string()))?;
    let path = log_file_path(&state.config.log_dir, name)?;
    let content = tokio::fs::read_to_string(path).await?;
    Ok(([(CONTENT_TYPE, "text/plain; charset=utf-8")], content))
}

async fn server_log_lines(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<LogQuery>,
) -> Result<Json<QueryResult<String>>, crate::error::AppError> {
    let path = log_file_path(&state.config.log_dir, &name)?;
    let content = tokio::fs::read_to_string(path).await?;
    let search_term = query
        .search_term
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    let lines = content
        .lines()
        .filter(|line| {
            search_term
                .as_ref()
                .is_none_or(|term| line.to_ascii_lowercase().contains(term))
        })
        .map(str::to_string)
        .collect::<Vec<_>>();

    let start_index = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(200).clamp(1, 5000) as usize;
    let total_record_count = lines.len() as i64;
    let items = lines.into_iter().skip(start_index).take(limit).collect();

    Ok(Json(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index as i64),
    }))
}

async fn activity_log_entries(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ActivityLogQuery>,
) -> Result<Json<QueryResult<crate::models::ActivityLogEntryDto>>, crate::error::AppError> {
    let items = repository::list_activity_logs(&state.pool, query.limit.unwrap_or(50)).await?;
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        start_index: Some(0),
        items,
    }))
}

async fn ping() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn is_private_or_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_private() || v4.is_link_local()
        }
        IpAddr::V6(v6) => {
            v6.is_unique_local() || v6.is_unicast_link_local()
        }
    }
}

fn log_file_path(log_dir: &FsPath, name: &str) -> Result<std::path::PathBuf, crate::error::AppError> {
    let file_name = name.trim();
    if file_name.is_empty() {
        return Err(crate::error::AppError::BadRequest("日志文件名不能为空".to_string()));
    }
    if file_name.contains(['\\', '/', ':']) {
        return Err(crate::error::AppError::BadRequest("日志文件名非法".to_string()));
    }

    let path = log_dir.join(file_name);
    if !path.is_file() {
        return Err(crate::error::AppError::NotFound(format!("日志文件不存在: {file_name}")));
    }
    Ok(path)
}

fn parse_named_configuration_body(body: &[u8]) -> Result<Value, crate::error::AppError> {
    if body.is_empty() {
        return Ok(json!({}));
    }

    let value = serde_json::from_slice::<Value>(body)?;
    match value {
        Value::String(inner) => Ok(serde_json::from_str::<Value>(&inner)?),
        other => Ok(other),
    }
}
