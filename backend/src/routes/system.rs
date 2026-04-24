use crate::{
    auth::{require_admin, AuthSession},
    models::{
        uuid_to_emby_guid, ActivityLogQuery, BrandingConfiguration, EncodingOptionsDto,
        EndpointInfo, LogFileDto, PublicSystemInfo, QueryResult, SystemInfo,
    },
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::IntoResponse,
    routing::{get, post},
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
        .route("/System/Logs", get(server_logs))
        .route("/System/Configuration", get(system_configuration).post(update_system_configuration))
        .route("/System/Configuration/Partial", post(update_system_configuration_partial))
        .route(
            "/System/Configuration/{name}",
            get(named_configuration).post(update_named_configuration),
        )
        .route(
            "/system/configuration/{name}",
            get(named_configuration).post(update_named_configuration),
        )
        .route("/System/MediaEncoder/Path", post(update_media_encoder_path))
        .route("/system/mediaencoder/path", post(update_media_encoder_path))
        .route("/System/Logs/Query", get(server_logs_query))
        .route("/System/Logs/{name}", get(server_log_content))
        .route("/System/Logs/{name}/Lines", get(server_log_lines))
        .route("/system/logs", get(server_logs))
        .route("/System/ActivityLog/Entries", get(activity_log_entries))
        .route("/system/activitylog/entries", get(activity_log_entries))
        .route("/System/ReleaseNotes", get(release_notes))
        .route("/System/ReleaseNotes/Versions", get(release_note_versions))
        .route("/System/WakeOnLanInfo", get(wake_on_lan_info))
        .route("/System/Restart", post(system_restart))
        .route("/System/Shutdown", post(system_shutdown))
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
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MediaEncoderPathRequest {
    #[serde(default, alias = "path")]
    path: String,
    #[serde(default, alias = "pathType")]
    path_type: String,
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
        encoder_location_type: repository::encoding_options(&state.pool, &state.config)
            .await?
            .encoder_location_type,
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
                && data.iter().all(|entry| {
                    entry.get("url").and_then(Value::as_str) != Some(normalized.as_str())
                })
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
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<BrandingConfiguration>, crate::error::AppError> {
    Ok(Json(
        repository::branding_configuration(&state.pool, &state.config).await?,
    ))
}

async fn branding_css(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, crate::error::AppError> {
    let css = repository::branding_css(&state.pool, &state.config).await?;
    Ok(([(CONTENT_TYPE, "text/css; charset=utf-8")], css))
}

async fn named_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Value>, crate::error::AppError> {
    require_admin(&session)?;

    match name.trim().to_ascii_lowercase().as_str() {
        "encoding" => Ok(Json(json!(
            repository::encoding_options(&state.pool, &state.config).await?
        ))),
        _ => Err(crate::error::AppError::NotFound(format!(
            "配置不存在: {name}"
        ))),
    }
}

async fn system_configuration(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, crate::error::AppError> {
    require_admin(&session)?;
    Ok(Json(build_system_configuration(&state).await?))
}

async fn update_system_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, crate::error::AppError> {
    require_admin(&session)?;
    apply_system_configuration_update(&state, payload).await?;
    Ok(Json(build_system_configuration(&state).await?))
}

async fn update_system_configuration_partial(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, crate::error::AppError> {
    require_admin(&session)?;
    apply_system_configuration_update(&state, payload).await?;
    Ok(Json(build_system_configuration(&state).await?))
}

async fn update_named_configuration(
    session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, crate::error::AppError> {
    require_admin(&session)?;

    match name.trim().to_ascii_lowercase().as_str() {
        "encoding" => {
            let options = serde_json::from_value::<EncodingOptionsDto>(payload)?;
            let saved =
                repository::update_encoding_options(&state.pool, &state.config, options).await?;
            Ok(Json(json!(saved)))
        }
        _ => Err(crate::error::AppError::NotFound(format!(
            "配置不存在: {name}"
        ))),
    }
}

async fn build_system_configuration(state: &AppState) -> Result<Value, crate::error::AppError> {
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let remote_access = repository::startup_remote_access(&state.pool, &state.config).await?;
    let encoding = repository::encoding_options(&state.pool, &state.config).await?;
    Ok(json!({
        "ServerName": startup.server_name,
        "UICulture": startup.ui_culture,
        "PreferredMetadataLanguage": startup.preferred_metadata_language,
        "MetadataCountryCode": startup.metadata_country_code,
        "LibraryScanThreadCount": startup.library_scan_thread_count,
        "StrmAnalysisThreadCount": startup.strm_analysis_thread_count,
        "TmdbMetadataThreadCount": startup.tmdb_metadata_thread_count,
        "EnableRemoteAccess": remote_access.enable_remote_access,
        "EnableAutomaticPortMapping": remote_access.enable_automatic_port_mapping.unwrap_or(false),
        "Encoding": encoding
    }))
}

async fn apply_system_configuration_update(
    state: &AppState,
    payload: Value,
) -> Result<(), crate::error::AppError> {
    if let Some(startup_value) = payload.get("StartupConfiguration") {
        let startup = serde_json::from_value::<crate::models::StartupConfiguration>(
            startup_value.clone(),
        )?;
        repository::update_startup_configuration(&state.pool, &startup).await?;
    } else if payload.get("ServerName").is_some()
        || payload.get("UICulture").is_some()
        || payload.get("PreferredMetadataLanguage").is_some()
        || payload.get("MetadataCountryCode").is_some()
        || payload.get("LibraryScanThreadCount").is_some()
        || payload.get("StrmAnalysisThreadCount").is_some()
        || payload.get("TmdbMetadataThreadCount").is_some()
    {
        let current = repository::startup_configuration(&state.pool, &state.config).await?;
        let mut current_value = serde_json::to_value(current)?;
        merge_object(&mut current_value, &payload);
        let startup = serde_json::from_value::<crate::models::StartupConfiguration>(current_value)?;
        repository::update_startup_configuration(&state.pool, &startup).await?;
    }

    if let Some(encoding_value) = payload.get("Encoding") {
        let options = serde_json::from_value::<EncodingOptionsDto>(encoding_value.clone())?;
        let _ = repository::update_encoding_options(&state.pool, &state.config, options).await?;
    }
    Ok(())
}

async fn update_media_encoder_path(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<MediaEncoderPathRequest>,
) -> Result<Json<EncodingOptionsDto>, crate::error::AppError> {
    require_admin(&session)?;
    Ok(Json(
        repository::update_media_encoder_path(
            &state.pool,
            &state.config,
            payload.path,
            payload.path_type,
        )
        .await?,
    ))
}

async fn server_logs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<LogFileDto>>, crate::error::AppError> {
    Ok(Json(
        repository::list_server_logs(&state.config.log_dir).await?,
    ))
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

async fn release_notes() -> Json<Value> {
    Json(json!({
        "Version": env!("CARGO_PKG_VERSION"),
        "Description": "Movie Rust Emby-compatible backend release.",
        "Url": "https://github.com"
    }))
}

async fn release_note_versions() -> Json<Value> {
    Json(json!([
        {
            "Version": env!("CARGO_PKG_VERSION"),
            "Date": chrono::Utc::now().date_naive().to_string()
        }
    ]))
}

async fn wake_on_lan_info() -> Json<Value> {
    Json(json!({
        "CanWake": false,
        "Entries": []
    }))
}

async fn system_restart(session: AuthSession) -> Result<StatusCode, crate::error::AppError> {
    require_admin(&session)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn system_shutdown(session: AuthSession) -> Result<StatusCode, crate::error::AppError> {
    require_admin(&session)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ping() -> StatusCode {
    StatusCode::NO_CONTENT
}

fn is_private_or_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_unique_local() || v6.is_unicast_link_local(),
    }
}

fn log_file_path(
    log_dir: &FsPath,
    name: &str,
) -> Result<std::path::PathBuf, crate::error::AppError> {
    let file_name = name.trim();
    if file_name.is_empty() {
        return Err(crate::error::AppError::BadRequest(
            "日志文件名不能为空".to_string(),
        ));
    }
    if file_name.contains(['\\', '/', ':']) {
        return Err(crate::error::AppError::BadRequest(
            "日志文件名非法".to_string(),
        ));
    }

    let path = log_dir.join(file_name);
    if !path.is_file() {
        return Err(crate::error::AppError::NotFound(format!(
            "日志文件不存在: {file_name}"
        )));
    }
    Ok(path)
}

fn merge_object(target: &mut Value, patch: &Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                if let Some(existing) = target_map.get_mut(key) {
                    merge_object(existing, value);
                } else {
                    target_map.insert(key.clone(), value.clone());
                }
            }
        }
        (target_slot, patch_value) => *target_slot = patch_value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_object_performs_deep_merge_on_nested_configuration() {
        let mut target = json!({
            "ServerName": "movie-rust",
            "EncodingOptions": {
                "Hardware": "qsv",
                "Preset": "fast"
            },
            "RemoteAccess": {
                "Enabled": true
            }
        });
        let patch = json!({
            "ServerName": "movie-rust-edge",
            "EncodingOptions": {
                "Preset": "slow",
                "EnableTonemapping": true
            },
            "Plugins": { "Tmdb": true }
        });

        merge_object(&mut target, &patch);

        assert_eq!(target["ServerName"], json!("movie-rust-edge"));
        assert_eq!(target["EncodingOptions"]["Hardware"], json!("qsv"));
        assert_eq!(target["EncodingOptions"]["Preset"], json!("slow"));
        assert_eq!(target["EncodingOptions"]["EnableTonemapping"], json!(true));
        assert_eq!(target["RemoteAccess"]["Enabled"], json!(true));
        assert_eq!(target["Plugins"]["Tmdb"], json!(true));
    }

    #[test]
    fn merge_object_overwrites_non_object_values() {
        let mut target = json!({ "MaxStreamingBitrate": 1_000_000 });
        merge_object(&mut target, &json!({ "MaxStreamingBitrate": 5_000_000 }));
        assert_eq!(target["MaxStreamingBitrate"], json!(5_000_000));
    }

    #[test]
    fn system_router_builds_with_new_configuration_endpoints() {
        let _router = super::router();
    }
}
