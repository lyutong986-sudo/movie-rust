use crate::{
    auth::AuthSession,
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
use serde::Deserialize;
use serde_json::{json, Value};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Plugins", get(plugins))
        .route("/Plugins/SecurityInfo", get(plugin_security_info).post(update_plugin_security_info))
        .route(
            "/Plugins/{id}/Configuration",
            get(plugin_configuration).post(update_plugin_configuration),
        )
        .route("/Connect/Pending", get(connect_pending).delete(delete_connect_pending))
        .route("/News/Product", get(product_news))
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

async fn plugins(_session: AuthSession) -> Json<Vec<Value>> {
    Json(Vec::new())
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

async fn product_news(_session: AuthSession) -> Json<Value> {
    Json(json!({
        "Items": [],
        "TotalRecordCount": 0
    }))
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
    Json(json!({
        "Name": name,
        "guid": null,
        "versions": [],
        "IsPremium": false
    }))
}

async fn package_updates(
    _session: AuthSession,
) -> Json<Vec<Value>> {
    Json(Vec::new())
}

async fn install_package(
    _session: AuthSession,
    Path(name): Path<String>,
) -> Json<Value> {
    Json(json!({
        "Name": name,
        "Status": "Queued"
    }))
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
