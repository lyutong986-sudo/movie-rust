use crate::{
    auth::AuthSession,
    error::AppError,
    models::{emby_id_to_uuid, BaseItemDto, EpisodesQuery, ItemsQuery, QueryResult, SeasonsQuery},
    repository::{self, ItemListOptions},
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use std::collections::BTreeSet;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Shows/{series_id}/Seasons", get(get_seasons))
        .route("/Shows/{series_id}/Episodes", get(get_episodes))
        .route("/Seasons/{season_id}/Episodes", get(get_episodes_by_season))
        .route("/Shows/NextUp", get(get_next_up))
        .route("/Shows/Upcoming", get(get_upcoming))
        .route("/Shows/Missing", get(get_missing))
}

async fn get_next_up(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    let scope_id = query.series_id.or(query.parent_id);
    let result = repository::get_next_up_episodes(
        &state.pool,
        user_id,
        scope_id,
        state.config.server_id,
        0,
        10_000,
    )
    .await?;
    Ok(Json(apply_items_query_to_show_result(result.items, &query)))
}

async fn get_upcoming(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    let scope_id = query.series_id.or(query.parent_id);
    let result = repository::get_upcoming_episodes(
        &state.pool,
        user_id,
        scope_id,
        state.config.server_id,
        0,
        10_000,
    )
    .await?;
    Ok(Json(apply_items_query_to_show_result(result.items, &query)))
}

async fn get_missing(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    let scope_id = query.series_id.or(query.parent_id);
    let result = repository::get_missing_episodes(
        &state.pool,
        user_id,
        scope_id,
        state.config.server_id,
        0,
        10_000,
    )
    .await?;
    Ok(Json(apply_items_query_to_show_result(result.items, &query)))
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
            user_id: Some(user_id),
            recursive: false,
            search_term: None,
            sort_by: Some("SortName".to_string()),
            sort_order: Some("Ascending".to_string()),
            filters: None,
            fields: None,
            start_index: 0,
            limit: 1000, // 假设季的数量不会太多
            ..ItemListOptions::default()
        },
    )
    .await?;

    let missing_season_numbers = if query.is_missing == Some(true) {
        collect_missing_season_numbers(
            &repository::get_missing_episodes(
                &state.pool,
                user_id,
                Some(series_id),
                state.config.server_id,
                0,
                10_000,
            )
            .await?,
        )
    } else {
        BTreeSet::new()
    };

    let mut filtered_seasons = seasons.items;
    if query.is_special_season == Some(false) {
        filtered_seasons.retain(|season| season.index_number.unwrap_or(0) > 0);
    } else if query.is_special_season == Some(true) {
        filtered_seasons.retain(|season| season.index_number.unwrap_or(0) <= 0);
    }
    if query.is_missing == Some(true) {
        filtered_seasons.retain(|season| {
            season
                .index_number
                .is_some_and(|index| missing_season_numbers.contains(&index))
        });
    }

    let mut season_dtos = Vec::with_capacity(filtered_seasons.len());
    for season in filtered_seasons {
        season_dtos.push(season_to_dto(&state, user_id, &season).await?);
    }

    let season_dtos = apply_adjacent_items(season_dtos, query.adjacent_to.as_deref());
    let total_record_count = season_dtos.len() as i64;

    Ok(Json(QueryResult {
        items: season_dtos,
        total_record_count,
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
    } else if let Some(season_number) = query.season {
        let seasons = repository::list_media_items(
            &state.pool,
            ItemListOptions {
                library_id: None,
                parent_id: Some(series_id),
                item_ids: vec![],
                include_types: vec!["Season".to_string()],
                genres: vec![],
                user_id: Some(user_id),
                recursive: false,
                search_term: None,
                sort_by: None,
                sort_order: None,
                filters: None,
                fields: None,
                start_index: 0,
                limit: 200,
                ..ItemListOptions::default()
            },
        )
        .await?;

        if let Some(season) = seasons
            .items
            .into_iter()
            .find(|season| season.index_number == Some(season_number))
        {
            parent_id = Some(season.id);
            recursive = false;
        } else {
            return Ok(Json(QueryResult {
                items: Vec::new(),
                total_record_count: 0,
                start_index: Some(query.start_index.unwrap_or(0).max(0)),
            }));
        }
    }

    let mut episode_dtos = if query.is_missing == Some(true) {
        let mut items = repository::get_missing_episodes(
            &state.pool,
            user_id,
            parent_id,
            state.config.server_id,
            0,
            10_000,
        )
        .await?
        .items;

        if let Some(season_number) = query.season {
            items.retain(|item| item.parent_index_number == Some(season_number));
        }
        items
    } else {
        let episodes = repository::list_media_items(
            &state.pool,
            ItemListOptions {
                library_id: None,
                parent_id,
                item_ids: vec![],
                include_types: vec!["Episode".to_string()],
                genres: parse_list(query.genres.as_deref()),
                user_id: Some(user_id),
                media_types: parse_list(query.media_types.as_deref()),
                video_types: parse_list(query.video_types.as_deref()),
                image_types: parse_list(query.image_types.as_deref()),
                official_ratings: parse_list(query.official_ratings.as_deref()),
                tags: parse_list(query.tags.as_deref()),
                years: parse_i32_list(query.years.as_deref()),
                containers: parse_list(query.containers.as_deref()),
                audio_codecs: parse_list(query.audio_codecs.as_deref()),
                video_codecs: parse_list(query.video_codecs.as_deref()),
                subtitle_codecs: parse_list(query.subtitle_codecs.as_deref()),
                is_played: query.is_played,
                is_favorite: query.is_favorite,
                is_hd: query.is_hd,
                has_subtitles: query.has_subtitles,
                has_trailer: query.has_trailer,
                min_premiere_date: query.min_premiere_date,
                max_premiere_date: query.max_premiere_date,
                min_start_date: query.min_start_date,
                max_start_date: query.max_start_date,
                min_end_date: query.min_end_date,
                max_end_date: query.max_end_date,
                recursive,
                search_term: query.search_term.clone(),
                sort_by: query.sort_by.clone().or_else(|| Some("SortName".to_string())),
                sort_order: query
                    .sort_order
                    .clone()
                    .or_else(|| Some("Ascending".to_string())),
                filters: None,
                fields: query.fields.clone(),
                start_index: 0,
                limit: 10_000,
                ..ItemListOptions::default()
            },
        )
        .await?;

        let mut items = Vec::with_capacity(episodes.items.len());
        for episode in episodes.items {
            items.push(episode_to_dto(&state, user_id, &episode).await?);
        }
        items
    };

    apply_episode_sort(
        &mut episode_dtos,
        query.sort_by.as_deref(),
        query.sort_order.as_deref(),
    );
    episode_dtos = apply_start_item(episode_dtos, query.start_item_id.as_deref());
    episode_dtos = apply_adjacent_items(episode_dtos, query.adjacent_to.as_deref());

    let total_record_count = episode_dtos.len() as i64;
    let start_index = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(100).clamp(1, 200) as usize;
    let items = episode_dtos
        .into_iter()
        .skip(start_index)
        .take(limit)
        .collect::<Vec<_>>();

    Ok(Json(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index as i64),
    }))
}

