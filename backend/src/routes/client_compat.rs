use crate::{
    auth::AuthSession,
    error::AppError,
    models::{emby_id_to_uuid, BaseItemDto, QueryResult},
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::{header::CONTENT_TYPE, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Notifications/Types", get(notification_types))
        .route("/Notifications/Services", get(notification_services))
        .route("/Notifications/{user_id}/Summary", get(notification_summary))
        .route("/Notifications/{user_id}", get(notification_list))
        .route("/Notifications/{user_id}/Read", post(mark_notifications_read))
        .route("/Notifications/{user_id}/Unread", post(mark_notifications_unread))
        .route("/Search/Hints", get(search_hints))
        .route("/Playback/BitrateTest", get(playback_bitrate_test))
        .route("/LiveStreams/Open", post(open_live_stream))
        .route("/LiveStreams/MediaInfo", post(live_stream_media_info))
        .route("/Libraries/AvailableOptions", get(library_available_options))
        .route("/Movies/Recommendations", get(movie_recommendations))
        .route("/Dlna/ProfileInfos", get(dlna_profile_infos))
        .route("/Dlna/Profiles", get(dlna_profiles).post(create_dlna_profile))
        .route(
            "/Dlna/Profiles/{id}",
            get(dlna_profile).post(update_dlna_profile).delete(delete_dlna_profile),
        )
        .route("/Sync/Options", get(sync_options))
        .route("/Sync/Jobs", get(sync_jobs).post(create_sync_job))
        .route("/Sync/Jobs/{id}", get(sync_job).post(update_sync_job).delete(delete_sync_job))
        .route("/Sync/JobItems", get(sync_job_items))
        .route("/Sync/JobItems/{id}", delete(delete_sync_job_item))
        .route("/Sync/JobItems/{id}/Enable", post(enable_sync_job_item))
        .route("/Sync/OfflineActions", post(sync_offline_actions))
        .route("/Sync/Data", post(sync_data))
        .route("/Sync/Items/Ready", get(sync_ready_items))
        .route("/Sync/JobItems/{id}/Transferred", post(sync_job_item_transferred))
        .route("/Sync/{target_id}/Items", delete(sync_cancel_items))
        .route("/Collections", get(collections).post(create_collection))
        .route("/Collections/{id}", get(collection).post(update_collection).delete(delete_collection))
        .route("/Collections/{id}/Delete", post(delete_collection))
        .route("/Collections/{id}/Items", get(collection_items).post(add_collection_items).delete(remove_collection_items))
        .route("/Collections/{id}/Items/Delete", post(remove_collection_items))
        .route("/Playlists", get(playlists).post(create_playlist))
        .route("/Playlists/{id}", get(playlist).post(update_playlist).delete(delete_playlist))
        .route("/Playlists/{id}/Delete", post(delete_playlist))
        .route("/Playlists/{id}/Items", get(playlist_items).post(add_playlist_items).delete(remove_playlist_items))
        .route("/Playlists/{id}/Items/Delete", post(remove_playlist_items))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NotificationIdsQuery {
    #[serde(default)]
    ids: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BitrateQuery {
    #[serde(default)]
    size: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SyncReadyQuery {
    #[serde(default, alias = "TargetId")]
    target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SyncCancelQuery {
    #[serde(default, alias = "ItemIds")]
    item_ids: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SyncJobItemsQuery {
    #[serde(default)]
    job_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SyncJobsQuery {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NamedCreateQuery {
    #[serde(default, alias = "name")]
    name: Option<String>,
    #[serde(default, alias = "Ids")]
    ids: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IdsQuery {
    #[serde(default, alias = "Ids")]
    ids: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct NamedPayload {
    #[serde(default, alias = "name")]
    name: Option<String>,
}

async fn notification_types(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    let value = repository::named_system_configuration(&state.pool, "notifications")
        .await?
        .unwrap_or_else(default_notifications_configuration);

    let enabled_map = value
        .get("Options")
        .and_then(Value::as_array)
        .map(|items| {
            items.iter()
                .filter_map(|item| {
                    Some((
                        item.get("Type")?.as_str()?.to_string(),
                        item.get("Enabled").and_then(Value::as_bool).unwrap_or(false),
                    ))
                })
                .collect::<std::collections::BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    Ok(Json(vec![
        json!({
            "Type": "PlaybackStart",
            "Name": "Playback Started",
            "Category": "Playback",
            "IsBasedOnUserEvent": true,
            "Enabled": enabled_map.get("PlaybackStart").copied().unwrap_or(false)
        }),
        json!({
            "Type": "PlaybackStopped",
            "Name": "Playback Stopped",
            "Category": "Playback",
            "IsBasedOnUserEvent": true,
            "Enabled": enabled_map.get("PlaybackStopped").copied().unwrap_or(false)
        }),
        json!({
            "Type": "SystemUpdate",
            "Name": "System Update",
            "Category": "System",
            "IsBasedOnUserEvent": false,
            "Enabled": enabled_map.get("SystemUpdate").copied().unwrap_or(false)
        }),
    ]))
}

async fn notification_services(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![
        json!({
            "Id": "log",
            "Name": "Server Log"
        }),
        json!({
            "Id": "webhook",
            "Name": "Webhook"
        }),
    ])
}

async fn notification_summary(
    _session: AuthSession,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "UnreadCount": 0,
        "MaxUnreadNotificationLevel": "Normal"
    }))
}

async fn notification_list(
    _session: AuthSession,
    Path(_user_id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0,
        "StartIndex": 0
    }))
}

async fn mark_notifications_read(
    _session: AuthSession,
    Path(_user_id): Path<String>,
    Query(_query): Query<NotificationIdsQuery>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn mark_notifications_unread(
    _session: AuthSession,
    Path(_user_id): Path<String>,
    Query(_query): Query<NotificationIdsQuery>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn search_hints(
    _session: AuthSession,
) -> Json<Value> {
    Json(json!({
        "SearchHints": [],
        "TotalRecordCount": 0
    }))
}

async fn playback_bitrate_test(
    _session: AuthSession,
    Query(query): Query<BitrateQuery>,
) -> impl IntoResponse {
    let size = query.size.unwrap_or(1024 * 512).clamp(1, 8 * 1024 * 1024);
    let bytes = vec![0u8; size];
    ([(CONTENT_TYPE, "application/octet-stream")], bytes)
}

async fn open_live_stream(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "MediaSource": null,
        "LiveStreamId": null,
        "PlaySessionId": Uuid::new_v4().to_string()
    }))
}

async fn live_stream_media_info(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "MediaSources": [],
        "PlaySessionId": null
    }))
}

async fn library_available_options(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "LibraryOptions": {
            "EnablePhotos": true,
            "EnableRealtimeMonitor": true,
            "EnableChapterImageExtraction": false,
            "EnableAutomaticSeriesGrouping": true,
            "ImportPlaylists": true,
            "EnableEmbeddedTitles": false,
            "EnableEmbeddedEpisodeInfos": false,
            "AutomaticRefreshIntervalDays": 0,
            "PreferredMetadataLanguage": "",
            "MetadataCountryCode": "",
            "DisabledLocalMetadataReaders": [],
            "LocalMetadataReaderOrder": [],
            "DisabledSubtitleFetchers": [],
            "SubtitleFetcherOrder": [],
            "DisabledMediaSegmentProviders": [],
            "MediaSegmentProviderOrder": [],
            "TypeOptions": []
        },
        "MetadataReaders": [],
        "MetadataFetchers": [],
        "ImageFetchers": [],
        "SubtitleFetchers": [],
        "MediaSegmentProviders": []
    }))
}

