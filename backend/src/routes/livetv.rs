use crate::{
    auth,
    auth::AuthSession,
    error::AppError,
    models::MediaSourceDto,
    repository, scanner,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Timelike, Utc};
use quick_xml::{events::Event, Reader};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path as FsPath, PathBuf},
    process::Stdio,
    sync::OnceLock,
};
use tokio::{
    process::Command,
    sync::Mutex,
    time::{sleep, Duration as TokioDuration},
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/LiveTv/Info", get(live_tv_info))
        .route("/LiveTv/GuideInfo", get(live_tv_guide_info))
        .route("/LiveTv/Channels", get(live_tv_channels))
        .route("/LiveTv/Programs", get(live_tv_programs).post(live_tv_programs_post))
        .route("/LiveTv/Programs/Recommended", get(live_tv_programs))
        .route("/LiveTv/Programs/{id}", get(live_tv_program_by_id))
        .route("/LiveTv/Recordings", get(live_tv_recordings))
        .route("/LiveTv/Recordings/Series", get(live_tv_recording_series))
        .route("/LiveTv/Recordings/Groups", get(live_tv_recording_groups))
        .route("/LiveTv/Recordings/Groups/{id}", get(live_tv_recording_group_by_id))
        .route("/LiveTv/Recordings/{id}", get(live_tv_recording_by_id).delete(delete_live_tv_recording))
        .route("/LiveTv/Timers", get(live_tv_timers).post(create_live_tv_timer))
        .route("/LiveTv/Timers/Defaults", get(timer_defaults))
        .route("/LiveTv/Timers/{id}", get(live_tv_timer_by_id).post(update_live_tv_timer).delete(delete_live_tv_timer))
        .route("/LiveTv/Timers/{id}/Delete", post(delete_live_tv_timer_post))
        .route("/LiveTv/SeriesTimers", get(live_tv_series_timers).post(create_live_tv_series_timer))
        .route("/LiveTv/SeriesTimers/{id}", get(live_tv_series_timer_by_id).post(update_live_tv_series_timer).delete(delete_live_tv_series_timer))
        .route("/LiveTv/SeriesTimers/{id}/Delete", post(delete_live_tv_series_timer_post))
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
        .route("/LiveTv/ListingProviders/Lineups", get(listing_provider_lineups))
        .route("/LiveTv/ListingProviders/SchedulesDirect/Countries", get(empty_vec))
        .route("/LiveTv/ChannelMappings", get(channel_mappings).post(no_content_admin).put(no_content_admin))
        .route("/LiveTv/ChannelMappingOptions", get(channel_mapping_options).post(no_content_admin).put(no_content_admin))
        .route("/LiveTv/Registration", get(live_tv_registration))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct OptionalIdQuery {
    #[serde(default, alias = "id")]
    id: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase")]
struct LiveTvChannelQuery {
    #[serde(default, alias = "userId", alias = "UserId")]
    user_id: Option<String>,
    #[serde(default, alias = "startIndex", alias = "StartIndex")]
    start_index: Option<usize>,
    #[serde(default, alias = "limit", alias = "Limit")]
    limit: Option<usize>,
    #[serde(default, alias = "sortBy", alias = "SortBy")]
    sort_by: Option<String>,
    #[serde(default, alias = "sortOrder", alias = "SortOrder")]
    sort_order: Option<String>,
    #[serde(default, alias = "addCurrentProgram", alias = "AddCurrentProgram")]
    add_current_program: Option<bool>,
    #[serde(default, alias = "isMovie", alias = "IsMovie")]
    is_movie: Option<bool>,
    #[serde(default, alias = "isSports", alias = "IsSports")]
    is_sports: Option<bool>,
    #[serde(default, alias = "isKids", alias = "IsKids")]
    is_kids: Option<bool>,
    #[serde(default, alias = "isNews", alias = "IsNews")]
    is_news: Option<bool>,
    #[serde(default, alias = "isSeries", alias = "IsSeries")]
    is_series: Option<bool>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase")]
struct LiveTvProgramsQuery {
    #[serde(default, alias = "userId", alias = "UserId")]
    user_id: Option<String>,
    #[serde(default, alias = "channelIds", alias = "ChannelIds")]
    channel_ids: Option<Value>,
    #[serde(default, alias = "startIndex", alias = "StartIndex")]
    start_index: Option<usize>,
    #[serde(default, alias = "limit", alias = "Limit")]
    limit: Option<usize>,
    #[serde(default, alias = "minEndDate", alias = "MinEndDate")]
    min_end_date: Option<String>,
    #[serde(default, alias = "maxStartDate", alias = "MaxStartDate")]
    max_start_date: Option<String>,
    #[serde(default, alias = "sortBy", alias = "SortBy")]
    sort_by: Option<String>,
    #[serde(default, alias = "sortOrder", alias = "SortOrder")]
    sort_order: Option<String>,
    #[serde(default, alias = "isAiring", alias = "IsAiring")]
    is_airing: Option<bool>,
    #[serde(default, alias = "isMovie", alias = "IsMovie")]
    is_movie: Option<bool>,
    #[serde(default, alias = "isSports", alias = "IsSports")]
    is_sports: Option<bool>,
    #[serde(default, alias = "isKids", alias = "IsKids")]
    is_kids: Option<bool>,
    #[serde(default, alias = "isNews", alias = "IsNews")]
    is_news: Option<bool>,
    #[serde(default, alias = "isSeries", alias = "IsSeries")]
    is_series: Option<bool>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase")]
struct LiveTvTimersQuery {
    #[serde(default, alias = "channelId", alias = "ChannelId")]
    channel_id: Option<String>,
    #[serde(default, alias = "seriesTimerId", alias = "SeriesTimerId")]
    series_timer_id: Option<String>,
    #[serde(default, alias = "isActive", alias = "IsActive")]
    is_active: Option<bool>,
    #[serde(default, alias = "userId", alias = "UserId")]
    user_id: Option<String>,
    #[serde(default, alias = "startIndex", alias = "StartIndex")]
    start_index: Option<usize>,
    #[serde(default, alias = "limit", alias = "Limit")]
    limit: Option<usize>,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase")]
struct LiveTvRecordingsQuery {
    #[serde(default, alias = "isActive", alias = "IsActive")]
    is_active: Option<bool>,
    #[serde(default, alias = "isMovie", alias = "IsMovie")]
    is_movie: Option<bool>,
    #[serde(default, alias = "isSports", alias = "IsSports")]
    is_sports: Option<bool>,
    #[serde(default, alias = "isKids", alias = "IsKids")]
    is_kids: Option<bool>,
    #[serde(default, alias = "isSeries", alias = "IsSeries")]
    is_series: Option<bool>,
    #[serde(default, alias = "groupId", alias = "GroupId")]
    group_id: Option<String>,
    #[serde(default, alias = "startIndex", alias = "StartIndex")]
    start_index: Option<usize>,
    #[serde(default, alias = "limit", alias = "Limit")]
    limit: Option<usize>,
}

#[derive(Debug, Clone)]
struct TunerHost {
    value: Value,
}

#[derive(Debug, Clone)]
struct ListingProvider {
    value: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct LiveTvChannel {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) number: String,
    pub(crate) stream_url: String,
    pub(crate) logo_url: Option<String>,
    pub(crate) source_type: String,
    pub(crate) source_id: String,
    pub(crate) tvg_id: Option<String>,
}

#[derive(Debug, Clone)]
struct XmlTvProgram {
    id: String,
    channel_key: String,
    title: String,
    overview: Option<String>,
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
    categories: Vec<String>,
    episode_title: Option<String>,
    icon_url: Option<String>,
    is_repeat: bool,
}

#[derive(Debug, Clone)]
struct LiveTvProgram {
    id: String,
    channel_id: String,
    channel_name: String,
    title: String,
    overview: Option<String>,
    start_date: DateTime<Utc>,
    end_date: DateTime<Utc>,
    categories: Vec<String>,
    episode_title: Option<String>,
    image_url: Option<String>,
    is_repeat: bool,
}

#[derive(Debug, Clone)]
struct StoredTimer {
    value: Value,
}

#[derive(Debug, Clone)]
struct StoredSeriesTimer {
    value: Value,
}

#[derive(Debug, Clone)]
struct StoredRecording {
    value: Value,
}

#[derive(Debug, Clone, Default)]
struct XmlTvChannelMeta {
    display_name: Option<String>,
    icon_url: Option<String>,
}

#[derive(Debug, Clone)]
struct RecordingTarget {
    library_name: String,
    library_path: PathBuf,
    collection_type: String,
    file_path: PathBuf,
}

const LIVE_TV_TIMERS_KEY: &str = "livetv:timers";
const LIVE_TV_SERIES_TIMERS_KEY: &str = "livetv:series_timers";
const LIVE_TV_RECORDINGS_KEY: &str = "livetv:recordings";

async fn live_tv_info(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(&state, &tuner_hosts, &listing_providers).await?;

    let tuner_lines = tuner_hosts
        .iter()
        .map(|host| live_tv_name(&host.value))
        .collect::<Vec<_>>();
    let listing_lines = listing_providers
        .iter()
        .map(|provider| live_tv_name(&provider.value))
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "IsEnabled": config.get("EnableLiveTv").and_then(Value::as_bool).unwrap_or(false),
        "EnabledUsers": [],
        "TunerHosts": tuner_lines,
        "ListingProviders": listing_lines,
        "ConfiguredChannelCount": channels.len()
    })))
}

async fn live_tv_guide_info(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(&state, &tuner_hosts, &listing_providers).await?;
    let programs = load_live_tv_programs(&state, &channels, &listing_providers).await?;

    let guide_days = livetv
        .get("GuideDays")
        .and_then(Value::as_i64)
        .unwrap_or(14)
        .max(1);
    let start = Utc::now();
    let end = start + Duration::days(guide_days);
    let current = Utc::now();
    let active_programs = programs
        .iter()
        .filter(|program| program.start_date <= current && program.end_date > current)
        .count();

    Ok(Json(json!({
        "StartDate": start,
        "EndDate": end,
        "GuideDays": guide_days,
        "ChannelCount": channels.len(),
        "ProgramCount": programs.len(),
        "ActiveProgramCount": active_programs,
        "ListingProviderCount": listing_providers.len(),
        "TunerHosts": tuner_hosts.iter().map(|item| item.value.clone()).collect::<Vec<_>>(),
        "ListingProviders": listing_providers.iter().map(|item| item.value.clone()).collect::<Vec<_>>(),
        "EnableGuideProviderCache": livetv
            .get("EnableGuideProviderCache")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        "SupportsFullRefresh": true,
        "LastUpdated": Utc::now()
    })))
}

async fn live_tv_channels(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LiveTvChannelQuery>,
) -> Result<Json<Value>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(&state, &tuner_hosts, &listing_providers).await?;
    let programs = if query_needs_programs(&query) {
        load_live_tv_programs(&state, &channels, &listing_providers).await?
    } else {
        Vec::new()
    };

    let mut items = channels
        .iter()
        .filter(|channel| channel_matches_query(channel, &programs, &query))
        .map(|channel| {
            let current_program = if query.add_current_program.unwrap_or(false) {
                current_program_for_channel(channel, &programs)
            } else {
                None
            };
            channel_to_item(channel, current_program.as_ref())
        })
        .collect::<Vec<_>>();

    sort_channel_items(&mut items, query.sort_by.as_deref(), query.sort_order.as_deref());
    Ok(Json(paginate_items(items, query.start_index, query.limit)))
}