fn collect_missing_season_numbers(result: &QueryResult<BaseItemDto>) -> BTreeSet<i32> {
    result
        .items
        .iter()
        .filter_map(|item| item.parent_index_number)
        .collect()
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

fn parse_i32_list(value: Option<&str>) -> Vec<i32> {
    value
        .unwrap_or_default()
        .split([',', '|'])
        .map(str::trim)
        .filter_map(|value| value.parse::<i32>().ok())
        .collect()
}

fn apply_items_query_to_show_result(
    mut items: Vec<BaseItemDto>,
    query: &ItemsQuery,
) -> QueryResult<BaseItemDto> {
    let media_types = parse_list(query.media_types.as_deref());
    let video_types = parse_list(query.video_types.as_deref());
    let image_types = parse_list(query.image_types.as_deref());
    let genres = parse_list(query.genres.as_deref());
    let official_ratings = parse_list(query.official_ratings.as_deref());
    let tags = parse_list(query.tags.as_deref());
    let years = parse_i32_list(query.years.as_deref());
    let containers = parse_list(query.containers.as_deref());
    let search_term = query.search_term.as_deref().map(str::trim).filter(|value| !value.is_empty()).map(str::to_ascii_lowercase);

    items.retain(|item| {
        if !media_types.is_empty()
            && !item
                .media_type
                .as_deref()
                .is_some_and(|value| contains_ignore_case(&media_types, value))
        {
            return false;
        }
        if !video_types.is_empty()
            && !video_types.iter().any(|value| {
                value.eq_ignore_ascii_case("Video")
                    || value.eq_ignore_ascii_case("VideoFile")
                    || value.eq_ignore_ascii_case("Episode")
            })
        {
            return false;
        }
        if !image_types.is_empty() && !matches_image_filter(item, &image_types) {
            return false;
        }
        if !genres.is_empty() && !item.genres.iter().any(|value| contains_ignore_case(&genres, value)) {
            return false;
        }
        if !official_ratings.is_empty()
            && !item
                .official_rating
                .as_deref()
                .is_some_and(|value| contains_ignore_case(&official_ratings, value))
        {
            return false;
        }
        if !tags.is_empty() && !item.tags.iter().any(|value| contains_ignore_case(&tags, value)) {
            return false;
        }
        if !years.is_empty() && !item.production_year.is_some_and(|year| years.contains(&year)) {
            return false;
        }
        if !containers.is_empty()
            && !item
                .container
                .as_deref()
                .is_some_and(|value| contains_ignore_case(&containers, value))
        {
            return false;
        }
        if let Some(is_played) = query.is_played {
            if item.user_data.played != is_played {
                return false;
            }
        }
        if let Some(is_favorite) = query.is_favorite {
            if item.user_data.is_favorite != is_favorite {
                return false;
            }
        }
        if let Some(is_hd) = query.is_hd {
            let is_item_hd = item.width.unwrap_or_default() >= 1280 || item.height.unwrap_or_default() >= 720;
            if is_item_hd != is_hd {
                return false;
            }
        }
        if let Some(has_subtitles) = query.has_subtitles {
            let item_has_subtitles = item
                .media_streams
                .iter()
                .any(|stream| stream.stream_type.eq_ignore_ascii_case("Subtitle"));
            if item_has_subtitles != has_subtitles {
                return false;
            }
        }
        if let Some(min_date) = query.min_premiere_date.or(query.min_start_date) {
            if !item.premiere_date.as_ref().is_some_and(|date| *date >= min_date) {
                return false;
            }
        }
        if let Some(max_date) = query.max_premiere_date.or(query.max_start_date) {
            if !item.premiere_date.as_ref().is_some_and(|date| *date <= max_date) {
                return false;
            }
        }
        if let Some(min_date) = query.min_date_last_saved {
            if !item.date_modified.as_ref().is_some_and(|date| *date >= min_date) {
                return false;
            }
        }
        if let Some(max_date) = query.max_date_last_saved {
            if !item.date_modified.as_ref().is_some_and(|date| *date <= max_date) {
                return false;
            }
        }
        if let Some(search_term) = &search_term {
            let haystack = [
                item.name.as_str(),
                item.original_title.as_deref().unwrap_or_default(),
                item.series_name.as_deref().unwrap_or_default(),
                item.season_name.as_deref().unwrap_or_default(),
                item.overview.as_deref().unwrap_or_default(),
            ]
            .join(" ")
            .to_ascii_lowercase();
            if !haystack.contains(search_term) {
                return false;
            }
        }
        true
    });

    apply_episode_sort(&mut items, query.sort_by.as_deref(), query.sort_order.as_deref());
    let total_record_count = items.len() as i64;
    let start_index = query.start_index.unwrap_or(0).max(0) as usize;
    let limit = query.limit.unwrap_or(100).clamp(1, 200) as usize;
    let items = items
        .into_iter()
        .skip(start_index)
        .take(limit)
        .map(|item| apply_show_response_shape(item, query))
        .collect::<Vec<_>>();

    QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index as i64),
    }
}

