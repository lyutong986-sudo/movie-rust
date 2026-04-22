use crate::{
    auth::AuthSession,
    scanner,
    models::{
        uuid_to_emby_guid, ActivityLogQuery, BrandingConfiguration, EndpointInfo, LogFileDto, PublicSystemInfo,
        QueryResult, SystemInfo,
    },
    repository,
    state::AppState,
};
use axum::{
    body::Bytes,
    extract::{Form, Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
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
        .route("/System/Configuration", get(system_configuration).post(update_system_configuration).put(update_system_configuration))
        .route("/System/Configuration/{key}", get(named_configuration).post(update_named_configuration).put(update_named_configuration))
        .route("/System/MediaEncoder/Path", get(media_encoder_path).post(update_media_encoder_path))
        .route("/System/Logs", get(server_logs))
        .route("/System/Logs/Query", get(server_logs_query))
        .route("/System/Logs/Log", get(server_log_content_by_query))
        .route("/System/Logs/{name}", get(server_log_content))
        .route("/System/Logs/{name}/Lines", get(server_log_lines))
        .route("/system/logs", get(server_logs))
        .route("/System/ActivityLog/Entries", get(activity_log_entries))
        .route("/system/activitylog/entries", get(activity_log_entries))
        .route("/System/ReleaseNotes", get(release_notes))
        .route("/System/ReleaseNotes/Versions", get(release_note_versions))
        .route("/System/WakeOnLanInfo", get(wake_on_lan_info))
        .route("/System/Restart", post(restart_server))
        .route("/System/Shutdown", post(shutdown_server))
        .route("/ScheduledTasks", get(scheduled_tasks))
        .route("/ScheduledTasks/{id}", get(scheduled_task_by_id))
        .route("/ScheduledTasks/{id}/Triggers", get(task_triggers))
        .route("/ScheduledTasks/Running/{id}", post(run_scheduled_task).delete(stop_scheduled_task))
        .route("/ScheduledTasks/Running/{id}/Delete", post(stop_scheduled_task))
        .route("/Plugins", get(plugins))
        .route("/Plugins/{id}/Configuration", get(plugin_configuration).post(update_plugin_configuration))
        .route("/Plugins/{id}/Delete", post(delete_plugin))
        .route("/Plugins/{id}", axum::routing::delete(delete_plugin))
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

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MediaEncoderPathForm {
    #[serde(default, alias = "path")]
    path: String,
    #[serde(default, alias = "PathType", alias = "pathType")]
    path_type: Option<String>,
}

async fn release_notes(_session: AuthSession) -> impl IntoResponse {
    ([(CONTENT_TYPE, "text/plain; charset=utf-8")], "")
}

async fn release_note_versions(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn wake_on_lan_info(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn restart_server(session: AuthSession) -> Result<StatusCode, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    Ok(StatusCode::NOT_IMPLEMENTED)
}

async fn shutdown_server(session: AuthSession) -> Result<StatusCode, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    Ok(StatusCode::NOT_IMPLEMENTED)
}

async fn scheduled_tasks(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    Ok(Json(build_scheduled_tasks(&state).await?))
}

async fn scheduled_task_by_id(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    find_scheduled_task(&state, &id)
        .await?
        .map(Json)
        .ok_or_else(|| crate::error::AppError::NotFound(format!("Scheduled task not found: {id}")))
}

async fn task_triggers(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Value>>, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    let task = find_scheduled_task(&state, &id)
        .await?
        .ok_or_else(|| crate::error::AppError::NotFound(format!("Scheduled task not found: {id}")))?;
    Ok(Json(
        task.get("Triggers")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    ))
}

async fn run_scheduled_task(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    match id.as_str() {
        "libraryscan" | "metadatarefresh" => {
            let _ = scanner::scan_all_libraries(&state.pool, state.metadata_manager.as_deref()).await?;
            Ok(StatusCode::NO_CONTENT)
        }
        _ => Err(crate::error::AppError::NotFound(format!("Scheduled task not found: {id}"))),
    }
}

async fn stop_scheduled_task(
    session: AuthSession,
    Path(id): Path<String>,
) -> Result<StatusCode, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    match id.as_str() {
        "libraryscan" | "metadatarefresh" => Ok(StatusCode::NOT_IMPLEMENTED),
        _ => Err(crate::error::AppError::NotFound(format!("Scheduled task not found: {id}"))),
    }
}

async fn plugins(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    Ok(Json(build_plugins(&state).await?))
}

async fn plugin_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let disabled = disabled_plugins(&config);
    Ok(Json(json!({
        "Id": id,
        "Enabled": !disabled.iter().any(|value| value == &id)
    })))
}

