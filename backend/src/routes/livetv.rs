use crate::{
    auth::AuthSession,
    error::AppError,
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/LiveTv/Info", get(live_tv_info))
        .route("/LiveTv/GuideInfo", get(live_tv_guide_info))
        .route("/LiveTv/Channels", get(live_tv_channels))
        .route("/LiveTv/Channels/{id}", get(live_tv_channel))
        .route("/LiveTv/Programs", get(live_tv_programs).post(live_tv_programs_post))
        .route("/LiveTv/Programs/Recommended", get(live_tv_programs_recommended))
        .route("/LiveTv/Programs/{id}", get(live_tv_program))
        .route("/LiveTv/Recordings", get(live_tv_recordings))
        .route("/LiveTv/Recordings/Folders", get(live_tv_recording_folders))
        .route("/LiveTv/Recordings/Series", get(live_tv_recording_series))
        .route("/LiveTv/Recordings/Groups", get(live_tv_recording_groups))
        .route("/LiveTv/Recordings/Groups/{id}", get(live_tv_recording_group))
        .route("/LiveTv/Recordings/{id}", get(live_tv_recording).delete(delete_live_tv_recording))
        .route("/LiveTv/Timers", get(live_tv_timers).post(create_live_tv_timer))
        .route("/LiveTv/Timers/Defaults", get(live_tv_timer_defaults))
        .route(
            "/LiveTv/Timers/{id}",
            get(live_tv_timer).post(update_live_tv_timer).delete(delete_live_tv_timer),
        )
        .route("/LiveTv/SeriesTimers", get(live_tv_series_timers).post(create_live_tv_series_timer))
        .route(
            "/LiveTv/SeriesTimers/{id}",
            get(live_tv_series_timer)
                .post(update_live_tv_series_timer)
                .delete(delete_live_tv_series_timer),
        )
        .route("/LiveTv/Tuners", get(live_tv_tuners))
        .route("/LiveTv/Tuners/Discvover", get(discover_tuners))
        .route("/LiveTv/Tuners/Discover", get(discover_tuners))
        .route("/LiveTv/Tuners/{id}/Reset", post(reset_tuner))
        .route("/LiveTv/TunerHosts", get(tuner_hosts).post(add_tuner_host).delete(delete_tuner_host))
        .route("/LiveTv/TunerHosts/Types", get(tuner_host_types))
        .route("/LiveTv/ChannelMappingOptions", get(channel_mapping_options))
        .route("/LiveTv/ChannelMappings", get(channel_mappings).post(update_channel_mappings))
        .route(
            "/LiveTv/ListingProviders",
            get(listing_providers).post(add_listing_provider).delete(delete_listing_provider),
        )
        .route("/LiveTv/ListingProviders/Default", get(default_listing_provider))
        .route("/LiveTv/ListingProviders/Lineups", get(listing_provider_lineups))
        .route(
            "/LiveTv/ListingProviders/SchedulesDirect/Countries",
            get(schedules_direct_countries),
        )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemQuery {
    #[serde(default)]
    id: Option<String>,
}

async fn live_tv_info(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "EnabledUsers": [],
        "EnabledFolders": [],
        "RecordingFolders": [],
        "IsEnabled": false,
        "IsAvailable": false
    }))
}

async fn live_tv_guide_info(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "StartDate": null,
        "EndDate": null
    }))
}