fn apply_show_response_shape(mut item: BaseItemDto, query: &ItemsQuery) -> BaseItemDto {
    let enable_image_types = parse_list(query.enable_image_types.as_deref());
    let images_disabled = query.enable_images == Some(false)
        || query.image_type_limit == Some(0)
        || (query.image_type_limit.is_some_and(|limit| limit <= 0));

    if images_disabled {
        clear_item_images(&mut item);
    } else if !enable_image_types.is_empty() {
        retain_item_images(&mut item, &enable_image_types);
    }

    if query.enable_user_data == Some(false) {
        item.user_data = repository::empty_user_data_for_item(
            emby_id_to_uuid(&item.id).unwrap_or_else(|_| Uuid::nil()),
        );
    }

    item
}

fn clear_item_images(item: &mut BaseItemDto) {
    item.primary_image_tag = None;
    item.image_tags.clear();
    item.backdrop_image_tags.clear();
    item.parent_logo_item_id = None;
    item.parent_logo_image_tag = None;
    item.parent_backdrop_item_id = None;
    item.parent_backdrop_image_tags.clear();
    item.parent_thumb_item_id = None;
    item.parent_thumb_image_tag = None;
    item.series_primary_image_tag = None;
    item.primary_image_item_id = None;
    item.primary_image_aspect_ratio = None;
}

