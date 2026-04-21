use crate::{
    auth::AuthSession,
    error::AppError,
    models::{emby_id_to_uuid, uuid_to_emby_guid, optional_uuid_to_emby_guid, EpisodeDto, EpisodesQuery, QueryResult, SeasonDto, SeasonsQuery},
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
) -> Result<Json<QueryResult<SeasonDto>>, AppError> {
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
) -> Result<Json<QueryResult<EpisodeDto>>, AppError> {
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
) -> Result<Json<QueryResult<EpisodeDto>>, AppError> {
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
) -> Result<SeasonDto, AppError> {
    // 获取用户数据
    let user_data = repository::get_user_item_data_dto(&state.pool, user_id, season.id)
        .await
        .ok();

    // 获取子项数量（剧集数量）
    let child_count = repository::count_item_children(&state.pool, season.id)
        .await
        .unwrap_or(0);

    // 构建图片标签
    let mut image_tags = std::collections::BTreeMap::new();
    if season.image_primary_path.is_some() {
        image_tags.insert(
            "Primary".to_string(),
            season.date_modified.timestamp().to_string(),
        );
    }

    Ok(SeasonDto {
        name: season.name.clone(),
        server_id: uuid_to_emby_guid(&state.config.server_id),
        id: uuid_to_emby_guid(&season.id),
        item_type: "Season".to_string(),
        is_folder: true,
        sort_name: Some(season.sort_name.clone()),
        index_number: season.index_number,
        parent_index_number: season.parent_index_number,
        series_name: season.series_name.clone(),
        series_id: optional_uuid_to_emby_guid(season.parent_id),
        overview: season.overview.clone(),
        premiere_date: season.premiere_date,
        production_year: season.production_year,
        image_tags,
        image_primary_path: season.image_primary_path.clone(),
        child_count: Some(child_count as i64),
        user_data,
    })
}

async fn episode_to_dto(
    state: &AppState,
    user_id: Uuid,
    episode: &crate::models::DbMediaItem,
) -> Result<EpisodeDto, AppError> {
    // 获取用户数据
    let user_data = repository::get_user_item_data_dto(&state.pool, user_id, episode.id)
        .await
        .ok();

    // 获取媒体源和媒体流
    let media_sources = Some(vec![
        repository::get_media_source_with_streams(&state.pool, episode, state.config.server_id)
            .await?,
    ]);
    let media_streams = media_sources
        .as_ref()
        .map(|sources| {
            sources
                .first()
                .map(|source| source.media_streams.clone())
                .unwrap_or_default()
        })
        .filter(|streams| !streams.is_empty())
        .or_else(|| Some(repository::media_streams_for_item(episode)));

    // 构建图片标签
    let mut image_tags = std::collections::BTreeMap::new();
    if episode.image_primary_path.is_some() {
        image_tags.insert(
            "Primary".to_string(),
            episode.date_modified.timestamp().to_string(),
        );
    }

    // 获取系列和季信息
    let series_name = episode.series_name.clone();
    let season_name = episode.season_name.clone();

    let series_id = if let Some(parent_id) = episode.parent_id {
        // 尝试找到系列ID（可能是祖父级）
        if let Some(parent_item) = repository::get_media_item(&state.pool, parent_id).await? {
            if parent_item.item_type == "Season" {
                optional_uuid_to_emby_guid(parent_item.parent_id)
            } else {
                Some(uuid_to_emby_guid(&parent_id))
            }
        } else {
            None
        }
    } else {
        None
    };

    let season_id = optional_uuid_to_emby_guid(episode.parent_id);

    Ok(EpisodeDto {
        name: episode.name.clone(),
        server_id: uuid_to_emby_guid(&state.config.server_id),
        id: uuid_to_emby_guid(&episode.id),
        item_type: "Episode".to_string(),
        is_folder: false,
        sort_name: Some(episode.sort_name.clone()),
        index_number: episode.index_number,
        index_number_end: episode.index_number_end,
        parent_index_number: episode.parent_index_number,
        series_name,
        series_id,
        season_name,
        season_id,
        overview: episode.overview.clone(),
        premiere_date: episode.premiere_date,
        production_year: episode.production_year,
        run_time_ticks: episode.runtime_ticks,
        image_tags,
        image_primary_path: episode.image_primary_path.clone(),
        user_data,
        media_sources,
        media_streams,
    })
}

fn ensure_user_access(session: &AuthSession, user_id: Uuid) -> Result<(), AppError> {
    if session.user_id == user_id || session.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}