async fn live_tv_channels(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_channel(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Err(AppError::NotFound(format!("频道不存在: {id}")))
}

async fn live_tv_programs(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_programs_post(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_programs_recommended(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_program(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Err(AppError::NotFound(format!("节目不存在: {id}")))
}

async fn live_tv_recordings(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_recording_folders(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn live_tv_recording_series(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_recording_groups(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_recording_group(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Err(AppError::NotFound(format!("录制分组不存在: {id}")))
}

async fn live_tv_recording(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Err(AppError::NotFound(format!("录制不存在: {id}")))
}

async fn delete_live_tv_recording(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn live_tv_timers(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_timer_defaults(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "RecordAnyTime": false,
        "RecordAnyChannel": false,
        "KeepUntil": "UntilDeleted",
        "SkipEpisodesInLibrary": true,
        "PrePaddingSeconds": 0,
        "PostPaddingSeconds": 0
    }))
}

async fn live_tv_timer(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Err(AppError::NotFound(format!("定时录制不存在: {id}")))
}

async fn create_live_tv_timer(
    _session: AuthSession,
) -> Json<Value> {
    Json(json!({
        "Id": Uuid::new_v4().to_string(),
        "Status": "Created"
    }))
}

async fn update_live_tv_timer(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "Id": id,
        "Status": "Updated"
    }))
}

async fn delete_live_tv_timer(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn live_tv_series_timers(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn live_tv_series_timer(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    Err(AppError::NotFound(format!("剧集录制不存在: {id}")))
}

async fn create_live_tv_series_timer(
    _session: AuthSession,
) -> Json<Value> {
    Json(json!({
        "Id": Uuid::new_v4().to_string(),
        "Status": "Created"
    }))
}

async fn update_live_tv_series_timer(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "Id": id,
        "Status": "Updated"
    }))
}

async fn delete_live_tv_series_timer(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn live_tv_tuners(_session: AuthSession) -> Json<Value> {
    empty_query_result()
}

async fn discover_tuners(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn reset_tuner(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn tuner_hosts(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(livetv_config_array(&state, "TunerHosts").await?))
}

async fn tuner_host_types(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![
        json!({
            "Id": "m3u",
            "Name": "M3U",
            "SetupUrl": "livetvtuner.html?type=m3u"
        }),
        json!({
            "Id": "hdhomerun",
            "Name": "HDHomeRun",
            "SetupUrl": "livetvtuner.html?type=hdhomerun"
        }),
    ])
}

async fn channel_mapping_options(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "TunerChannels": [],
        "ProviderChannels": [],
        "Mappings": []
    }))
}

async fn channel_mappings(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn update_channel_mappings(_session: AuthSession) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn add_tuner_host(
    _session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    update_livetv_array(&state, "TunerHosts", payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tuner_host(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemQuery>,
) -> Result<StatusCode, AppError> {
    remove_livetv_array_item(&state, "TunerHosts", query.id.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_listing_provider(
    _session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    update_livetv_array(&state, "ListingProviders", payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn listing_providers(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(livetv_config_array(&state, "ListingProviders").await?))
}

async fn default_listing_provider(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Id": null,
        "Type": "xmltv",
        "Username": "",
        "Password": "",
        "ListingsId": "",
        "ZipCode": "",
        "Country": "",
        "Path": "",
        "EnabledTuners": [],
        "EnableAllTuners": true
    }))
}

async fn listing_provider_lineups(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn schedules_direct_countries(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![
        json!({"Name": "United States", "Value": "USA"}),
        json!({"Name": "Canada", "Value": "CAN"}),
    ])
}

async fn delete_listing_provider(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemQuery>,
) -> Result<StatusCode, AppError> {
    remove_livetv_array_item(&state, "ListingProviders", query.id.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn update_livetv_array(state: &AppState, key: &str, mut payload: Value) -> Result<(), AppError> {
    let mut current = repository::named_system_configuration(&state.pool, "livetv")
        .await?
        .unwrap_or_else(|| {
            json!({
                "TunerHosts": [],
                "ListingProviders": []
            })
        });

    let Some(map) = current.as_object_mut() else {
        return Err(AppError::Internal("livetv 配置格式无效".to_string()));
    };

    if payload.get("Id").is_none() {
        payload["Id"] = json!(Uuid::new_v4().to_string());
    }

    let items = map
        .entry(key.to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| AppError::Internal(format!("{key} 配置格式无效")))?;
    items.push(payload);

    repository::update_named_system_configuration(&state.pool, "livetv", &current).await
}

async fn remove_livetv_array_item(
    state: &AppState,
    key: &str,
    id: Option<&str>,
) -> Result<(), AppError> {
    let Some(id) = id.filter(|value| !value.trim().is_empty()) else {
        return Ok(());
    };

    let mut current = repository::named_system_configuration(&state.pool, "livetv")
        .await?
        .unwrap_or_else(|| {
            json!({
                "TunerHosts": [],
                "ListingProviders": []
            })
        });

    let Some(map) = current.as_object_mut() else {
        return Err(AppError::Internal("livetv 配置格式无效".to_string()));
    };
    let items = map
        .entry(key.to_string())
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or_else(|| AppError::Internal(format!("{key} 配置格式无效")))?;
    items.retain(|item| item.get("Id").and_then(Value::as_str) != Some(id));

    repository::update_named_system_configuration(&state.pool, "livetv", &current).await
}

async fn livetv_config_array(state: &AppState, key: &str) -> Result<Vec<Value>, AppError> {
    let current = repository::named_system_configuration(&state.pool, "livetv")
        .await?
        .unwrap_or_else(|| {
            json!({
                "TunerHosts": [],
                "ListingProviders": []
            })
        });
    Ok(current
        .get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

fn empty_query_result() -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0,
        "StartIndex": 0
    }))
}