async fn live_tv_programs(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LiveTvProgramsQuery>,
) -> Result<Json<Value>, AppError> {
    live_tv_programs_impl(state, query).await
}

async fn live_tv_programs_post(
    _session: AuthSession,
    State(state): State<AppState>,
    Json(query): Json<LiveTvProgramsQuery>,
) -> Result<Json<Value>, AppError> {
    live_tv_programs_impl(state, query).await
}

async fn live_tv_programs_impl(
    state: AppState,
    query: LiveTvProgramsQuery,
) -> Result<Json<Value>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(&state, &tuner_hosts, &listing_providers).await?;
    let mut programs = load_live_tv_programs(&state, &channels, &listing_providers).await?;
    let allowed_channels = channel_ids_from_value(query.channel_ids.as_ref());
    let now = Utc::now();
    let min_end = parse_live_tv_datetime(query.min_end_date.as_deref()).unwrap_or(now - Duration::days(1));
    let max_start = parse_live_tv_datetime(query.max_start_date.as_deref()).unwrap_or(now + Duration::days(14));

    programs.retain(|program| {
        if !allowed_channels.is_empty() && !allowed_channels.contains(&program.channel_id) {
            return false;
        }
        if program.end_date < min_end || program.start_date > max_start {
            return false;
        }
        if let Some(is_airing) = query.is_airing {
            let airing = program.start_date <= now && program.end_date > now;
            if airing != is_airing {
                return false;
            }
        }
        matches_program_flags(program, &query)
    });

    sort_programs(&mut programs, query.sort_by.as_deref(), query.sort_order.as_deref());
    let items = programs.into_iter().map(program_to_item).collect::<Vec<_>>();
    Ok(Json(paginate_items(items, query.start_index, query.limit)))
}

async fn live_tv_program_by_id(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(&state, &tuner_hosts, &listing_providers).await?;
    let programs = load_live_tv_programs(&state, &channels, &listing_providers).await?;
    let program = programs
        .into_iter()
        .find(|program| program.id.eq_ignore_ascii_case(&id))
        .ok_or_else(|| AppError::NotFound("鑺傜洰涓嶅瓨鍦?".to_string()))?;
    Ok(Json(program_to_item(program)))
}

async fn live_tv_timers(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LiveTvTimersQuery>,
) -> Result<Json<Value>, AppError> {
    let timers = list_stored_timers(&state.pool).await?;
    let now = Utc::now();
    let mut items = timers
        .into_iter()
        .filter(|timer| timer_matches_query(timer, &query, now))
        .map(|timer| timer.value)
        .collect::<Vec<_>>();
    items.sort_by(|left, right| live_tv_value_date(left, "StartDate").cmp(&live_tv_value_date(right, "StartDate")));
    Ok(Json(paginate_items(items, query.start_index, query.limit)))
}

async fn create_live_tv_timer(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let normalized = normalize_timer(payload, None)?;
    let saved = save_timer(&state.pool, normalized.clone()).await?;
    Ok(Json(saved))
}

async fn live_tv_timer_by_id(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let timer = get_timer(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound("褰曞埗瀹氭椂鍣ㄤ笉瀛樺湪".to_string()))?;
    Ok(Json(timer))
}

async fn update_live_tv_timer(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let normalized = normalize_timer(payload, Some(id))?;
    let saved = save_timer(&state.pool, normalized.clone()).await?;
    Ok(Json(saved))
}

async fn delete_live_tv_timer(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    delete_timer(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_live_tv_timer_post(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    delete_live_tv_timer(session, State(state), Path(id)).await
}

async fn live_tv_series_timers(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LiveTvTimersQuery>,
) -> Result<Json<Value>, AppError> {
    let mut items = list_stored_series_timers(&state.pool)
        .await?
        .into_iter()
        .filter(|timer| {
            query.channel_id.as_ref().is_none_or(|channel_id| {
                timer.value.get("ChannelId").and_then(Value::as_str) == Some(channel_id.as_str())
            })
        })
        .map(|timer| timer.value)
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.get("Name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(right.get("Name").and_then(Value::as_str).unwrap_or_default())
    });
    Ok(Json(paginate_items(items, query.start_index, query.limit)))
}

async fn create_live_tv_series_timer(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let normalized = normalize_series_timer(payload, None)?;
    let saved = save_series_timer(&state.pool, normalized.clone()).await?;
    Ok(Json(saved))
}

async fn live_tv_series_timer_by_id(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let timer = get_series_timer(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound("鍓ч泦褰曞埗瀹氭椂鍣ㄤ笉瀛樺湪".to_string()))?;
    Ok(Json(timer))
}

async fn update_live_tv_series_timer(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let normalized = normalize_series_timer(payload, Some(id))?;
    let saved = save_series_timer(&state.pool, normalized.clone()).await?;
    Ok(Json(saved))
}

async fn delete_live_tv_series_timer(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    delete_series_timer(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_live_tv_series_timer_post(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    delete_live_tv_series_timer(session, State(state), Path(id)).await
}

async fn live_tv_recordings(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LiveTvRecordingsQuery>,
) -> Result<Json<Value>, AppError> {
    let items = derived_recordings(&state, &query).await?;
    Ok(Json(paginate_items(items, query.start_index, query.limit)))
}

async fn live_tv_recording_series(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(_query): Query<LiveTvRecordingsQuery>,
) -> Result<Json<Value>, AppError> {
    let items = list_stored_series_timers(&state.pool)
        .await?
        .into_iter()
        .map(|series_timer| recording_series_item(&series_timer.value))
        .collect::<Vec<_>>();
    Ok(Json(query_result(items)))
}

async fn live_tv_recording_groups(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<LiveTvRecordingsQuery>,
) -> Result<Json<Value>, AppError> {
    let recordings = derived_recordings(&state, &query).await?;
    let mut groups = HashMap::<String, Vec<Value>>::new();
    for item in recordings {
        let key = item
            .get("GroupId")
            .and_then(Value::as_str)
            .unwrap_or("default")
            .to_string();
        groups.entry(key).or_default().push(item);
    }
    let items = groups
        .into_iter()
        .map(|(id, items)| {
            let name = items
                .first()
                .and_then(|item| item.get("SeriesName").or_else(|| item.get("Name")))
                .and_then(Value::as_str)
                .unwrap_or("Recordings")
                .to_string();
            json!({
                "Id": id,
                "Name": name,
                "Type": "RecordingGroup",
                "RecordingCount": items.len(),
                "Items": items
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(query_result(items)))
}

async fn live_tv_recording_group_by_id(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let mut query = LiveTvRecordingsQuery::default();
    query.group_id = Some(id.clone());
    let recordings = derived_recordings(&state, &query).await?;
    let name = recordings
        .first()
        .and_then(|item| item.get("SeriesName").or_else(|| item.get("Name")))
        .and_then(Value::as_str)
        .unwrap_or("Recordings");
    Ok(Json(json!({
        "Id": id,
        "Name": name,
        "Type": "RecordingGroup",
        "RecordingCount": recordings.len(),
        "Items": recordings
    })))
}

async fn live_tv_recording_by_id(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let query = LiveTvRecordingsQuery::default();
    let recording = derived_recordings(&state, &query)
        .await?
        .into_iter()
        .find(|item| item.get("Id").and_then(Value::as_str) == Some(id.as_str()))
        .ok_or_else(|| AppError::NotFound("褰曞埗涓嶅瓨鍦?".to_string()))?;
    Ok(Json(recording))
}

async fn delete_live_tv_recording(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    cancel_recording_task(&id).await;
    if let Some(recording) = get_recording(&state.pool, &id).await? {
        if let Some(process_id) = recording
            .get("ProcessId")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
        {
            let _ = terminate_recording_process(process_id).await;
        }
        if let Some(media_item_id) = recording
            .get("MediaItemId")
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
        {
            let _ = repository::delete_media_item(&state.pool, media_item_id).await;
        }
        if let Some(path) = recording.get("Path").and_then(Value::as_str) {
            let file_path = PathBuf::from(path);
            if file_path.exists() {
                let _ = tokio::fs::remove_file(&file_path).await;
            }
        }
        delete_recording(&state.pool, &id).await?;
    }
    delete_timer(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn tuner_hosts(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    Ok(Json(
        tuner_hosts_from_config(&livetv)
            .into_iter()
            .map(|item| item.value)
            .collect(),
    ))
}

async fn update_tuner_host(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let mut livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner = normalize_tuner_host(payload);
    upsert_live_tv_array(&mut livetv, "TunerHosts", tuner.clone())?;
    repository::update_named_configuration_value(&state.pool, "livetv", livetv.clone()).await?;
    sync_live_tv_text_fields(&state, &livetv).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_tuner_host(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<OptionalIdQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let Some(id) = query.id.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let mut livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    retain_live_tv_array(&mut livetv, "TunerHosts", |item| item.get("Id").and_then(Value::as_str) != Some(id))?;
    repository::update_named_configuration_value(&state.pool, "livetv", livetv.clone()).await?;
    sync_live_tv_text_fields(&state, &livetv).await?;
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
        "Source": "Movie Rust",
        "EnableTvgId": true
    }))
}

async fn listing_providers(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    Ok(Json(
        listing_providers_from_config(&livetv)
            .into_iter()
            .map(|item| item.value)
            .collect(),
    ))
}

async fn update_listing_provider(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let mut livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let provider = normalize_listing_provider(payload);
    upsert_live_tv_array(&mut livetv, "ListingProviders", provider.clone())?;
    repository::update_named_configuration_value(&state.pool, "livetv", livetv.clone()).await?;
    sync_live_tv_text_fields(&state, &livetv).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_listing_provider(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<OptionalIdQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let Some(id) = query.id.as_deref().map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(StatusCode::NO_CONTENT);
    };
    let mut livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    retain_live_tv_array(&mut livetv, "ListingProviders", |item| item.get("Id").and_then(Value::as_str) != Some(id))?;
    repository::update_named_configuration_value(&state.pool, "livetv", livetv.clone()).await?;
    sync_live_tv_text_fields(&state, &livetv).await?;
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
        "ChannelMappings": [],
        "MovieCategories": [],
        "SportsCategories": [],
        "KidsCategories": [],
        "NewsCategories": [],
        "MoviePrefix": ""
    }))
}

async fn listing_provider_lineups(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(&state, &tuner_hosts, &listing_providers).await?;
    Ok(Json(
        channels
            .into_iter()
            .map(|channel| {
                json!({
                    "Id": channel.id,
                    "Name": channel.name,
                    "Number": channel.number,
                    "TunerHostId": channel.source_id
                })
            })
            .collect(),
    ))
}

async fn channel_mappings(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let providers = listing_providers_from_config(&livetv);
    let mut mappings = Vec::new();
    for provider in providers {
        if let Some(items) = provider.value.get("ChannelMappings").and_then(Value::as_array) {
            mappings.extend(items.iter().cloned());
        }
    }
    Ok(Json(mappings))
}

async fn channel_mapping_options(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(&state, &tuner_hosts, &listing_providers).await?;
    let xmltv = load_xmltv_index(&state, &listing_providers).await?;

    Ok(Json(json!({
        "TunerChannels": channels.iter().map(|channel| {
            json!({
                "Id": channel.id,
                "Name": channel.name,
                "Number": channel.number,
                "TvgId": channel.tvg_id
            })
        }).collect::<Vec<_>>(),
        "ProviderChannels": xmltv.channels.iter().map(|(id, meta)| {
            json!({
                "Id": id,
                "Name": meta.display_name.clone().unwrap_or_else(|| id.clone())
            })
        }).collect::<Vec<_>>(),
        "Mappings": collect_provider_mappings(&listing_providers)
    })))
}

async fn live_tv_registration(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "IsValid": true,
        "RequiresUnlock": false,
        "Status": "Available"
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

async fn no_content_admin(session: AuthSession) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    Ok(StatusCode::NO_CONTENT)
}

fn active_recording_tasks() -> &'static Mutex<HashSet<String>> {
    static TASKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    TASKS.get_or_init(|| Mutex::new(HashSet::new()))
}

async fn cancel_recording_task(timer_id: &str) {
    cancelled_recording_tasks().lock().await.insert(timer_id.to_string());
}

async fn clear_recording_cancellation(timer_id: &str) {
    cancelled_recording_tasks().lock().await.remove(timer_id);
}

async fn recording_was_cancelled(timer_id: &str) -> bool {
    cancelled_recording_tasks().lock().await.contains(timer_id)
}

async fn register_recording_process(timer_id: &str, process_id: u32) {
    recording_processes()
        .lock()
        .await
        .insert(timer_id.to_string(), process_id);
}

async fn unregister_recording_process(timer_id: &str) {
    recording_processes().lock().await.remove(timer_id);
}

async fn terminate_recording_process(process_id: u32) -> Result<(), AppError> {
    #[cfg(target_os = "windows")]
    let status = Command::new("taskkill")
        .args(["/PID", &process_id.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    #[cfg(not(target_os = "windows"))]
    let status = Command::new("kill")
        .args(["-TERM", &process_id.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(AppError::Internal(format!("failed to stop recording process {process_id}")))
    }
}

fn recording_processes() -> &'static Mutex<HashMap<String, u32>> {
    static PROCESSES: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    PROCESSES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cancelled_recording_tasks() -> &'static Mutex<HashSet<String>> {
    static TASKS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    TASKS.get_or_init(|| Mutex::new(HashSet::new()))
}

pub fn start_recording_worker(state: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err(error) = poll_recording_jobs(&state).await {
                tracing::warn!(error = %error, "LiveTV 瑜版洖鍩楀銉ょ稊閸ｃ劏鐤嗙拠銏犮亼鐠?");
            }
            sleep(TokioDuration::from_secs(15)).await;
        }
    });
}

pub(crate) fn is_live_tv_channel_id(item_id: &str) -> bool {
    item_id.starts_with("livetv-channel-")
}

pub(crate) async fn recording_media_item_id(
    pool: &sqlx::PgPool,
    recording_id: &str,
) -> Result<Option<Uuid>, AppError> {
    Ok(get_recording(pool, recording_id)
        .await?
        .and_then(|recording| recording.get("MediaItemId").and_then(Value::as_str).map(ToOwned::to_owned))
        .and_then(|value| Uuid::parse_str(&value).ok()))
}

pub(crate) async fn find_live_tv_channel(
    state: &AppState,
    channel_id: &str,
) -> Result<Option<LiveTvChannel>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(state, &tuner_hosts, &listing_providers).await?;
    Ok(channels.into_iter().find(|channel| channel.id.eq_ignore_ascii_case(channel_id)))
}

