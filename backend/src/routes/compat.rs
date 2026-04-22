use crate::{
    auth::AuthSession,
    error::AppError,
    models::UserConfigurationDto,
    repository,
    state::AppState,
};
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
        .route("/Localization/Countries", get(localization_countries))
        .route("/Localization/ParentalRatings", get(localization_parental_ratings))
        .route("/UserSettings/{user_id}", get(user_settings).post(update_user_settings))
        .route("/UserSettings/{user_id}/Partial", post(update_user_settings_partial))
        .route("/Users/{user_id}/Settings", get(user_settings).post(update_user_settings))
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
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let mut value = display_preferences_value(
        display_preferences_id,
        user_id,
        &client,
        &startup.ui_culture,
    );
    if let Some(template) = repository::get_display_preferences_template(&state.pool, &client).await?
    {
        merge_json(&mut value, template);
    }
    Ok(Json(value))
}

async fn user_display_preferences(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, display_preferences_id)): Path<(Uuid, String)>,
    Query(query): Query<DisplayPreferencesQuery>,
) -> Result<Json<Value>, AppError> {
    ensure_settings_access(&session, user_id)?;
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
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let mut value = display_preferences_value(
        display_preferences_id,
        user_id,
        &client,
        &startup.ui_culture,
    );
    if let Some(template) = repository::get_display_preferences_template(&state.pool, &client).await?
    {
        merge_json(&mut value, template);
    }
    Ok(Json(value))
}

async fn update_display_preferences(
    session: AuthSession,
    State(state): State<AppState>,
    Path(display_preferences_id): Path<String>,
    Query(query): Query<DisplayPreferencesQuery>,
    Json(mut payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    update_display_preferences_for_user(
        &state,
        &session,
        user_id,
        display_preferences_id,
        query.client.as_deref(),
        &mut payload,
    )
    .await
}

async fn update_user_display_preferences(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, display_preferences_id)): Path<(Uuid, String)>,
    Query(query): Query<DisplayPreferencesQuery>,
    Json(mut payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    update_display_preferences_for_user(
        &state,
        &session,
        user_id,
        display_preferences_id,
        query.client.as_deref(),
        &mut payload,
    )
    .await
}