async fn movie_recommendations(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn dlna_profile_infos(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![json!({
        "Id": "html5",
        "Name": "HTML5",
        "Type": "System"
    })])
}

async fn dlna_profiles(_session: AuthSession) -> Json<Vec<Value>> {
    Json(vec![default_dlna_profile("html5", "HTML5", "System")])
}

async fn dlna_profile(_session: AuthSession, Path(id): Path<String>) -> Json<Value> {
    Json(default_dlna_profile(&id, &id, "User"))
}

async fn create_dlna_profile(
    _session: AuthSession,
    State(state): State<AppState>,
    Json(mut payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let id = payload
        .get("Id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    payload["Id"] = json!(id);
    repository::update_named_system_configuration(&state.pool, &format!("dlna_profile_{id}"), &payload).await?;
    Ok(Json(payload))
}

async fn update_dlna_profile(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(mut payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    payload["Id"] = json!(id.clone());
    repository::update_named_system_configuration(&state.pool, &format!("dlna_profile_{id}"), &payload).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_dlna_profile(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    repository::update_named_system_configuration(&state.pool, &format!("dlna_profile_{id}"), &json!(null)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_options(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Targets": [],
        "Options": ["Profile", "Quality", "UnwatchedOnly", "SyncNewContent", "ItemLimit"],
        "ProfileOptions": [{
            "Id": "original",
            "Name": "Original",
            "Description": "Keep original media when possible.",
            "IsDefault": true,
            "EnableQualityOptions": true
        }],
        "QualityOptions": [{
            "Id": "original",
            "Name": "Original",
            "Description": "Original quality",
            "IsDefault": true
        }, {
            "Id": "custom",
            "Name": "Custom",
            "Description": "Custom bitrate",
            "IsDefault": false
        }]
    }))
}

async fn sync_jobs(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<SyncJobsQuery>,
) -> Result<Json<Value>, AppError> {
    let jobs = load_sync_jobs(&state).await?;
    let items: Vec<Value> = jobs
        .into_iter()
        .filter(|job| {
            query
                .user_id
                .as_deref()
                .is_none_or(|user_id| string_field_eq(job, "UserId", user_id))
        })
        .filter(|job| {
            query
                .target_id
                .as_deref()
                .is_none_or(|target_id| string_field_eq(job, "TargetId", target_id))
        })
        .collect();
    Ok(query_result_value(items))
}

async fn sync_job(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let jobs = load_sync_jobs(&state).await?;
    let job = jobs
        .into_iter()
        .find(|job| string_field_eq(job, "Id", &id))
        .ok_or_else(|| AppError::NotFound("Sync job not found".to_string()))?;
    Ok(Json(job))
}

async fn create_sync_job(
    _session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let mut jobs = load_sync_jobs(&state).await?;
    let job = normalize_sync_job(payload, None, _session.user_id);
    jobs.push(job.clone());
    save_sync_jobs(&state, &jobs).await?;
    Ok(Json(job))
}

async fn update_sync_job(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let mut jobs = load_sync_jobs(&state).await?;
    let Some(index) = jobs.iter().position(|job| string_field_eq(job, "Id", &id)) else {
        return Err(AppError::NotFound("Sync job not found".to_string()));
    };
    jobs[index] = normalize_sync_job(payload, Some(id), _session.user_id);
    let job = jobs[index].clone();
    save_sync_jobs(&state, &jobs).await?;
    Ok(Json(job))
}

async fn delete_sync_job(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let mut jobs = load_sync_jobs(&state).await?;
    let before = jobs.len();
    jobs.retain(|job| !string_field_eq(job, "Id", &id));
    if jobs.len() == before {
        return Err(AppError::NotFound("Sync job not found".to_string()));
    }
    save_sync_jobs(&state, &jobs).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_job_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<SyncJobItemsQuery>,
) -> Result<Json<Value>, AppError> {
    let mut items = Vec::new();
    for job in load_sync_jobs(&state).await? {
        if query
            .job_id
            .as_deref()
            .is_some_and(|job_id| !string_field_eq(&job, "Id", job_id))
        {
            continue;
        }
        items.extend(job.get("JobItems").and_then(Value::as_array).cloned().unwrap_or_default());
    }
    Ok(query_result_value(items))
}

async fn delete_sync_job_item(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    update_sync_job_item_status(&state, &id, "Cancelled").await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn enable_sync_job_item(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    update_sync_job_item_status(&state, &id, "ReadyToTransfer").await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_offline_actions(_session: AuthSession) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn sync_data(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let jobs = load_sync_jobs(&state).await?;
    let job_items = jobs
        .iter()
        .flat_map(|job| job.get("JobItems").and_then(Value::as_array).cloned().unwrap_or_default())
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "Items": [],
        "SyncJobs": jobs,
        "SyncJobItems": job_items
    })))
}

async fn sync_ready_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<SyncReadyQuery>,
) -> Result<Json<Value>, AppError> {
    let mut items = Vec::new();
    for job in load_sync_jobs(&state).await? {
        if query
            .target_id
            .as_deref()
            .is_some_and(|target_id| !string_field_eq(&job, "TargetId", target_id))
        {
            continue;
        }
        items.extend(
            job.get("JobItems")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .filter(|item| string_field_eq(item, "Status", "ReadyToTransfer")),
        );
    }
    Ok(query_result_value(items))
}

