use crate::{
    auth::AuthSession,
    error::AppError,
    media_analyzer,
    metadata::person_service::PersonService,
    models::{
        emby_id_to_uuid, uuid_to_emby_guid, BaseItemDto, ContentSectionDto, GetSimilarItems, ItemCountsDto,
        ItemsQuery, PlaybackInfoDto, PlaybackInfoResponse, QueryResult,
        UpdateUserItemDataRequest, UserItemDataDto, UserItemDataQuery,
    },
    naming,
    repository::{self, ItemListOptions, UpdateUserDataInput},
    state::AppState,
};
use axum::{
    body::to_bytes,
    extract::{Path, Query, Request, State},
    http::{self, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Users/{user_id}/Views", get(user_views))
        .route("/Library/MediaFolders", get(media_folders))
        .route("/Items/Root", get(root_item))
        .route("/Users/{user_id}/Items/Root", get(root_item_for_user))
        .route("/Items/Counts", get(item_counts))
        .route("/Users/{user_id}/Items/Counts", get(user_item_counts))
        .route("/Items/Filters", get(item_filters))
        .route("/Items", get(items))
        .route("/Users/{user_id}/Items", get(user_items))
        .route("/Users/{user_id}/Items/Filters", get(user_item_filters))
        .route("/Users/{user_id}/HomeSections", get(home_sections))
        .route("/Users/{user_id}/Suggestions", get(user_suggestions))
        .route(
            "/Users/{user_id}/Sections/{section_id}/Items",
            get(user_section_items),
        )
        .route("/Users/{user_id}/Items/Latest", get(latest_items))
        .route("/Users/{user_id}/Items/Resume", get(user_resume_items))
        .route(
            "/Items/{item_id}/PlaybackInfo",
            get(playback_info).post(playback_info),
        )
        .route(
            "/UserItems/{item_id}/UserData",
            get(user_item_data).post(update_user_item_data),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/UserData",
            get(legacy_user_item_data).post(legacy_update_user_item_data),
        )
        .route(
            "/UserFavoriteItems/{item_id}",
            post(mark_favorite).delete(unmark_favorite),
        )
        .route(
            "/Users/{user_id}/FavoriteItems/{item_id}",
            post(legacy_mark_favorite).delete(legacy_unmark_favorite),
        )
        .route(
            "/Users/{user_id}/FavoriteItems/{item_id}/Delete",
            post(legacy_unmark_favorite),
        )
        .route(
            "/UserPlayedItems/{item_id}",
            post(mark_played).delete(mark_unplayed),
        )
        .route(
            "/Users/{user_id}/PlayedItems/{item_id}",
            post(legacy_mark_played).delete(legacy_mark_unplayed),
        )
        .route(
            "/Users/{user_id}/PlayedItems/{item_id}/Delete",
            post(legacy_mark_unplayed),
        )
        .route("/Items/{item_id}", get(item_by_id))
        .route("/Items/{item_id}/Refresh", post(refresh_item_metadata))
        .route("/Items/{item_id}/Chapters", get(item_chapters))
        .route("/Items/{item_id}/IntroTimestamps", get(no_intro_timestamps))
        .route("/Videos/{item_id}/IntroTimestamps", get(no_intro_timestamps))
        .route("/Episodes/{item_id}/IntroTimestamps", get(no_intro_timestamps))
        .route("/Users/{user_id}/Items/{item_id}", get(user_item_by_id))
        .route("/Users/{user_id}/Items/{item_id}/Similar", get(get_user_similar_items))
        .route("/Users/{user_id}/Items/{item_id}/Intros", get(empty_item_query_result))
        .route("/Users/{user_id}/Items/{item_id}/LocalTrailers", get(empty_item_list))
        .route("/Users/{user_id}/Items/{item_id}/SpecialFeatures", get(empty_item_list))
        .route("/Users/{user_id}/Items/{item_id}/HideFromResume", post(hide_from_resume))
        .route("/Videos/{item_id}/AdditionalParts", get(additional_parts))
        .route("/Items/{item_id}/Similar", get(get_similar_items))
        .route("/Movies/{item_id}/Similar", get(get_similar_items))
        .route("/Shows/{item_id}/Similar", get(get_similar_items))
        .route("/Trailers/{item_id}/Similar", get(get_similar_items))
}

async fn user_views(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(_user_id): Path<Uuid>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    libraries_as_query_result(&state).await
}

async fn media_folders(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    libraries_as_query_result(&state).await
}

async fn libraries_as_query_result(
    state: &AppState,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let libraries = repository::list_libraries(&state.pool).await?;
    let mut items = Vec::with_capacity(libraries.len());

    for library in libraries {
        items.push(
            repository::library_to_item_dto(&state.pool, &library, state.config.server_id).await?,
        );
    }

    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn root_item(_session: AuthSession, State(state): State<AppState>) -> Json<BaseItemDto> {
    Json(repository::root_item_dto(state.config.server_id))
}

async fn root_item_for_user(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(_user_id): Path<Uuid>,
) -> Json<BaseItemDto> {
    Json(repository::root_item_dto(state.config.server_id))
}

async fn item_counts(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<ItemCountsDto>, AppError> {
    Ok(Json(repository::item_counts(&state.pool).await?))
}

async fn user_item_counts(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ItemCountsDto>, AppError> {
    ensure_user_access(&session, user_id)?;
    Ok(Json(repository::item_counts(&state.pool).await?))
}

async fn items(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    list_items_for_user(&state, session.user_id, query).await
}

async fn user_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    query.user_id = Some(user_id);
    list_items_for_user(&state, user_id, query).await
}

async fn home_sections(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<Vec<ContentSectionDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
    let libraries = repository::list_libraries(&state.pool).await?;
    let mut sections = Vec::with_capacity(libraries.len());
    for library in libraries {
        let parent_item =
            repository::library_to_item_dto(&state.pool, &library, state.config.server_id).await?;
        sections.push(ContentSectionDto {
            name: library.name.clone(),
            id: uuid_to_emby_guid(&library.id),
            section_type: "Library".to_string(),
            subtitle: None,
            collection_type: Some(library.collection_type),
            view_type: Some("Library".to_string()),
            monitor: Vec::new(),
            card_size_offset: 0,
            scroll_direction: Some("Horizontal".to_string()),
            parent_item: Some(parent_item),
            text_info: None,
            premium_feature: None,
            premium_message: None,
            refresh_interval: None,
        });
    }

    Ok(Json(sections))
}

async fn user_suggestions(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
    query.user_id = Some(user_id);
    query.recursive = Some(true);
    query.sort_by = query.sort_by.or_else(|| Some("DateCreated".to_string()));
    query.sort_order = query.sort_order.or_else(|| Some("Descending".to_string()));
    query.limit = query.limit.or(Some(20));
    if query.include_item_types.is_none() {
        query.include_item_types = Some("Movie,Series".to_string());
    }
    list_items_for_user(&state, user_id, query).await
}

async fn user_section_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, section_id)): Path<(Uuid, String)>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
    if let Ok(parent_id) = emby_id_to_uuid(&section_id) {
        query.parent_id = Some(parent_id);
    }
    query.user_id = Some(user_id);
    query.recursive = Some(true);
    query.limit = query.limit.or(Some(20));
    list_items_for_user(&state, user_id, query).await
}

