use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    error::AppError,
    models::{BaseItemDto, PersonDto, QueryResult},
    repository,
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetPersonsQuery {
    #[serde(default, alias = "StartIndex", alias = "startIndex")]
    start_index: Option<i32>,
    #[serde(default, alias = "Limit")]
    limit: Option<i32>,
    #[serde(default, alias = "Fields")]
    fields: Option<String>,
    #[serde(default, alias = "Filters")]
    filters: Option<String>,
    #[serde(default, alias = "SortBy", alias = "sortBy")]
    sort_by: Option<String>,
    #[serde(default, alias = "SortOrder", alias = "sortOrder")]
    sort_order: Option<String>,
    #[serde(default, alias = "NameStartsWith", alias = "nameStartsWith")]
    name_starts_with: Option<String>,
}

pub async fn get_persons(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<GetPersonsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let persons = repository::get_persons(
        &state.pool,
        query.start_index,
        query.limit,
        query.name_starts_with,
    )
    .await?;
    let items: Vec<BaseItemDto> = persons
        .into_iter()
        .map(|person| person_to_base_item(person, state.config.server_id))
        .collect();

    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(query.start_index.unwrap_or(0).max(0) as i64),
    }))
}

pub async fn get_person(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(person_id_or_name): Path<String>,
) -> Result<Json<BaseItemDto>, AppError> {
    if let Ok(uuid) = Uuid::parse_str(&person_id_or_name) {
        if let Some(person) = repository::get_person_by_uuid(&state.pool, uuid).await? {
            return Ok(Json(person_to_base_item(person, state.config.server_id)));
        }
    }

    let person = repository::get_person_by_name(&state.pool, &person_id_or_name).await?;
    Ok(Json(person_to_base_item(person, state.config.server_id)))
}

pub async fn get_person_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(person_id): Path<String>,
    Query(query): Query<GetPersonsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let items = repository::get_items_by_person(
        &state.pool,
        &person_id,
        state.config.server_id,
        query.start_index,
        query.limit,
    )
    .await?;
    Ok(Json(items))
}

fn person_to_base_item(person: PersonDto, server_id: Uuid) -> BaseItemDto {
    let mut item = repository::root_item_dto(server_id);
    let mut image_tags = BTreeMap::new();
    if let Some(tag) = person.primary_image_tag.clone() {
        image_tags.insert("Primary".to_string(), tag);
    }

    item.name = person.name.clone();
    item.id = person.id.clone();
    item.guid = None;
    item.etag = person.primary_image_tag.clone();
    item.can_delete = false;
    item.can_download = false;
    item.can_edit_items = Some(false);
    item.presentation_unique_key = Some(format!("{}_", person.id));
    item.item_type = "Person".to_string();
    item.is_folder = true;
    item.sort_name = person
        .sort_name
        .or_else(|| Some(person.name.to_lowercase()));
    item.forced_sort_name = item.sort_name.clone();
    item.primary_image_tag = person.primary_image_tag;
    item.overview = person.overview;
    item.production_year = person.production_year;
    item.location_type = Some("Virtual".to_string());
    item.display_preferences_id = Some(person.id);
    item.size = None;
    item.special_feature_count = None;
    item.image_tags = image_tags;
    item.provider_ids = person
        .provider_ids
        .unwrap_or_default()
        .into_iter()
        .collect();
    item
}

pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/Persons", axum::routing::get(get_persons))
        .route(
            "/Persons/{personId}/Items",
            axum::routing::get(get_person_items),
        )
        .route("/Persons/{personId}", axum::routing::get(get_person))
}
