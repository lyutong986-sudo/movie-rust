use crate::{auth::AuthSession, error::AppError, repository, state::AppState};
use axum::{
    extract::{Path, Query, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/DisplayPreferences/{display_preferences_id}", get(display_preferences))
        .route("/DisplayPreferences/{display_preferences_id}", post(update_display_preferences))
        .route(
            "/Users/{user_id}/DisplayPreferences/{display_preferences_id}",
            get(user_display_preferences).post(update_user_display_preferences),
        )
        .route("/Localization/Options", get(localization_options))
        .route("/Localization/Cultures", get(localization_cultures))
        .route("/UserSettings/{user_id}", get(user_settings))
        .route("/Users/{user_id}/Settings", get(user_settings))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DisplayPreferencesQuery {
    #[serde(default, alias = "UserId", alias = "userId")]
    user_id: Option<Uuid>,
    #[serde(default, alias = "Client", alias = "client")]
    client: Option<String>,
}

async fn display_preferences(
    session: AuthSession,
    State(state): State<AppState>,
    Path(display_preferences_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
) -> Result<Json<Value>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    let client = normalized_display_preferences_client(query.client.as_deref());
    if let Some(saved) = repository::get_display_preferences(
        &state.pool,
        user_id,
        &display_preferences_id,
        &client,
    )
    .await?
    {
        return Ok(Json(saved));
    }
    Ok(Json(display_preferences_value(display_preferences_id, user_id, &client)))
}

async fn user_display_preferences(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, display_preferences_id)): Path<(Uuid, String)>,
    Query(query): Query<DisplayPreferencesQuery>,
) -> Result<Json<Value>, AppError> {
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let client = normalized_display_preferences_client(query.client.as_deref());
    if let Some(saved) = repository::get_display_preferences(
        &state.pool,
        user_id,
        &display_preferences_id,
        &client,
    )
    .await?
    {
        return Ok(Json(saved));
    }
    Ok(Json(display_preferences_value(display_preferences_id, user_id, &client)))
}

async fn update_display_preferences(
    session: AuthSession,
    State(state): State<AppState>,
    Path(display_preferences_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
    Json(mut payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    update_display_preferences_for_user(&state, &session, user_id, display_preferences_id, query.client.as_deref(), &mut payload).await
}

async fn update_user_display_preferences(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, display_preferences_id)): Path<(Uuid, String)>,
    Query(query): Query<DisplayPreferencesQuery>,
    Json(mut payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    update_display_preferences_for_user(&state, &session, user_id, display_preferences_id, query.client.as_deref(), &mut payload).await
}

async fn update_display_preferences_for_user(
    state: &AppState,
    session: &AuthSession,
    user_id: Uuid,
    display_preferences_id: String,
    client: Option<&str>,
    payload: &mut Value,
) -> Result<Json<Value>, AppError> {
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let client = normalized_display_preferences_client(client);
    if let Some(object) = payload.as_object_mut() {
        object.entry("Id".to_string()).or_insert_with(|| json!(display_preferences_id.clone()));
        object.entry("UserId".to_string()).or_insert_with(|| json!(user_id.to_string().to_uppercase()));
        object.entry("Client".to_string()).or_insert_with(|| json!(client.clone()));
    }
    let saved = repository::upsert_display_preferences(
        &state.pool,
        user_id,
        &display_preferences_id,
        &client,
        payload.clone(),
    )
    .await?;
    Ok(Json(saved))
}

fn normalized_display_preferences_client(client: Option<&str>) -> String {
    client
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("emby")
        .to_string()
}

fn display_preferences_value(id: String, user_id: Uuid, client: &str) -> Value {
    json!({
        "Id": id,
        "UserId": user_id.to_string().to_uppercase(),
        "Client": client,
        "ViewType": "Poster",
        "SortBy": "SortName",
        "IndexBy": "SortName",
        "RememberIndexing": false,
        "PrimaryImageHeight": 250,
        "PrimaryImageWidth": 166,
        "ScrollDirection": "Horizontal",
        "ShowBackdrop": true,
        "ShowSidebar": true,
        "CustomPrefs": {}
    })
}

async fn localization_options(_session: AuthSession) -> Json<Value> {
    Json(json!([
        { "Name": "中文（简体）", "Value": "zh-CN" },
        { "Name": "English", "Value": "en-US" }
    ]))
}

async fn localization_cultures(_session: AuthSession) -> Json<Value> {
    Json(json!([
        { "DisplayName": "中文（简体）", "Name": "zh-CN", "ThreeLetterISOLanguageName": "zho", "TwoLetterISOLanguageName": "zh" },
        { "DisplayName": "English", "Name": "en-US", "ThreeLetterISOLanguageName": "eng", "TwoLetterISOLanguageName": "en" }
    ]))
}

async fn user_settings(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }

    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let dto = repository::user_to_dto(&user, state.config.server_id);

    Ok(Json(json!({
        "UserId": dto.id,
        "Configuration": dto.configuration,
        "Policy": dto.policy
    })))
}