async fn latest_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    query.user_id = Some(user_id);
    query.recursive = Some(true);
    query.sort_by = Some("DateCreated".to_string());
    query.sort_order = Some("Descending".to_string());
    query.limit = query.limit.or(Some(20));
    
    // 如果没有指定包含的类型，默认显示Movie和Series，不显示Episode
    if query.include_item_types.is_none() {
        query.include_item_types = Some("Movie,Series".to_string());
    }

    let result = list_items_for_user(&state, user_id, query).await?;
    Ok(Json(result.0.items))
}

async fn list_items_for_user(
    state: &AppState,
    user_id: Uuid,
    query: ItemsQuery,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let mut query = query;
    let parent_is_user_root = query.parent_id == Some(user_id);
    if parent_is_user_root {
        query.parent_id = None;
    }
    let mut requested_item_ids = parse_emby_uuid_list(query.list_item_ids.as_deref());
    requested_item_ids.extend(parse_emby_uuid_list(query.ids.as_deref()));
    requested_item_ids.sort_unstable();
    requested_item_ids.dedup();
    let requested_include_types = parse_include_types(query.include_item_types.as_deref());

    if is_top_level_items_request(&query, &requested_item_ids, &requested_include_types) {
        return libraries_as_query_result(state).await;
    }

    // Emby 客户端进入详情页时会用 ListItemIds + IncludeItemTypes=BoxSet 查询“该条目所在合集”。
    // 当前项目还没有真实 BoxSet 扫描/建模，这里返回空结果比误返回无关媒体更接近预期行为。
    if !requested_item_ids.is_empty()
        && !requested_include_types.is_empty()
        && requested_include_types
            .iter()
            .all(|item_type| item_type.eq_ignore_ascii_case("BoxSet"))
    {
        return Ok(Json(
            repository::get_boxsets_for_item_ids(
                &state.pool,
                user_id,
                &requested_item_ids,
                state.config.server_id,
                query.start_index.unwrap_or(0).max(0) as i64,
                query.limit.unwrap_or(100).clamp(1, 200) as i64,
            )
            .await?,
        ));
    }

    let recursive = if parent_is_user_root {
        true
    } else {
        query.recursive.unwrap_or_else(|| {
            query
                .search_term
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        })
    };

    if let Some(parent_id) = query.parent_id {
        if let Some(library) = repository::get_library(&state.pool, parent_id).await? {
            // 对于电视剧库，如果没有指定包含的类型，默认只显示Series
            let mut include_types = parse_include_types(query.include_item_types.as_deref());
            if include_types.is_empty() && library.collection_type.eq_ignore_ascii_case("tvshows") {
                include_types = vec!["Series".to_string()];
            }
            
            let result = repository::list_media_items(
                &state.pool,
                item_list_options_from_query(
                    &query,
                    user_id,
                    Some(library.id),
                    None,
                    requested_item_ids.clone(),
                    include_types,
                    recursive,
                ),
            )
            .await?;
            return media_items_to_dto_result(state, user_id, result).await;
        }
    }

    let result = repository::list_media_items(
        &state.pool,
        item_list_options_from_query(
            &query,
            user_id,
            None,
            query.parent_id,
            requested_item_ids,
            requested_include_types,
            recursive,
        ),
    )
    .await?;

    media_items_to_dto_result(state, user_id, result).await
}

async fn item_filters(
    session: AuthSession,
    State(state): State<AppState>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    query.user_id = Some(user_id);
    item_filters_for_query(&state, user_id, query).await
}

