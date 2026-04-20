use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    error::AppError,
    models::{BaseItemDto, GenreDto},
    repository,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct GetGenresQuery {
    user_id: Option<String>,
    start_index: Option<i32>,
    limit: Option<i32>,
}

pub async fn get_genres(
    State(state): State<AppState>,
    Query(query): Query<GetGenresQuery>,
) -> Result<Json<Vec<GenreDto>>, AppError> {
    let genres = repository::get_genres(&state.pool, query.start_index, query.limit).await?;
    Ok(Json(genres))
}

pub async fn get_genre_items(
    State(state): State<AppState>,
    Path(genre_name): Path<String>,
    Query(query): Query<GetGenresQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let items = repository::get_items_by_genre(&state.pool, &genre_name, query.start_index, query.limit).await?;
    Ok(Json(items))
}

pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/Genres", axum::routing::get(get_genres))
        .route("/Genres/{genreName}/Items", axum::routing::get(get_genre_items))
}