async fn sync_job_item_transferred(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    update_sync_job_item_status(&state, &id, "Transferred").await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_cancel_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(target_id): Path<String>,
    Query(query): Query<SyncCancelQuery>,
) -> Result<StatusCode, AppError> {
    let item_ids = query
        .item_ids
        .as_deref()
        .map(csv_text_list)
        .unwrap_or_default();
    let mut jobs = load_sync_jobs(&state).await?;
    for job in &mut jobs {
        if !string_field_eq(job, "TargetId", &target_id) {
            continue;
        }
        if let Some(job_items) = job.get_mut("JobItems").and_then(Value::as_array_mut) {
            for item in job_items {
                let matches = item_ids.is_empty()
                    || item_ids.iter().any(|id| string_field_eq(item, "ItemId", id));
                if matches {
                    item["Status"] = json!("Cancelled");
                }
            }
        }
        job["Status"] = json!("Cancelled");
    }
    save_sync_jobs(&state, &jobs).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn collections(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    collection_query_result(&state, "BoxSet").await
}

async fn collection(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BaseItemDto>, AppError> {
    collection_dto(&state, _session.user_id, &id, "BoxSet").await
}

async fn create_collection(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<NamedCreateQuery>,
) -> Result<Json<BaseItemDto>, AppError> {
    let item = repository::create_virtual_collection_item(
        &state.pool,
        query.name.as_deref().unwrap_or("Collection"),
        "BoxSet",
        "Video",
        "virtual://collections",
    )
    .await?;
    let ids = parse_emby_ids(query.ids.as_deref());
    repository::add_items_to_collection(&state.pool, item.id, &ids).await?;
    Ok(Json(
        repository::media_item_to_dto(&state.pool, &item, Some(_session.user_id), state.config.server_id).await?,
    ))
}

async fn update_collection(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<NamedPayload>,
) -> Result<Json<BaseItemDto>, AppError> {
    update_named_collection(&state, _session.user_id, &id, payload.name, "BoxSet").await
}

async fn delete_collection(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let collection_id = parse_required_emby_id(&id)?;
    repository::delete_collection_item(&state.pool, collection_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn collection_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    children_query_result(&state, _session.user_id, &id).await
}

async fn add_collection_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<IdsQuery>,
) -> Result<StatusCode, AppError> {
    let collection_id = parse_required_emby_id(&id)?;
    let ids = parse_emby_ids(query.ids.as_deref());
    repository::add_items_to_collection(&state.pool, collection_id, &ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_collection_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<IdsQuery>,
) -> Result<StatusCode, AppError> {
    let collection_id = parse_required_emby_id(&id)?;
    let ids = parse_emby_ids(query.ids.as_deref());
    repository::remove_items_from_collection(&state.pool, collection_id, &ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn playlists(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    collection_query_result(&state, "Playlist").await
}

async fn playlist(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<BaseItemDto>, AppError> {
    collection_dto(&state, _session.user_id, &id, "Playlist").await
}

async fn create_playlist(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<NamedCreateQuery>,
) -> Result<Json<BaseItemDto>, AppError> {
    let item = repository::create_virtual_collection_item(
        &state.pool,
        query.name.as_deref().unwrap_or("Playlist"),
        "Playlist",
        "Audio",
        "virtual://playlists",
    )
    .await?;
    let ids = parse_emby_ids(query.ids.as_deref());
    repository::add_items_to_collection(&state.pool, item.id, &ids).await?;
    Ok(Json(
        repository::media_item_to_dto(&state.pool, &item, Some(_session.user_id), state.config.server_id).await?,
    ))
}

async fn update_playlist(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<NamedPayload>,
) -> Result<Json<BaseItemDto>, AppError> {
    update_named_collection(&state, _session.user_id, &id, payload.name, "Playlist").await
}

async fn delete_playlist(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let playlist_id = parse_required_emby_id(&id)?;
    repository::delete_collection_item(&state.pool, playlist_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn playlist_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    children_query_result(&state, _session.user_id, &id).await
}

async fn add_playlist_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<IdsQuery>,
) -> Result<StatusCode, AppError> {
    let playlist_id = parse_required_emby_id(&id)?;
    let ids = parse_emby_ids(query.ids.as_deref());
    repository::add_items_to_collection(&state.pool, playlist_id, &ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_playlist_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<IdsQuery>,
) -> Result<StatusCode, AppError> {
    let playlist_id = parse_required_emby_id(&id)?;
    let ids = parse_emby_ids(query.ids.as_deref());
    repository::remove_items_from_collection(&state.pool, playlist_id, &ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn load_sync_jobs(state: &AppState) -> Result<Vec<Value>, AppError> {
    Ok(repository::named_system_configuration(&state.pool, "sync_jobs")
        .await?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default())
}

async fn save_sync_jobs(state: &AppState, jobs: &[Value]) -> Result<(), AppError> {
    repository::update_named_system_configuration(&state.pool, "sync_jobs", &json!(jobs)).await
}

fn normalize_sync_job(mut payload: Value, id: Option<String>, user_id: Uuid) -> Value {
    let job_id = id
        .or_else(|| payload.get("Id").and_then(Value::as_str).map(str::to_string))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let requested_item_ids = value_text_list(Some(&payload), &["RequestedItemIds", "ItemIds", "Items"]);
    let target_id = payload
        .get("TargetId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .unwrap_or("server")
        .to_string();
    let name = payload
        .get("Name")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Sync Job".to_string());

    if !payload.is_object() {
        payload = json!({});
    }
    payload["Id"] = json!(job_id);
    payload["Name"] = json!(name);
    payload["TargetId"] = json!(target_id);
    payload["UserId"] = payload
        .get("UserId")
        .cloned()
        .unwrap_or_else(|| json!(user_id.to_string()));
    payload["RequestedItemIds"] = json!(requested_item_ids);
    payload["Category"] = payload
        .get("Category")
        .cloned()
        .unwrap_or_else(|| json!("Latest"));
    payload["Status"] = payload
        .get("Status")
        .cloned()
        .unwrap_or_else(|| json!("ReadyToTransfer"));
    payload["Profile"] = payload
        .get("Profile")
        .cloned()
        .unwrap_or_else(|| json!("original"));
    payload["Quality"] = payload
        .get("Quality")
        .cloned()
        .unwrap_or_else(|| json!("original"));
    payload["JobItems"] = json!(sync_job_items_for_job(
        payload.get("Id").and_then(Value::as_str).unwrap_or_default(),
        payload
            .get("TargetId")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        payload
            .get("RequestedItemIds")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect::<Vec<_>>()
            .as_slice(),
    ));
    payload
}

fn sync_job_items_for_job(job_id: &str, target_id: &str, item_ids: &[String]) -> Vec<Value> {
    item_ids
        .iter()
        .map(|item_id| {
            let item_key = format!("{job_id}:{target_id}:{item_id}");
            json!({
                "Id": Uuid::new_v5(&Uuid::NAMESPACE_OID, item_key.as_bytes()).to_string(),
                "JobId": job_id,
                "TargetId": target_id,
                "ItemId": item_id,
                "Status": "ReadyToTransfer",
                "Progress": 100,
                "Transferred": false
            })
        })
        .collect()
}

async fn update_sync_job_item_status(
    state: &AppState,
    item_id: &str,
    status: &str,
) -> Result<(), AppError> {
    let mut jobs = load_sync_jobs(state).await?;
    let mut found = false;
    for job in &mut jobs {
        if let Some(job_items) = job.get_mut("JobItems").and_then(Value::as_array_mut) {
            for item in job_items {
                if string_field_eq(item, "Id", item_id) {
                    item["Status"] = json!(status);
                    item["Transferred"] = json!(status == "Transferred");
                    found = true;
                }
            }
        }
    }
    if !found {
        return Err(AppError::NotFound("Sync job item not found".to_string()));
    }
    save_sync_jobs(state, &jobs).await
}

fn query_result_value(items: Vec<Value>) -> Json<Value> {
    let total_record_count = items.len();
    Json(json!({
        "Items": items,
        "TotalRecordCount": total_record_count,
        "StartIndex": 0
    }))
}

fn string_field_eq(value: &Value, field: &str, expected: &str) -> bool {
    value
        .get(field)
        .and_then(Value::as_str)
        .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
}

fn value_text_list(value: Option<&Value>, fields: &[&str]) -> Vec<String> {
    for field in fields {
        let Some(field_value) = value.and_then(|value| value.get(*field)) else {
            continue;
        };
        if let Some(items) = field_value.as_array() {
            return items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .or_else(|| item.get("Id").and_then(Value::as_str))
                        .map(str::to_string)
                })
                .collect();
        }
        if let Some(text) = field_value.as_str() {
            return text
                .split(',')
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .collect();
        }
    }
    Vec::new()
}

fn csv_text_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .collect()
}

fn default_notifications_configuration() -> Value {
    json!({
        "Options": []
    })
}

fn default_dlna_profile(id: &str, name: &str, profile_type: &str) -> Value {
    json!({
        "Id": id,
        "Name": name,
        "Type": profile_type,
        "Identification": {},
        "FriendlyName": name,
        "Manufacturer": "Movie Rust",
        "ManufacturerUrl": "",
        "ModelName": name,
        "ModelNumber": "",
        "ModelUrl": "",
        "SerialNumber": "",
        "EnableAlbumArtInDidl": true,
        "EnableSingleAlbumArtLimit": false,
        "EnableSingleSubtitleLimit": false,
        "SupportedMediaTypes": "Audio,Photo,Video",
        "TranscodingProfiles": [],
        "DirectPlayProfiles": [],
        "ResponseProfiles": [],
        "ContainerProfiles": [],
        "CodecProfiles": [],
        "SubtitleProfiles": []
    })
}

async fn collection_query_result(
    state: &AppState,
    item_type: &str,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = repository::list_collection_items(&state.pool, item_type).await?;
    let mut dtos = Vec::with_capacity(items.len());
    for item in &items {
        dtos.push(
            repository::media_item_to_dto(&state.pool, item, None, state.config.server_id).await?,
        );
    }
    Ok(Json(QueryResult {
        total_record_count: dtos.len() as i64,
        items: dtos,
        start_index: Some(0),
    }))
}

async fn children_query_result(
    state: &AppState,
    user_id: Uuid,
    id: &str,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let collection_id = parse_required_emby_id(id)?;
    let items = repository::collection_children(&state.pool, collection_id).await?;
    let mut dtos = Vec::with_capacity(items.len());
    for item in &items {
        dtos.push(
            repository::media_item_to_dto(&state.pool, item, Some(user_id), state.config.server_id)
                .await?,
        );
    }
    Ok(Json(QueryResult {
        total_record_count: dtos.len() as i64,
        items: dtos,
        start_index: Some(0),
    }))
}

async fn collection_dto(
    state: &AppState,
    user_id: Uuid,
    id: &str,
    item_type: &str,
) -> Result<Json<BaseItemDto>, AppError> {
    let collection_id = parse_required_emby_id(id)?;
    let item = repository::get_media_item(&state.pool, collection_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Collection or playlist not found".to_string()))?;
    if item.item_type != item_type {
        return Err(AppError::NotFound("Collection or playlist not found".to_string()));
    }
    Ok(Json(
        repository::media_item_to_dto(&state.pool, &item, Some(user_id), state.config.server_id)
            .await?,
    ))
}

async fn update_named_collection(
    state: &AppState,
    user_id: Uuid,
    id: &str,
    name: Option<String>,
    item_type: &str,
) -> Result<Json<BaseItemDto>, AppError> {
    let collection_id = parse_required_emby_id(id)?;
    let existing = repository::get_media_item(&state.pool, collection_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Collection or playlist not found".to_string()))?;
    if existing.item_type != item_type {
        return Err(AppError::NotFound("Collection or playlist not found".to_string()));
    }
    let item = repository::rename_collection_item(
        &state.pool,
        collection_id,
        name.as_deref().unwrap_or(existing.name.as_str()),
    )
    .await?;
    Ok(Json(
        repository::media_item_to_dto(&state.pool, &item, Some(user_id), state.config.server_id)
            .await?,
    ))
}

fn parse_required_emby_id(id: &str) -> Result<Uuid, AppError> {
    emby_id_to_uuid(id).map_err(|_| AppError::BadRequest("无效的 Emby 项目 ID".to_string()))
}

fn parse_emby_ids(ids: Option<&str>) -> Vec<Uuid> {
    ids.unwrap_or_default()
        .split(',')
        .filter_map(|id| emby_id_to_uuid(id.trim()).ok())
        .collect()
}