async fn user_item_filters(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    ensure_user_access(&session, user_id)?;
    query.user_id = Some(user_id);
    item_filters_for_query(&state, user_id, query).await
}

async fn item_filters_for_query(
    state: &AppState,
    user_id: Uuid,
    mut query: ItemsQuery,
) -> Result<Json<Value>, AppError> {
    let parent_is_user_root = query.parent_id == Some(user_id);
    if parent_is_user_root {
        query.parent_id = None;
    }
    let mut requested_item_ids = parse_emby_uuid_list(query.list_item_ids.as_deref());
    requested_item_ids.extend(parse_emby_uuid_list(query.ids.as_deref()));
    requested_item_ids.sort_unstable();
    requested_item_ids.dedup();

    let include_types = parse_include_types(query.include_item_types.as_deref());
    let recursive = if parent_is_user_root {
        true
    } else {
        query.recursive.unwrap_or(true)
    };
    query.start_index = Some(0);
    query.limit = Some(10_000);

    let (library_id, parent_id, include_types) = if let Some(parent_id) = query.parent_id {
        if let Some(library) = repository::get_library(&state.pool, parent_id).await? {
            let mut include_types = include_types;
            if include_types.is_empty() && library.collection_type.eq_ignore_ascii_case("tvshows") {
                include_types = vec!["Series".to_string()];
            }
            (Some(library.id), None, include_types)
        } else {
            (None, Some(parent_id), include_types)
        }
    } else {
        (None, None, include_types)
    };

    let result = repository::list_media_items(
        &state.pool,
        item_list_options_from_query(
            &query,
            user_id,
            library_id,
            parent_id,
            requested_item_ids,
            include_types,
            recursive,
        ),
    )
    .await?;

    let mut genres = BTreeSet::new();
    let mut tags = BTreeSet::new();
    let mut official_ratings = BTreeSet::new();
    let mut containers = BTreeSet::new();
    let mut years = BTreeSet::new();
    let mut series_statuses = BTreeSet::new();

    for item in result.items {
        for genre in item.genres {
            if !genre.trim().is_empty() {
                genres.insert(genre);
            }
        }
        for tag in item.tags {
            if !tag.trim().is_empty() {
                tags.insert(tag);
            }
        }
        if let Some(rating) = item.official_rating.filter(|value| !value.trim().is_empty()) {
            official_ratings.insert(rating);
        }
        if let Some(container) = item.container.filter(|value| !value.trim().is_empty()) {
            containers.insert(container);
        }
        if let Some(year) = item.production_year {
            years.insert(year);
        }
        if let Some(status) = item.status.filter(|value| !value.trim().is_empty()) {
            series_statuses.insert(status);
        }
    }

    let genres_vec: Vec<String> = genres.into_iter().collect();
    let genre_items: Vec<Value> = genres_vec
        .iter()
        .map(|name| json!({ "Name": name, "Id": name }))
        .collect();
    let years_vec: Vec<i32> = years.into_iter().collect();

    Ok(Json(json!({
        "Genres": genres_vec,
        "GenreItems": genre_items,
        "Tags": tags.into_iter().collect::<Vec<_>>(),
        "OfficialRatings": official_ratings.into_iter().collect::<Vec<_>>(),
        "Containers": containers.into_iter().collect::<Vec<_>>(),
        "Years": years_vec.clone(),
        "ProductionYears": years_vec,
        "SeriesStatuses": series_statuses.into_iter().collect::<Vec<_>>(),
    })))
}

