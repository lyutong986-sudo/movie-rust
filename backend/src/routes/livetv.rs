use crate::{auth, auth::AuthSession, error::AppError, repository, state::AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/LiveTv/Info", get(live_tv_info))
        .route("/LiveTv/Channels", get(empty_query_result))
        .route("/LiveTv/Programs", get(empty_query_result).post(empty_query_result_post))
        .route("/LiveTv/Recordings", get(empty_query_result))
        .route("/LiveTv/Recordings/Series", get(empty_query_result))
        .route("/LiveTv/Timers", get(empty_query_result).post(no_content_admin))
        .route("/LiveTv/Timers/Defaults", get(timer_defaults))
        .route("/LiveTv/Timers/{id}", get(timer_by_id).post(no_content_admin))
        .route("/LiveTv/Timers/{id}/Delete", post(no_content_admin))
        .route("/LiveTv/SeriesTimers", get(empty_query_result).post(no_content_admin))
        .route("/LiveTv/SeriesTimers/{id}", get(timer_by_id).post(no_content_admin))
        .route("/LiveTv/SeriesTimers/{id}/Delete", post(no_content_admin))
        .route("/LiveTv/TunerHosts", get(tuner_hosts).post(update_tuner_host))
        .route("/LiveTv/TunerHosts/Types", get(tuner_host_types))
        .route("/LiveTv/TunerHosts/Default/{kind}", get(default_tuner_host))
        .route("/LiveTv/TunerHosts/Delete", post(delete_tuner_host))
        .route("/LiveTv/Tuners/Discover", get(empty_vec))
        .route("/LiveTv/Tuners/Discvover", get(empty_vec))
        .route("/LiveTv/Tuners/{id}/Reset", post(no_content_admin))
        .route("/LiveTv/ListingProviders", get(listing_providers).post(update_listing_provider))
        .route("/LiveTv/ListingProviders/Available", get(available_listing_providers))
        .route("/LiveTv/ListingProviders/Default", get(default_listing_provider))
        .route("/LiveTv/ListingProviders/Delete", post(delete_listing_provider))
        .route("/LiveTv/ListingProviders/Lineups", get(empty_vec))
        .route("/LiveTv/ListingProviders/SchedulesDirect/Countries", get(empty_vec))
        .route("/LiveTv/ChannelMappings", get(empty_vec).post(no_content_admin).put(no_content_admin))
        .route("/LiveTv/ChannelMappingOptions", get(channel_mapping_options).post(no_content_admin).put(no_content_admin))
        .route("/LiveTv/Registration", get(live_tv_registration))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct OptionalIdQuery {
    #[serde(default, alias = "id")]
    id: Option<String>,
}

async fn live_tv_info(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let is_enabled = config
        .get("EnableLiveTv")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(Json(json!({
        "IsEnabled": is_enabled,
        "EnabledUsers": [],
        "TunerHosts": live_tv_lines(&config, "LiveTvTunerHostsText"),
        "ListingProviders": live_tv_lines(&config, "LiveTvListingProvidersText")
    })))
}

async fn tuner_hosts(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    Ok(Json(
        live_tv_lines(&config, "LiveTvTunerHostsText")
            .into_iter()
            .map(|line| tuner_host_from_line(&line))
            .collect(),
    ))
}

async fn update_tuner_host(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    append_live_tv_line(&state, "LiveTvTunerHostsText", live_tv_name(&payload)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tuner_host(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<OptionalIdQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    remove_live_tv_line(&state, "LiveTvTunerHostsText", query.id.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn tuner_host_types(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![
        json!({ "Name": "M3U", "Type": "m3u" }),
        json!({ "Name": "HDHomeRun", "Type": "hdhomerun" }),
        json!({ "Name": "SAT>IP", "Type": "satip" }),
    ])
}

async fn default_tuner_host(
    _session: AuthSession,
    Path(kind): Path<String>,
) -> Json<Value> {
    Json(json!({
        "Id": "",
        "Type": kind,
        "DeviceId": "",
        "FriendlyName": "",
        "Url": "",
        "ImportFavoritesOnly": false,
        "AllowHWTranscoding": true,
        "EnableStreamLooping": false,
        "Source": "Movie Rust"
    }))
}

async fn listing_providers(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    Ok(Json(
        live_tv_lines(&config, "LiveTvListingProvidersText")
            .into_iter()
            .map(|line| listing_provider_from_line(&line))
            .collect(),
    ))
}

async fn update_listing_provider(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    append_live_tv_line(&state, "LiveTvListingProvidersText", live_tv_name(&payload)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_listing_provider(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<OptionalIdQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    remove_live_tv_line(&state, "LiveTvListingProvidersText", query.id.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn available_listing_providers(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![
        json!({ "Name": "XMLTV", "Type": "xmltv", "Id": "xmltv" }),
        json!({ "Name": "Schedules Direct", "Type": "schedulesdirect", "Id": "schedulesdirect" }),
    ])
}

async fn default_listing_provider(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Name": "",
        "SetupUrl": "",
        "Id": "",
        "Type": "xmltv",
        "Path": "",
        "EnableAllTuners": true,
        "EnabledTuners": [],
        "ChannelMappings": []
    }))
}

async fn channel_mapping_options(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "TunerChannels": [],
        "ProviderChannels": [],
        "Mappings": []
    }))
}

async fn live_tv_registration(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "IsValid": false,
        "RequiresUnlock": false,
        "Status": "Unavailable"
    }))
}

