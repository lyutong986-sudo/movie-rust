use crate::{
    auth::AuthSession,
    error::AppError,
    models::{emby_id_to_uuid, BaseItemDto, EpisodesQuery, QueryResult, SeasonsQuery},
    repository::{self, ItemListOptions},
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Shows/{series_id}/Seasons", get(get_seasons))
        .route("/Shows/{series_id}/Episodes", get(get_episodes))
        .route("/Seasons/{season_id}/Episodes", get(get_episodes_by_season))
}

async fn get_seasons(
    session: AuthSession,
    State(state): State<AppState>,
    Path(series_id_str): Path<String>,
    Query(query): Query<SeasonsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let series_id = emby_id_to_uuid(&series_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的系列ID格式: {}", series_id_str)))?;
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;

    // 首先验证系列是否存在
    let series = repository::get_media_item(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Series not found".to_string()))?;

    if series.item_type != "Series" {
        return Err(AppError::BadRequest("Item is not a series".to_string()));
    }

    // 获取该系列下的所有季
    let seasons = repository::list_media_items(
        &state.pool,
        ItemListOptions {
            library_id: None,
            parent_id: Some(series_id),
            item_ids: vec![],
            include_types: vec!["Season".to_string()],
            genres: vec![],
            recursive: false,
            search_term: None,
            sort_by: Some("SortName".to_string()),
            sort_order: Some("Ascending".to_string()),
            filters: None,
            fields: None,
            start_index: 0,
            limit: 1000, // 假设季的数量不会太多
        },
    )
    .await?;

    let mut season_dtos = Vec::with_capacity(seasons.items.len());
    for season in seasons.items {
        season_dtos.push(season_to_dto(&state, user_id, &season).await?);
    }

    Ok(Json(QueryResult {
        items: season_dtos,
        total_record_count: seasons.total_record_count,
        start_index: Some(0),
    }))
}

async fn get_episodes(
    session: AuthSession,
    State(state): State<AppState>,
    Path(series_id_str): Path<String>,
    Query(query): Query<EpisodesQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let series_id = emby_id_to_uuid(&series_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的系列ID格式: {}", series_id_str)))?;
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;

    // 首先验证系列是否存在
    let series = repository::get_media_item(&state.pool, series_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Series not found".to_string()))?;

    if series.item_type != "Series" {
        return Err(AppError::BadRequest("Item is not a series".to_string()));
    }

    let mut parent_id = Some(series_id);
    let mut recursive = true;
    // 如果提供了 SeasonId，则只获取该季；否则递归获取整部剧下的分集。
    if let Some(season_id) = query.season_id {
        let season = repository::get_media_item(&state.pool, season_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Season not found".to_string()))?;

        if season.item_type != "Season" {
            return Err(AppError::BadRequest("Item is not a season".to_string()));
        }

        parent_id = Some(season_id);
        recursive = false;
    }

    // 获取剧集
    let episodes = repository::list_media_items(
        &state.pool,
        ItemListOptions {
            library_id: None,
            parent_id,
            item_ids: vec![],
            include_types: vec!["Episode".to_string()],
            genres: vec![],
            recursive,
            search_term: None,
            sort_by: Some("SortName".to_string()),
            sort_order: Some("Ascending".to_string()),
            filters: None,
            fields: None,
            start_index: query.start_index.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
        },
    )
    .await?;

    let mut episode_dtos = Vec::with_capacity(episodes.items.len());
    for episode in episodes.items {
        episode_dtos.push(episode_to_dto(&state, user_id, &episode).await?);
    }

    Ok(Json(QueryResult {
        items: episode_dtos,
        total_record_count: episodes.total_record_count,
        start_index: query.start_index,
    }))
}

async fn get_episodes_by_season(
    session: AuthSession,
    State(state): State<AppState>,
    Path(season_id_str): Path<String>,
    Query(query): Query<EpisodesQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let season_id = emby_id_to_uuid(&season_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的季节ID格式: {}", season_id_str)))?;
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;

    // 验证季是否存在
    let season = repository::get_media_item(&state.pool, season_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Season not found".to_string()))?;

    if season.item_type != "Season" {
        return Err(AppError::BadRequest("Item is not a season".to_string()));
    }

    // 获取该季下的所有剧集
    let episodes = repository::list_media_items(
        &state.pool,
        ItemListOptions {
            library_id: None,
            parent_id: Some(season_id),
            item_ids: vec![],
            include_types: vec!["Episode".to_string()],
            genres: vec![],
            recursive: false,
            search_term: None,
            sort_by: Some("SortName".to_string()),
            sort_order: Some("Ascending".to_string()),
            filters: None,
            fields: None,
            start_index: query.start_index.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
        },
    )
    .await?;

    let mut episode_dtos = Vec::with_capacity(episodes.items.len());
    for episode in episodes.items {
        episode_dtos.push(episode_to_dto(&state, user_id, &episode).await?);
    }

    Ok(Json(QueryResult {
        items: episode_dtos,
        total_record_count: episodes.total_record_count,
        start_index: query.start_index,
    }))
}

async fn season_to_dto(
    state: &AppState,
    user_id: Uuid,
    season: &crate::models::DbMediaItem,
) -> Result<BaseItemDto, AppError> {
    repository::media_item_to_dto(
        &state.pool,
        season,
        Some(user_id),
        state.config.server_id,
    )
    .await
}

async fn episode_to_dto(
    state: &AppState,
    user_id: Uuid,
    episode: &crate::models::DbMediaItem,
) -> Result<BaseItemDto, AppError> {
    repository::media_item_to_dto(
        &state.pool,
        episode,
        Some(user_id),
        state.config.server_id,
    )
    .await
}

fn ensure_user_access(session: &AuthSession, user_id: Uuid) -> Result<(), AppError> {
    if session.user_id == user_id || session.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}