fn item_list_options_from_query(
    query: &ItemsQuery,
    user_id: Uuid,
    library_id: Option<Uuid>,
    parent_id: Option<Uuid>,
    item_ids: Vec<Uuid>,
    mut include_types: Vec<String>,
    recursive: bool,
) -> ItemListOptions {
    let mut exclude_types = parse_list(query.exclude_item_types.as_deref());
    let filters = parse_filter_list(query.filters.as_deref());
    let mut is_folder = query.is_folder;
    let mut is_played = query.is_played;
    let mut resume_only = false;
    for filter in &filters {
        if filter.eq_ignore_ascii_case("IsFolder") {
            is_folder = Some(true);
        } else if filter.eq_ignore_ascii_case("IsNotFolder") {
            is_folder = Some(false);
        } else if filter.eq_ignore_ascii_case("IsResumable") {
            resume_only = true;
        } else if filter.eq_ignore_ascii_case("IsPlayed") {
            is_played.get_or_insert(true);
        } else if filter.eq_ignore_ascii_case("IsUnplayed") {
            is_played.get_or_insert(false);
        }
    }
    if query.is_movie == Some(true) && !include_types.iter().any(|value| value.eq_ignore_ascii_case("Movie")) {
        include_types.push("Movie".to_string());
    } else if query.is_movie == Some(false) {
        exclude_types.push("Movie".to_string());
    }
    if query.is_series == Some(true) && !include_types.iter().any(|value| value.eq_ignore_ascii_case("Series")) {
        include_types.push("Series".to_string());
    } else if query.is_series == Some(false) {
        exclude_types.push("Series".to_string());
    }

    ItemListOptions {
        user_id: Some(query.user_id.unwrap_or(user_id)),
        library_id,
        parent_id,
        item_ids,
        include_types,
        exclude_types,
        media_types: parse_list(query.media_types.as_deref()),
        genres: parse_list(query.genres.as_deref()),
        official_ratings: parse_list(query.official_ratings.as_deref()),
        tags: parse_list(query.tags.as_deref()),
        exclude_tags: parse_list(query.exclude_tags.as_deref()),
        years: parse_i32_list(query.years.as_deref()),
        person_ids: parse_emby_uuid_list(query.person_ids.as_deref()),
        person_types: parse_list(query.person_types.as_deref()),
        studios: parse_list(query.studios.as_deref()),
        studio_ids: parse_list(query.studio_ids.as_deref()),
        containers: parse_list(query.containers.as_deref()),
        audio_codecs: parse_list(query.audio_codecs.as_deref()),
        video_codecs: parse_list(query.video_codecs.as_deref()),
        subtitle_codecs: parse_list(query.subtitle_codecs.as_deref()),
        any_provider_id_equals: parse_list(query.any_provider_id_equals.as_deref()),
        is_played,
        is_favorite: query.is_favorite,
        is_folder,
        has_overview: query.has_overview,
        has_tmdb_id: query.has_tmdb_id,
        has_imdb_id: query.has_imdb_id,
        series_status: parse_list(query.series_status.as_deref()),
        min_community_rating: query.min_community_rating,
        min_critic_rating: query.min_critic_rating,
        min_premiere_date: query.min_premiere_date,
        max_premiere_date: query.max_premiere_date,
        resume_only,
        recursive,
        search_term: query.search_term.clone(),
        sort_by: query.sort_by.clone(),
        sort_order: query.sort_order.clone(),
        filters: query.filters.clone(),
        fields: query.fields.clone(),
        start_index: query.start_index.unwrap_or(0),
        limit: query.limit.unwrap_or(100),
        group_items_into_collections: query.group_items_into_collections.unwrap_or(true),
        ..ItemListOptions::default()
    }
}

fn is_top_level_items_request(
    query: &ItemsQuery,
    requested_item_ids: &[Uuid],
    requested_include_types: &[String],
) -> bool {
    let parent_is_root = query
        .parent_id
        .is_none_or(|parent_id| parent_id.is_nil());
    parent_is_root
        && !query.recursive.unwrap_or(false)
        && requested_item_ids.is_empty()
        && requested_include_types.is_empty()
        && query
            .search_term
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && query
            .genres
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && query
            .media_types
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && query
            .ids
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && query
            .list_item_ids
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
        && query
            .filters
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
}

async fn media_items_to_dto_result(
    state: &AppState,
    user_id: Uuid,
    result: QueryResult<crate::models::DbMediaItem>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
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

    Ok(Json(QueryResult {
        items,
        total_record_count: result.total_record_count,
        start_index: result.start_index,
    }))
}

async fn user_item_data(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<UserItemDataQuery>,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    ensure_media_item_exists(&state, item_id).await?;
    Ok(Json(
        repository::get_user_item_data_dto(&state.pool, user_id, item_id).await?,
    ))
}

async fn legacy_user_item_data(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, AppError> {
    ensure_user_access(&session, user_id)?;
    ensure_media_item_exists(&state, item_id).await?;
    Ok(Json(
        repository::get_user_item_data_dto(&state.pool, user_id, item_id).await?,
    ))
}

async fn update_user_item_data(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<UserItemDataQuery>,
    Json(payload): Json<UpdateUserItemDataRequest>,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    update_user_data_for_user(&state, &session, user_id, item_id, payload).await
}

async fn legacy_update_user_item_data(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateUserItemDataRequest>,
) -> Result<Json<UserItemDataDto>, AppError> {
    update_user_data_for_user(&state, &session, user_id, item_id, payload).await
}

async fn mark_favorite(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<UserItemDataQuery>,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    set_favorite_for_user(&state, &session, user_id, item_id, true).await
}

async fn unmark_favorite(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<UserItemDataQuery>,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    set_favorite_for_user(&state, &session, user_id, item_id, false).await
}

async fn legacy_mark_favorite(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_favorite_for_user(&state, &session, user_id, item_id, true).await
}

async fn legacy_unmark_favorite(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_favorite_for_user(&state, &session, user_id, item_id, false).await
}

async fn mark_played(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<UserItemDataQuery>,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    set_played_for_user(&state, &session, user_id, item_id, true, query.date_played).await
}

async fn mark_unplayed(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id): Path<Uuid>,
    Query(query): Query<UserItemDataQuery>,
) -> Result<Json<UserItemDataDto>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    set_played_for_user(&state, &session, user_id, item_id, false, None).await
}

async fn legacy_mark_played(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<UserItemDataQuery>,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_played_for_user(&state, &session, user_id, item_id, true, query.date_played).await
}