fn retain_item_images(item: &mut BaseItemDto, image_types: &[String]) {
    item.image_tags
        .retain(|key, _| contains_ignore_case(image_types, key));
    if !contains_ignore_case(image_types, "Primary") {
        item.primary_image_tag = None;
        item.series_primary_image_tag = None;
        item.primary_image_item_id = None;
        item.primary_image_aspect_ratio = None;
    }
    if !contains_ignore_case(image_types, "Backdrop") {
        item.backdrop_image_tags.clear();
        item.parent_backdrop_item_id = None;
        item.parent_backdrop_image_tags.clear();
    }
    if !contains_ignore_case(image_types, "Logo") {
        item.parent_logo_item_id = None;
        item.parent_logo_image_tag = None;
    }
    if !contains_ignore_case(image_types, "Thumb") {
        item.parent_thumb_item_id = None;
        item.parent_thumb_image_tag = None;
    }
}

fn contains_ignore_case(values: &[String], candidate: &str) -> bool {
    values.iter().any(|value| value.eq_ignore_ascii_case(candidate))
}

fn matches_image_filter(item: &BaseItemDto, image_types: &[String]) -> bool {
    image_types.iter().any(|image_type| {
        item.image_tags
            .keys()
            .any(|key| key.eq_ignore_ascii_case(image_type))
            || (image_type.eq_ignore_ascii_case("Backdrop") && !item.backdrop_image_tags.is_empty())
    })
}

fn apply_start_item(mut items: Vec<BaseItemDto>, start_item_id: Option<&str>) -> Vec<BaseItemDto> {
    let Some(start_item_id) = start_item_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return items;
    };
    if let Some(index) = items
        .iter()
        .position(|item| item.id.eq_ignore_ascii_case(start_item_id))
    {
        items.drain(0..index);
    }
    items
}

fn apply_adjacent_items(mut items: Vec<BaseItemDto>, adjacent_to: Option<&str>) -> Vec<BaseItemDto> {
    let Some(adjacent_to) = adjacent_to.map(str::trim).filter(|value| !value.is_empty()) else {
        return items;
    };
    if let Some(index) = items
        .iter()
        .position(|item| item.id.eq_ignore_ascii_case(adjacent_to))
    {
        let start = index.saturating_sub(3);
        let end = (index + 4).min(items.len());
        return items.drain(start..end).collect();
    }
    items
}

fn apply_episode_sort(items: &mut [BaseItemDto], sort_by: Option<&str>, sort_order: Option<&str>) {
    match sort_by.unwrap_or("SortName") {
        "PremiereDate" => items.sort_by(|a, b| a.premiere_date.cmp(&b.premiere_date)),
        "IndexNumber" => items.sort_by(|a, b| {
            a.parent_index_number
                .cmp(&b.parent_index_number)
                .then(a.index_number.cmp(&b.index_number))
                .then(a.sort_name.cmp(&b.sort_name))
        }),
        "Random" => items.sort_by(|a, b| a.id.cmp(&b.id)),
        _ => items.sort_by(|a, b| {
            a.sort_name
                .cmp(&b.sort_name)
                .then(a.parent_index_number.cmp(&b.parent_index_number))
                .then(a.index_number.cmp(&b.index_number))
        }),
    }

    if sort_order.is_some_and(|value| value.eq_ignore_ascii_case("Descending")) {
        items.reverse();
    }
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
            genres: parse_list(query.genres.as_deref()),
            user_id: Some(user_id),
            media_types: parse_list(query.media_types.as_deref()),
            video_types: parse_list(query.video_types.as_deref()),
            image_types: parse_list(query.image_types.as_deref()),
            official_ratings: parse_list(query.official_ratings.as_deref()),
            tags: parse_list(query.tags.as_deref()),
            years: parse_i32_list(query.years.as_deref()),
            containers: parse_list(query.containers.as_deref()),
            audio_codecs: parse_list(query.audio_codecs.as_deref()),
            video_codecs: parse_list(query.video_codecs.as_deref()),
            subtitle_codecs: parse_list(query.subtitle_codecs.as_deref()),
            is_played: query.is_played,
            is_favorite: query.is_favorite,
            is_hd: query.is_hd,
            has_subtitles: query.has_subtitles,
            has_trailer: query.has_trailer,
            min_premiere_date: query.min_premiere_date,
            max_premiere_date: query.max_premiere_date,
            min_start_date: query.min_start_date,
            max_start_date: query.max_start_date,
            min_end_date: query.min_end_date,
            max_end_date: query.max_end_date,
            recursive: false,
            search_term: query.search_term.clone(),
            sort_by: query.sort_by.clone().or_else(|| Some("SortName".to_string())),
            sort_order: query
                .sort_order
                .clone()
                .or_else(|| Some("Ascending".to_string())),
            filters: None,
            fields: query.fields.clone(),
            start_index: query.start_index.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
            ..ItemListOptions::default()
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
