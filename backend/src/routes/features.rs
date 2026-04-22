use crate::{
    auth::{self, AuthSession},
    error::AppError,
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

const DLNA_PROFILES_KEY: &str = "feature:dlna_profiles";
const SYNC_TARGET_INDEX_KEY: &str = "feature:sync_targets";
const SYNC_OFFLINE_ACTIONS_KEY: &str = "feature:sync_offline_actions";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Dlna/ProfileInfos", get(dlna_profile_infos))
        .route("/Dlna/Profiles", get(dlna_profiles).post(create_dlna_profile))
        .route("/Dlna/Profiles/Default", get(default_dlna_profile))
        .route(
            "/Dlna/Profiles/{id}",
            get(get_dlna_profile)
                .post(update_dlna_profile)
                .put(update_dlna_profile)
                .delete(delete_dlna_profile),
        )
        .route("/Notifications/Types", get(notification_types))
        .route("/Notifications/Services", get(notification_services))
        .route(
            "/Notifications/Services/Defaults",
            get(notification_service_defaults),
        )
        .route("/Notifications/Services/Test", post(test_notification_service))
        .route("/Packages", get(package_catalog))
        .route("/Packages/Installed", get(installed_packages))
        .route("/Packages/Updates", get(package_updates))
        .route("/Sync/Data", get(sync_data).post(update_sync_data))
        .route(
            "/Sync/OfflineActions",
            get(sync_offline_actions).post(record_sync_offline_actions),
        )
        .route("/Sync/Items/Ready", get(sync_items_ready))
        .route(
            "/Sync/JobItems/{job_id}/Transferred",
            post(mark_sync_job_item_transferred),
        )
        .route("/Sync/{target_id}/Items", delete(cancel_sync_items))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct IdQuery {
    #[serde(default, alias = "id")]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SyncQuery {
    #[serde(default, alias = "targetId")]
    target_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PackageQuery {
    #[serde(default, alias = "packageType")]
    package_type: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CancelSyncItemsQuery {
    #[serde(default, alias = "itemIds")]
    item_ids: Option<String>,
}

async fn dlna_profile_infos(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let mut infos = vec![dlna_profile_info(&default_dlna_profile_value())];
    infos.extend(
        load_user_dlna_profiles(&state)
            .await?
            .into_iter()
            .map(|profile| dlna_profile_info(&profile)),
    );
    Ok(Json(infos))
}

async fn dlna_profiles(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let mut profiles = vec![default_dlna_profile_value()];
    profiles.extend(load_user_dlna_profiles(&state).await?);
    Ok(Json(profiles))
}

async fn default_dlna_profile(
    session: AuthSession,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(default_dlna_profile_value()))
}

async fn get_dlna_profile(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    if id.eq_ignore_ascii_case("default") {
        return Ok(Json(default_dlna_profile_value()));
    }
    let profile = load_user_dlna_profiles(&state)
        .await?
        .into_iter()
        .find(|profile| {
            profile
                .get("Id")
                .and_then(Value::as_str)
                .is_some_and(|value| value == id)
        })
        .ok_or_else(|| AppError::NotFound(format!("DLNA profile not found: {id}")))?;
    Ok(Json(profile))
}

async fn create_dlna_profile(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let mut profiles = load_user_dlna_profiles(&state).await?;
    let profile = normalize_dlna_profile(payload, None)?;
    profiles.push(profile.clone());
    save_user_dlna_profiles(&state, &profiles).await?;
    Ok(Json(profile))
}

async fn update_dlna_profile(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    if id.eq_ignore_ascii_case("default") {
        return Err(AppError::BadRequest(
            "Default DLNA profile is read-only".to_string(),
        ));
    }
    let mut profiles = load_user_dlna_profiles(&state).await?;
    let index = profiles
        .iter()
        .position(|profile| {
            profile
                .get("Id")
                .and_then(Value::as_str)
                .is_some_and(|value| value == id)
        })
        .ok_or_else(|| AppError::NotFound(format!("DLNA profile not found: {id}")))?;
    let profile = normalize_dlna_profile(payload, Some(id.as_str()))?;
    profiles[index] = profile.clone();
    save_user_dlna_profiles(&state, &profiles).await?;
    Ok(Json(profile))
}

async fn delete_dlna_profile(
    session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    if id.eq_ignore_ascii_case("default") {
        return Err(AppError::BadRequest(
            "Default DLNA profile cannot be deleted".to_string(),
        ));
    }
    let mut profiles = load_user_dlna_profiles(&state).await?;
    let before = profiles.len();
    profiles.retain(|profile| {
        profile
            .get("Id")
            .and_then(Value::as_str)
            .is_none_or(|value| value != id)
    });
    if profiles.len() == before {
        return Err(AppError::NotFound(format!("DLNA profile not found: {id}")));
    }
    save_user_dlna_profiles(&state, &profiles).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn notification_types(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let config = repository::named_configuration_value(&state.pool, "notifications").await?;
    let enabled_types = notification_enabled_types(&config);
    Ok(Json(
        default_notification_types()
            .into_iter()
            .map(|mut item| {
                let enabled = item
                    .get("Type")
                    .and_then(Value::as_str)
                    .is_some_and(|value| enabled_types.iter().any(|candidate| candidate == value));
                if let Some(object) = item.as_object_mut() {
                    object.insert("Enabled".to_string(), json!(enabled));
                }
                item
            })
            .collect(),
    ))
}

async fn notification_services(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(build_notification_services(&state).await?))
}

async fn notification_service_defaults(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<IdQuery>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let service_id = query.id.unwrap_or_else(|| "log".to_string());
    let services = build_notification_services(&state).await?;
    let service = services
        .into_iter()
        .find(|item| item.get("Id").and_then(Value::as_str) == Some(service_id.as_str()))
        .unwrap_or_else(|| {
            json!({
                "Id": service_id,
                "Name": "Custom Notification Service",
                "Enabled": true,
                "Url": "",
                "Method": "POST",
                "ContentType": "application/json"
            })
        });
    Ok(Json(service))
}

async fn test_notification_service(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let service_id = payload
        .get("Id")
        .and_then(Value::as_str)
        .unwrap_or("log");
    let message = payload
        .get("Message")
        .and_then(Value::as_str)
        .unwrap_or("Notification test");
    let services = build_notification_services(&state).await?;
    let matched = services
        .into_iter()
        .find(|item| item.get("Id").and_then(Value::as_str) == Some(service_id))
        .ok_or_else(|| AppError::NotFound(format!("Notification service not found: {service_id}")))?;

    tracing::info!(
        service_id = service_id,
        message = message,
        "notification service test accepted"
    );

    Ok(Json(json!({
        "Success": true,
        "Service": matched,
        "Message": message,
        "Date": Utc::now()
    })))
}

async fn package_catalog(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<PackageQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(filter_packages(
        build_package_catalog(&state).await?,
        query.package_type.as_deref(),
    )))
}

async fn installed_packages(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<PackageQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(
        filter_packages(build_package_catalog(&state).await?, query.package_type.as_deref())
            .into_iter()
            .filter(|item| item.get("IsInstalled").and_then(Value::as_bool) == Some(true))
            .collect(),
    ))
}

