use crate::{
    auth::AuthSession,
    error::AppError,
    media_analyzer,
    metadata::person_service::PersonService,
    models::{
        emby_id_to_uuid, uuid_to_emby_guid, BaseItemDto, ContentSectionDto, GetSimilarItems, ItemCountsDto,
        ItemsQuery, PlaybackInfoDto, PlaybackInfoResponse, QueryResult, TranscodingInfoDto,
        UpdateUserItemDataRequest, UserItemDataDto, UserItemDataQuery,
    },
    naming,
    repository::{self, ItemListOptions, UpdateUserDataInput},
    state::AppState,
};
use axum::{
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
        .route("/Artists", get(artists))
        .route("/Artists/{name}", get(artist))
        .route("/Artists/{name}/Items", get(artist_items))
        .route("/Studios", get(studios))
        .route("/Studios/{name}", get(studio))
        .route("/Studios/{name}/Items", get(studio_items))
        .route("/Years", get(years))
        .route("/Tags", get(tags))
        .route("/Tags/{name}", get(tag))
        .route("/Tags/{name}/Items", get(tag_items))
        .route("/OfficialRatings", get(official_ratings))
        .route("/Containers", get(containers))
        .route("/AudioCodecs", get(audio_codecs))
        .route("/VideoCodecs", get(video_codecs))
        .route("/SubtitleCodecs", get(subtitle_codecs))
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
        .route("/Items/{item_id}/IntroTimestamps", get(intro_timestamps))
        .route("/Videos/{item_id}/IntroTimestamps", get(intro_timestamps))
        .route("/Episodes/{item_id}/IntroTimestamps", get(intro_timestamps))
        .route("/Users/{user_id}/Items/{item_id}", get(user_item_by_id))
        .route("/Users/{user_id}/Items/{item_id}/Similar", get(get_user_similar_items))
        .route("/Users/{user_id}/Items/{item_id}/Intros", get(item_intros))
        .route("/Users/{user_id}/Items/{item_id}/LocalTrailers", get(local_trailers))
        .route("/Users/{user_id}/Items/{item_id}/SpecialFeatures", get(special_features))
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

async fn studios(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = repository::aggregate_array_values(&state.pool, "studios")
        .await?
        .into_iter()
        .map(|name| virtual_folder_item(&name, "Studio", state.config.server_id))
        .collect::<Vec<_>>();
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn artists(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = repository::aggregate_artists(&state.pool)
        .await?
        .into_iter()
        .map(|(id, name)| {
            let mut item = virtual_folder_item(&name, "MusicArtist", state.config.server_id);
            item.id = uuid_to_emby_guid(&id);
            item.guid = Some(uuid_to_emby_guid(&id));
            item.display_preferences_id = Some(uuid_to_emby_guid(&id));
            item
        })
        .collect::<Vec<_>>();
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn artist(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<BaseItemDto> {
    Json(virtual_folder_item(&name, "MusicArtist", state.config.server_id))
}

async fn artist_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    virtual_folder_items(&state, &session, query, VirtualFilter::Artist(name)).await
}

async fn studio(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<BaseItemDto> {
    Json(virtual_folder_item(&name, "Studio", state.config.server_id))
}

async fn studio_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    virtual_folder_items(&state, &session, query, VirtualFilter::Studio(name)).await
}

async fn tags(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = repository::aggregate_array_values(&state.pool, "tags")
        .await?
        .into_iter()
        .map(|name| virtual_folder_item(&name, "Tag", state.config.server_id))
        .collect::<Vec<_>>();
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn tag(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Json<BaseItemDto> {
    Json(virtual_folder_item(&name, "Tag", state.config.server_id))
}

async fn tag_items(
    session: AuthSession,
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    virtual_folder_items(&state, &session, query, VirtualFilter::Tag(name)).await
}

async fn official_ratings(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(repository::aggregate_text_values(&state.pool, "official_rating").await?)))
}

async fn containers(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(repository::aggregate_text_values(&state.pool, "container").await?)))
}

async fn audio_codecs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(repository::aggregate_stream_codecs(&state.pool, "Audio").await?)))
}

