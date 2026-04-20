use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{BaseItemDto, PersonDto},
    repository,
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetPersonsQuery {
    user_id: Option<String>,
    start_index: Option<i32>,
    limit: Option<i32>,
    fields: Option<String>,
    filters: Option<String>,
    sort_by: Option<String>,
    sort_order: Option<String>,
    name_starts_with: Option<String>,
}

pub async fn get_persons(
    State(state): State<AppState>,
    Query(query): Query<GetPersonsQuery>,
) -> Result<Json<Vec<PersonDto>>, AppError> {
    let persons = repository::get_persons(
        &state.pool,
        query.start_index,
        query.limit,
        query.name_starts_with,
    ).await?;
    Ok(Json(persons))
}

pub async fn get_person(
    State(state): State<AppState>,
    Path(person_id_or_name): Path<String>,
) -> Result<Json<PersonDto>, AppError> {
    // 尝试解析为UUID
    if let Ok(uuid) = Uuid::parse_str(&person_id_or_name) {
        // 按ID查找
        if let Some(person) = repository::get_person_by_uuid(&state.pool, uuid).await? {
            return Ok(Json(person));
        }
    }
    
    // 按名称查找
    let person = repository::get_person_by_name(&state.pool, &person_id_or_name).await?;
    Ok(Json(person))
}

pub async fn get_person_items(
    State(state): State<AppState>,
    Path(person_id): Path<String>,
    Query(query): Query<GetPersonsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let items = repository::get_items_by_person(
        &state.pool,
        &person_id,
        query.start_index,
        query.limit,
    ).await?;
    Ok(Json(items))
}

pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/Persons", axum::routing::get(get_persons))
        .route("/Persons/{personId}", axum::routing::get(get_person))
        .route("/Persons/{personId}/Items", axum::routing::get(get_person_items))
}