async fn package_updates(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<PackageQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let stored = repository::named_configuration_value(&state.pool, "packages").await?;
    let available = stored
        .get("AvailableUpdates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(Json(filter_packages(available, query.package_type.as_deref())))
}

async fn sync_data(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    if let Some(target_id) = query.target_id.as_deref().filter(|value| !value.trim().is_empty()) {
        return Ok(Json(load_sync_target(&state, target_id).await?));
    }

    let mut targets = Vec::new();
    for target_id in load_sync_target_ids(&state).await? {
        targets.push(load_sync_target(&state, &target_id).await?);
    }
    Ok(Json(json!({
        "Targets": targets,
        "DateLastUpdated": Utc::now()
    })))
}

async fn update_sync_data(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let mut target = payload;
    let target_id = target
        .get("TargetId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or(query.target_id)
        .ok_or_else(|| AppError::BadRequest("TargetId is required".to_string()))?;

    let object = target
        .as_object_mut()
        .ok_or_else(|| AppError::BadRequest("Sync data must be an object".to_string()))?;
    object.insert("TargetId".to_string(), json!(target_id.clone()));
    object.entry("Items".to_string()).or_insert_with(|| json!([]));
    object.insert("DateLastUpdated".to_string(), json!(Utc::now()));

    store_sync_target(&state, &target_id, target.clone()).await?;
    Ok(Json(target))
}

async fn sync_offline_actions(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    Ok(Json(load_sync_offline_actions(&state).await?))
}

async fn record_sync_offline_actions(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let mut current = load_sync_offline_actions(&state).await?;
    let actions = if let Some(items) = payload.as_array() {
        items.clone()
    } else {
        vec![payload]
    };

    for action in &actions {
        apply_sync_action(&state, action).await?;
    }
    current.extend(actions);
    repository::set_json_setting(&state.pool, SYNC_OFFLINE_ACTIONS_KEY, json!(current)).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn sync_items_ready(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<SyncQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    auth::require_admin(&session)?;
    let target_id = query
        .target_id
        .ok_or_else(|| AppError::BadRequest("TargetId is required".to_string()))?;
    let target = load_sync_target(&state, &target_id).await?;
    let items = target
        .get("Items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|item| {
            item.get("Status")
                .and_then(Value::as_str)
                .is_none_or(|status| !matches!(status, "Transferred" | "Cancelled"))
        })
        .collect();
    Ok(Json(items))
}

async fn mark_sync_job_item_transferred(
    session: AuthSession,
    State(state): State<AppState>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    apply_sync_action(
        &state,
        &json!({
            "Type": "Transferred",
            "SyncJobItemId": job_id
        }),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn cancel_sync_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(target_id): Path<String>,
    Query(query): Query<CancelSyncItemsQuery>,
) -> Result<StatusCode, AppError> {
    auth::require_admin(&session)?;
    let item_ids = split_csv(query.item_ids.as_deref());
    if item_ids.is_empty() {
        return Err(AppError::BadRequest("ItemIds is required".to_string()));
    }

    let mut target = load_sync_target(&state, &target_id).await?;
    if let Some(items) = target.get_mut("Items").and_then(Value::as_array_mut) {
        items.retain(|item| {
            let item_id = item.get("ItemId").and_then(Value::as_str);
            let job_id = item.get("SyncJobItemId").and_then(Value::as_str);
            !item_ids
                .iter()
                .any(|candidate| Some(candidate.as_str()) == item_id || Some(candidate.as_str()) == job_id)
        });
    }
    store_sync_target(&state, &target_id, target).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) fn default_dlna_profile_value() -> Value {
    let direct_play_profiles = json!([
        {
            "Container": "mp4,mkv,webm,mov,avi,ts",
            "Type": "Video",
            "AudioCodec": "aac,mp3,ac3,eac3,flac",
            "VideoCodec": "h264,hevc,vp9"
        },
        {
            "Container": "mp3,flac,aac,m4a,ogg,wav",
            "Type": "Audio",
            "AudioCodec": "aac,mp3,flac,vorbis,pcm_s16le"
        }
    ]);
    let transcoding_profiles = json!([
        {
            "Container": "ts",
            "Type": "Video",
            "Protocol": "hls",
            "AudioCodec": "aac",
            "VideoCodec": "h264",
            "Context": "Streaming"
        },
        {
            "Container": "mp3",
            "Type": "Audio",
            "Protocol": "http",
            "AudioCodec": "mp3",
            "Context": "Streaming"
        }
    ]);
    let subtitle_profiles = json!([
        {
            "Format": "srt",
            "Method": "External"
        },
        {
            "Format": "vtt",
            "Method": "External"
        }
    ]);
    let identification = json!({
        "Headers": [],
        "FriendlyName": "",
        "ModelName": "",
        "ModelNumber": "",
        "ModelDescription": "",
        "ModelUrl": "",
        "Manufacturer": "",
        "ManufacturerUrl": "",
        "SerialNumber": "",
        "DeviceDescription": ""
    });

    json!({
        "Id": "default",
        "Name": "Movie Rust Default Profile",
        "Type": "System",
        "UserId": null,
        "SupportedMediaTypes": "Audio,Video,Photo",
        "FriendlyName": "Movie Rust DLNA",
        "Manufacturer": "Movie Rust",
        "ManufacturerUrl": "https://example.invalid/movie-rust",
        "ModelName": "Generic DLNA Client",
        "ModelNumber": "1",
        "ModelDescription": "Default DLNA profile generated by Movie Rust",
        "ModelUrl": "",
        "SerialNumber": "",
        "EnableAlbumArtInDidl": true,
        "EnableSingleAlbumArtLimit": false,
        "AlbumArtPn": "JPEG_TN",
        "MaxAlbumArtWidth": 1920,
        "MaxAlbumArtHeight": 1080,
        "MaxIconWidth": 256,
        "MaxIconHeight": 256,
        "IgnoreTranscodeByteRangeRequests": false,
        "MaxStreamingBitrate": 120000000,
        "MusicStreamingTranscodingBitrate": 192000,
        "RequiresPlainFolders": false,
        "RequiresPlainVideoItems": false,
        "ProtocolInfo": "http-get:*:*:*",
        "XDlnaCap": "",
        "XDlnaDoc": "DMS-1.50",
        "SonyAggregationFlags": "",
        "XmlRootAttributes": [],
        "DirectPlayProfiles": direct_play_profiles,
        "TranscodingProfiles": transcoding_profiles,
        "ContainerProfiles": [],
        "CodecProfiles": [],
        "ResponseProfiles": [],
        "SubtitleProfiles": subtitle_profiles,
        "Identification": identification
    })
}

fn dlna_profile_info(profile: &Value) -> Value {
    json!({
        "Id": profile.get("Id").and_then(Value::as_str).unwrap_or_default(),
        "Name": profile.get("Name").and_then(Value::as_str).unwrap_or("Unnamed Profile"),
        "Type": profile.get("Type").and_then(Value::as_str).unwrap_or("User")
    })
}

async fn load_user_dlna_profiles(state: &AppState) -> Result<Vec<Value>, AppError> {
    Ok(repository::get_json_setting(&state.pool, DLNA_PROFILES_KEY)
        .await?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default())
}

async fn save_user_dlna_profiles(state: &AppState, profiles: &[Value]) -> Result<(), AppError> {
    repository::set_json_setting(&state.pool, DLNA_PROFILES_KEY, json!(profiles)).await
}

fn normalize_dlna_profile(payload: Value, existing_id: Option<&str>) -> Result<Value, AppError> {
    let mut object = payload
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::BadRequest("DLNA profile must be an object".to_string()))?;

    let id = existing_id
        .map(ToOwned::to_owned)
        .or_else(|| {
            object
                .get("Id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| format!("dlna-{}", Uuid::new_v4().simple()));
    let name = object
        .get("Name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Custom DLNA Profile")
        .to_string();

    object.insert("Id".to_string(), json!(id));
    object.insert("Name".to_string(), json!(name));
    object.insert("Type".to_string(), json!("User"));
    object
        .entry("SupportedMediaTypes".to_string())
        .or_insert_with(|| json!("Audio,Video,Photo"));
    object
        .entry("DirectPlayProfiles".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("TranscodingProfiles".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("ContainerProfiles".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("CodecProfiles".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("ResponseProfiles".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("SubtitleProfiles".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("XmlRootAttributes".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("Identification".to_string())
        .or_insert_with(|| json!({ "Headers": [] }));

    Ok(Value::Object(object))
}

fn default_notification_types() -> Vec<Value> {
    vec![
        json!({
            "Type": "PlaybackStart",
            "Name": "Playback Started",
            "Category": "Playback",
            "IsBasedOnUserEvent": true,
            "Variables": ["UserName", "ItemName", "DeviceName"],
            "DefaultTitle": "Playback started"
        }),
        json!({
            "Type": "LibraryScanCompleted",
            "Name": "Library Scan Completed",
            "Category": "Library",
            "IsBasedOnUserEvent": false,
            "Variables": ["LibraryName", "CompletedAt"],
            "DefaultTitle": "Library scan completed"
        }),
        json!({
            "Type": "PluginStateChanged",
            "Name": "Plugin State Changed",
            "Category": "System",
            "IsBasedOnUserEvent": false,
            "Variables": ["PluginName", "State"],
            "DefaultTitle": "Plugin state changed"
        }),
    ]
}

fn notification_enabled_types(config: &Value) -> Vec<String> {
    config
        .get("Options")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|item| item.get("Enabled").and_then(Value::as_bool) == Some(true))
        .filter_map(|item| item.get("Type").and_then(Value::as_str))
        .map(ToOwned::to_owned)
        .collect()
}

async fn build_notification_services(state: &AppState) -> Result<Vec<Value>, AppError> {
    let config = repository::server_configuration_value(&state.pool, &state.config).await?;
    let targets = config
        .get("NotificationTargetsText")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .enumerate()
        .map(|(index, target)| {
            json!({
                "Id": format!("target-{index}"),
                "Name": target,
                "Type": "Webhook",
                "Target": target,
                "Enabled": true
            })
        });

    let mut services = vec![json!({
        "Id": "log",
        "Name": "Server Log",
        "Type": "BuiltIn",
        "Target": "tracing",
        "Enabled": true
    })];
    services.extend(targets);
    Ok(services)
}

async fn build_package_catalog(state: &AppState) -> Result<Vec<Value>, AppError> {
    let mut packages = vec![json!({
        "Id": "movie-rust-server",
        "Name": "Movie Rust Server",
        "Description": "Core Movie Rust server package",
        "Version": env!("CARGO_PKG_VERSION"),
        "LatestVersion": env!("CARGO_PKG_VERSION"),
        "PackageType": "System",
        "Classification": "System",
        "IsInstalled": true
    })];

    packages.extend(crate::routes::system::build_plugins(state).await?.into_iter().map(|item| {
        json!({
            "Id": item.get("Id").and_then(Value::as_str).unwrap_or_default(),
            "Name": item.get("Name").cloned().unwrap_or_else(|| json!("Unnamed Plugin")),
            "Description": item.get("Description").cloned().unwrap_or_else(|| json!("")),
            "Version": item.get("Version").cloned().unwrap_or_else(|| json!(env!("CARGO_PKG_VERSION"))),
            "LatestVersion": item.get("Version").cloned().unwrap_or_else(|| json!(env!("CARGO_PKG_VERSION"))),
            "PackageType": "UserInstalled",
            "Classification": "Plugin",
            "IsInstalled": true,
            "Enabled": item.get("Enabled").cloned().unwrap_or_else(|| json!(false))
        })
    }));

    Ok(packages)
}

fn filter_packages(items: Vec<Value>, package_type: Option<&str>) -> Vec<Value> {
    let Some(package_type) = package_type.filter(|value| !value.trim().is_empty()) else {
        return items;
    };
    items
        .into_iter()
        .filter(|item| {
            item.get("PackageType")
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(package_type))
        })
        .collect()
}

async fn load_sync_target_ids(state: &AppState) -> Result<Vec<String>, AppError> {
    Ok(repository::get_json_setting(&state.pool, SYNC_TARGET_INDEX_KEY)
        .await?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| value.as_str().map(ToOwned::to_owned))
        .collect())
}

async fn save_sync_target_ids(state: &AppState, ids: &[String]) -> Result<(), AppError> {
    repository::set_json_setting(&state.pool, SYNC_TARGET_INDEX_KEY, json!(ids)).await
}

fn sync_target_key(target_id: &str) -> String {
    format!("feature:sync_target:{target_id}")
}

async fn load_sync_target(state: &AppState, target_id: &str) -> Result<Value, AppError> {
    Ok(repository::get_json_setting(&state.pool, &sync_target_key(target_id))
        .await?
        .unwrap_or_else(|| {
            json!({
                "TargetId": target_id,
                "Items": [],
                "DateLastUpdated": Utc::now()
            })
        }))
}

async fn store_sync_target(state: &AppState, target_id: &str, value: Value) -> Result<(), AppError> {
    repository::set_json_setting(&state.pool, &sync_target_key(target_id), value).await?;
    let mut ids = load_sync_target_ids(state).await?;
    if !ids.iter().any(|value| value == target_id) {
        ids.push(target_id.to_string());
        ids.sort();
        ids.dedup();
        save_sync_target_ids(state, &ids).await?;
    }
    Ok(())
}

async fn load_sync_offline_actions(state: &AppState) -> Result<Vec<Value>, AppError> {
    Ok(repository::get_json_setting(&state.pool, SYNC_OFFLINE_ACTIONS_KEY)
        .await?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default())
}

async fn apply_sync_action(state: &AppState, action: &Value) -> Result<(), AppError> {
    let action_type = action
        .get("Type")
        .or_else(|| action.get("Action"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let target_id = action
        .get("TargetId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let item_id = action
        .get("ItemId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let job_id = action
        .get("SyncJobItemId")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);

    let mut target_ids = if let Some(target_id) = target_id {
        vec![target_id]
    } else {
        load_sync_target_ids(state).await?
    };
    target_ids.sort();
    target_ids.dedup();

    for target_id in target_ids {
        let mut target = load_sync_target(state, &target_id).await?;
        let mut changed = false;
        if let Some(items) = target.get_mut("Items").and_then(Value::as_array_mut) {
            match action_type.to_ascii_lowercase().as_str() {
                "transferred" => {
                    for item in items.iter_mut() {
                        let matches = item
                            .get("SyncJobItemId")
                            .and_then(Value::as_str)
                            .zip(job_id.as_deref())
                            .is_some_and(|(left, right)| left == right);
                        if matches {
                            if let Some(object) = item.as_object_mut() {
                                object.insert("Status".to_string(), json!("Transferred"));
                                object.insert("DateTransferred".to_string(), json!(Utc::now()));
                            }
                            changed = true;
                        }
                    }
                }
                "cancelled" | "delete" => {
                    let before = items.len();
                    items.retain(|item| {
                        let current_item_id = item.get("ItemId").and_then(Value::as_str);
                        let current_job_id = item.get("SyncJobItemId").and_then(Value::as_str);
                        !(item_id.as_deref() == current_item_id || job_id.as_deref() == current_job_id)
                    });
                    changed = before != items.len();
                }
                _ => {}
            }
        }
        if changed {
            if let Some(object) = target.as_object_mut() {
                object.insert("DateLastUpdated".to_string(), json!(Utc::now()));
            }
            store_sync_target(state, &target_id, target).await?;
        }
    }
    Ok(())
}

fn split_csv(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

pub(crate) fn auth_pin_key(device_id: &str, pin: &str) -> String {
    format!("feature:auth_pin:{}:{}", device_id.trim(), pin.trim())
}

pub(crate) fn build_auth_pin(device_id: &str, app_name: Option<&str>) -> Value {
    let raw = Uuid::new_v4().as_u128() % 1_000_000;
    let pin = format!("{raw:06}");
    json!({
        "Pin": pin,
        "DeviceId": device_id,
        "AppName": app_name.unwrap_or("Movie Rust Client"),
        "DateCreated": Utc::now(),
        "ExpirationDate": Utc::now() + Duration::minutes(10),
        "IsConfirmed": false,
        "IsExpired": false,
        "HasTimeout": false,
        "LinkUrl": format!("/web/#/pin?code={pin}")
    })
}
