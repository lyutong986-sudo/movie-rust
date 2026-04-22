use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::{
    auth::AuthSession,
    error::AppError,
    models::{BaseItemDto, GenreDto, QueryResult},
    repository,
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetGenresQuery {
    #[serde(default, alias = "StartIndex", alias = "startIndex")]
    start_index: Option<i32>,
    #[serde(default, alias = "Limit")]
    limit: Option<i32>,
}

pub async fn get_genres(
    _session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<GetGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let genres = repository::get_genres(&state.pool, query.start_index, query.limit).await?;
    let items: Vec<BaseItemDto> = genres
        .into_iter()
        .map(|genre| genre_to_base_item(genre, state.config.server_id))
        .collect();

    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(query.start_index.unwrap_or(0).max(0) as i64),
    }))
}

pub async fn get_genre(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(genre_name): Path<String>,
) -> Result<Json<BaseItemDto>, AppError> {
    Ok(Json(genre_to_base_item(
        GenreDto {
            name: genre_name,
            id: None,
            image_tags: None,
        },
        state.config.server_id,
    )))
}

pub async fn get_genre_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(genre_name): Path<String>,
    Query(query): Query<GetGenresQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let items = repository::get_items_by_genre(
        &state.pool,
        &genre_name,
        state.config.server_id,
        query.start_index,
        query.limit,
    )
    .await?;
    Ok(Json(items))
}

fn genre_to_base_item(genre: GenreDto, server_id: uuid::Uuid) -> BaseItemDto {
    let mut item = repository::root_item_dto(server_id);
    let id = genre.id.clone().unwrap_or_else(|| genre.name.clone());
    item.name = genre.name;
    item.id = id.clone();
    item.guid = None;
    item.etag = None;
    item.can_delete = false;
    item.can_download = false;
    item.can_edit_items = Some(false);
    item.presentation_unique_key = Some(format!("{id}_"));
    item.item_type = "Genre".to_string();
    item.is_folder = true;
    item.sort_name = Some(item.name.to_lowercase());
    item.forced_sort_name = item.sort_name.clone();
    item.location_type = Some("Virtual".to_string());
    item.display_preferences_id = Some(id);
    item.size = None;
    item.special_feature_count = None;
    item.image_tags = genre.image_tags.unwrap_or_default().into_iter().collect::<BTreeMap<_, _>>();
    item
}

pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/Genres", axum::routing::get(get_genres))
        .route("/Users/{userId}/Genres", axum::routing::get(get_user_genres))
        .route("/Genres/{genreName}/Items", axum::routing::get(get_genre_items))
        .route("/Genres/{genreName}", axum::routing::get(get_genre))
}

pub async fn get_user_genres(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<uuid::Uuid>,
    Query(query): Query<GetGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    get_genres(session, State(state), Query(query)).await
}