async fn update_display_preferences_for_user(
    state: &AppState,
    session: &AuthSession,
    user_id: Uuid,
    display_preferences_id: String,
    client: Option<&str>,
    payload: &mut Value,
) -> Result<Json<Value>, AppError> {
    ensure_settings_access(session, user_id)?;
    let client = normalized_display_preferences_client(client);
    if let Some(object) = payload.as_object_mut() {
        object
            .entry("Id".to_string())
            .or_insert_with(|| json!(display_preferences_id.clone()));
        object
            .entry("UserId".to_string())
            .or_insert_with(|| json!(user_id.to_string().to_uppercase()));
        object
            .entry("Client".to_string())
            .or_insert_with(|| json!(client.clone()));
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

fn display_preferences_value(id: String, user_id: Uuid, client: &str, ui_culture: &str) -> Value {
    let lower_client = client.to_ascii_lowercase();
    let (view_type, primary_image_width, primary_image_height) =
        if lower_client.contains("androidtv") || lower_client.contains("tv") {
            ("Thumb", 320, 180)
        } else {
            ("Poster", 166, 250)
        };

    json!({
        "Id": id,
        "UserId": user_id.to_string().to_uppercase(),
        "Client": client,
        "ViewType": view_type,
        "SortBy": "SortName",
        "SortOrder": "Ascending",
        "IndexBy": "SortName",
        "RememberIndexing": false,
        "RememberSorting": false,
        "PrimaryImageHeight": primary_image_height,
        "PrimaryImageWidth": primary_image_width,
        "ScrollDirection": "Horizontal",
        "ShowBackdrop": true,
        "ShowSidebar": true,
        "ShowLocalTrailers": true,
        "ShowMissingEpisodes": false,
        "CustomPrefs": {
            "landing-libraries": "views",
            "skip-details": "false",
            "ui-culture": ui_culture
        }
    })
}

async fn localization_options(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let mut ordered = vec![startup.ui_culture.clone(), "zh-CN".to_string(), "en-US".to_string()];
    ordered.dedup();
    let mut options = Vec::new();
    for culture in ordered {
        if options.iter().any(|entry: &Value| entry["Value"] == culture) {
            continue;
        }
        options.push(json!({
            "Name": culture_display_name(&culture),
            "Value": culture
        }));
    }
    Ok(Json(json!(options)))
}
async fn localization_cultures(
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let mut ordered = vec![startup.ui_culture.clone(), "zh-CN".to_string(), "en-US".to_string()];
    ordered.dedup();
    let cultures = ordered
        .into_iter()
        .map(|culture| {
            let language = culture_language_code(&culture, &startup.preferred_metadata_language);
            json!({
                "DisplayName": culture_display_name(&culture),
                "Name": culture,
                "ThreeLetterISOLanguageName": to_three_letter_language(&language),
                "TwoLetterISOLanguageName": to_two_letter_language(&language)
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!(cultures)))
}

async fn localization_countries() -> Result<Json<Value>, AppError> {
    Ok(Json(json!([
        country_info("China", "中国", "CN", "CHN"),
        country_info("United States", "United States", "US", "USA"),
        country_info("Taiwan", "台湾", "TW", "TWN"),
        country_info("Hong Kong", "香港", "HK", "HKG"),
        country_info("Japan", "日本", "JP", "JPN"),
        country_info("South Korea", "대한민국", "KR", "KOR"),
        country_info("United Kingdom", "United Kingdom", "GB", "GBR")
    ])))
}

async fn localization_parental_ratings() -> Result<Json<Value>, AppError> {
    Ok(Json(json!([
        { "Name": "G", "Value": 1 },
        { "Name": "PG", "Value": 5 },
        { "Name": "PG-13", "Value": 7 },
        { "Name": "R", "Value": 9 },
        { "Name": "NC-17", "Value": 10 },
        { "Name": "TV-Y", "Value": 1 },
        { "Name": "TV-Y7", "Value": 3 },
        { "Name": "TV-G", "Value": 4 },
        { "Name": "TV-PG", "Value": 5 },
        { "Name": "TV-14", "Value": 7 },
        { "Name": "TV-MA", "Value": 9 }
    ])))
}

async fn user_settings(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Value>, AppError> {
    ensure_settings_access(&session, user_id)?;

    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let dto = repository::user_to_dto(&user, state.config.server_id);
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;

    Ok(Json(json!({
        "UserId": dto.id,
        "Configuration": dto.configuration,
        "Policy": dto.policy,
        "PreferredMetadataLanguage": startup.preferred_metadata_language,
        "PreferredMetadataCountryCode": startup.metadata_country_code
    })))
}

async fn update_user_settings(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(configuration): Json<UserConfigurationDto>,
) -> Result<Json<Value>, AppError> {
    ensure_settings_access(&session, user_id)?;
    repository::update_user_configuration(&state.pool, user_id, &configuration).await?;
    Ok(Json(json!(configuration)))
}

async fn update_user_settings_partial(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Json(payload): Json<Value>,
) -> Result<Json<Value>, AppError> {
    ensure_settings_access(&session, user_id)?;
    let user = repository::get_user_by_id(&state.pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let dto = repository::user_to_dto(&user, state.config.server_id);
    let mut current = serde_json::to_value(dto.configuration)?;
    merge_json(&mut current, payload);
    let next = serde_json::from_value::<UserConfigurationDto>(current.clone())
        .map_err(|error| AppError::BadRequest(format!("无效的 UserSettings 请求: {error}")))?;
    repository::update_user_configuration(&state.pool, user_id, &next).await?;
    Ok(Json(current))
}

fn ensure_settings_access(session: &AuthSession, user_id: Uuid) -> Result<(), AppError> {
    if session.user_id != user_id && !session.is_admin {
        Err(AppError::Forbidden)
    } else {
        Ok(())
    }
}

fn merge_json(target: &mut Value, patch: Value) {
    match (target, patch) {
        (Value::Object(target_map), Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                match target_map.get_mut(&key) {
                    Some(existing) => merge_json(existing, value),
                    None => {
                        target_map.insert(key, value);
                    }
                }
            }
        }
        (slot, value) => *slot = value,
    }
}
fn culture_display_name(culture: &str) -> String {
    match culture.to_ascii_lowercase().as_str() {
        "zh-cn" => "中文（简体）".to_string(),
        "zh-tw" => "中文（繁体）".to_string(),
        "en-us" => "English".to_string(),
        _ => culture.to_string(),
    }
}
fn culture_language_code(culture: &str, fallback: &str) -> String {
    culture
        .split(['-', '_'])
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(fallback)
        .to_ascii_lowercase()
}
fn to_two_letter_language(language: &str) -> String {
    language.chars().take(2).collect::<String>().to_ascii_lowercase()
}
fn to_three_letter_language(language: &str) -> String {
    match language.to_ascii_lowercase().as_str() {
        "zh" => "zho".to_string(),
        "en" => "eng".to_string(),
        other => other.to_string(),
    }
}

fn country_info(name: &str, display_name: &str, two_letter: &str, three_letter: &str) -> Value {
    json!({
        "Name": name,
        "DisplayName": display_name,
        "TwoLetterISORegionName": two_letter,
        "ThreeLetterISORegionName": three_letter
    })
}