async fn legacy_mark_unplayed(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<UserItemDataDto>, AppError> {
    set_played_for_user(&state, &session, user_id, item_id, false, None).await
}

async fn update_user_data_for_user(
    state: &AppState,
    session: &AuthSession,
    user_id: Uuid,
    item_id: Uuid,
    payload: UpdateUserItemDataRequest,
) -> Result<Json<UserItemDataDto>, AppError> {
    ensure_user_access(session, user_id)?;
    ensure_media_item_exists(state, item_id).await?;
    let UpdateUserItemDataRequest {
        playback_position_ticks,
        play_count,
        is_favorite,
        likes: _likes,
        played,
        last_played_date,
        rating: _rating,
        played_percentage: _played_percentage,
        unplayed_item_count: _unplayed_item_count,
    } = payload;
    Ok(Json(
        repository::update_user_item_data(
            &state.pool,
            user_id,
            item_id,
            UpdateUserDataInput {
                playback_position_ticks,
                play_count,
                is_favorite,
                played,
                last_played_date,
            },
        )
        .await?,
    ))
}

async fn set_favorite_for_user(
    state: &AppState,
    session: &AuthSession,
    user_id: Uuid,
    item_id: Uuid,
    is_favorite: bool,
) -> Result<Json<UserItemDataDto>, AppError> {
    ensure_user_access(session, user_id)?;
    ensure_media_item_exists(state, item_id).await?;
    Ok(Json(
        repository::set_user_favorite(&state.pool, user_id, item_id, is_favorite).await?,
    ))
}

async fn set_played_for_user(
    state: &AppState,
    session: &AuthSession,
    user_id: Uuid,
    item_id: Uuid,
    is_played: bool,
    date_played: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Json<UserItemDataDto>, AppError> {
    ensure_user_access(session, user_id)?;
    ensure_media_item_exists(state, item_id).await?;
    Ok(Json(
        repository::set_user_played(&state.pool, user_id, item_id, is_played, date_played).await?,
    ))
}

async fn ensure_media_item_exists(state: &AppState, item_id: Uuid) -> Result<(), AppError> {
    if repository::get_media_item(&state.pool, item_id).await?.is_some() {
        return Ok(());
    }
    if repository::get_missing_episode_dto(
        &state.pool,
        item_id,
        Uuid::nil(),
        state.config.server_id,
    )
    .await?
    .is_some()
    {
        return Ok(());
    }
    Err(AppError::NotFound("媒体条目不存在".to_string()))
}

async fn item_chapters(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<QueryResult<Value>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    ensure_media_item_exists(&state, item_id).await?;
    let chapters = repository::get_media_chapters(&state.pool, item_id).await?;
    let items: Vec<Value> = chapters
        .into_iter()
        .map(|chapter| {
            json!({
                "StartPositionTicks": chapter.start_position_ticks,
                "Name": chapter.name.unwrap_or_else(|| format!("Chapter {}", chapter.chapter_index + 1)),
                "ImageTag": chapter.image_path.as_ref().map(|_| chapter.updated_at.timestamp().to_string()),
                "MarkerType": chapter.marker_type.unwrap_or_else(|| "Chapter".to_string()),
                "ChapterIndex": chapter.chapter_index
            })
        })
        .collect();

    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn no_intro_timestamps(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<StatusCode, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    ensure_media_item_exists(&state, item_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn ensure_user_access(session: &AuthSession, user_id: Uuid) -> Result<(), AppError> {
    if session.user_id == user_id || session.is_admin {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn item_by_id(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<BaseItemDto>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    item_dto(&state, session.user_id, item_id).await
}

async fn user_item_by_id(
    _session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<BaseItemDto>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    item_dto(&state, user_id, item_id).await
}

async fn empty_item_query_result(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("鏃犳晥鐨勯」鐩甀D鏍煎紡: {}", item_id_str)))?;
    ensure_media_item_exists(&state, item_id).await?;
    Ok(Json(QueryResult {
        items: Vec::new(),
        total_record_count: 0,
        start_index: Some(0),
    }))
}

async fn empty_item_list(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("鏃犳晥鐨勯」鐩甀D鏍煎紡: {}", item_id_str)))?;
    ensure_media_item_exists(&state, item_id).await?;
    Ok(Json(Vec::new()))
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HideFromResumeQuery {
    #[serde(default, alias = "hide")]
    hide: Option<bool>,
}

async fn hide_from_resume(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
    Query(query): Query<HideFromResumeQuery>,
) -> Result<Json<UserItemDataDto>, AppError> {
    ensure_user_access(&session, user_id)?;
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("鏃犳晥鐨勯」鐩甀D鏍煎紡: {}", item_id_str)))?;
    ensure_media_item_exists(&state, item_id).await?;
    if query.hide != Some(true) {
        return Ok(Json(
            repository::get_user_item_data_dto(&state.pool, user_id, item_id).await?,
        ));
    }
    Ok(Json(
        repository::update_user_item_data(
            &state.pool,
            user_id,
            item_id,
            UpdateUserDataInput {
                playback_position_ticks: Some(0),
                play_count: None,
                is_favorite: None,
                played: None,
                last_played_date: None,
            },
        )
        .await?,
    ))
}

async fn additional_parts(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("鏃犳晥鐨勯」鐩甀D鏍煎紡: {}", item_id_str)))?;
    let result = repository::get_additional_parts_for_item(
        &state.pool,
        item_id,
        user_id,
        state.config.server_id,
        query.start_index.unwrap_or(0).max(0),
        query.limit.unwrap_or(100).clamp(1, 200),
    )
    .await?;
    Ok(Json(result))
}