pub(crate) fn build_live_tv_media_source(
    channel: &LiveTvChannel,
    access_token: &str,
    live_stream_id: Option<&str>,
) -> MediaSourceDto {
    let container = live_tv_stream_container(&channel.stream_url);
    let live_stream_id = live_stream_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let requires_opening = live_stream_id.is_none();
    let live_stream_id = live_stream_id.unwrap_or_else(|| format!("livestream-{}", uuid_from_seed(&channel.id)));
    let media_source_id = format!("mediasource_{}", channel.id);
    let path = format!(
        "/Videos/{}/stream.{}?Static=true&MediaSourceId={}&mediaSourceId={}&LiveStreamId={}&liveStreamId={}&api_key={}",
        channel.id,
        container,
        media_source_id,
        media_source_id,
        live_stream_id,
        live_stream_id,
        access_token
    );

    MediaSourceDto {
        chapters: Vec::new(),
        id: media_source_id,
        path,
        protocol: "Http".to_string(),
        source_type: "Default".to_string(),
        container,
        name: channel.name.clone(),
        sort_name: Some(channel.name.clone()),
        is_remote: false,
        encoder_path: None,
        encoder_protocol: None,
        probe_path: None,
        probe_protocol: None,
        has_mixed_protocols: Some(false),
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: false,
        direct_stream_url: None,
        formats: Vec::new(),
        size: None,
        e_tag: None,
        bitrate: None,
        default_audio_stream_index: None,
        default_subtitle_stream_index: None,
        run_time_ticks: None,
        container_start_time_ticks: None,
        is_infinite_stream: Some(true),
        requires_opening: Some(requires_opening),
        open_token: Some(format!("livetv-open:{}", channel.id)),
        requires_closing: Some(false),
        live_stream_id: Some(live_stream_id),
        buffer_ms: Some(3000),
        requires_looping: Some(false),
        supports_probing: Some(false),
        video_3d_format: None,
        timestamp: None,
        required_http_headers: std::collections::BTreeMap::new(),
        add_api_key_to_direct_stream_url: Some(false),
        transcoding_url: None,
        transcoding_sub_protocol: None,
        transcoding_container: None,
        analyze_duration_ms: None,
        read_at_native_framerate: Some(true),
        item_id: Some(channel.id.clone()),
        server_id: None,
        media_streams: Vec::new(),
    }
}

pub(crate) fn live_tv_stream_url(channel: &LiveTvChannel) -> &str {
    &channel.stream_url
}

async fn list_stored_timers(pool: &sqlx::PgPool) -> Result<Vec<StoredTimer>, AppError> {
    let value = repository::get_json_setting(pool, LIVE_TV_TIMERS_KEY)
        .await?
        .unwrap_or_else(|| json!([]));
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.is_object())
        .cloned()
        .map(|value| StoredTimer { value })
        .collect())
}

async fn list_stored_series_timers(pool: &sqlx::PgPool) -> Result<Vec<StoredSeriesTimer>, AppError> {
    let value = repository::get_json_setting(pool, LIVE_TV_SERIES_TIMERS_KEY)
        .await?
        .unwrap_or_else(|| json!([]));
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.is_object())
        .cloned()
        .map(|value| StoredSeriesTimer { value })
        .collect())
}

async fn save_timer(pool: &sqlx::PgPool, timer: Value) -> Result<Value, AppError> {
    let mut timers = list_stored_timers(pool).await?;
    let id = timer.get("Id").and_then(Value::as_str).unwrap_or_default();
    if let Some(existing) = timers
        .iter_mut()
        .find(|existing| existing.value.get("Id").and_then(Value::as_str) == Some(id))
    {
        existing.value = timer.clone();
    } else {
        timers.push(StoredTimer { value: timer.clone() });
    }
    repository::set_json_setting(
        pool,
        LIVE_TV_TIMERS_KEY,
        Value::Array(timers.into_iter().map(|timer| timer.value).collect()),
    )
    .await?;
    Ok(timer)
}

async fn save_series_timer(pool: &sqlx::PgPool, timer: Value) -> Result<Value, AppError> {
    let mut timers = list_stored_series_timers(pool).await?;
    let id = timer.get("Id").and_then(Value::as_str).unwrap_or_default();
    if let Some(existing) = timers
        .iter_mut()
        .find(|existing| existing.value.get("Id").and_then(Value::as_str) == Some(id))
    {
        existing.value = timer.clone();
    } else {
        timers.push(StoredSeriesTimer { value: timer.clone() });
    }
    repository::set_json_setting(
        pool,
        LIVE_TV_SERIES_TIMERS_KEY,
        Value::Array(timers.into_iter().map(|timer| timer.value).collect()),
    )
    .await?;
    Ok(timer)
}

async fn get_timer(pool: &sqlx::PgPool, id: &str) -> Result<Option<Value>, AppError> {
    Ok(list_stored_timers(pool)
        .await?
        .into_iter()
        .find(|timer| timer.value.get("Id").and_then(Value::as_str) == Some(id))
        .map(|timer| timer.value))
}

async fn get_series_timer(pool: &sqlx::PgPool, id: &str) -> Result<Option<Value>, AppError> {
    Ok(list_stored_series_timers(pool)
        .await?
        .into_iter()
        .find(|timer| timer.value.get("Id").and_then(Value::as_str) == Some(id))
        .map(|timer| timer.value))
}

async fn delete_timer(pool: &sqlx::PgPool, id: &str) -> Result<(), AppError> {
    let timers = list_stored_timers(pool)
        .await?
        .into_iter()
        .filter(|timer| timer.value.get("Id").and_then(Value::as_str) != Some(id))
        .map(|timer| timer.value)
        .collect::<Vec<_>>();
    repository::set_json_setting(pool, LIVE_TV_TIMERS_KEY, Value::Array(timers)).await
}

async fn delete_series_timer(pool: &sqlx::PgPool, id: &str) -> Result<(), AppError> {
    let timers = list_stored_series_timers(pool)
        .await?
        .into_iter()
        .filter(|timer| timer.value.get("Id").and_then(Value::as_str) != Some(id))
        .map(|timer| timer.value)
        .collect::<Vec<_>>();
    repository::set_json_setting(pool, LIVE_TV_SERIES_TIMERS_KEY, Value::Array(timers)).await
}

