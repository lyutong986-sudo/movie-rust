use crate::{
    auth::AuthSession,
    error::AppError,
    models::{emby_id_to_uuid, uuid_to_emby_guid, BaseItemDto, DbPlaylist, QueryResult},
    repository,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Playlists", post(create_playlist).get(list_playlists_me))
        .route("/playlists", post(create_playlist).get(list_playlists_me))
        .route(
            "/Playlists/{playlist_id}",
            get(get_playlist).post(update_playlist),
        )
        .route(
            "/playlists/{playlist_id}",
            get(get_playlist).post(update_playlist),
        )
        .route(
            "/Playlists/{playlist_id}/Delete",
            post(delete_playlist_route),
        )
        .route("/Playlists/{playlist_id}", delete(delete_playlist_route))
        .route(
            "/Playlists/{playlist_id}/Items",
            get(list_playlist_items).post(add_playlist_items_route).delete(remove_playlist_items_route),
        )
        .route(
            "/playlists/{playlist_id}/items",
            get(list_playlist_items).post(add_playlist_items_route).delete(remove_playlist_items_route),
        )
        .route(
            "/Playlists/{playlist_id}/Items/Delete",
            post(remove_playlist_items_route),
        )
        .route(
            "/Playlists/{playlist_id}/Items/{entry_id}/Move/{new_index}",
            post(move_playlist_item_route),
        )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct PlaylistDto {
    id: String,
    name: String,
    server_id: String,
    media_type: String,
    user_id: String,
    overview: Option<String>,
    child_count: i64,
    date_created: String,
    date_modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    primary_image_tag: Option<String>,
}

impl PlaylistDto {
    fn from_db(server_id: uuid::Uuid, playlist: &DbPlaylist, child_count: i64) -> Self {
        Self {
            id: uuid_to_emby_guid(&playlist.id),
            name: playlist.name.clone(),
            server_id: uuid_to_emby_guid(&server_id),
            media_type: playlist.media_type.clone(),
            user_id: uuid_to_emby_guid(&playlist.user_id),
            overview: playlist.overview.clone(),
            child_count,
            date_created: playlist.created_at.to_rfc3339(),
            date_modified: playlist.updated_at.to_rfc3339(),
            primary_image_tag: playlist.image_primary_path.clone(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct CreatePlaylistRequest {
    name: String,
    #[serde(default, alias = "mediaType")]
    media_type: Option<String>,
    #[serde(default)]
    overview: Option<String>,
    #[serde(default, alias = "ids", alias = "Ids")]
    ids: Option<Vec<String>>,
    #[serde(default, alias = "userId")]
    user_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct UpdatePlaylistRequest {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    overview: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
struct ListItemsQuery {
    #[serde(default, alias = "startIndex", alias = "StartIndex")]
    start_index: Option<i64>,
    #[serde(default, alias = "limit", alias = "Limit")]
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct AddItemsQuery {
    #[serde(default, alias = "ids", alias = "Ids")]
    ids: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RemoveItemsRequest {
    #[serde(
        default,
        alias = "entryIds",
        alias = "EntryIds",
        alias = "playlistItemIds",
        alias = "PlaylistItemIds"
    )]
    entry_ids: Option<Vec<String>>,
}

async fn create_playlist(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<CreatePlaylistRequest>,
) -> Result<Json<PlaylistDto>, AppError> {
    let owner_id = payload
        .user_id
        .as_deref()
        .map(|value| emby_id_to_uuid(value))
        .transpose()
        .map_err(|error| AppError::BadRequest(format!("无效的 UserId: {error}")))?
        .unwrap_or(session.user_id);
    if owner_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("播放列表名称不能为空".to_string()));
    }
    let media_type = payload.media_type.unwrap_or_else(|| "Video".to_string());
    let created = repository::create_playlist(
        &state.pool,
        owner_id,
        name,
        &media_type,
        payload.overview.as_deref(),
    )
    .await?;
    if let Some(ids) = payload.ids {
        let parsed: Vec<Uuid> = ids
            .into_iter()
            .filter_map(|value| emby_id_to_uuid(&value).ok())
            .collect();
        repository::add_playlist_items(&state.pool, created.id, &parsed).await?;
    }
    let child_count = repository::list_playlist_items(&state.pool, created.id)
        .await?
        .len() as i64;
    Ok(Json(PlaylistDto::from_db(
        state.config.server_id,
        &created,
        child_count,
    )))
}

async fn list_playlists_me(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<QueryResult<PlaylistDto>>, AppError> {
    let playlists = repository::list_playlists_for_user(&state.pool, session.user_id).await?;
    let mut items = Vec::with_capacity(playlists.len());
    for playlist in playlists {
        let child_count = repository::list_playlist_items(&state.pool, playlist.id)
            .await?
            .len() as i64;
        items.push(PlaylistDto::from_db(
            state.config.server_id,
            &playlist,
            child_count,
        ));
    }
    let total = items.len() as i64;
    Ok(Json(QueryResult {
        items,
        total_record_count: total,
        start_index: Some(0),
    }))
}

async fn get_playlist(
    session: AuthSession,
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
) -> Result<Json<PlaylistDto>, AppError> {
    let id = emby_id_to_uuid(&playlist_id)
        .map_err(|error| AppError::BadRequest(format!("无效的播放列表 ID: {error}")))?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    if playlist.user_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let child_count = repository::list_playlist_items(&state.pool, playlist.id)
        .await?
        .len() as i64;
    Ok(Json(PlaylistDto::from_db(
        state.config.server_id,
        &playlist,
        child_count,
    )))
}

async fn update_playlist(
    session: AuthSession,
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Json(payload): Json<UpdatePlaylistRequest>,
) -> Result<Json<PlaylistDto>, AppError> {
    let id = emby_id_to_uuid(&playlist_id)
        .map_err(|error| AppError::BadRequest(format!("无效的播放列表 ID: {error}")))?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    if playlist.user_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let name_ref = payload.name.as_deref();
    let overview_ref = payload.overview.as_ref().map(|value| value.as_deref());
    repository::update_playlist(&state.pool, id, name_ref, overview_ref).await?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    let child_count = repository::list_playlist_items(&state.pool, playlist.id)
        .await?
        .len() as i64;
    Ok(Json(PlaylistDto::from_db(
        state.config.server_id,
        &playlist,
        child_count,
    )))
}

async fn delete_playlist_route(
    session: AuthSession,
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = emby_id_to_uuid(&playlist_id)
        .map_err(|error| AppError::BadRequest(format!("无效的播放列表 ID: {error}")))?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    if playlist.user_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    repository::delete_playlist(&state.pool, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_playlist_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Query(query): Query<ListItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let id = emby_id_to_uuid(&playlist_id)
        .map_err(|error| AppError::BadRequest(format!("无效的播放列表 ID: {error}")))?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    if playlist.user_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let entries = repository::list_playlist_items(&state.pool, id).await?;
    let start_index = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(200).clamp(1, 2000) as usize;
    let total = entries.len() as i64;
    let selected: Vec<_> = entries.into_iter().skip(start_index).take(limit).collect();
    let mut items = Vec::with_capacity(selected.len());
    for entry in selected {
        let Some(media) = repository::get_media_item(&state.pool, entry.media_item_id).await?
        else {
            continue;
        };
        let mut dto = repository::media_item_to_dto(
            &state.pool,
            &media,
            Some(session.user_id),
            state.config.server_id,
        )
        .await?;
        dto.playlist_item_id = Some(entry.playlist_item_id.clone());
        items.push(dto);
    }
    Ok(Json(QueryResult {
        items,
        total_record_count: total,
        start_index: Some(start_index as i64),
    }))
}

async fn add_playlist_items_route(
    session: AuthSession,
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Query(query): Query<AddItemsQuery>,
) -> Result<StatusCode, AppError> {
    let id = emby_id_to_uuid(&playlist_id)
        .map_err(|error| AppError::BadRequest(format!("无效的播放列表 ID: {error}")))?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    if playlist.user_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let ids = query
        .ids
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| emby_id_to_uuid(value).ok())
        .collect::<Vec<_>>();
    repository::add_playlist_items(&state.pool, id, &ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_playlist_items_route(
    session: AuthSession,
    State(state): State<AppState>,
    Path(playlist_id): Path<String>,
    Json(payload): Json<RemoveItemsRequest>,
) -> Result<StatusCode, AppError> {
    let id = emby_id_to_uuid(&playlist_id)
        .map_err(|error| AppError::BadRequest(format!("无效的播放列表 ID: {error}")))?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    if playlist.user_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let ids = payload.entry_ids.unwrap_or_default();
    repository::remove_playlist_items(&state.pool, id, &ids).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn move_playlist_item_route(
    session: AuthSession,
    State(state): State<AppState>,
    Path((playlist_id, entry_id, new_index)): Path<(String, String, i32)>,
) -> Result<StatusCode, AppError> {
    let id = emby_id_to_uuid(&playlist_id)
        .map_err(|error| AppError::BadRequest(format!("无效的播放列表 ID: {error}")))?;
    let playlist = repository::get_playlist(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("播放列表不存在".to_string()))?;
    if playlist.user_id != session.user_id && !session.is_admin {
        return Err(AppError::Forbidden);
    }
    repository::move_playlist_item(&state.pool, id, &entry_id, new_index).await?;
    Ok(StatusCode::NO_CONTENT)
}