async fn item_dto(
    state: &AppState,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<Json<BaseItemDto>, AppError> {
    if let Some(item) = repository::get_missing_episode_dto(
        &state.pool,
        item_id,
        user_id,
        state.config.server_id,
    )
    .await?
    {
        return Ok(Json(item));
    }

    if let Some(library) = repository::get_library(&state.pool, item_id).await? {
        return Ok(Json(
            repository::library_to_item_dto(&state.pool, &library, state.config.server_id).await?,
        ));
    }

    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    Ok(Json(
        repository::media_item_to_dto(&state.pool, &item, Some(user_id), state.config.server_id)
            .await?,
    ))
}

async fn refresh_item_metadata(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<StatusCode, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;

    if !item.item_type.eq_ignore_ascii_case("Series") && !item.item_type.eq_ignore_ascii_case("Movie") {
        return Ok(StatusCode::NO_CONTENT);
    }

    let Some(tmdb_id) = tmdb_id_from_provider_ids(&item.provider_ids) else {
        tracing::debug!(
            item_id = %item.id,
            item_type = %item.item_type,
            "跳过远程元数据刷新：条目缺少 TMDb provider id"
        );
        return Ok(StatusCode::NO_CONTENT);
    };
    let metadata_manager = state
        .metadata_manager
        .as_ref()
        .ok_or_else(|| AppError::BadRequest("未配置远程元数据提供者".to_string()))?;
    let provider = metadata_manager
        .get_provider("tmdb")
        .ok_or_else(|| AppError::BadRequest("未配置 TMDb 元数据提供者".to_string()))?;

    if item.item_type.eq_ignore_ascii_case("Series") {
        let metadata = provider.get_series_details(&tmdb_id).await?;
        repository::update_media_item_series_metadata(&state.pool, item.id, &metadata).await?;
        let catalog = provider.get_series_episode_catalog(&tmdb_id).await?;
        repository::replace_series_episode_catalog(&state.pool, item.id, &catalog).await?;
    } else if item.item_type.eq_ignore_ascii_case("Movie") {
        let metadata = provider.get_movie_details(&tmdb_id).await?;
        repository::update_media_item_movie_metadata(&state.pool, item.id, &metadata).await?;
    }

    let media_type = if item.item_type.eq_ignore_ascii_case("Series") {
        "tv"
    } else {
        "movie"
    };
    let person_service = PersonService::new(state.pool.clone(), metadata_manager.clone());
    person_service
        .fetch_persons_for_item(item.id, "tmdb", &tmdb_id, media_type)
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

fn tmdb_id_from_provider_ids(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    ["Tmdb", "TMDb", "tmdb"].iter().find_map(|key| {
        object
            .get(*key)
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .or_else(|| object.get(*key).and_then(|value| value.as_i64().map(|id| id.to_string())))
    })
}

async fn playback_info(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query_info): Query<PlaybackInfoDto>,
    request: Request,
) -> Result<Json<PlaybackInfoResponse>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    // 根据请求方法处理
    let info = if request.method() == http::Method::POST {
        // POST请求：尝试从请求体解析JSON
        let body_bytes = axum::body::to_bytes(request.into_body(), 10 * 1024 * 1024) // 10MB限制
            .await
            .map_err(|e| AppError::BadRequest(format!("无法读取请求体: {}", e)))?;
        
        if body_bytes.is_empty() {
            // 空请求体，使用查询参数
            query_info
        } else {
            match serde_json::from_slice::<PlaybackInfoDto>(&body_bytes) {
                Ok(mut body_info) => {
                    // 合并查询参数和请求体，请求体优先
                    if body_info.user_id.is_none() && query_info.user_id.is_some() {
                        body_info.user_id = query_info.user_id;
                    }
                    if body_info.max_streaming_bitrate.is_none() && query_info.max_streaming_bitrate.is_some() {
                        body_info.max_streaming_bitrate = query_info.max_streaming_bitrate;
                    }
                    body_info
                }
                Err(e) => {
                    // JSON解析失败，只使用查询参数
                    tracing::debug!("无法解析PlaybackInfo请求体JSON: {}, 使用查询参数", e);
                    query_info
                }
            }
        }
    } else {
        // GET请求：只使用查询参数
        query_info
    };
    
    let mut item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    if matches!(item.item_type.as_str(), "Series" | "Season" | "Folder") {
        return Err(AppError::BadRequest("目录条目没有播放源".to_string()));
    }

    let needs_metadata = item.video_codec.is_none() || item.audio_codec.is_none() || item.runtime_ticks.is_none();
    if needs_metadata {
        let item_path = item.path.clone();
        let path = std::path::Path::new(&item_path);
        if path.exists() {
            if naming::is_strm(path) {
                // 对于.strm文件，尝试分析远程URL
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        if let Some(target_url) = naming::strm_target_from_text(&content) {
                            tracing::debug!("分析.strm文件远程URL: {}", target_url);
                            match media_analyzer::analyze_remote_media(&target_url).await {
                                Ok(analysis) => {
                                    repository::update_media_item_metadata(&state.pool, item_id, &analysis).await?;
                                    item = repository::get_media_item(&state.pool, item_id)
                                        .await?
                                        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
                                    tracing::info!("已更新.strm文件远程媒体元数据: {}", path.display());
                                }
                                Err(e) => {
                                    tracing::warn!("无法分析.strm远程媒体 {}: {}", target_url, e);
                                }
                            }
                        } else {
                            tracing::debug!(".strm文件中未找到有效URL: {}", path.display());
                        }
                    }
                    Err(e) => {
                        tracing::warn!("无法读取.strm文件 {}: {}", path.display(), e);
                    }
                }
            } else {
                // 对于普通文件，进行本地分析
                match media_analyzer::analyze_media_file(path).await {
                    Ok(analysis) => {
                        repository::update_media_item_metadata(&state.pool, item_id, &analysis).await?;
                        item = repository::get_media_item(&state.pool, item_id)
                            .await?
                            .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
                    }
                    Err(e) => {
                        tracing::warn!("无法分析媒体文件 {}: {}", path.display(), e);
                    }
                }
            }
        } else {
            tracing::debug!("跳过媒体文件分析（文件不存在）: {}", path.display());
        }
    }

    let play_session_id = info
        .current_play_session_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());

    let mut media_sources =
        repository::get_media_sources_for_item(&state.pool, &item, state.config.server_id).await?;
    if media_sources.is_empty() {
        media_sources.push(
            repository::get_media_source_with_streams(&state.pool, &item, state.config.server_id)
                .await?,
        );
    }

    let selected_media_source_index = info
        .media_source_id
        .as_ref()
        .and_then(|requested_id| {
            media_sources.iter().position(|source| {
                source.id.eq_ignore_ascii_case(requested_id)
                    || source
                        .item_id
                        .as_ref()
                        .is_some_and(|item_id| item_id.eq_ignore_ascii_case(requested_id))
            })
        })
        .unwrap_or(0);

    if let Some(selected_source) = media_sources.get_mut(selected_media_source_index) {
        apply_requested_stream_selection(
            selected_source,
            info.audio_stream_index,
            info.subtitle_stream_index,
        );
    }

    let effective_max_bitrate = info
        .max_streaming_bitrate
        .or_else(|| info.device_profile.as_ref().and_then(|profile| profile.max_streaming_bitrate));

    let force_transcoding = media_sources
        .get(selected_media_source_index)
        .is_some_and(|source| {
            should_force_transcoding(
                &info,
                source,
                effective_max_bitrate,
            )
        });

    for media_source in &mut media_sources {
        if let Some(url) = media_source.direct_stream_url.as_mut() {
            url.push_str("&PlaySessionId=");
            url.push_str(&play_session_id);
            url.push_str("&X-Emby-Token=");
            url.push_str(&session.access_token);
        }
        media_source
            .required_http_headers
            .insert("X-Emby-Token".to_string(), session.access_token.clone());
        media_source
            .required_http_headers
            .insert("X-MediaBrowser-Token".to_string(), session.access_token.clone());
        media_source.add_api_key_to_direct_stream_url = Some(true);
    }
    
    // 设备配置文件处理
    if let Some(device_profile) = &info.device_profile {
        for media_source in &mut media_sources {
            // 根据最大流比特率决定是否支持转码
            if let Some(max_bitrate) = device_profile.max_streaming_bitrate {
                if let Some(media_bitrate) = media_source.bitrate {
                    if media_bitrate > max_bitrate as i32 {
                        media_source.supports_transcoding = true;
                    }
                }
            }

            if !device_profile.direct_play_profiles.is_empty() {
                media_source.supports_direct_play = true;
            }

            media_source.supports_direct_stream = true;
        }
    }

    if let Some(enable_direct_play) = info.enable_direct_play {
        for media_source in &mut media_sources {
            media_source.supports_direct_play = enable_direct_play;
        }
    }

    if let Some(enable_direct_stream) = info.enable_direct_stream {
        for media_source in &mut media_sources {
            media_source.supports_direct_stream = enable_direct_stream;
        }
    }

    if let Some(enable_transcoding) = info.enable_transcoding {
        for media_source in &mut media_sources {
            media_source.supports_transcoding = enable_transcoding;
        }
    }

    if force_transcoding {
        let item_emby_id = crate::models::uuid_to_emby_guid(&item.id);
        let selected_media_source_id = media_sources
            .get(selected_media_source_index)
            .map(|source| source.id.clone())
            .unwrap_or_else(|| format!("mediasource_{item_emby_id}"));

        let transcoding_container = preferred_transcoding_container(&info);
        let transcoding_sub_protocol = preferred_transcoding_sub_protocol(&info, &transcoding_container);
        let transcoding_url = build_transcoding_url(
            &item_emby_id,
            &selected_media_source_id,
            &play_session_id,
            &session.access_token,
            info.audio_stream_index,
            info.subtitle_stream_index,
            info.start_time_ticks,
            effective_max_bitrate,
            &transcoding_container,
        );

        if let Some(selected_source) = media_sources.get_mut(selected_media_source_index) {
            selected_source.supports_direct_play = false;
            selected_source.supports_direct_stream = false;
            selected_source.transcoding_url = Some(transcoding_url.clone());
            selected_source.transcoding_container = Some(transcoding_container.clone());
            selected_source.transcoding_sub_protocol = Some(transcoding_sub_protocol.clone());
        }

        return Ok(Json(PlaybackInfoResponse {
            media_sources,
            play_session_id,
            ..Default::default()
        }));
    }

    Ok(Json(PlaybackInfoResponse {
        media_sources,
        play_session_id,
        ..Default::default()
    }))
}