async fn list_stored_recordings(pool: &sqlx::PgPool) -> Result<Vec<StoredRecording>, AppError> {
    let value = repository::get_json_setting(pool, LIVE_TV_RECORDINGS_KEY)
        .await?
        .unwrap_or_else(|| json!([]));
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter(|item| item.is_object())
        .cloned()
        .map(|value| StoredRecording { value })
        .collect())
}

async fn get_recording(pool: &sqlx::PgPool, id: &str) -> Result<Option<Value>, AppError> {
    Ok(list_stored_recordings(pool)
        .await?
        .into_iter()
        .find(|recording| recording.value.get("Id").and_then(Value::as_str) == Some(id))
        .map(|recording| recording.value))
}

async fn save_recording(pool: &sqlx::PgPool, recording: Value) -> Result<Value, AppError> {
    let mut recordings = list_stored_recordings(pool).await?;
    let id = recording.get("Id").and_then(Value::as_str).unwrap_or_default();
    if let Some(existing) = recordings
        .iter_mut()
        .find(|existing| existing.value.get("Id").and_then(Value::as_str) == Some(id))
    {
        existing.value = recording.clone();
    } else {
        recordings.push(StoredRecording { value: recording.clone() });
    }
    repository::set_json_setting(
        pool,
        LIVE_TV_RECORDINGS_KEY,
        Value::Array(recordings.into_iter().map(|recording| recording.value).collect()),
    )
    .await?;
    Ok(recording)
}

async fn delete_recording(pool: &sqlx::PgPool, id: &str) -> Result<(), AppError> {
    let recordings = list_stored_recordings(pool)
        .await?
        .into_iter()
        .filter(|recording| recording.value.get("Id").and_then(Value::as_str) != Some(id))
        .map(|recording| recording.value)
        .collect::<Vec<_>>();
    repository::set_json_setting(pool, LIVE_TV_RECORDINGS_KEY, Value::Array(recordings)).await
}

async fn poll_recording_jobs(state: &AppState) -> Result<(), AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(state, &tuner_hosts, &listing_providers).await?;
    let programs = load_live_tv_programs(state, &channels, &listing_providers).await?;
    let channels_by_id = channels
        .into_iter()
        .map(|channel| (channel.id.clone(), channel))
        .collect::<HashMap<_, _>>();
    let programs_by_id = programs
        .into_iter()
        .map(|program| (program.id.clone(), program))
        .collect::<HashMap<_, _>>();
    let now = Utc::now();

    for timer in list_stored_timers(&state.pool).await? {
        let timer_id = timer
            .value
            .get("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if timer_id.is_empty() {
            continue;
        }

        let existing_recording = get_recording(&state.pool, &timer_id).await?;
        if existing_recording
            .as_ref()
            .and_then(|recording| recording.get("Status").and_then(Value::as_str))
            .is_some_and(|status| matches!(status, "Completed" | "Failed" | "Cancelled" | "Missed"))
        {
            continue;
        }

        let start = recording_window_start(&timer.value);
        let end = recording_window_end(&timer.value);
        if now >= end {
            if existing_recording.is_none() {
                save_recording(
                    &state.pool,
                    json!({
                        "Id": timer_id,
                        "TimerId": timer.value.get("Id").cloned().unwrap_or(Value::Null),
                        "Status": "Missed",
                        "StartDate": start,
                        "EndDate": end,
                        "DateLastUpdated": now
                    }),
                )
                .await?;
            }
            continue;
        }

        if now < start {
            continue;
        }

        let mut active = active_recording_tasks().lock().await;
        if !active.insert(timer_id.clone()) {
            continue;
        }
        drop(active);

        let Some(channel_id) = timer.value.get("ChannelId").and_then(Value::as_str) else {
            let mut active = active_recording_tasks().lock().await;
            active.remove(&timer_id);
            continue;
        };
        let Some(channel) = channels_by_id.get(channel_id).cloned() else {
            save_recording(
                &state.pool,
                json!({
                    "Id": timer_id,
                    "TimerId": timer.value.get("Id").cloned().unwrap_or(Value::Null),
                    "Status": "Failed",
                    "ErrorMessage": format!("Channel {channel_id} not found"),
                    "DateLastUpdated": now
                }),
            )
            .await?;
            let mut active = active_recording_tasks().lock().await;
            active.remove(timer.value.get("Id").and_then(Value::as_str).unwrap_or_default());
            continue;
        };
        let program = timer
            .value
            .get("ProgramId")
            .and_then(Value::as_str)
            .and_then(|program_id| programs_by_id.get(program_id).cloned());

        let state_clone = state.clone();
        let timer_value = timer.value.clone();
        tokio::spawn(async move {
            if let Err(error) = run_recording_job(state_clone.clone(), timer_value.clone(), channel, program).await {
                let timer_id = timer_value.get("Id").and_then(Value::as_str).unwrap_or_default().to_string();
                if !recording_was_cancelled(&timer_id).await {
                    let _ = save_recording(
                        &state_clone.pool,
                        merge_recording_state(
                            get_recording(&state_clone.pool, &timer_id)
                                .await
                                .ok()
                                .flatten()
                                .unwrap_or_else(|| json!({ "Id": timer_id })),
                            json!({
                                "Status": "Failed",
                                "ErrorMessage": error.to_string(),
                                "DateLastUpdated": Utc::now()
                            }),
                        ),
                    )
                    .await;
                }
            }
            let timer_id = timer_value.get("Id").and_then(Value::as_str).unwrap_or_default().to_string();
            let mut active = active_recording_tasks().lock().await;
            active.remove(&timer_id);
            unregister_recording_process(&timer_id).await;
            clear_recording_cancellation(&timer_id).await;
        });
    }

    Ok(())
}

