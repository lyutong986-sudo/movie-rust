use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use std::collections::BTreeMap;

use crate::{
    auth::AuthSession,
    error::AppError,
    models::{emby_id_to_uuid, BaseItemDto, GenreDto, QueryResult},
    repository::{self, ItemListOptions},
    state::AppState,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetGenresQuery {
    #[serde(default, alias = "StartIndex", alias = "startIndex")]
    start_index: Option<i32>,
    #[serde(default, alias = "Limit", alias = "limit")]
    limit: Option<i32>,
    #[serde(default, alias = "UserId", alias = "userId")]
    user_id: Option<uuid::Uuid>,
    #[serde(default, alias = "ParentId", alias = "parentId")]
    parent_id: Option<String>,
    #[serde(default, alias = "IncludeItemTypes", alias = "includeItemTypes")]
    include_item_types: Option<String>,
    #[serde(default, alias = "Recursive", alias = "recursive", deserialize_with = "crate::models::deserialize_option_bool_lenient")]
    recursive: Option<bool>,
}

pub async fn get_genres(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<GetGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }

    let parent_id = query
        .parent_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(emby_id_to_uuid)
        .transpose()
        .map_err(|_| AppError::BadRequest("无效的 ParentId".to_string()))?;
    let include_types = parse_list(query.include_item_types.as_deref());
    let recursive = query.recursive.unwrap_or(true);

    let (genres, total) = if parent_id.is_none() && include_types.is_empty() && recursive {
        repository::get_genres(&state.pool, query.start_index, query.limit, Some(user_id)).await?
    } else {
        let g = genres_for_scope(&state, user_id, parent_id, include_types, recursive).await?;
        let len = g.len() as i64;
        (g, len)
    };

    let items: Vec<BaseItemDto> = genres
        .into_iter()
        .map(|genre| genre_to_base_item(genre, state.config.server_id))
        .collect();

    Ok(Json(QueryResult {
        total_record_count: total,
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
    session: AuthSession,
    State(state): State<AppState>,
    Path(genre_name): Path<String>,
    Query(query): Query<GetGenresQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }

    let parent_id = query
        .parent_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(emby_id_to_uuid)
        .transpose()
        .map_err(|_| AppError::BadRequest("无效的 ParentId".to_string()))?;
    let include_types = parse_list(query.include_item_types.as_deref());
    let recursive = query.recursive.unwrap_or(true);

    let items = if parent_id.is_none() && include_types.is_empty() && recursive {
        repository::get_items_by_genre(
            &state.pool,
            &genre_name,
            state.config.server_id,
            query.start_index,
            query.limit,
            Some(user_id),
        )
        .await?
    } else {
        genre_items_for_scope(
            &state,
            user_id,
            &genre_name,
            parent_id,
            include_types,
            recursive,
            query.start_index,
            query.limit,
        )
        .await?
    };
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
    item.image_tags = genre
        .image_tags
        .unwrap_or_default()
        .into_iter()
        .collect::<BTreeMap<_, _>>();
    item
}

pub fn router() -> axum::Router<crate::state::AppState> {
    axum::Router::new()
        .route("/Genres", axum::routing::get(get_genres))
        .route("/MusicGenres", axum::routing::get(get_genres))
        .route("/GameGenres", axum::routing::get(get_genres))
        .route(
            "/Users/{userId}/Genres",
            axum::routing::get(get_user_genres),
        )
        .route(
            "/Genres/{genreName}/Items",
            axum::routing::get(get_genre_items),
        )
        .route("/Genres/{genreName}", axum::routing::get(get_genre))
        .route(
            "/MusicGenres/{genreName}/Items",
            axum::routing::get(get_genre_items),
        )
        .route("/MusicGenres/{genreName}", axum::routing::get(get_genre))
        .route(
            "/GameGenres/{genreName}/Items",
            axum::routing::get(get_genre_items),
        )
        .route("/GameGenres/{genreName}", axum::routing::get(get_genre))
}

pub async fn get_user_genres(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<uuid::Uuid>,
    Query(mut query): Query<GetGenresQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    if session.user_id != user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    query.user_id = Some(user_id);
    get_genres(session, State(state), Query(query)).await
}

async fn genres_for_scope(
    state: &AppState,
    user_id: uuid::Uuid,
    parent_id: Option<uuid::Uuid>,
    include_types: Vec<String>,
    recursive: bool,
) -> Result<Vec<GenreDto>, AppError> {
    let (library_id, scoped_parent_id) = match parent_id {
        Some(parent_id) => {
            if let Some(library) = repository::get_library(&state.pool, parent_id).await? {
                (Some(library.id), None)
            } else {
                (None, Some(parent_id))
            }
        }
        None => (None, None),
    };

    let result = repository::list_media_items(
        &state.pool,
        ItemListOptions {
            user_id: Some(user_id),
            library_id,
            parent_id: scoped_parent_id,
            include_types,
            recursive,
            start_index: 0,
            limit: 10_000,
            ..ItemListOptions::default()
        },
    )
    .await?;

    let mut genres = std::collections::BTreeSet::new();
    for item in result.items {
        for genre in item.genres {
            let genre = genre.trim();
            if !genre.is_empty() {
                genres.insert(genre.to_string());
            }
        }
    }

    Ok(genres
        .into_iter()
        .map(|name| GenreDto {
            name,
            id: None,
            image_tags: None,
        })
        .collect())
}

async fn genre_items_for_scope(
    state: &AppState,
    user_id: uuid::Uuid,
    genre_name: &str,
    parent_id: Option<uuid::Uuid>,
    include_types: Vec<String>,
    recursive: bool,
    start_index: Option<i32>,
    limit: Option<i32>,
) -> Result<Vec<BaseItemDto>, AppError> {
    let (library_id, scoped_parent_id) = match parent_id {
        Some(parent_id) => {
            if let Some(library) = repository::get_library(&state.pool, parent_id).await? {
                (Some(library.id), None)
            } else {
                (None, Some(parent_id))
            }
        }
        None => (None, None),
    };

    let result = repository::list_media_items(
        &state.pool,
        ItemListOptions {
            user_id: Some(user_id),
            library_id,
            parent_id: scoped_parent_id,
            include_types,
            genres: vec![genre_name.to_string()],
            recursive,
            sort_by: Some("SortName".to_string()),
            sort_order: Some("Ascending".to_string()),
            start_index: start_index.unwrap_or(0).max(0) as i64,
            limit: limit.unwrap_or(100).clamp(1, 200) as i64,
            ..ItemListOptions::default()
        },
    )
    .await?;

    let mut items = Vec::with_capacity(result.items.len());
    for item in result.items {
        items.push(
            repository::media_item_to_dto(
                &state.pool,
                &item,
                Some(user_id),
                state.config.server_id,
            )
            .await?,
        );
    }

    Ok(items)
}

fn parse_list(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split([',', '|'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
