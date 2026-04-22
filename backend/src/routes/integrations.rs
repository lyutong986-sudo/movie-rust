use crate::{
    auth::AuthSession,
    error::AppError,
    repository,
    state::AppState,
};
use axum::{
    extract::{Form, Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Plugins", get(plugins))
        .route("/Plugins/{id}", delete(uninstall_plugin))
        .route("/Plugins/SecurityInfo", get(plugin_security_info).post(update_plugin_security_info))
        .route(
            "/Plugins/{id}/Configuration",
            get(plugin_configuration).post(update_plugin_configuration),
        )
        .route("/Connect/Pending", get(connect_pending).delete(delete_connect_pending))
        .route("/Connect/Invite", post(connect_invite))
        .route("/News/Product", get(product_news))
        .route("/Packages", get(packages))
        .route("/Packages/{id}/Reviews", get(package_reviews))
        .route("/Packages/Reviews/{id}", post(create_package_review))
        .route("/Packages/{name}", get(package_info))
        .route("/Packages/Updates", get(package_updates))
        .route("/Packages/Installed/{name}", post(install_package))
        .route("/Packages/Installing/{id}", delete(cancel_package_installation))
        .route("/Registrations/{feature}", get(registration_info))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PendingQuery {
    #[serde(default)]
    id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ConnectInviteRequest {
    connect_username: Option<String>,
    enabled_libraries: Option<String>,
    sending_user_id: Option<String>,
    enable_live_tv: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct PackagesQuery {
    #[serde(default)]
    target_systems: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
struct InstallPackageQuery {
    #[serde(default)]
    guid: Option<String>,
    #[serde(default, alias = "updateClass")]
    update_class: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

async fn plugins(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    Ok(Json(load_installed_plugins(&state).await?))
}

async fn uninstall_plugin(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let mut installed = load_installed_plugins(&state).await?;
    installed.retain(|plugin| plugin.get("Id").and_then(Value::as_str) != Some(id.as_str()));
    persist_installed_plugins(&state, &installed).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn plugin_security_info(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let value = repository::named_system_configuration(&state.pool, "plugins_security_info")
        .await?
        .unwrap_or_else(|| {
            json!({
                "SupporterKey": "",
                "IsMBSupporter": false
            })
        });
    Ok(Json(value))
}

async fn update_plugin_security_info(
    _session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    repository::update_named_system_configuration(&state.pool, "plugins_security_info", &payload)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn plugin_configuration(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let value = repository::named_system_configuration(&state.pool, &format!("plugin_{id}_configuration"))
        .await?
        .unwrap_or_else(|| json!({}));
    Ok(Json(value))
}

async fn update_plugin_configuration(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<StatusCode, AppError> {
    repository::update_named_system_configuration(
        &state.pool,
        &format!("plugin_{id}_configuration"),
        &payload,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn connect_pending(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn delete_connect_pending(
    _session: AuthSession,
    Query(_query): Query<PendingQuery>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn connect_invite(
    _session: AuthSession,
    Form(payload): Form<ConnectInviteRequest>,
) -> Json<Value> {
    let guest_display_name = payload
        .connect_username
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Guest");
    Json(json!({
        "IsPending": false,
        "IsNewUserInvitation": false,
        "GuestDisplayName": guest_display_name,
        "EnabledLibraries": payload.enabled_libraries.unwrap_or_default(),
        "SendingUserId": payload.sending_user_id,
        "EnableLiveTv": payload.enable_live_tv.unwrap_or(false)
    }))
}

async fn product_news(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0
    }))
}

async fn packages(
    _session: AuthSession,
    Query(query): Query<PackagesQuery>,
) -> Json<Vec<Value>> {
    let requested_system = query
        .target_systems
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let items = available_packages()
        .into_iter()
        .filter(|package| match requested_system {
            Some(system) => package
                .get("targetSystem")
                .and_then(Value::as_str)
                .map(|target| target.eq_ignore_ascii_case(system))
                .unwrap_or(false),
            None => true,
        })
        .collect();
    Json(items)
}

async fn package_reviews(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn create_package_review(
    _session: AuthSession,
    Path(id): Path<String>,
) -> Json<Value> {
    Json(json!({
        "Id": id,
        "Status": "Created"
    }))
}

async fn package_info(
    _session: AuthSession,
    Path(name): Path<String>,
) -> Json<Value> {
    Json(find_package(&name).unwrap_or_else(|| {
        json!({
            "name": name,
            "guid": null,
            "versions": [],
            "targetSystem": "Server",
            "isPremium": false
        })
    }))
}

async fn package_updates(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<Value>>, AppError> {
    let installed = load_installed_plugins(&state).await?;
    let updates = installed
        .into_iter()
        .filter_map(|plugin| {
            let id = plugin.get("Id").and_then(Value::as_str)?;
            let version = plugin.get("Version").and_then(Value::as_str).unwrap_or_default();
            let package = available_packages()
                .into_iter()
                .find(|candidate| candidate.get("guid").and_then(Value::as_str) == Some(id))?;
            let latest = latest_package_version(&package)?;
            if latest.eq_ignore_ascii_case(version) {
                None
            } else {
                Some(json!({
                    "guid": id,
                    "name": package.get("name").cloned().unwrap_or(Value::Null),
                    "currentVersion": version,
                    "availableVersion": latest
                }))
            }
        })
        .collect();
    Ok(Json(updates))
}

async fn install_package(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<InstallPackageQuery>,
) -> Result<Json<Value>, AppError> {
    let package = find_package(&name).unwrap_or_else(|| {
        json!({
            "name": name,
            "guid": format!("custom-{}", name.to_lowercase()),
            "targetSystem": "Server",
            "versions": [{
                "versionStr": "1.0.0.0",
                "classification": "Release",
                "description": "Local compatibility package"
            }],
            "shortDescription": "Local compatibility package",
            "overview": "Local compatibility package",
            "owner": "Movie Rust",
            "isPremium": false
        })
    });

    let installed_version = query
        .version
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| latest_package_version(&package))
        .unwrap_or("1.0.0.0");
    let guid = package
        .get("guid")
        .and_then(Value::as_str)
        .or(query.guid.as_deref())
        .unwrap_or("custom-package");
    let mut installed = load_installed_plugins(&state).await?;

    if let Some(existing) = installed.iter_mut().find(|plugin| {
        plugin.get("Id").and_then(Value::as_str) == Some(guid)
    }) {
        *existing = installed_plugin_from_package(&package, installed_version);
    } else {
        installed.push(installed_plugin_from_package(&package, installed_version));
    }

    persist_installed_plugins(&state, &installed).await?;

    Ok(Json(json!({
        "Name": package.get("name").cloned().unwrap_or(Value::String(name)),
        "Guid": guid,
        "Status": "Completed",
        "UpdateClass": query.update_class
    })))
}

async fn cancel_package_installation(
    _session: AuthSession,
    Path(_id): Path<String>,
) -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn registration_info(
    _session: AuthSession,
    Path(feature): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let security = repository::named_system_configuration(&state.pool, "plugins_security_info")
        .await?
        .unwrap_or_else(|| json!({}));
    let is_registered = security
        .get("IsMBSupporter")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(Json(json!({
        "Name": feature,
        "IsRegistered": is_registered,
        "ExpirationDate": null
    })))
}

async fn load_installed_plugins(state: &AppState) -> Result<Vec<Value>, AppError> {
    let stored = repository::named_system_configuration(&state.pool, "installed_plugins")
        .await?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    Ok(stored)
}

async fn persist_installed_plugins(state: &AppState, plugins: &[Value]) -> Result<(), AppError> {
    repository::update_named_system_configuration(
        &state.pool,
        "installed_plugins",
        &Value::Array(plugins.to_vec()),
    )
    .await
}

fn find_package(name: &str) -> Option<Value> {
    available_packages().into_iter().find(|package| {
        package
            .get("name")
            .and_then(Value::as_str)
            .map(|candidate| candidate.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    })
}

fn latest_package_version(package: &Value) -> Option<&str> {
    package
        .get("versions")
        .and_then(Value::as_array)
        .and_then(|versions| {
            versions
                .iter()
                .find(|version| {
                    version
                        .get("classification")
                        .and_then(Value::as_str)
                        .map(|classification| classification.eq_ignore_ascii_case("Release"))
                        .unwrap_or(false)
                })
                .or_else(|| versions.first())
        })
        .and_then(|version| version.get("versionStr"))
        .and_then(Value::as_str)
}

fn installed_plugin_from_package(package: &Value, version: &str) -> Value {
    json!({
        "Id": package.get("guid").cloned().unwrap_or(Value::Null),
        "Name": package.get("name").cloned().unwrap_or(Value::Null),
        "Version": version,
        "Description": package.get("shortDescription").cloned().unwrap_or(Value::Null),
        "Status": "Active",
        "ImageUrl": package.get("thumbImage").cloned().unwrap_or(Value::Null),
        "ConfigurationFileName": format!(
            "{}.json",
            package
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("plugin")
                .to_lowercase()
                .replace(' ', "-")
        )
    })
}

fn available_packages() -> Vec<Value> {
    vec![
        json!({
            "name": "Metadata Manager",
            "guid": "6D0F4E31-33D2-4C8C-B6F5-7D1F9A6A7001",
            "category": "Metadata",
            "categoryDisplayName": "Metadata",
            "targetSystem": "Server",
            "type": "UserInstalled",
            "installs": 4821,
            "thumbImage": Value::Null,
            "previewImage": Value::Null,
            "externalUrl": Value::Null,
            "isPremium": false,
            "isRegistered": false,
            "price": 0.0,
            "featureId": Value::Null,
            "shortDescription": "Adds batch metadata cleanup and refresh helpers.",
            "overview": "Provides metadata cleanup, image refresh presets, and editor shortcuts for Emby-compatible libraries.",
            "owner": "Movie Rust",
            "richDescUrl": Value::Null,
            "versions": [
                {
                    "versionStr": "1.2.0.0",
                    "classification": "Release",
                    "description": "Stable metadata maintenance tools for the dashboard."
                },
                {
                    "versionStr": "1.3.0.0-beta",
                    "classification": "Beta",
                    "description": "Preview support for advanced remote metadata refresh."
                }
            ]
        }),
        json!({
            "name": "Theme Media Companion",
            "guid": "7A2E0C24-03B1-4F9F-95C9-D7C0D5A19002",
            "category": "Theme",
            "categoryDisplayName": "Themes",
            "targetSystem": "Server",
            "type": "UserInstalled",
            "installs": 3174,
            "thumbImage": Value::Null,
            "previewImage": Value::Null,
            "externalUrl": Value::Null,
            "isPremium": false,
            "isRegistered": false,
            "price": 0.0,
            "featureId": Value::Null,
            "shortDescription": "Helps organize intro, outro and theme media assets.",
            "overview": "Scans media folders for theme songs, theme videos and intro clips so the dashboard can manage them consistently.",
            "owner": "Movie Rust",
            "richDescUrl": Value::Null,
            "versions": [
                {
                    "versionStr": "2.0.0.0",
                    "classification": "Release",
                    "description": "Initial Emby WebDashboard compatible release."
                }
            ]
        }),
        json!({
            "name": "Subtitle Toolkit",
            "guid": "8C3D9252-A0E2-4B5E-8D0A-9FD0B2BC9003",
            "category": "General",
            "categoryDisplayName": "General",
            "targetSystem": "Server",
            "type": "UserInstalled",
            "installs": 2590,
            "thumbImage": Value::Null,
            "previewImage": Value::Null,
            "externalUrl": Value::Null,
            "isPremium": false,
            "isRegistered": false,
            "price": 0.0,
            "featureId": Value::Null,
            "shortDescription": "Subtitle import and cleanup helpers.",
            "overview": "Provides dashboard actions for local subtitle indexing, cleanup and language tagging.",
            "owner": "Movie Rust",
            "richDescUrl": Value::Null,
            "versions": [
                {
                    "versionStr": "1.0.5.0",
                    "classification": "Release",
                    "description": "Adds subtitle indexing and cleanup compatibility endpoints."
                }
            ]
        }),
    ]
}