async fn run_recording_job(
    state: AppState,
    timer: Value,
    channel: LiveTvChannel,
    program: Option<LiveTvProgram>,
) -> Result<(), AppError> {
    let timer_id = timer
        .get("Id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::BadRequest("Timer missing Id".to_string()))?
        .to_string();
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let target = resolve_recording_target(&state, &livetv, &timer, program.as_ref())?;
    let start = recording_window_start(&timer);
    let end = recording_window_end(&timer);
    let now = Utc::now();
    let duration = (end - now).num_seconds().max(5);

    if let Some(parent) = target.file_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let initial_state = merge_recording_state(
        get_recording(&state.pool, &timer_id)
            .await?
            .unwrap_or_else(|| json!({ "Id": timer_id })),
        json!({
            "TimerId": timer.get("Id").cloned().unwrap_or(Value::Null),
            "ProgramId": timer.get("ProgramId").cloned().unwrap_or(Value::Null),
            "SeriesTimerId": timer.get("SeriesTimerId").cloned().unwrap_or(Value::Null),
            "Name": timer.get("Name").cloned().unwrap_or_else(|| json!("Recording")),
            "ChannelId": timer.get("ChannelId").cloned().unwrap_or(Value::Null),
            "Status": "InProgress",
            "Path": target.file_path.to_string_lossy().to_string(),
            "LibraryPath": target.library_path.to_string_lossy().to_string(),
            "LibraryName": target.library_name,
            "CollectionType": target.collection_type,
            "ActualStartDate": now,
            "StartDate": start,
            "EndDate": end,
            "DateLastUpdated": now,
            "MediaItemId": Value::Null,
            "ErrorMessage": Value::Null
        }),
    );
    save_recording(&state.pool, initial_state).await?;

    let ffmpeg_args = vec![
        "-y".to_string(),
        "-nostdin".to_string(),
        "-reconnect".to_string(),
        "1".to_string(),
        "-reconnect_streamed".to_string(),
        "1".to_string(),
        "-reconnect_at_eof".to_string(),
        "1".to_string(),
        "-reconnect_delay_max".to_string(),
        "2".to_string(),
        "-i".to_string(),
        channel.stream_url.clone(),
        "-t".to_string(),
        duration.to_string(),
        "-c".to_string(),
        "copy".to_string(),
        "-map".to_string(),
        "0".to_string(),
        "-f".to_string(),
        "mpegts".to_string(),
        target.file_path.to_string_lossy().to_string(),
    ];

    tracing::info!(timer_id = %timer_id, channel = %channel.name, path = %target.file_path.display(), "LiveTV recording started");
    let mut child = Command::new(&state.config.ffmpeg_path)
        .args(&ffmpeg_args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| AppError::FfmpegError(format!("failed to start FFmpeg recording process: {error}")))?;
    if let Some(process_id) = child.id() {
        register_recording_process(&timer_id, process_id).await;
        save_recording(
            &state.pool,
            merge_recording_state(
                get_recording(&state.pool, &timer_id)
                    .await?
                    .unwrap_or_else(|| json!({ "Id": timer_id.clone() })),
                json!({
                    "ProcessId": process_id,
                    "DateLastUpdated": Utc::now()
                }),
            ),
        )
        .await?;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| AppError::FfmpegError(format!("FFmpeg recording process failed: {error}")))?;

    if recording_was_cancelled(&timer_id).await {
        let _ = tokio::fs::remove_file(&target.file_path).await;
        return Ok(());
    }
    if !output.status.success() {
        let error_text = String::from_utf8_lossy(&output.stderr).trim().to_string();
        save_recording(
            &state.pool,
            merge_recording_state(
                get_recording(&state.pool, &timer_id)
                    .await?
                    .unwrap_or_else(|| json!({ "Id": timer_id.clone() })),
                json!({
                    "Status": "Failed",
                    "ErrorMessage": if error_text.is_empty() { "ffmpeg exited with failure" } else { &error_text },
                    "DateLastUpdated": Utc::now()
                }),
            ),
        )
        .await?;
        return Err(AppError::FfmpegError(error_text));
    }

    ensure_recording_library(&state.pool, &target.library_name, &target.library_path, &target.collection_type).await?;
    let _ = scanner::scan_all_libraries(&state.pool, state.metadata_manager.as_deref()).await?;
    let media_item_id = repository::get_media_item_by_path(&state.pool, &target.file_path.to_string_lossy())
        .await?
        .map(|item| item.id.to_string());
    let file_size = tokio::fs::metadata(&target.file_path).await.ok().map(|meta| meta.len());
    save_recording(
        &state.pool,
        merge_recording_state(
            get_recording(&state.pool, &timer_id)
                .await?
                .unwrap_or_else(|| json!({ "Id": timer_id.clone() })),
            json!({
                "Status": "Completed",
                "Path": target.file_path.to_string_lossy().to_string(),
                "ActualEndDate": Utc::now(),
                "DateLastUpdated": Utc::now(),
                "FileSize": file_size,
                "MediaItemId": media_item_id
            }),
        ),
    )
    .await?;
    tracing::info!(timer_id = %timer_id, path = %target.file_path.display(), "LiveTV recording completed");
    Ok(())
}

async fn ensure_recording_library(
    pool: &sqlx::PgPool,
    name: &str,
    path: &FsPath,
    collection_type: &str,
) -> Result<(), AppError> {
    let canonical = path.to_string_lossy().to_string();
    let libraries = repository::list_libraries(pool).await?;
    if libraries.iter().any(|library| {
        repository::library_paths(library)
            .into_iter()
            .any(|library_path| library_path.eq_ignore_ascii_case(&canonical))
    }) {
        return Ok(());
    }
    repository::create_library(
        pool,
        name,
        collection_type,
        &[canonical],
        crate::models::LibraryOptionsDto::default(),
    )
    .await?;
    Ok(())
}

fn resolve_recording_target(
    state: &AppState,
    livetv: &Value,
    timer: &Value,
    program: Option<&LiveTvProgram>,
) -> Result<RecordingTarget, AppError> {
    let is_series = program.is_some_and(|program| program.has_flag("__series"));
    let is_movie = program.is_some_and(|program| program.has_flag("__movie"));
    let base_root = if is_series {
        livetv
            .get("SeriesRecordingPath")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                livetv
                    .get("RecordingPath")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| PathBuf::from(value).join("Series"))
            })
            .unwrap_or_else(|| PathBuf::from("data").join("recordings").join("Series"))
    } else if is_movie {
        livetv
            .get("MovieRecordingPath")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                livetv
                    .get("RecordingPath")
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .map(|value| PathBuf::from(value).join("Movies"))
            })
            .unwrap_or_else(|| PathBuf::from("data").join("recordings").join("Movies"))
    } else {
        livetv
            .get("RecordingPath")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(|value| PathBuf::from(value).join("Programs"))
            .unwrap_or_else(|| PathBuf::from("data").join("recordings").join("Programs"))
    };
    let library_path = if base_root.is_relative() {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(base_root)
    } else {
        base_root
    };
    let timer_name = timer.get("Name").and_then(Value::as_str).unwrap_or("Recording");
    let timer_start = recording_window_start(timer);
    let program_title = program
        .map(|value| value.title.as_str())
        .unwrap_or(timer_name);
    let safe_series = sanitize_recording_segment(timer_name);
    let safe_program = sanitize_recording_segment(program_title);
    let date_stamp = timer_start.date_naive().to_string();
    let time_stamp = format!(
        "{:02}-{:02}-{:02}",
        timer_start.hour(),
        timer_start.minute(),
        timer_start.second()
    );
    let (collection_type, library_name, relative_path) = if is_series {
        let series_name = if safe_series.is_empty() {
            String::from("Series Recording")
        } else {
            safe_series
        };
        (
            String::from("tvshows"),
            String::from("Live TV Series Recordings"),
            PathBuf::from(&series_name)
                .join(format!("{} {} {}.ts", series_name, date_stamp, safe_program)),
        )
    } else if is_movie {
        let name = if safe_program.is_empty() {
            String::from("Movie Recording")
        } else {
            safe_program
        };
        (
            String::from("movies"),
            String::from("Live TV Movie Recordings"),
            PathBuf::from(format!("{} {} {}.ts", name, date_stamp, time_stamp)),
        )
    } else {
        let name = if safe_program.is_empty() {
            String::from("Program Recording")
        } else {
            safe_program
        };
        (
            String::from("movies"),
            String::from("Live TV Program Recordings"),
            PathBuf::from(format!("{} {} {}.ts", name, date_stamp, time_stamp)),
        )
    };
    let file_path = library_path.join(relative_path);
    let _ = state;
    Ok(RecordingTarget {
        library_name,
        library_path,
        collection_type,
        file_path,
    })
}
fn sanitize_recording_segment(value: &str) -> String {
    let disallowed = ["<", ">", ":", "\"", "/", "\\", "|", "?", "*"];
    value
        .chars()
        .map(|ch| if disallowed.iter().any(|token| token.starts_with(ch)) { ' ' } else { ch })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn recording_window_start(timer: &Value) -> DateTime<Utc> {
    let start = live_tv_value_date(timer, "StartDate");
    let padding = timer
        .get("PrePaddingSeconds")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    start - Duration::seconds(padding)
}

fn recording_window_end(timer: &Value) -> DateTime<Utc> {
    let end = live_tv_value_date(timer, "EndDate");
    let padding = timer
        .get("PostPaddingSeconds")
        .and_then(Value::as_i64)
        .unwrap_or(0)
        .max(0);
    end + Duration::seconds(padding)
}

fn merge_recording_state(base: Value, overlay: Value) -> Value {
    let mut object = base.as_object().cloned().unwrap_or_default();
    if let Some(overlay_object) = overlay.as_object() {
        for (key, value) in overlay_object {
            object.insert(key.clone(), value.clone());
        }
    }
    Value::Object(object)
}

fn normalize_timer(mut payload: Value, forced_id: Option<String>) -> Result<Value, AppError> {
    let object = payload
        .as_object_mut()
        .ok_or_else(|| AppError::BadRequest("Timer payload must be an object".to_string()))?;
    let id = forced_id.unwrap_or_else(|| {
        object
            .get("Id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("livetv-timer-{}", Uuid::new_v4().simple()))
    });
    let start_date = object
        .get("StartDate")
        .and_then(Value::as_str)
        .and_then(|value| parse_live_tv_datetime(Some(value)))
        .unwrap_or_else(Utc::now);
    let end_date = object
        .get("EndDate")
        .and_then(Value::as_str)
        .and_then(|value| parse_live_tv_datetime(Some(value)))
        .unwrap_or_else(|| start_date + Duration::minutes(30));
    let channel_id = object
        .get("ChannelId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let program_id = object
        .get("ProgramId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let name = object
        .get("Name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Recording")
        .to_string();
    let status = if end_date <= Utc::now() {
        "Completed"
    } else if start_date <= Utc::now() {
        "InProgress"
    } else {
        "New"
    };
    object.insert("Id".to_string(), json!(id.clone()));
    object.insert("Type".to_string(), json!("Timer"));
    object.insert("ProgramId".to_string(), json!(program_id));
    object.insert("ChannelId".to_string(), json!(channel_id));
    object.insert("Name".to_string(), json!(name.clone()));
    object.insert("StartDate".to_string(), json!(start_date));
    object.insert("EndDate".to_string(), json!(end_date));
    object.insert("Status".to_string(), json!(status));
    object.entry("SeriesTimerId".to_string()).or_insert(Value::Null);
    object.entry("PrePaddingSeconds".to_string()).or_insert_with(|| json!(0));
    object.entry("PostPaddingSeconds".to_string()).or_insert_with(|| json!(0));
    object.entry("KeepUntil".to_string()).or_insert_with(|| json!("UntilDeleted"));
    object.entry("Priority".to_string()).or_insert_with(|| json!(0));
    object.entry("IsPostPaddingRequired".to_string()).or_insert_with(|| json!(false));
    object.entry("IsPrePaddingRequired".to_string()).or_insert_with(|| json!(false));
    object.entry("CreatedAt".to_string()).or_insert_with(|| json!(Utc::now()));
    Ok(payload)
}

fn normalize_series_timer(mut payload: Value, forced_id: Option<String>) -> Result<Value, AppError> {
    let object = payload
        .as_object_mut()
        .ok_or_else(|| AppError::BadRequest("SeriesTimer payload must be an object".to_string()))?;
    let id = forced_id.unwrap_or_else(|| {
        object
            .get("Id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| format!("livetv-seriestimer-{}", Uuid::new_v4().simple()))
    });
    let name = object
        .get("Name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Series Recording")
        .to_string();
    object.insert("Id".to_string(), json!(id));
    object.insert("Type".to_string(), json!("SeriesTimer"));
    object.insert("Name".to_string(), json!(name));
    object.entry("RecordAnyChannel".to_string()).or_insert_with(|| json!(true));
    object.entry("RecordAnyTime".to_string()).or_insert_with(|| json!(true));
    object.entry("RecordNewOnly".to_string()).or_insert_with(|| json!(false));
    object.entry("KeepUpTo".to_string()).or_insert_with(|| json!(0));
    object.entry("KeepUntil".to_string()).or_insert_with(|| json!("UntilDeleted"));
    object.entry("Priority".to_string()).or_insert_with(|| json!(0));
    object.entry("CreatedAt".to_string()).or_insert_with(|| json!(Utc::now()));
    Ok(payload)
}

fn timer_matches_query(timer: &StoredTimer, query: &LiveTvTimersQuery, now: DateTime<Utc>) -> bool {
    if query
        .channel_id
        .as_ref()
        .is_some_and(|channel_id| timer.value.get("ChannelId").and_then(Value::as_str) != Some(channel_id.as_str()))
    {
        return false;
    }
    if query
        .series_timer_id
        .as_ref()
        .is_some_and(|series_timer_id| timer.value.get("SeriesTimerId").and_then(Value::as_str) != Some(series_timer_id.as_str()))
    {
        return false;
    }
    if let Some(is_active) = query.is_active {
        let start = live_tv_value_date(&timer.value, "StartDate");
        let end = live_tv_value_date(&timer.value, "EndDate");
        let active = start <= now && end > now;
        if active != is_active {
            return false;
        }
    }
    true
}

async fn derived_recordings(state: &AppState, query: &LiveTvRecordingsQuery) -> Result<Vec<Value>, AppError> {
    let livetv = repository::named_configuration_value(&state.pool, "livetv").await?;
    let tuner_hosts = tuner_hosts_from_config(&livetv);
    let listing_providers = listing_providers_from_config(&livetv);
    let channels = load_live_tv_channels(state, &tuner_hosts, &listing_providers).await?;
    let programs = load_live_tv_programs(state, &channels, &listing_providers).await?;
    let channels_by_id = channels
        .iter()
        .map(|channel| (channel.id.clone(), channel))
        .collect::<HashMap<_, _>>();
    let programs_by_id = programs
        .iter()
        .map(|program| (program.id.clone(), program))
        .collect::<HashMap<_, _>>();
    let now = Utc::now();
    let recordings_by_id = list_stored_recordings(&state.pool)
        .await?
        .into_iter()
        .map(|recording| {
            (
                recording
                    .value
                    .get("Id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                recording.value,
            )
        })
        .collect::<HashMap<_, _>>();
    let mut items = Vec::new();

    for timer in list_stored_timers(&state.pool).await? {
        let recording = recording_item_from_timer(
            &timer.value,
            recordings_by_id
                .get(timer.value.get("Id").and_then(Value::as_str).unwrap_or_default()),
            &channels_by_id,
            &programs_by_id,
            now,
        );
        if recording_matches_query(&recording, query, now) {
            items.push(recording);
        }
    }

    items.sort_by(|left, right| live_tv_value_date(right, "StartDate").cmp(&live_tv_value_date(left, "StartDate")));
    Ok(items)
}

fn recording_item_from_timer(
    timer: &Value,
    recording_state: Option<&Value>,
    channels_by_id: &HashMap<String, &LiveTvChannel>,
    programs_by_id: &HashMap<String, &LiveTvProgram>,
    now: DateTime<Utc>,
) -> Value {
    let timer_id = timer.get("Id").and_then(Value::as_str).unwrap_or_default();
    let channel_id = timer.get("ChannelId").and_then(Value::as_str).unwrap_or_default();
    let program_id = timer.get("ProgramId").and_then(Value::as_str).unwrap_or_default();
    let program = programs_by_id.get(program_id);
    let channel = channels_by_id.get(channel_id);
    let start = recording_state
        .map(|value| live_tv_value_date(value, "StartDate"))
        .unwrap_or_else(|| live_tv_value_date(timer, "StartDate"));
    let end = recording_state
        .map(|value| live_tv_value_date(value, "EndDate"))
        .unwrap_or_else(|| live_tv_value_date(timer, "EndDate"));
    let status = recording_state
        .and_then(|value| value.get("Status").and_then(Value::as_str))
        .unwrap_or_else(|| {
            if end <= now {
                "Completed"
            } else if start <= now {
                "InProgress"
            } else {
                "New"
            }
        });
    let is_movie = program.is_some_and(|program| program.has_flag("__movie"));
    let is_sports = program.is_some_and(|program| program.has_flag("__sports"));
    let is_kids = program.is_some_and(|program| program.has_flag("__kids"));
    let is_series = program.is_some_and(|program| program.has_flag("__series"));
    let series_timer_id = timer.get("SeriesTimerId").and_then(Value::as_str);
    let group_id = series_timer_id
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("recording-group-{timer_id}"));
    json!({
        "Id": timer_id,
        "Type": "Recording",
        "Name": timer.get("Name").and_then(Value::as_str).unwrap_or("Recording"),
        "ChannelId": channel_id,
        "ChannelName": channel.map(|channel| channel.name.clone()),
        "ProgramId": program_id,
        "Overview": program.and_then(|program| program.overview.clone()),
        "StartDate": start,
        "EndDate": end,
        "Status": status,
        "IsMovie": is_movie,
        "IsSports": is_sports,
        "IsKids": is_kids,
        "IsSeries": is_series,
        "SeriesTimerId": series_timer_id,
        "SeriesName": timer.get("Name").and_then(Value::as_str),
        "GroupId": group_id,
        "ImageUrl": program.and_then(|program| program.image_url.clone()),
        "RunTimeTicks": (end - start).num_seconds().saturating_mul(10_000_000),
        "TimerId": timer_id,
        "Path": recording_state.and_then(|value| value.get("Path").cloned()).unwrap_or(Value::Null),
        "FileSize": recording_state.and_then(|value| value.get("FileSize").cloned()).unwrap_or(Value::Null),
        "MediaItemId": recording_state.and_then(|value| value.get("MediaItemId").cloned()).unwrap_or(Value::Null),
        "ErrorMessage": recording_state.and_then(|value| value.get("ErrorMessage").cloned()).unwrap_or(Value::Null)
    })
}

fn recording_matches_query(recording: &Value, query: &LiveTvRecordingsQuery, now: DateTime<Utc>) -> bool {
    let status = recording.get("Status").and_then(Value::as_str).unwrap_or("New");
    let start = live_tv_value_date(recording, "StartDate");
    let end = live_tv_value_date(recording, "EndDate");
    let active = start <= now && end > now;
    if status == "New" && !active {
        return false;
    }
    if query
        .group_id
        .as_ref()
        .is_some_and(|group_id| recording.get("GroupId").and_then(Value::as_str) != Some(group_id.as_str()))
    {
        return false;
    }
    if let Some(is_active) = query.is_active {
        if active != is_active {
            return false;
        }
    }
    if !option_matches(query.is_movie, recording.get("IsMovie").and_then(Value::as_bool).unwrap_or(false)) {
        return false;
    }
    if !option_matches(query.is_sports, recording.get("IsSports").and_then(Value::as_bool).unwrap_or(false)) {
        return false;
    }
    if !option_matches(query.is_kids, recording.get("IsKids").and_then(Value::as_bool).unwrap_or(false)) {
        return false;
    }
    if !option_matches(query.is_series, recording.get("IsSeries").and_then(Value::as_bool).unwrap_or(false)) {
        return false;
    }
    true
}

fn recording_series_item(series_timer: &Value) -> Value {
    json!({
        "Id": series_timer.get("Id").cloned().unwrap_or(Value::Null),
        "Type": "RecordingSeries",
        "Name": series_timer.get("Name").cloned().unwrap_or(Value::Null),
        "ChannelId": series_timer.get("ChannelId").cloned().unwrap_or(Value::Null),
        "KeepUntil": series_timer.get("KeepUntil").cloned().unwrap_or(Value::Null),
        "RecordAnyChannel": series_timer.get("RecordAnyChannel").cloned().unwrap_or(Value::Null),
        "RecordAnyTime": series_timer.get("RecordAnyTime").cloned().unwrap_or(Value::Null),
        "RecordNewOnly": series_timer.get("RecordNewOnly").cloned().unwrap_or(Value::Null)
    })
}

fn live_tv_value_date(value: &Value, key: &str) -> DateTime<Utc> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| parse_live_tv_datetime(Some(value)))
        .unwrap_or_else(Utc::now)
}

fn query_result(items: Vec<Value>) -> Value {
    json!({
        "Items": items.clone(),
        "TotalRecordCount": items.len(),
        "StartIndex": 0
    })
}

fn paginate_items(items: Vec<Value>, start_index: Option<usize>, limit: Option<usize>) -> Value {
    let total = items.len();
    let start = start_index.unwrap_or(0).min(total);
    let page = if let Some(limit) = limit {
        items.into_iter().skip(start).take(limit).collect::<Vec<_>>()
    } else {
        items.into_iter().skip(start).collect::<Vec<_>>()
    };
    json!({
        "Items": page,
        "TotalRecordCount": total,
        "StartIndex": start
    })
}

fn tuner_hosts_from_config(livetv: &Value) -> Vec<TunerHost> {
    livetv
        .get("TunerHosts")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|value| value.is_object())
                .cloned()
                .map(|value| TunerHost { value })
                .collect()
        })
        .unwrap_or_default()
}