fn should_force_transcoding(
    info: &PlaybackInfoDto,
    media_source: &crate::models::MediaSourceDto,
    effective_max_bitrate: Option<i64>,
) -> bool {
    if matches!(info.enable_direct_play, Some(false)) || matches!(info.enable_direct_stream, Some(false)) {
        return true;
    }

    if matches!(info.enable_transcoding, Some(true))
        && matches!(info.enable_direct_play, Some(false) | None)
        && matches!(info.enable_direct_stream, Some(false) | None)
    {
        return true;
    }

    if let (Some(max_bitrate), Some(media_bitrate)) =
        (effective_max_bitrate, media_source.bitrate.map(i64::from))
    {
        if media_bitrate > max_bitrate && matches!(info.enable_transcoding, Some(true) | None) {
            return true;
        }
    }

    false
}

fn preferred_transcoding_container(info: &PlaybackInfoDto) -> String {
    if info
        .device_profile
        .as_ref()
        .is_some_and(|profile| profile.direct_play_protocols.iter().any(|value| value.eq_ignore_ascii_case("hls")))
    {
        "ts".to_string()
    } else {
        "ts".to_string()
    }
}

fn preferred_transcoding_sub_protocol(_info: &PlaybackInfoDto, _container: &str) -> String {
    "hls".to_string()
}