async fn update_plugin_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    let mut config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let enabled = payload.get("Enabled").and_then(Value::as_bool).unwrap_or(true);
    let mut disabled = disabled_plugins(&config);
    if enabled {
        disabled.retain(|value| value != &id);
    } else if !disabled.iter().any(|value| value == &id) {
        disabled.push(id.clone());
    }
    disabled.sort();
    disabled.dedup();
    if let Some(object) = config.as_object_mut() {
        object.insert("DisabledPluginsText".to_string(), json!(disabled.join("\n")));
    }
    repository::update_server_configuration_value(&state.pool, &state.config, config).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_plugin(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    update_plugin_configuration(session, State(state), Path(id), Json(json!({ "Enabled": false }))).await
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

    Ok(Json(repository::named_configuration_value(&state.pool, &key).await?))
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
    } else {
        let payload = parse_named_configuration_body(&body)?;
        repository::update_named_configuration_value(&state.pool, &key, payload).await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn media_encoder_path(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    Ok(Json(json!({
        "Path": config
            .get("EncoderAppPath")
            .or_else(|| config.get("MediaEncoderPath"))
            .and_then(Value::as_str)
            .unwrap_or(state.config.ffmpeg_path.as_str()),
        "PathType": config
            .get("EncoderAppPathType")
            .or_else(|| config.get("MediaEncoderPathType"))
            .and_then(Value::as_str)
            .unwrap_or("System")
    })))
}

async fn update_media_encoder_path(
    session: AuthSession,
    State(state): State<AppState>,
    Form(payload): Form<MediaEncoderPathForm>,
) -> Result<StatusCode, crate::error::AppError> {
    crate::auth::require_admin(&session)?;
    let path = payload.path.trim();
    if path.is_empty() {
        return Err(crate::error::AppError::BadRequest("Media encoder path is required".to_string()));
    }
    let mut config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| crate::error::AppError::Internal("server configuration is not an object".to_string()))?;
    let path_type = payload.path_type.unwrap_or_else(|| "Custom".to_string());
    object.insert("EncoderAppPath".to_string(), json!(path));
    object.insert("MediaEncoderPath".to_string(), json!(path));
    object.insert("EncoderAppPathType".to_string(), json!(path_type));
    object.insert("MediaEncoderPathType".to_string(), json!(path_type));
    repository::update_server_configuration_value(&state.pool, &state.config, config).await?;
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

pub(crate) async fn build_scheduled_tasks(state: &AppState) -> Result<Vec<Value>, crate::error::AppError> {
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let enable_tasks = config
        .get("EnableScheduledTasks")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let library_scan_hours = config
        .get("LibraryScanIntervalHours")
        .and_then(Value::as_i64)
        .unwrap_or(24);
    let metadata_refresh_hours = config
        .get("MetadataRefreshIntervalHours")
        .and_then(Value::as_i64)
        .unwrap_or(72);
    let now = Utc::now();

    Ok(vec![
        json!({
            "Name": "Scan Media Library",
            "State": "Idle",
            "CurrentProgressPercentage": 0,
            "Id": "libraryscan",
            "LastExecutionResult": {
                "StartTimeUtc": (now - Duration::hours(library_scan_hours)).to_rfc3339(),
                "EndTimeUtc": (now - Duration::hours(library_scan_hours) + Duration::minutes(5)).to_rfc3339(),
                "Status": "Completed",
                "Name": "Scan Media Library",
                "Key": "libraryscan",
                "Id": "libraryscan",
                "ErrorMessage": null,
                "LongErrorMessage": null
            },
            "Triggers": [{
                "Type": "IntervalTrigger",
                "IntervalTicks": library_scan_hours * 60 * 60 * 10_000_000_i64
            }],
            "Description": "Scans all configured libraries for new or changed content.",
            "Category": "Library",
            "IsHidden": false,
            "Key": "libraryscan",
            "IsEnabled": enable_tasks
        }),
        json!({
            "Name": "Refresh Metadata",
            "State": "Idle",
            "CurrentProgressPercentage": 0,
            "Id": "metadatarefresh",
            "LastExecutionResult": {
                "StartTimeUtc": (now - Duration::hours(metadata_refresh_hours)).to_rfc3339(),
                "EndTimeUtc": (now - Duration::hours(metadata_refresh_hours) + Duration::minutes(8)).to_rfc3339(),
                "Status": "Completed",
                "Name": "Refresh Metadata",
                "Key": "metadatarefresh",
                "Id": "metadatarefresh",
                "ErrorMessage": null,
                "LongErrorMessage": null
            },
            "Triggers": [{
                "Type": "IntervalTrigger",
                "IntervalTicks": metadata_refresh_hours * 60 * 60 * 10_000_000_i64
            }],
            "Description": "Refreshes provider metadata and artwork for library items.",
            "Category": "Library",
            "IsHidden": false,
            "Key": "metadatarefresh",
            "IsEnabled": enable_tasks
        }),
    ])
}

async fn find_scheduled_task(
    state: &AppState,
    id: &str,
) -> Result<Option<Value>, crate::error::AppError> {
    Ok(build_scheduled_tasks(state)
        .await?
        .into_iter()
        .find(|task| task.get("Id").and_then(Value::as_str) == Some(id)))
}

pub(crate) async fn build_plugins(state: &AppState) -> Result<Vec<Value>, crate::error::AppError> {
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let disabled = disabled_plugins(&config);
    let global_enabled = config
        .get("EnablePlugins")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut items = vec![json!({
        "Name": "Local Metadata Reader",
        "Version": env!("CARGO_PKG_VERSION"),
        "ConfigurationFileName": "local-metadata.json",
        "Description": "Reads local NFO files, images, and embedded media metadata.",
        "Id": "local-metadata",
        "ImageTag": null,
        "Enabled": global_enabled && !disabled.iter().any(|value| value == "local-metadata")
    })];

    if state
        .metadata_manager
        .as_ref()
        .and_then(|manager| manager.get_provider("tmdb"))
        .is_some()
    {
        items.push(json!({
            "Name": "TMDb Metadata Provider",
            "Version": env!("CARGO_PKG_VERSION"),
            "ConfigurationFileName": "tmdb.json",
            "Description": "Fetches remote metadata, people, and images from TMDb.",
            "Id": "tmdb",
            "ImageTag": null,
            "Enabled": global_enabled && !disabled.iter().any(|value| value == "tmdb")
        }));
    }

    Ok(items)
}

fn disabled_plugins(config: &Value) -> Vec<String> {
    config
        .get("DisabledPluginsText")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
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