async fn timer_defaults(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "PrePaddingSeconds": 0,
        "PostPaddingSeconds": 0,
        "IsPrePaddingRequired": false,
        "IsPostPaddingRequired": false,
        "KeepUntil": "UntilDeleted",
        "Priority": 0
    }))
}

async fn timer_by_id(_session: AuthSession, Path(id): Path<String>) -> Json<Value> {
    Json(json!({ "Id": id }))
}

async fn empty_vec(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn empty_query_result(_session: AuthSession) -> Json<Value> {
    Json(query_result(Vec::new()))
}

async fn empty_query_result_post(_session: AuthSession) -> Json<Value> {
    Json(query_result(Vec::new()))
}

async fn no_content_admin(session: AuthSession) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    Ok(StatusCode::NO_CONTENT)
}

fn query_result(items: Vec<Value>) -> Value {
    json!({
        "Items": items,
        "TotalRecordCount": 0,
        "StartIndex": 0
    })
}

fn live_tv_lines(config: &Value, key: &str) -> Vec<String> {
    config
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn live_tv_name(payload: &Value) -> String {
    payload
        .get("Name")
        .or_else(|| payload.get("FriendlyName"))
        .or_else(|| payload.get("Url"))
        .or_else(|| payload.get("Path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Live TV Source")
        .to_string()
}

fn tuner_host_from_line(line: &str) -> Value {
    json!({
        "Id": line,
        "FriendlyName": line,
        "Type": "m3u",
        "Url": line,
        "Source": "Movie Rust"
    })
}

fn listing_provider_from_line(line: &str) -> Value {
    json!({
        "Id": line,
        "Name": line,
        "Type": "xmltv",
        "Path": line,
        "EnableAllTuners": true,
        "EnabledTuners": [],
        "ChannelMappings": []
    })
}

async fn append_live_tv_line(state: &AppState, key: &str, value: String) -> Result<(), AppError> {
    let mut config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| AppError::Internal("server configuration is not an object".to_string()))?;
    let mut lines = live_tv_lines(&Value::Object(object.clone()), key);
    if !lines.iter().any(|line| line == &value) {
        lines.push(value);
    }
    object.insert(key.to_string(), json!(lines.join("\n")));
    repository::update_server_configuration_value(&state.pool, &state.config, config).await
}

async fn remove_live_tv_line(state: &AppState, key: &str, id: Option<&str>) -> Result<(), AppError> {
    let Some(id) = id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let mut config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| AppError::Internal("server configuration is not an object".to_string()))?;
    let mut lines = live_tv_lines(&Value::Object(object.clone()), key);
    lines.retain(|line| line != id);
    object.insert(key.to_string(), json!(lines.join("\n")));
    repository::update_server_configuration_value(&state.pool, &state.config, config).await
}