fn listing_providers_from_config(livetv: &Value) -> Vec<ListingProvider> {
    livetv
        .get("ListingProviders")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|value| value.is_object())
                .cloned()
                .map(|value| ListingProvider { value })
                .collect()
        })
        .unwrap_or_default()
}

fn normalize_tuner_host(mut payload: Value) -> Value {
    let friendly_name = live_tv_name(&payload);
    let object = payload.as_object_mut().expect("tuner payload must be object");
    let id = object
        .get("Id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let host_type = object
        .get("Type")
        .and_then(Value::as_str)
        .unwrap_or("m3u")
        .to_ascii_lowercase();
    let url = object
        .get("Url")
        .or_else(|| object.get("Path"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    object.insert("Id".to_string(), json!(id));
    object.insert("Type".to_string(), json!(host_type));
    object.insert("FriendlyName".to_string(), json!(friendly_name));
    object.insert("Url".to_string(), json!(url));
    object.entry("Source".to_string()).or_insert_with(|| json!("Movie Rust"));
    object.entry("EnableTvgId".to_string()).or_insert_with(|| json!(true));
    object.entry("ImportFavoritesOnly".to_string()).or_insert_with(|| json!(false));
    object.entry("AllowHWTranscoding".to_string()).or_insert_with(|| json!(true));
    object.entry("EnableStreamLooping".to_string()).or_insert_with(|| json!(false));
    payload
}

fn normalize_listing_provider(mut payload: Value) -> Value {
    let name = live_tv_name(&payload);
    let object = payload.as_object_mut().expect("listing provider payload must be object");
    let id = object
        .get("Id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let provider_type = object
        .get("Type")
        .and_then(Value::as_str)
        .unwrap_or("xmltv")
        .to_ascii_lowercase();
    let path = object
        .get("Path")
        .or_else(|| object.get("Url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();

    object.insert("Id".to_string(), json!(id));
    object.insert("Type".to_string(), json!(provider_type));
    object.insert("Name".to_string(), json!(name));
    object.insert("Path".to_string(), json!(path));
    object.entry("EnableAllTuners".to_string()).or_insert_with(|| json!(true));
    object.entry("EnabledTuners".to_string()).or_insert_with(|| json!([]));
    object.entry("ChannelMappings".to_string()).or_insert_with(|| json!([]));
    object.entry("MovieCategories".to_string()).or_insert_with(|| json!([]));
    object.entry("SportsCategories".to_string()).or_insert_with(|| json!([]));
    object.entry("KidsCategories".to_string()).or_insert_with(|| json!([]));
    object.entry("NewsCategories".to_string()).or_insert_with(|| json!([]));
    object.entry("MoviePrefix".to_string()).or_insert_with(|| json!(""));
    payload
}

fn upsert_live_tv_array(livetv: &mut Value, key: &str, item: Value) -> Result<(), AppError> {
    let object = livetv
        .as_object_mut()
        .ok_or_else(|| AppError::Internal("livetv configuration is not an object".to_string()))?;
    let items = object.entry(key.to_string()).or_insert_with(|| json!([]));
    let array = items
        .as_array_mut()
        .ok_or_else(|| AppError::Internal(format!("livetv configuration field {key} is not an array")))?;
    let id = item.get("Id").and_then(Value::as_str).unwrap_or_default();
    if let Some(existing) = array
        .iter_mut()
        .find(|existing| existing.get("Id").and_then(Value::as_str) == Some(id))
    {
        *existing = item;
    } else {
        array.push(item);
    }
    Ok(())
}

fn retain_live_tv_array<F>(livetv: &mut Value, key: &str, mut predicate: F) -> Result<(), AppError>
where
    F: FnMut(&Value) -> bool,
{
    let object = livetv
        .as_object_mut()
        .ok_or_else(|| AppError::Internal("livetv configuration is not an object".to_string()))?;
    let items = object.entry(key.to_string()).or_insert_with(|| json!([]));
    let array = items
        .as_array_mut()
        .ok_or_else(|| AppError::Internal(format!("livetv configuration field {key} is not an array")))?;
    array.retain(|item| predicate(item));
    Ok(())
}

async fn sync_live_tv_text_fields(state: &AppState, livetv: &Value) -> Result<(), AppError> {
    let mut config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let object = config
        .as_object_mut()
        .ok_or_else(|| AppError::Internal("server configuration is not an object".to_string()))?;
    let tuner_text = tuner_hosts_from_config(livetv)
        .iter()
        .map(|item| live_tv_name(&item.value))
        .collect::<Vec<_>>()
        .join("\n");
    let provider_text = listing_providers_from_config(livetv)
        .iter()
        .map(|item| live_tv_name(&item.value))
        .collect::<Vec<_>>()
        .join("\n");
    object.insert("LiveTvTunerHostsText".to_string(), json!(tuner_text));
    object.insert("LiveTvListingProvidersText".to_string(), json!(provider_text));
    object.insert(
        "EnableLiveTv".to_string(),
        json!(!tuner_text.is_empty() || !provider_text.is_empty()),
    );
    repository::update_server_configuration_value(&state.pool, &state.config, config).await
}

async fn load_live_tv_channels(
    state: &AppState,
    tuner_hosts: &[TunerHost],
    listing_providers: &[ListingProvider],
) -> Result<Vec<LiveTvChannel>, AppError> {
    let xmltv = load_xmltv_index(state, listing_providers).await?;
    let mut channels = Vec::new();
    let mut seen = HashSet::new();

    for tuner in tuner_hosts {
        let host_type = tuner
            .value
            .get("Type")
            .and_then(Value::as_str)
            .unwrap_or("m3u")
            .to_ascii_lowercase();
        if host_type != "m3u" {
            continue;
        }
        let source = tuner
            .value
            .get("Url")
            .or_else(|| tuner.value.get("Path"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(source) = source else {
            continue;
        };
        let text = load_text_from_source(source).await?;
        for channel in parse_m3u_channels(&text, tuner, &xmltv.channels) {
            if seen.insert(channel.id.clone()) {
                channels.push(channel);
            }
        }
    }

    channels.sort_by(|left, right| left.number.cmp(&right.number).then_with(|| left.name.cmp(&right.name)));
    Ok(channels)
}

async fn load_live_tv_programs(
    state: &AppState,
    channels: &[LiveTvChannel],
    listing_providers: &[ListingProvider],
) -> Result<Vec<LiveTvProgram>, AppError> {
    let xmltv = load_xmltv_index(state, listing_providers).await?;
    let mut programs = Vec::new();
    for provider in listing_providers {
        let enabled_tuners = enabled_tuners_for_provider(provider);
        let type_name = provider
            .value
            .get("Type")
            .and_then(Value::as_str)
            .unwrap_or("xmltv")
            .to_ascii_lowercase();
        if type_name != "xmltv" && type_name != "schedulesdirect" {
            continue;
        }

        for program in &xmltv.programs {
            let Some(channel) = match_program_channel(program, channels, &enabled_tuners) else {
                continue;
            };
            let flags = classify_program(provider, program);
            programs.push(LiveTvProgram {
                id: program.id.clone(),
                channel_id: channel.id.clone(),
                channel_name: channel.name.clone(),
                title: program.title.clone(),
                overview: program.overview.clone(),
                start_date: program.start_date,
                end_date: program.end_date,
                categories: program.categories.clone(),
                episode_title: program.episode_title.clone(),
                image_url: program.icon_url.clone().or_else(|| channel.logo_url.clone()),
                is_repeat: program.is_repeat,
            }
            .with_flags(flags));
        }
    }

    programs.sort_by(|left, right| {
        left.start_date
            .cmp(&right.start_date)
            .then_with(|| left.channel_name.cmp(&right.channel_name))
            .then_with(|| left.title.cmp(&right.title))
    });
    Ok(programs)
}

#[derive(Debug, Clone, Default)]
struct XmlTvIndex {
    channels: HashMap<String, XmlTvChannelMeta>,
    programs: Vec<XmlTvProgram>,
}

async fn load_xmltv_index(
    _state: &AppState,
    listing_providers: &[ListingProvider],
) -> Result<XmlTvIndex, AppError> {
    let mut channels = HashMap::new();
    let mut programs = Vec::new();

    for provider in listing_providers {
        let provider_type = provider
            .value
            .get("Type")
            .and_then(Value::as_str)
            .unwrap_or("xmltv")
            .to_ascii_lowercase();
        if provider_type != "xmltv" && provider_type != "schedulesdirect" {
            continue;
        }

        let source = provider
            .value
            .get("Path")
            .or_else(|| provider.value.get("Url"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(source) = source else {
            continue;
        };
        let text = load_text_from_source(source).await?;
        let parsed = parse_xmltv(&text)?;
        channels.extend(parsed.channels);
        programs.extend(parsed.programs);
    }

    Ok(XmlTvIndex { channels, programs })
}

async fn load_text_from_source(source: &str) -> Result<String, AppError> {
    if source.starts_with("http://") || source.starts_with("https://") {
        Ok(reqwest::get(source).await?.text().await?)
    } else {
        Ok(tokio::fs::read_to_string(source).await?)
    }
}

fn parse_m3u_channels(
    text: &str,
    tuner: &TunerHost,
    xmltv_channels: &HashMap<String, XmlTvChannelMeta>,
) -> Vec<LiveTvChannel> {
    let mut channels = Vec::new();
    let mut pending_info: Option<(HashMap<String, String>, String)> = None;
    let source_id = tuner
        .value
        .get("Id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let source_type = tuner
        .value
        .get("Type")
        .and_then(Value::as_str)
        .unwrap_or("m3u")
        .to_string();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXTINF:") {
            let title = rest
                .split_once(',')
                .map(|(_, name)| name.trim().to_string())
                .unwrap_or_default();
            let attrs = parse_extinf_attributes(rest);
            pending_info = Some((attrs, title));
            continue;
        }
        if line.starts_with('#') {
            continue;
        }
        if let Some((attrs, title)) = pending_info.take() {
            let tvg_id = attrs.get("tvg-id").cloned().filter(|value| !value.is_empty());
            let logo_url = attrs
                .get("tvg-logo")
                .cloned()
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    tvg_id
                        .as_ref()
                        .and_then(|id| xmltv_channels.get(id))
                        .and_then(|meta| meta.icon_url.clone())
                });
            let number = attrs
                .get("tvg-chno")
                .cloned()
                .or_else(|| attrs.get("channel-number").cloned())
                .unwrap_or_else(|| (channels.len() + 1).to_string());
            let name = attrs
                .get("tvg-name")
                .cloned()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| title.clone());
            let id_seed = tvg_id
                .clone()
                .unwrap_or_else(|| format!("{}::{}", source_id, name.to_ascii_lowercase()));
            channels.push(LiveTvChannel {
                id: format!("livetv-channel-{}", uuid_from_seed(&id_seed)),
                name,
                number: number.clone(),
                stream_url: line.to_string(),
                logo_url,
                source_type: source_type.clone(),
                source_id: source_id.clone(),
                tvg_id,
            });
        }
    }

    channels
}

fn parse_extinf_attributes(extinf: &str) -> HashMap<String, String> {
    let metadata = extinf.split_once(',').map(|(left, _)| left).unwrap_or(extinf);
    let mut attributes = HashMap::new();
    for part in metadata.split_whitespace() {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        let trimmed = value.trim_matches('"').trim_matches('\'').to_string();
        attributes.insert(key.trim().to_ascii_lowercase(), trimmed);
    }
    attributes
}

fn parse_xmltv(text: &str) -> Result<XmlTvIndex, AppError> {
    let mut reader = Reader::from_str(text);
    reader.config_mut().trim_text(true);

    let mut channels = HashMap::new();
    let mut programs = Vec::new();
    let mut current_channel_id: Option<String> = None;
    let mut current_channel_meta = XmlTvChannelMeta::default();
    let mut current_program: Option<XmlTvProgramBuilder> = None;
    let mut current_text_tag: Option<String> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match name.as_str() {
                    "channel" => {
                        current_channel_id = xml_attr(&event, b"id");
                        current_channel_meta = XmlTvChannelMeta::default();
                    }
                    "programme" => {
                        current_program = Some(XmlTvProgramBuilder {
                            channel_key: xml_attr(&event, b"channel").unwrap_or_default(),
                            start: xml_attr(&event, b"start").and_then(|value| parse_xmltv_datetime(&value)),
                            stop: xml_attr(&event, b"stop").and_then(|value| parse_xmltv_datetime(&value)),
                            ..XmlTvProgramBuilder::default()
                        });
                    }
                    "display-name" | "title" | "desc" | "sub-title" | "category" | "previously-shown" => {
                        current_text_tag = Some(name);
                    }
                    "icon" => {
                        let src = xml_attr(&event, b"src");
                        if let Some(program) = current_program.as_mut() {
                            if program.icon_url.is_none() {
                                program.icon_url = src;
                            }
                        } else if current_channel_id.is_some() && current_channel_meta.icon_url.is_none() {
                            current_channel_meta.icon_url = src;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(event)) => {
                let name = String::from_utf8_lossy(event.name().as_ref()).to_string();
                match name.as_str() {
                    "channel" => {
                        if let Some(id) = current_channel_id.take() {
                            channels.insert(id, current_channel_meta.clone());
                        }
                    }
                    "programme" => {
                        if let Some(program) = current_program.take().and_then(XmlTvProgramBuilder::build) {
                            programs.push(program);
                        }
                    }
                    "display-name" | "title" | "desc" | "sub-title" | "category" | "previously-shown" => {
                        current_text_tag = None;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(text_node)) => {
                let text = text_node
                    .decode()
                    .map_err(|error| AppError::Internal(format!("XMLTV 鏂囨湰瑙ｆ瀽澶辫触: {error}")))?
                    .to_string();
                match current_text_tag.as_deref() {
                    Some("display-name") => {
                        if current_channel_meta.display_name.is_none() && !text.is_empty() {
                            current_channel_meta.display_name = Some(text);
                        }
                    }
                    Some("title") => {
                        if let Some(program) = current_program.as_mut() {
                            if program.title.is_none() && !text.is_empty() {
                                program.title = Some(text);
                            }
                        }
                    }
                    Some("desc") => {
                        if let Some(program) = current_program.as_mut() {
                            if program.overview.is_none() && !text.is_empty() {
                                program.overview = Some(text);
                            }
                        }
                    }
                    Some("sub-title") => {
                        if let Some(program) = current_program.as_mut() {
                            if program.episode_title.is_none() && !text.is_empty() {
                                program.episode_title = Some(text);
                            }
                        }
                    }
                    Some("category") => {
                        if let Some(program) = current_program.as_mut() {
                            if !text.is_empty() {
                                program.categories.push(text);
                            }
                        }
                    }
                    Some("previously-shown") => {
                        if let Some(program) = current_program.as_mut() {
                            program.is_repeat = true;
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(error) => {
                return Err(AppError::Internal(format!("XMLTV 瑙ｆ瀽澶辫触: {error}")));
            }
            _ => {}
        }
    }

    Ok(XmlTvIndex { channels, programs })
}

#[derive(Debug, Default)]
struct XmlTvProgramBuilder {
    channel_key: String,
    title: Option<String>,
    overview: Option<String>,
    start: Option<DateTime<Utc>>,
    stop: Option<DateTime<Utc>>,
    categories: Vec<String>,
    episode_title: Option<String>,
    icon_url: Option<String>,
    is_repeat: bool,
}

impl XmlTvProgramBuilder {
    fn build(self) -> Option<XmlTvProgram> {
        let title = self.title?;
        let start_date = self.start?;
        let end_date = self.stop.unwrap_or_else(|| start_date + Duration::minutes(30));
        let id_seed = format!("{}:{}:{}", self.channel_key, start_date.timestamp(), title);
        Some(XmlTvProgram {
            id: format!("livetv-program-{}", uuid_from_seed(&id_seed)),
            channel_key: self.channel_key,
            title,
            overview: self.overview,
            start_date,
            end_date,
            categories: self.categories,
            episode_title: self.episode_title,
            icon_url: self.icon_url,
            is_repeat: self.is_repeat,
        })
    }
}

fn xml_attr(event: &quick_xml::events::BytesStart<'_>, key: &[u8]) -> Option<String> {
    event
        .attributes()
        .flatten()
        .find(|attr| attr.key.as_ref() == key)
        .and_then(|attr| String::from_utf8(attr.value.into_owned()).ok())
}

fn parse_xmltv_datetime(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    let normalized = value
        .split_whitespace()
        .next()
        .unwrap_or(value)
        .chars()
        .take(14)
        .collect::<String>();
    NaiveDateTime::parse_from_str(&normalized, "%Y%m%d%H%M%S")
        .ok()
        .map(|naive| Utc.from_utc_datetime(&naive))
}

fn parse_live_tv_datetime(value: Option<&str>) -> Option<DateTime<Utc>> {
    value.and_then(|text| {
        DateTime::parse_from_rfc3339(text)
            .map(|date| date.with_timezone(&Utc))
            .ok()
    })
}

fn enabled_tuners_for_provider(provider: &ListingProvider) -> HashSet<String> {
    if provider
        .value
        .get("EnableAllTuners")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        return HashSet::new();
    }
    provider
        .value
        .get("EnabledTuners")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn match_program_channel<'a>(
    program: &XmlTvProgram,
    channels: &'a [LiveTvChannel],
    enabled_tuners: &HashSet<String>,
) -> Option<&'a LiveTvChannel> {
    channels.iter().find(|channel| {
        if !enabled_tuners.is_empty() && !enabled_tuners.contains(&channel.source_id) {
            return false;
        }
        channel.id.eq_ignore_ascii_case(&program.channel_key)
            || channel
                .tvg_id
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(&program.channel_key))
            || channel.name.eq_ignore_ascii_case(&program.channel_key)
    })
}

#[derive(Debug, Clone, Default)]
struct ProgramFlags {
    is_movie: bool,
    is_sports: bool,
    is_kids: bool,
    is_news: bool,
    is_series: bool,
}

impl LiveTvProgram {
    fn with_flags(mut self, flags: ProgramFlags) -> Self {
        self.categories.extend(
            [
                flags.is_movie.then_some("__movie"),
                flags.is_sports.then_some("__sports"),
                flags.is_kids.then_some("__kids"),
                flags.is_news.then_some("__news"),
                flags.is_series.then_some("__series"),
            ]
            .into_iter()
            .flatten()
            .map(ToOwned::to_owned),
        );
        self
    }

    fn has_flag(&self, key: &str) -> bool {
        self.categories.iter().any(|value| value == key)
    }
}

fn classify_program(provider: &ListingProvider, program: &XmlTvProgram) -> ProgramFlags {
    let lower_categories = program
        .categories
        .iter()
        .map(|item| item.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let provider_categories = |key: &str| -> Vec<String> {
        provider
            .value
            .get(key)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(|item| item.to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    };
    let contains_any = |values: &[String]| values.iter().any(|value| lower_categories.iter().any(|item| item == value));
    let movie_prefix = provider
        .value
        .get("MoviePrefix")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let title_lower = program.title.to_ascii_lowercase();
    let is_movie = contains_any(&provider_categories("MovieCategories"))
        || (!movie_prefix.is_empty() && title_lower.starts_with(&movie_prefix))
        || lower_categories.iter().any(|item| item.contains("movie") || item.contains("film"));
    let is_sports = contains_any(&provider_categories("SportsCategories"))
        || lower_categories.iter().any(|item| item.contains("sport"));
    let is_kids = contains_any(&provider_categories("KidsCategories"))
        || lower_categories.iter().any(|item| item.contains("kids") || item.contains("children"));
    let is_news = contains_any(&provider_categories("NewsCategories"))
        || lower_categories.iter().any(|item| item.contains("news"));
    let is_series = !is_movie
        && (program.episode_title.is_some()
            || lower_categories.iter().any(|item| item.contains("series") || item.contains("drama")));

    ProgramFlags {
        is_movie,
        is_sports,
        is_kids,
        is_news,
        is_series,
    }
}

fn channel_matches_query(
    channel: &LiveTvChannel,
    programs: &[LiveTvProgram],
    query: &LiveTvChannelQuery,
) -> bool {
    if !query_needs_programs(query) {
        return true;
    }
    let relevant = programs
        .iter()
        .filter(|program| program.channel_id == channel.id)
        .collect::<Vec<_>>();
    if relevant.is_empty() {
        return false;
    }
    relevant.into_iter().any(|program| matches_program_flags_for_channel(program, query))
}

fn query_needs_programs(query: &LiveTvChannelQuery) -> bool {
    query.add_current_program.unwrap_or(false)
        || query.is_movie.is_some()
        || query.is_sports.is_some()
        || query.is_kids.is_some()
        || query.is_news.is_some()
        || query.is_series.is_some()
}

fn matches_program_flags_for_channel(program: &LiveTvProgram, query: &LiveTvChannelQuery) -> bool {
    option_matches(query.is_movie, program.has_flag("__movie"))
        && option_matches(query.is_sports, program.has_flag("__sports"))
        && option_matches(query.is_kids, program.has_flag("__kids"))
        && option_matches(query.is_news, program.has_flag("__news"))
        && option_matches(query.is_series, program.has_flag("__series"))
}

fn matches_program_flags(program: &LiveTvProgram, query: &LiveTvProgramsQuery) -> bool {
    option_matches(query.is_movie, program.has_flag("__movie"))
        && option_matches(query.is_sports, program.has_flag("__sports"))
        && option_matches(query.is_kids, program.has_flag("__kids"))
        && option_matches(query.is_news, program.has_flag("__news"))
        && option_matches(query.is_series, program.has_flag("__series"))
}

fn option_matches(filter: Option<bool>, actual: bool) -> bool {
    match filter {
        Some(expected) => expected == actual,
        None => true,
    }
}

fn current_program_for_channel(channel: &LiveTvChannel, programs: &[LiveTvProgram]) -> Option<LiveTvProgram> {
    let now = Utc::now();
    programs
        .iter()
        .find(|program| program.channel_id == channel.id && program.start_date <= now && program.end_date > now)
        .cloned()
        .or_else(|| {
            programs
                .iter()
                .find(|program| program.channel_id == channel.id && program.start_date > now)
                .cloned()
        })
}

fn channel_to_item(channel: &LiveTvChannel, current_program: Option<&LiveTvProgram>) -> Value {
    let mut item = json!({
        "Id": channel.id,
        "Type": "TvChannel",
        "MediaType": "Video",
        "Name": channel.name,
        "SortName": channel.name,
        "ChannelNumber": channel.number,
        "Number": channel.number,
        "Overview": channel.stream_url,
        "SourceType": channel.source_type
    });
    if let Some(current_program) = current_program {
        item["CurrentProgram"] = program_to_item(current_program.clone());
    }
    item
}

fn program_to_item(program: LiveTvProgram) -> Value {
    let runtime_ticks = (program.end_date - program.start_date)
        .num_seconds()
        .saturating_mul(10_000_000);
    let mut item = json!({
        "Id": program.id,
        "Type": "Program",
        "Name": program.title,
        "SortName": program.title,
        "ChannelId": program.channel_id,
        "ChannelName": program.channel_name,
        "StartDate": program.start_date,
        "EndDate": program.end_date,
        "RunTimeTicks": runtime_ticks,
        "IsRepeat": program.is_repeat,
        "Overview": program.overview,
        "EpisodeTitle": program.episode_title,
        "IsMovie": program.has_flag("__movie"),
        "IsSports": program.has_flag("__sports"),
        "IsKids": program.has_flag("__kids"),
        "IsNews": program.has_flag("__news"),
        "IsSeries": program.has_flag("__series")
    });
    if let Some(image_url) = program.image_url {
        item["ImageUrl"] = json!(image_url);
    }
    item
}

fn channel_ids_from_value(value: Option<&Value>) -> HashSet<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        Some(Value::String(text)) => text
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => HashSet::new(),
    }
}

fn sort_channel_items(items: &mut [Value], sort_by: Option<&str>, sort_order: Option<&str>) {
    let descending = matches!(sort_order, Some(order) if order.eq_ignore_ascii_case("descending"));
    match sort_by.unwrap_or("Number").to_ascii_lowercase().as_str() {
        "dateplayed" => {}
        _ => items.sort_by(|left, right| {
            let left_number = left.get("Number").and_then(Value::as_str).unwrap_or_default();
            let right_number = right.get("Number").and_then(Value::as_str).unwrap_or_default();
            let ord = left_number.cmp(right_number).then_with(|| {
                left.get("Name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .cmp(right.get("Name").and_then(Value::as_str).unwrap_or_default())
            });
            if descending { ord.reverse() } else { ord }
        }),
    }
}

fn sort_programs(programs: &mut [LiveTvProgram], sort_by: Option<&str>, sort_order: Option<&str>) {
    let descending = matches!(sort_order, Some(order) if order.eq_ignore_ascii_case("descending"));
    let sort_key = sort_by.unwrap_or("StartDate").to_ascii_lowercase();
    programs.sort_by(|left, right| {
        let ord = match sort_key.as_str() {
            "sortname" => left.title.cmp(&right.title),
            _ => left.start_date.cmp(&right.start_date).then_with(|| left.title.cmp(&right.title)),
        };
        if descending { ord.reverse() } else { ord }
    });
}

fn collect_provider_mappings(listing_providers: &[ListingProvider]) -> Vec<Value> {
    let mut mappings = Vec::new();
    for provider in listing_providers {
        if let Some(items) = provider.value.get("ChannelMappings").and_then(Value::as_array) {
            mappings.extend(items.iter().cloned());
        }
    }
    mappings
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

fn live_tv_stream_container(stream_url: &str) -> String {
    let path = stream_url.split('?').next().unwrap_or(stream_url);
    let extension = path
        .rsplit_once('.')
        .map(|(_, ext)| ext.trim().to_ascii_lowercase())
        .unwrap_or_else(|| "ts".to_string());
    match extension.as_str() {
        "m3u8" | "mp4" | "ts" | "mpd" | "webm" | "mkv" | "aac" | "mp3" => extension,
        _ => "ts".to_string(),
    }
}

fn uuid_from_seed(seed: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, seed.as_bytes())
}