fn build_transcoding_url(
    item_emby_id: &str,
    media_source_id: &str,
    play_session_id: &str,
    access_token: &str,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
    start_time_ticks: Option<i64>,
    max_streaming_bitrate: Option<i64>,
    transcoding_container: &str,
) -> String {
    let mut params = vec![
        format!("MediaSourceId={media_source_id}"),
        format!("mediaSourceId={media_source_id}"),
        format!("PlaySessionId={play_session_id}"),
        format!("Container={transcoding_container}"),
        format!("X-Emby-Token={access_token}"),
    ];

    if let Some(value) = audio_stream_index {
        params.push(format!("AudioStreamIndex={value}"));
    }
    if let Some(value) = subtitle_stream_index {
        params.push(format!("SubtitleStreamIndex={value}"));
    }
    if let Some(value) = start_time_ticks {
        params.push(format!("StartTimeTicks={value}"));
    }
    if let Some(value) = max_streaming_bitrate {
        params.push(format!("VideoBitRate={value}"));
        params.push(format!("MaxStreamingBitrate={value}"));
    }

    format!("/Videos/{item_emby_id}/master.m3u8?{}", params.join("&"))
}

fn parse_include_types(value: Option<&str>) -> Vec<String> {
    parse_list(value)
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

fn parse_filter_list(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split([',', '|'])
        .flat_map(|value| value.split(';'))
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

fn parse_emby_uuid_list(value: Option<&str>) -> Vec<Uuid> {
    value
        .unwrap_or_default()
        .split([',', '|'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter_map(|value| emby_id_to_uuid(value).ok())
        .collect()
}

fn apply_requested_stream_selection(
    media_source: &mut crate::models::MediaSourceDto,
    requested_audio_stream_index: Option<i32>,
    requested_subtitle_stream_index: Option<i32>,
) {
    if let Some(audio_index) = requested_audio_stream_index {
        media_source.default_audio_stream_index = Some(audio_index);
        for stream in &mut media_source.media_streams {
            if stream.stream_type == "Audio" {
                stream.is_default = stream.index == audio_index;
            }
        }
    }

    if let Some(subtitle_index) = requested_subtitle_stream_index {
        if subtitle_index >= 0 {
            media_source.default_subtitle_stream_index = Some(subtitle_index);
        } else {
            media_source.default_subtitle_stream_index = None;
        }

        for stream in &mut media_source.media_streams {
            if stream.stream_type == "Subtitle" {
                stream.is_default = subtitle_index >= 0 && stream.index == subtitle_index;
            }
        }
    }
}

async fn user_resume_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    query.user_id = Some(user_id);
    let parent_is_user_root = query.parent_id == Some(user_id);
    if parent_is_user_root {
        query.parent_id = None;
    }

    let mut requested_item_ids = parse_emby_uuid_list(query.list_item_ids.as_deref());
    requested_item_ids.extend(parse_emby_uuid_list(query.ids.as_deref()));
    requested_item_ids.sort_unstable();
    requested_item_ids.dedup();

    let recursive = if parent_is_user_root {
        true
    } else {
        query.recursive.unwrap_or(true)
    };
    let mut options = item_list_options_from_query(
        &query,
        user_id,
        None,
        query.parent_id,
        requested_item_ids,
        parse_include_types(query.include_item_types.as_deref()),
        recursive,
    );
    options.resume_only = true;
    options.sort_by = query.sort_by.or_else(|| Some("DatePlayed".to_string()));
    options.sort_order = query.sort_order.or_else(|| Some("Descending".to_string()));
    options.limit = query.limit.unwrap_or(50);

    let result = repository::list_media_items(
        &state.pool,
        options,
    ).await?;

    media_items_to_dto_result(&state, user_id, result).await
}

async fn get_similar_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<GetSimilarItems>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    // 获取目标项目
    let target_item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    
    // 简单的相似性算法：基于类型和标签查找相似项目
    let user_id = query.user_id.unwrap_or(session.user_id);
    let similar_items = repository::find_similar_items(
        &state.pool,
        &target_item,
        query.limit.unwrap_or(20),
        Some(user_id),
        state.config.server_id,
    ).await?;
    
    let total_record_count = similar_items.len() as i64;
    Ok(Json(QueryResult {
        items: similar_items,
        total_record_count,
        start_index: Some(0),
    }))
}

async fn get_user_similar_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
    Query(mut query): Query<GetSimilarItems>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
    query.user_id = Some(user_id);
    get_similar_items(session, State(state), Path(item_id_str), Query(query)).await
}