async fn video_codecs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(repository::aggregate_stream_codecs(&state.pool, "Video").await?)))
}

async fn subtitle_codecs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(repository::aggregate_stream_codecs(&state.pool, "Subtitle").await?)))
}

async fn years(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    let items = repository::aggregate_years(&state.pool)
        .await?
        .into_iter()
        .map(|year| {
            json!({
                "Name": year.to_string(),
                "Id": year.to_string(),
                "Type": "Year",
                "ProductionYear": year,
                "IsFolder": true
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(query_result_from_items(items)))
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

enum VirtualFilter {
    Artist(String),
    Studio(String),
    Tag(String),
}

fn string_list_result(values: Vec<String>) -> Value {
    let total = values.len();
    json!({ "Items": values, "TotalRecordCount": total, "StartIndex": 0 })
}

fn query_result_from_items(items: Vec<Value>) -> Value {
    let total = items.len();
    json!({
        "Items": items,
        "TotalRecordCount": total,
        "StartIndex": 0
    })
}

fn virtual_folder_item(name: &str, item_type: &str, server_id: Uuid) -> BaseItemDto {
    let mut item = repository::root_item_dto(server_id);
    item.name = name.to_string();
    item.id = name.to_string();
    item.guid = None;
    item.etag = None;
    item.can_delete = false;
    item.can_download = false;
    item.can_edit_items = Some(false);
    item.presentation_unique_key = Some(format!("{name}_"));
    item.item_type = item_type.to_string();
    item.is_folder = true;
    item.sort_name = Some(name.to_lowercase());
    item.forced_sort_name = item.sort_name.clone();
    item.location_type = Some("Virtual".to_string());
    item.display_preferences_id = Some(name.to_string());
    item.size = None;
    item.special_feature_count = None;
    item
}

async fn virtual_folder_items(
    state: &AppState,
    session: &AuthSession,
    mut query: ItemsQuery,
    filter: VirtualFilter,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(session, user_id)?;
    query.user_id = Some(user_id);

    let parent_is_user_root = query.parent_id == Some(user_id);
    if parent_is_user_root {
        query.parent_id = None;
    }

    let (library_id, parent_id) = if let Some(parent_id) = query.parent_id {
        if let Some(library) = repository::get_library(&state.pool, parent_id).await? {
            (Some(library.id), None)
        } else {
            (None, Some(parent_id))
        }
    } else {
        (None, None)
    };

    let mut include_types = parse_include_types(query.include_item_types.as_deref());
    if include_types.is_empty() {
        include_types = vec!["Movie".to_string(), "Series".to_string(), "Episode".to_string()];
    }

    let mut requested_item_ids = parse_emby_uuid_list(query.list_item_ids.as_deref());
    requested_item_ids.extend(parse_emby_uuid_list(query.ids.as_deref()));
    requested_item_ids.sort_unstable();
    requested_item_ids.dedup();

    let mut options = item_list_options_from_query(
        &query,
        user_id,
        library_id,
        parent_id,
        requested_item_ids,
        include_types,
        query.recursive.unwrap_or(true),
    );

    match filter {
        VirtualFilter::Artist(name) => {
            if let Ok(id) = emby_id_to_uuid(&name) {
                options.artist_ids = vec![id];
            } else {
                options.artists = vec![name];
            }
        }
        VirtualFilter::Studio(name) => options.studios = vec![name],
        VirtualFilter::Tag(name) => options.tags = vec![name],
    }

    let result = repository::list_media_items(&state.pool, options).await?;
    let mut items = Vec::with_capacity(result.items.len());
    for item in result.items {
        items.push(
            repository::media_item_to_dto(&state.pool, &item, Some(user_id), state.config.server_id)
                .await?,
        );
    }
    Ok(Json(items))
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
    let mut audio_codecs = BTreeSet::new();
    let mut video_codecs = BTreeSet::new();
    let mut subtitle_codecs = BTreeSet::new();
    let mut years = BTreeSet::new();
    let mut series_statuses = BTreeSet::new();
    let item_ids = result.items.iter().map(|item| item.id).collect::<Vec<_>>();

    for item in &result.items {
        for genre in &item.genres {
            if !genre.trim().is_empty() {
                genres.insert(genre.clone());
            }
        }
        for tag in &item.tags {
            if !tag.trim().is_empty() {
                tags.insert(tag.clone());
            }
        }
        if let Some(rating) = item.official_rating.as_ref().filter(|value| !value.trim().is_empty()) {
            official_ratings.insert(rating.clone());
        }
        if let Some(container) = item.container.as_ref().filter(|value| !value.trim().is_empty()) {
            containers.insert(container.clone());
        }
        if let Some(codec) = item.audio_codec.as_ref().filter(|value| !value.trim().is_empty()) {
            audio_codecs.insert(codec.clone());
        }
        if let Some(codec) = item.video_codec.as_ref().filter(|value| !value.trim().is_empty()) {
            video_codecs.insert(codec.clone());
        }
        if let Some(year) = item.production_year {
            years.insert(year);
        }
        if let Some(status) = item.status.as_ref().filter(|value| !value.trim().is_empty()) {
            series_statuses.insert(status.clone());
        }
    }
    for (stream_type, codec) in repository::media_stream_codecs_for_items(&state.pool, &item_ids).await? {
        match stream_type.as_str() {
            "Audio" => {
                audio_codecs.insert(codec);
            }
            "Video" => {
                video_codecs.insert(codec);
            }
            "Subtitle" => {
                subtitle_codecs.insert(codec);
            }
            _ => {}
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
        "AudioCodecs": audio_codecs.into_iter().collect::<Vec<_>>(),
        "VideoCodecs": video_codecs.into_iter().collect::<Vec<_>>(),
        "SubtitleCodecs": subtitle_codecs.into_iter().collect::<Vec<_>>(),
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
    let requested_folder_like_type = include_types
        .iter()
        .any(|item_type| is_folder_like_item_type(item_type));
    for filter in &filters {
        if filter.eq_ignore_ascii_case("IsFolder") {
            if !include_types.is_empty() && !requested_folder_like_type {
                continue;
            }
            is_folder = Some(true);
        } else if filter.eq_ignore_ascii_case("IsNotFolder") {
            if requested_folder_like_type {
                continue;
            }
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
        video_types: parse_list(query.video_types.as_deref()),
        image_types: parse_list(query.image_types.as_deref()),
        genres: parse_list(query.genres.as_deref()),
        official_ratings: parse_list(query.official_ratings.as_deref()),
        tags: parse_list(query.tags.as_deref()),
        exclude_tags: parse_list(query.exclude_tags.as_deref()),
        years: parse_i32_list(query.years.as_deref()),
        person_ids: parse_emby_uuid_list(query.person_ids.as_deref()),
        person_types: parse_list(query.person_types.as_deref()),
        artists: parse_list(query.artists.as_deref()),
        artist_ids: parse_emby_uuid_list(query.artist_ids.as_deref()),
        albums: parse_list(query.albums.as_deref()),
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
        is_hd: query.is_hd,
        is_3d: query.is_3d,
        is_locked: query.is_locked,
        is_place_holder: query.is_place_holder,
        has_overview: query.has_overview,
        has_subtitles: query.has_subtitles,
        has_trailer: query.has_trailer,
        has_theme_song: query.has_theme_song,
        has_theme_video: query.has_theme_video,
        has_special_feature: query.has_special_feature,
        has_tmdb_id: query.has_tmdb_id,
        has_imdb_id: query.has_imdb_id,
        series_status: parse_list(query.series_status.as_deref()),
        min_community_rating: query.min_community_rating,
        min_critic_rating: query.min_critic_rating,
        min_premiere_date: query.min_premiere_date,
        max_premiere_date: query.max_premiere_date,
        min_start_date: query.min_start_date,
        max_start_date: query.max_start_date,
        min_end_date: query.min_end_date,
        max_end_date: query.max_end_date,
        min_date_last_saved: query.min_date_last_saved,
        max_date_last_saved: query.max_date_last_saved,
        min_date_last_saved_for_user: query.min_date_last_saved_for_user,
        max_date_last_saved_for_user: query.max_date_last_saved_for_user,
        aired_during_season: query.aired_during_season,
        project_to_media: query.project_to_media.unwrap_or(false),
        resume_only,
        recursive,
        search_term: query.search_term.clone(),
        name_starts_with: query.name_starts_with.clone(),
        name_starts_with_or_greater: query.name_starts_with_or_greater.clone(),
        name_less_than: query.name_less_than.clone(),
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

fn is_folder_like_item_type(item_type: &str) -> bool {
    matches!(
        item_type.trim().to_ascii_lowercase().as_str(),
        "series" | "season" | "boxset" | "folder" | "collectionfolder"
    )
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

async fn item_intros(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = related_child_items(
        &session,
        &state,
        user_id,
        &item_id_str,
        &["Intro"],
    )
    .await?;
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn local_trailers(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let items = related_child_items(
        &session,
        &state,
        user_id,
        &item_id_str,
        &["Trailer"],
    )
    .await?;
    Ok(Json(items))
}

async fn special_features(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let items = related_child_items(
        &session,
        &state,
        user_id,
        &item_id_str,
        &["SpecialFeature", "Extra"],
    )
    .await?;
    Ok(Json(items))
}

async fn intro_timestamps(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    ensure_media_item_exists(&state, item_id).await?;
    let chapters = repository::get_media_chapters(&state.pool, item_id).await?;
    let intro_start = marker_ticks(&chapters, "IntroStart");
    let intro_end = marker_ticks(&chapters, "IntroEnd");
    let credits_start = marker_ticks(&chapters, "CreditsStart");

    Ok(Json(json!({
        "Valid": intro_start.is_some() || intro_end.is_some() || credits_start.is_some(),
        "IntroStart": intro_start,
        "IntroEnd": intro_end,
        "CreditsStart": credits_start
    })))
}

async fn related_child_items(
    session: &AuthSession,
    state: &AppState,
    user_id: Uuid,
    item_id_str: &str,
    include_types: &[&str],
) -> Result<Vec<BaseItemDto>, AppError> {
    ensure_user_access(session, user_id)?;
    let item_id = emby_id_to_uuid(item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    ensure_media_item_exists(state, item_id).await?;

    let result = repository::list_media_items(
        &state.pool,
        ItemListOptions {
            library_id: None,
            parent_id: Some(item_id),
            item_ids: vec![],
            include_types: include_types.iter().map(|value| (*value).to_string()).collect(),
            genres: vec![],
            user_id: Some(user_id),
            recursive: false,
            search_term: None,
            sort_by: Some("SortName".to_string()),
            sort_order: Some("Ascending".to_string()),
            filters: None,
            fields: None,
            start_index: 0,
            limit: 200,
            ..ItemListOptions::default()
        },
    )
    .await?;

    let mut items = Vec::with_capacity(result.items.len());
    for item in result.items {
        items.push(
            repository::media_item_to_dto(&state.pool, &item, Some(user_id), state.config.server_id)
                .await?,
        );
    }

    Ok(items)
}

fn marker_ticks(chapters: &[crate::models::DbMediaChapter], marker_type: &str) -> Option<i64> {
    chapters
        .iter()
        .find(|chapter| {
            chapter
                .marker_type
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(marker_type))
        })
        .map(|chapter| chapter.start_position_ticks)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct HideFromResumeQuery {
    #[serde(default, alias = "hide")]
    hide: Option<bool>,
}

fn should_hide_from_resume(query: &HideFromResumeQuery) -> bool {
    query.hide != Some(false)
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
    if !should_hide_from_resume(&query) {
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
                media_source.supports_direct_play =
                    device_profile_supports_direct_play(device_profile, media_source);
            }

            media_source.supports_direct_stream =
                media_source.supports_direct_play || device_profile.transcoding_profiles.is_empty();
            if !device_profile.transcoding_profiles.is_empty() {
                media_source.supports_transcoding =
                    device_profile_supports_transcoding(device_profile, media_source);
            }
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
            &transcoding_sub_protocol,
        );

        if let Some(selected_source) = media_sources.get_mut(selected_media_source_index) {
            selected_source.supports_direct_play = false;
            selected_source.supports_direct_stream = false;
            selected_source.transcoding_url = Some(transcoding_url.clone());
            selected_source.transcoding_container = Some(transcoding_container.clone());
            selected_source.transcoding_sub_protocol = Some(transcoding_sub_protocol.clone());
        }

        let transcoding_info = media_sources
            .get(selected_media_source_index)
            .map(|source| build_transcoding_info(source, &transcoding_container, effective_max_bitrate));

        return Ok(Json(PlaybackInfoResponse {
            media_sources,
            play_session_id,
            transcoding_info,
            ..Default::default()
        }));
    }

    Ok(Json(PlaybackInfoResponse {
        media_sources,
        play_session_id,
        ..Default::default()
    }))
}

fn build_transcoding_info(
    source: &crate::models::MediaSourceDto,
    container: &str,
    max_streaming_bitrate: Option<i64>,
) -> TranscodingInfoDto {
    let video_stream = source
        .media_streams
        .iter()
        .find(|stream| stream.stream_type.eq_ignore_ascii_case("Video"));
    let audio_stream = source
        .media_streams
        .iter()
        .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"));
    let source_bitrate = source.bitrate;
    let mut reasons = Vec::new();
    if let (Some(source_bitrate), Some(max_bitrate)) = (source_bitrate, max_streaming_bitrate) {
        if i64::from(source_bitrate) > max_bitrate {
            reasons.push("ContainerBitrateExceedsLimit".to_string());
        }
    }
    if !source.supports_direct_play {
        reasons.push("DirectPlayError".to_string());
    }
    if !source.supports_direct_stream {
        reasons.push("DirectStreamError".to_string());
    }
    if reasons.is_empty() {
        reasons.push("ContainerNotSupported".to_string());
    }

    TranscodingInfoDto {
        audio_codec: audio_stream.and_then(|stream| stream.codec.clone()),
        video_codec: video_stream.and_then(|stream| stream.codec.clone()),
        container: Some(container.to_string()),
        is_video_direct: false,
        is_audio_direct: false,
        bitrate: source_bitrate,
        framerate: video_stream.and_then(|stream| stream.real_frame_rate.or(stream.average_frame_rate)),
        completion_percentage: None,
        width: video_stream.and_then(|stream| stream.width),
        height: video_stream.and_then(|stream| stream.height),
        hardware_acceleration_type: None,
        transcode_reasons: reasons,
    }
}

fn device_profile_supports_direct_play(
    profile: &crate::models::DeviceProfile,
    source: &crate::models::MediaSourceDto,
) -> bool {
    profile.direct_play_profiles.iter().any(|direct_profile| {
        direct_profile
            .r#type
            .as_deref()
            .is_none_or(|value| value.eq_ignore_ascii_case("Video"))
            && csv_option_contains(direct_profile.container.as_deref(), &source.container)
            && codec_profile_matches(direct_profile.video_codec.as_deref(), source, "Video")
            && codec_profile_matches(direct_profile.audio_codec.as_deref(), source, "Audio")
    })
}

fn device_profile_supports_transcoding(
    profile: &crate::models::DeviceProfile,
    source: &crate::models::MediaSourceDto,
) -> bool {
    profile.transcoding_profiles.iter().any(|transcoding_profile| {
        transcoding_profile
            .r#type
            .as_deref()
            .is_none_or(|value| value.eq_ignore_ascii_case("Video"))
            && transcoding_profile
                .container
                .as_deref()
                .is_none_or(|value| !value.trim().is_empty())
            && codec_profile_matches(transcoding_profile.video_codec.as_deref(), source, "Video")
            && codec_profile_matches(transcoding_profile.audio_codec.as_deref(), source, "Audio")
    })
}

fn codec_profile_matches(
    codec_csv: Option<&str>,
    source: &crate::models::MediaSourceDto,
    stream_type: &str,
) -> bool {
    let Some(codec_csv) = codec_csv else {
        return true;
    };
    let allowed = parse_list(Some(codec_csv))
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if allowed.is_empty() {
        return true;
    }

    source
        .media_streams
        .iter()
        .filter(|stream| stream.stream_type.eq_ignore_ascii_case(stream_type))
        .filter_map(|stream| stream.codec.as_deref())
        .all(|codec| allowed.iter().any(|allowed_codec| allowed_codec.eq_ignore_ascii_case(codec)))
}

fn csv_option_contains(csv: Option<&str>, value: &str) -> bool {
    csv.is_none_or(|csv| {
        parse_list(Some(csv))
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(value))
    })
}

fn should_force_transcoding(
    info: &PlaybackInfoDto,
    media_source: &crate::models::MediaSourceDto,
    effective_max_bitrate: Option<i64>,
) -> bool {
    if matches!(info.enable_transcoding, Some(false)) {
        return false;
    }

    if matches!(info.enable_direct_play, Some(false)) || matches!(info.enable_direct_stream, Some(false)) {
        return true;
    }

    if let Some(profile) = &info.device_profile {
        if !profile.direct_play_profiles.is_empty()
            && !device_profile_supports_direct_play(profile, media_source)
        {
            return true;
        }

        if let Some(max_audio_channels) = info.max_audio_channels {
            if media_source.media_streams.iter().any(|stream| {
                stream.stream_type.eq_ignore_ascii_case("Audio")
                    && stream.channels.is_some_and(|channels| channels > max_audio_channels)
            }) {
                return true;
            }
        }

        if matches!(info.allow_video_stream_copy, Some(false))
            && media_source
                .media_streams
                .iter()
                .any(|stream| stream.stream_type.eq_ignore_ascii_case("Video"))
        {
            return true;
        }

        if matches!(info.allow_audio_stream_copy, Some(false))
            && media_source
                .media_streams
                .iter()
                .any(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
        {
            return true;
        }

        if matches!(info.allow_interlaced_video_stream_copy, Some(false))
            && media_source.media_streams.iter().any(|stream| {
                stream.stream_type.eq_ignore_ascii_case("Video")
                    && stream.is_interlaced.unwrap_or(false)
            })
        {
            return true;
        }
    }

    if matches!(info.always_burn_in_subtitle_when_transcoding, Some(true))
        && selected_subtitle_stream(media_source, info.subtitle_stream_index).is_some()
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

fn selected_subtitle_stream<'a>(
    media_source: &'a crate::models::MediaSourceDto,
    subtitle_stream_index: Option<i32>,
) -> Option<&'a crate::models::MediaStreamDto> {
    let index = subtitle_stream_index?;
    if index < 0 {
        return None;
    }
    media_source.media_streams.iter().find(|stream| {
        stream.stream_type.eq_ignore_ascii_case("Subtitle") && stream.index == index
    })
}

fn preferred_transcoding_profile(info: &PlaybackInfoDto) -> Option<&crate::models::TranscodingProfile> {
    info.device_profile.as_ref()?.transcoding_profiles.iter().find(|profile| {
        profile
            .r#type
            .as_deref()
            .is_none_or(|value| value.eq_ignore_ascii_case("Video"))
    })
}

fn preferred_transcoding_container(info: &PlaybackInfoDto) -> String {
    preferred_transcoding_profile(info)
        .and_then(|profile| profile.container.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("ts")
        .to_string()
}

fn preferred_transcoding_sub_protocol(info: &PlaybackInfoDto, container: &str) -> String {
    preferred_transcoding_profile(info)
        .and_then(|profile| profile.protocol.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if container.eq_ignore_ascii_case("ts") {
                "hls"
            } else {
                "http"
            }
        })
        .to_string()
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
    transcoding_sub_protocol: &str,
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

    if transcoding_sub_protocol.eq_ignore_ascii_case("hls") {
        format!("/Videos/{item_emby_id}/master.m3u8?{}", params.join("&"))
    } else {
        format!(
            "/Videos/{item_emby_id}/stream.{transcoding_container}?{}",
            params.join("&")
        )
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn explicit_series_request_is_not_cancelled_by_is_not_folder_filter() {
        let query: ItemsQuery = serde_json::from_value(json!({
            "Filters": "IsNotFolder",
            "IncludeItemTypes": "Series,Movie"
        }))
        .expect("valid items query");

        let options = item_list_options_from_query(
            &query,
            Uuid::nil(),
            None,
            None,
            Vec::new(),
            vec!["Series".to_string(), "Movie".to_string()],
            true,
        );

        assert_eq!(options.is_folder, None);
        assert_eq!(
            options.include_types,
            vec!["Series".to_string(), "Movie".to_string()]
        );
    }

    #[test]
    fn hide_from_resume_defaults_to_hiding_like_emby_clients() {
        let default_query: HideFromResumeQuery =
            serde_json::from_value(json!({})).expect("valid empty query");
        assert!(should_hide_from_resume(&default_query));

        let explicit_false: HideFromResumeQuery =
            serde_json::from_value(json!({ "Hide": false })).expect("valid hide=false query");
        assert!(!should_hide_from_resume(&explicit_false));
    }

    #[test]
    fn playback_info_uses_client_transcoding_profile() {
        let info: PlaybackInfoDto = serde_json::from_value(json!({
            "DeviceProfile": {
                "TranscodingProfiles": [
                    {
                        "Type": "Video",
                        "Container": "mp4",
                        "Protocol": "http",
                        "VideoCodec": "h264",
                        "AudioCodec": "aac"
                    }
                ]
            }
        }))
        .expect("valid playback info");

        assert_eq!(preferred_transcoding_container(&info), "mp4");
        assert_eq!(preferred_transcoding_sub_protocol(&info, "mp4"), "http");

        let url = build_transcoding_url(
            "ITEMID",
            "mediasource_ITEMID",
            "PLAYSESSION",
            "TOKEN",
            Some(1),
            Some(2),
            Some(123),
            Some(4_000_000),
            "mp4",
            "http",
        );

        assert!(url.starts_with("/Videos/ITEMID/stream.mp4?"));
        assert!(url.contains("MediaSourceId=mediasource_ITEMID"));
        assert!(url.contains("PlaySessionId=PLAYSESSION"));
    }

    #[test]
    fn playback_info_accepts_emby_sdk_profile_object_arrays() {
        let info: PlaybackInfoDto = serde_json::from_value(json!({
            "DeviceProfile": {
                "DirectPlayProfiles": [
                    { "Type": "Video", "Container": "mkv,mp4", "VideoCodec": "h264,hevc" }
                ],
                "ContainerProfiles": [
                    {
                        "Type": "Video",
                        "Conditions": [
                            { "Condition": "LessThanEqual", "Property": "Width", "Value": "3840" }
                        ]
                    }
                ],
                "CodecProfiles": [
                    {
                        "Type": "Video",
                        "Codec": "hevc",
                        "Conditions": [
                            { "Condition": "LessThanEqual", "Property": "VideoBitDepth", "Value": "10" }
                        ]
                    }
                ],
                "ResponseProfiles": [
                    { "Type": "Video", "Container": "mkv", "MimeType": "video/x-matroska" }
                ],
                "SubtitleProfiles": [
                    { "Format": "srt", "Method": "External" }
                ]
            }
        }))
        .expect("EmbySDK DeviceProfile object arrays should deserialize");

        let profile = info.device_profile.expect("device profile");
        assert_eq!(profile.container_profiles.len(), 1);
        assert_eq!(profile.codec_profiles.len(), 1);
        assert_eq!(profile.response_profiles.len(), 1);
        assert_eq!(profile.subtitle_profiles.len(), 1);
    }
}

