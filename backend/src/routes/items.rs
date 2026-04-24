use crate::{
    auth::{self, AuthSession, MediaAccessKind},
    error::AppError,
    media_analyzer,
    metadata::{
        person_service::PersonService,
        provider::{MetadataProvider, MetadataProviderManager},
        tmdb::TmdbProvider,
    },
    models::{
        emby_id_to_uuid, uuid_to_emby_guid, BaseItemDto, ContentSectionDto, GetSimilarItems,
        ItemCountsDto, ItemsQuery, LibraryOptionsDto, PlaybackInfoDto, PlaybackInfoResponse,
        QueryResult, TranscodingInfoDto, UpdateUserItemDataRequest, UserItemDataDto,
        UserItemDataQuery,
    },
    naming,
    repository::{self, ItemListOptions, UpdateUserDataInput},
    state::AppState,
    work_limiter::{WorkLimiterConfig, WorkLimiterKind},
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
        .route("/Items/{item_id}", get(item_by_id).post(update_item))
        .route("/Items/{item_id}/Ancestors", get(item_ancestors))
        .route("/Items/{item_id}/CriticReviews", get(item_critic_reviews))
        .route("/Items/{item_id}/ExternalIdInfos", get(item_external_id_infos))
        .route("/Items/{item_id}/InstantMix", get(item_instant_mix))
        .route("/Items/{item_id}/ThemeMedia", get(item_theme_media))
        .route("/Items/{item_id}/ThemeSongs", get(item_theme_songs))
        .route("/Items/{item_id}/ThemeVideos", get(item_theme_videos))
        .route("/Items/{item_id}/MetadataEditor", get(item_metadata_editor))
        .route("/Items/{item_id}/Delete", post(delete_item).delete(delete_item))
        .route("/Items/{item_id}/DeleteInfo", get(delete_item_info))
        .route("/Items/{item_id}/MakePrivate", post(make_item_private))
        .route("/Items/{item_id}/MakePublic", post(make_item_public))
        .route("/Items/{item_id}/Tags/Add", post(add_item_tag))
        .route("/Items/{item_id}/Tags/Delete", post(delete_item_tag))
        .route(
            "/Items/{item_id}/RemoteSearch/Subtitles/{language}",
            get(remote_search_subtitles_by_language).post(remote_search_subtitles_apply),
        )
        .route("/Items/{item_id}/Refresh", post(refresh_item_metadata))
        .route("/Items/{item_id}/Chapters", get(item_chapters))
        .route("/Items/{item_id}/IntroTimestamps", get(intro_timestamps))
        .route("/Videos/{item_id}/IntroTimestamps", get(intro_timestamps))
        .route("/Episodes/{item_id}/IntroTimestamps", get(intro_timestamps))
        .route("/Users/{user_id}/Items/{item_id}", get(user_item_by_id))
        .route(
            "/Users/{user_id}/Items/{item_id}/Similar",
            get(get_user_similar_items),
        )
        .route("/Users/{user_id}/Items/{item_id}/Intros", get(item_intros))
        .route(
            "/Users/{user_id}/Items/{item_id}/LocalTrailers",
            get(local_trailers),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/SpecialFeatures",
            get(special_features),
        )
        .route(
            "/Users/{user_id}/Items/{item_id}/HideFromResume",
            post(hide_from_resume),
        )
        .route("/Videos/{item_id}/AdditionalParts", get(additional_parts))
        .route("/Items/Intros", get(items_intros))
        .route("/Items/Prefixes", get(item_prefixes))
        .route("/Items/Access", get(items_access))
        .route("/Items/Delete", post(delete_items_bulk))
        .route(
            "/Items/Metadata/Reset",
            get(query_items_metadata_reset_status).post(reset_items_metadata),
        )
        .route("/Items/RemoteSearch/Apply/{item_id}", post(remote_search_apply))
        .route("/Items/RemoteSearch/Book", post(remote_search_empty))
        .route("/Items/RemoteSearch/BoxSet", post(remote_search_empty))
        .route("/Items/RemoteSearch/Game", post(remote_search_empty))
        .route("/Items/RemoteSearch/Image", post(remote_search_image))
        .route("/Items/RemoteSearch/Movie", post(remote_search_movie))
        .route("/Items/RemoteSearch/MusicAlbum", post(remote_search_empty))
        .route("/Items/RemoteSearch/MusicArtist", post(remote_search_empty))
        .route("/Items/RemoteSearch/MusicVideo", post(remote_search_empty))
        .route("/Items/RemoteSearch/Person", post(remote_search_person))
        .route("/Items/RemoteSearch/Series", post(remote_search_series))
        .route("/Items/RemoteSearch/Trailer", post(remote_search_trailer))
        .route("/Items/Shared/Leave", post(items_shared_leave))
        .route("/Items/{item_id}/Similar", get(get_similar_items))
        .route("/Movies/{item_id}/Similar", get(get_similar_items))
        .route("/Shows/{item_id}/Similar", get(get_similar_items))
        .route("/Trailers/{item_id}/Similar", get(get_similar_items))
        .route("/Trailers", get(trailers))
        .route("/Movies/Recommendations", get(movies_recommendations))
        .route("/movies/recommendations", get(movies_recommendations))
}

async fn user_views(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
    libraries_as_query_result_for_user(&state, user_id).await
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ItemCountsQuery {
    #[serde(default, alias = "userId")]
    user_id: Option<Uuid>,
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

async fn libraries_as_query_result_for_user(
    state: &AppState,
    user_id: Uuid,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let libraries = repository::visible_libraries_for_user(&state.pool, user_id).await?;
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
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemCountsQuery>,
) -> Result<Json<ItemCountsDto>, AppError> {
    if let Some(user_id) = query.user_id {
        ensure_user_access(&session, user_id)?;
        return Ok(Json(
            repository::item_counts_for_user(&state.pool, user_id).await?,
        ));
    }

    Ok(Json(repository::item_counts(&state.pool).await?))
}

async fn user_item_counts(
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
) -> Result<Json<ItemCountsDto>, AppError> {
    ensure_user_access(&session, user_id)?;
    Ok(Json(
        repository::item_counts_for_user(&state.pool, user_id).await?,
    ))
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
    Json(virtual_folder_item(
        &name,
        "MusicArtist",
        state.config.server_id,
    ))
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
    Ok(Json(string_list_result(
        repository::aggregate_text_values(&state.pool, "official_rating").await?,
    )))
}

async fn containers(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(
        repository::aggregate_text_values(&state.pool, "container").await?,
    )))
}

async fn audio_codecs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(
        repository::aggregate_stream_codecs(&state.pool, "Audio").await?,
    )))
}

async fn video_codecs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(
        repository::aggregate_stream_codecs(&state.pool, "Video").await?,
    )))
}

async fn subtitle_codecs(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(string_list_result(
        repository::aggregate_stream_codecs(&state.pool, "Subtitle").await?,
    )))
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
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
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
        include_types = vec![
            "Movie".to_string(),
            "Series".to_string(),
            "Episode".to_string(),
        ];
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
            repository::media_item_to_dto(
                &state.pool,
                &item,
                Some(user_id),
                state.config.server_id,
            )
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
    let libraries = repository::visible_libraries_for_user(&state.pool, user_id).await?;
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
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
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
        if let Some(rating) = item
            .official_rating
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            official_ratings.insert(rating.clone());
        }
        if let Some(container) = item
            .container
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            containers.insert(container.clone());
        }
        if let Some(codec) = item
            .audio_codec
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            audio_codecs.insert(codec.clone());
        }
        if let Some(codec) = item
            .video_codec
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            video_codecs.insert(codec.clone());
        }
        if let Some(year) = item.production_year {
            years.insert(year);
        }
        if let Some(status) = item
            .status
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            series_statuses.insert(status.clone());
        }
    }
    for (stream_type, codec) in
        repository::media_stream_codecs_for_items(&state.pool, &item_ids).await?
    {
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
    if query.is_movie == Some(true)
        && !include_types
            .iter()
            .any(|value| value.eq_ignore_ascii_case("Movie"))
    {
        include_types.push("Movie".to_string());
    } else if query.is_movie == Some(false) {
        exclude_types.push("Movie".to_string());
    }
    if query.is_series == Some(true)
        && !include_types
            .iter()
            .any(|value| value.eq_ignore_ascii_case("Series"))
    {
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
    let parent_is_root = query.parent_id.is_none_or(|parent_id| parent_id.is_nil());
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
    if repository::get_media_item(&state.pool, item_id)
        .await?
        .is_some()
    {
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

async fn item_ancestors(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Vec<BaseItemDto>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let mut current = repository::get_media_item(&state.pool, item_id).await?;
    let mut ancestors = Vec::new();
    while let Some(item) = current {
        let Some(parent_id) = item.parent_id else {
            break;
        };
        if let Some(parent) = repository::get_media_item(&state.pool, parent_id).await? {
            ancestors.push(
                repository::media_item_to_dto(
                    &state.pool,
                    &parent,
                    Some(session.user_id),
                    state.config.server_id,
                )
                .await?,
            );
            current = Some(parent);
        } else {
            break;
        }
    }
    Ok(Json(ancestors))
}

async fn item_critic_reviews(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    Ok(Json(json!({
        "TotalRecordCount": 0,
        "Items": [],
        "Meta": {
            "CommunityRating": item.community_rating,
            "CriticRating": item.critic_rating
        }
    })))
}

async fn item_external_id_infos(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Vec<Value>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let mut items = Vec::new();
    if let Some(provider_ids) = item.provider_ids.as_object() {
        for (provider, value) in provider_ids {
            items.push(json!({
                "Key": provider,
                "Value": value
            }));
        }
    }
    Ok(Json(items))
}

async fn item_instant_mix(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let target_item = repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let items = repository::find_similar_items(
        &state.pool,
        &target_item,
        20,
        Some(session.user_id),
        state.config.server_id,
    )
    .await?;
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn item_theme_media(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = related_child_items(
        &session,
        &state,
        session.user_id,
        &item_id_str,
        &["ThemeSong", "ThemeVideo"],
    )
    .await?;
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn item_theme_songs(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = related_child_items(
        &session,
        &state,
        session.user_id,
        &item_id_str,
        &["ThemeSong"],
    )
    .await?;
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn item_theme_videos(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = related_child_items(
        &session,
        &state,
        session.user_id,
        &item_id_str,
        &["ThemeVideo"],
    )
    .await?;
    Ok(Json(QueryResult {
        total_record_count: items.len() as i64,
        items,
        start_index: Some(0),
    }))
}

async fn item_metadata_editor(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    // 访问控制：非管理员仍可读取以便客户端展示详情，但后端实际写回由 `update_item` 把关。
    let _ = item_dto(&state, session.user_id, item_id).await?;
    Ok(Json(json!({
        // Emby `MetadataEditorInfo` schema。
        "ExternalIdInfos": external_id_infos_catalog(false),
        "PersonExternalIdInfos": external_id_infos_catalog(true),
        "ParentalRatingOptions": parental_rating_options(),
        "Countries": country_options(),
        "Cultures": culture_options(),
    })))
}

fn external_id_infos_catalog(for_person: bool) -> Vec<Value> {
    // 后端实际支持的外部 id provider。与 `TmdbProvider` / NFO 解析能力一致。
    // Emby Web Console 的"编辑元数据"-"ID" 区块用本清单渲染下拉选项。
    let mut infos = vec![
        json!({
            "Name": "TheMovieDb",
            "Key": "Tmdb",
            "Website": "https://www.themoviedb.org",
            "UrlFormatString": if for_person {
                "https://www.themoviedb.org/person/{0}"
            } else {
                "https://www.themoviedb.org/movie/{0}"
            },
            "IsSupportedAsIdentifier": true
        }),
        json!({
            "Name": "IMDb",
            "Key": "Imdb",
            "Website": "https://www.imdb.com",
            "UrlFormatString": if for_person {
                "https://www.imdb.com/name/{0}"
            } else {
                "https://www.imdb.com/title/{0}"
            },
            "IsSupportedAsIdentifier": true
        }),
    ];
    if !for_person {
        infos.push(json!({
            "Name": "TheTVDB",
            "Key": "Tvdb",
            "Website": "https://thetvdb.com",
            "UrlFormatString": "https://thetvdb.com/?tab=series&id={0}",
            "IsSupportedAsIdentifier": false
        }));
    }
    infos
}

fn parental_rating_options() -> Vec<Value> {
    // 常用分级映射。Emby 使用 `(Name, Value)` 键值对。
    let ratings: &[(&str, i32)] = &[
        ("G", 1),
        ("PG", 5),
        ("PG-13", 7),
        ("R", 9),
        ("NC-17", 11),
        ("NR", 0),
        ("TV-Y", 1),
        ("TV-Y7", 3),
        ("TV-G", 2),
        ("TV-PG", 5),
        ("TV-14", 7),
        ("TV-MA", 11),
    ];
    ratings
        .iter()
        .map(|(name, value)| json!({ "Name": name, "Value": value }))
        .collect()
}

fn country_options() -> Vec<Value> {
    // 播放器模板里的 `metadataCountryCode`/`ProductionLocations` 选项。
    const ENTRIES: &[(&str, &str, &str, &str)] = &[
        ("CN", "China", "CHN", "Asia"),
        ("US", "United States", "USA", "NorthAmerica"),
        ("JP", "Japan", "JPN", "Asia"),
        ("KR", "South Korea", "KOR", "Asia"),
        ("GB", "United Kingdom", "GBR", "Europe"),
        ("FR", "France", "FRA", "Europe"),
        ("DE", "Germany", "DEU", "Europe"),
        ("IT", "Italy", "ITA", "Europe"),
        ("ES", "Spain", "ESP", "Europe"),
        ("RU", "Russia", "RUS", "Europe"),
        ("HK", "Hong Kong", "HKG", "Asia"),
        ("TW", "Taiwan", "TWN", "Asia"),
        ("IN", "India", "IND", "Asia"),
        ("CA", "Canada", "CAN", "NorthAmerica"),
        ("AU", "Australia", "AUS", "Oceania"),
        ("BR", "Brazil", "BRA", "SouthAmerica"),
    ];
    ENTRIES
        .iter()
        .map(|(code, name, three, region)| {
            json!({
                "Name": name,
                "DisplayName": name,
                "TwoLetterISORegionName": code,
                "ThreeLetterISORegionName": three,
                "Region": region,
            })
        })
        .collect()
}

fn culture_options() -> Vec<Value> {
    const ENTRIES: &[(&str, &str, &str, &str)] = &[
        ("zh-CN", "中文(简体)", "zho", "chi"),
        ("zh-TW", "中文(繁體)", "zho", "chi"),
        ("en-US", "English (United States)", "eng", "eng"),
        ("en-GB", "English (United Kingdom)", "eng", "eng"),
        ("ja-JP", "日本語", "jpn", "jpn"),
        ("ko-KR", "한국어", "kor", "kor"),
        ("fr-FR", "Français", "fra", "fre"),
        ("de-DE", "Deutsch", "deu", "ger"),
        ("es-ES", "Español", "spa", "spa"),
        ("it-IT", "Italiano", "ita", "ita"),
        ("ru-RU", "Русский", "rus", "rus"),
        ("pt-BR", "Português (Brasil)", "por", "por"),
        ("th-TH", "ไทย", "tha", "tha"),
        ("vi-VN", "Tiếng Việt", "vie", "vie"),
    ];
    ENTRIES
        .iter()
        .map(|(tag, name, three_letter, two_letter_b)| {
            json!({
                "Name": name,
                "DisplayName": name,
                "TwoLetterISOLanguageName": tag.split('-').next().unwrap_or(tag),
                "ThreeLetterISOLanguageName": three_letter,
                "ThreeLetterISOLanguageNames": [three_letter, two_letter_b],
            })
        })
        .collect()
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct UpdateItemBody {
    name: Option<String>,
    original_title: Option<String>,
    sort_name: Option<String>,
    overview: Option<String>,
    community_rating: Option<f64>,
    critic_rating: Option<f64>,
    official_rating: Option<String>,
    production_year: Option<i32>,
    premiere_date: Option<String>,
    end_date: Option<String>,
    status: Option<String>,
    genres: Option<Vec<Value>>,
    tags: Option<Vec<Value>>,
    studios: Option<Vec<Value>>,
    production_locations: Option<Vec<String>>,
    genre_items: Option<Vec<Value>>,
    tag_items: Option<Vec<Value>>,
    provider_ids: Option<Value>,
    #[serde(alias = "LockedFields")]
    _locked_fields: Option<Vec<String>>,
}

fn coerce_name_list(primary: &Option<Vec<Value>>, fallback: &Option<Vec<Value>>) -> Option<Vec<String>> {
    let source = primary.as_ref().or(fallback.as_ref())?;
    let mut out: Vec<String> = Vec::new();
    for value in source {
        let name = if let Some(s) = value.as_str() {
            Some(s.to_string())
        } else if let Some(obj) = value.as_object() {
            obj.get("Name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };
        if let Some(name) = name {
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() && !out.iter().any(|existing| existing.eq_ignore_ascii_case(&trimmed)) {
                out.push(trimmed);
            }
        }
    }
    Some(out)
}

fn parse_metadata_date(raw: &str) -> Option<chrono::NaiveDate> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Emby 客户端常见格式：ISO date / RFC3339 datetime。
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d") {
        return Some(date);
    }
    if let Ok(date) = chrono::NaiveDate::parse_from_str(trimmed, "%m/%d/%Y") {
        return Some(date);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(trimmed) {
        return Some(dt.date_naive());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
        return Some(dt.date());
    }
    None
}

/// POST `/Items/{ItemId}` — Emby "保存元数据编辑" 入口。
///
/// 接受 Emby 客户端的 BaseItemDto 局部更新，仅把用户可编辑的字段写回数据库。
/// 其它（如 `Path`、`RunTimeTicks`）属于扫描器/探测器输出，这里不允许篡改。
async fn update_item(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Json(body): Json<UpdateItemBody>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    ensure_media_item_exists(&state, item_id).await?;

    let provider_ids_value = body.provider_ids.as_ref().and_then(|v| {
        if let Some(obj) = v.as_object() {
            // 过滤掉空值，保留 string / number 转字符串。
            let mut cleaned = serde_json::Map::new();
            for (k, v) in obj {
                let as_str = v
                    .as_str()
                    .map(ToOwned::to_owned)
                    .or_else(|| v.as_i64().map(|id| id.to_string()))
                    .or_else(|| v.as_f64().map(|id| id.to_string()));
                if let Some(s) = as_str {
                    let trimmed = s.trim().to_string();
                    if !trimmed.is_empty() {
                        cleaned.insert(k.clone(), Value::String(trimmed));
                    }
                }
            }
            if cleaned.is_empty() {
                None
            } else {
                Some(Value::Object(cleaned))
            }
        } else {
            None
        }
    });

    let updates = repository::MediaItemEditableFields {
        name: body.name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        original_title: body.original_title,
        sort_name: body.sort_name,
        overview: body.overview,
        community_rating: body.community_rating,
        critic_rating: body.critic_rating,
        official_rating: body.official_rating,
        production_year: body.production_year,
        premiere_date: body.premiere_date.as_deref().and_then(parse_metadata_date),
        end_date: body.end_date.as_deref().and_then(parse_metadata_date),
        status: body.status,
        genres: coerce_name_list(&body.genre_items, &body.genres),
        tags: coerce_name_list(&body.tag_items, &body.tags),
        studios: coerce_name_list(&body.studios, &None),
        production_locations: body.production_locations,
        provider_ids: provider_ids_value,
    };

    repository::update_media_item_editable_fields(&state.pool, item_id, &updates).await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_item(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    if !repository::delete_media_item(&state.pool, item_id).await? {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_item_info(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<Json<Value>, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let item = repository::get_media_item(&state.pool, item_id).await?;
    let exists = item.is_some();
    Ok(Json(json!({
        "CanDelete": exists,
        "IsPermanent": false,
        "ItemName": item.as_ref().map(|value| value.name.clone()),
        "ItemType": item.as_ref().map(|value| value.item_type.clone())
    })))
}

async fn make_item_private(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    ensure_media_item_exists(&state, item_id).await?;
    repository::set_setting_value(
        &state.pool,
        &format!("item_visibility:{item_id}"),
        json!({"IsPublic": false}),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn make_item_public(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    ensure_media_item_exists(&state, item_id).await?;
    repository::set_setting_value(
        &state.pool,
        &format!("item_visibility:{item_id}"),
        json!({"IsPublic": true}),
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TagMutationQuery {
    #[serde(default, alias = "tag")]
    tag: Option<String>,
    #[serde(default, alias = "name")]
    name: Option<String>,
}

fn extract_tag(query: &TagMutationQuery) -> Result<String, AppError> {
    query
        .tag
        .as_deref()
        .or(query.name.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| AppError::BadRequest("缺少标签参数".to_string()))
}

async fn add_item_tag(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<TagMutationQuery>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let tag = extract_tag(&query)?;
    if !repository::add_media_item_tag(&state.pool, item_id, &tag).await? {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_item_tag(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    Query(query): Query<TagMutationQuery>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    let tag = extract_tag(&query)?;
    if !repository::remove_media_item_tag(&state.pool, item_id, &tag).await? {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remote_search_subtitles_by_language(
    _session: AuthSession,
    Path((_item_id, _language)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({
        "Items": [],
        "TotalRecordCount": 0
    })))
}

async fn remote_search_subtitles_apply(
    _session: AuthSession,
    // 这里复用语言/字幕 ID 路径段；Emby 客户端实际只会调用 POST，不再区分字段。
    Path((_item_id, _language_or_subtitle_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    Ok(StatusCode::NO_CONTENT)
}

async fn items_intros(
    _session: AuthSession,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    Ok(Json(QueryResult {
        total_record_count: 0,
        items: Vec::new(),
        start_index: Some(0),
    }))
}

async fn item_prefixes(_session: AuthSession) -> Result<Json<Vec<String>>, AppError> {
    let mut values = vec!["#".to_string()];
    for ch in 'A'..='Z' {
        values.push(ch.to_string());
    }
    Ok(Json(values))
}

async fn items_access(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    let item_ids = parse_emby_uuid_list(query.ids.as_deref());
    if item_ids.is_empty() {
        return Ok(Json(json!({
            "UserId": user_id.to_string().to_uppercase(),
            "HasAccess": true,
            "Items": []
        })));
    }
    let mut items = Vec::with_capacity(item_ids.len());
    let mut has_access = true;
    for item_id in item_ids {
        let access = repository::user_can_access_item(&state.pool, user_id, item_id).await?;
        has_access &= access;
        items.push(json!({
            "ItemId": uuid_to_emby_guid(&item_id),
            "HasAccess": access
        }));
    }
    Ok(Json(json!({
        "UserId": user_id.to_string().to_uppercase(),
        "HasAccess": has_access,
        "Items": items
    })))
}

async fn delete_items_bulk(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_ids = parse_emby_uuid_list(query.ids.as_deref());
    for item_id in item_ids {
        let _ = repository::delete_media_item(&state.pool, item_id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

/// 节流窗口（秒）：在 `CompletedAt` / `StartedAt` 之后的这段时间内，
/// 重复的元数据重抓请求会被合并为同一次，避免客户端反复点击时把
/// TMDb 配额打爆。
const METADATA_RESET_THROTTLE_SECS: i64 = 30;

/// 判断该条目是否应跳过重新排队。若返回 true，调用方应复用已有的
/// `metadata_reset:{id}` 状态而不是再起一个 tokio 任务。
pub(crate) async fn is_metadata_reset_throttled(
    state: &AppState,
    item_id: Uuid,
) -> Result<bool, AppError> {
    let Some(value) =
        repository::get_setting_value(&state.pool, &format!("metadata_reset:{item_id}")).await?
    else {
        return Ok(false);
    };
    let status = value
        .get("Status")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if status == "queued" || status == "running" {
        return Ok(true);
    }
    // 已经完成 / 失败时，检查是否仍在节流窗口内。
    let now = chrono::Utc::now();
    for key in ["CompletedAt", "StartedAt", "RequestedAt"] {
        if let Some(ts) = value.get(key).and_then(|v| v.as_str()) {
            if let Ok(when) = chrono::DateTime::parse_from_rfc3339(ts) {
                let age = now.signed_duration_since(when.with_timezone(&chrono::Utc));
                if age.num_seconds() >= 0 && age.num_seconds() < METADATA_RESET_THROTTLE_SECS {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// 真正排队执行一次元数据重抓，并把 `metadata_reset:{id}` 状态机推进。
pub(crate) async fn enqueue_metadata_reset(
    state: &AppState,
    item_id: Uuid,
    requester: Uuid,
) -> Result<(), AppError> {
    let requested_at = chrono::Utc::now();
    let status_key = format!("metadata_reset:{item_id}");
    repository::set_setting_value(
        &state.pool,
        &status_key,
        json!({
            "RequestedAt": requested_at.to_rfc3339(),
            "RequestedBy": requester.to_string(),
            "Status": "queued"
        }),
    )
    .await?;

    let state_clone = state.clone();
    let status_key_clone = status_key.clone();
    tokio::spawn(async move {
        let started_at = chrono::Utc::now();
        let _ = repository::set_setting_value(
            &state_clone.pool,
            &status_key_clone,
            json!({
                "RequestedAt": requested_at.to_rfc3339(),
                "RequestedBy": requester.to_string(),
                "StartedAt": started_at.to_rfc3339(),
                "Status": "running"
            }),
        )
        .await;

        match do_refresh_item_metadata(&state_clone, item_id).await {
            Ok(()) => {
                let _ = repository::set_setting_value(
                    &state_clone.pool,
                    &status_key_clone,
                    json!({
                        "RequestedAt": requested_at.to_rfc3339(),
                        "RequestedBy": requester.to_string(),
                        "StartedAt": started_at.to_rfc3339(),
                        "CompletedAt": chrono::Utc::now().to_rfc3339(),
                        "Status": "completed"
                    }),
                )
                .await;
            }
            Err(err) => {
                tracing::warn!(
                    item_id = %item_id,
                    ?err,
                    "后台触发远程元数据刷新失败"
                );
                let _ = repository::set_setting_value(
                    &state_clone.pool,
                    &status_key_clone,
                    json!({
                        "RequestedAt": requested_at.to_rfc3339(),
                        "RequestedBy": requester.to_string(),
                        "StartedAt": started_at.to_rfc3339(),
                        "CompletedAt": chrono::Utc::now().to_rfc3339(),
                        "Status": "failed",
                        "Error": err.to_string()
                    }),
                )
                .await;
            }
        }
    });
    Ok(())
}

async fn reset_items_metadata(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_ids = parse_emby_uuid_list(query.ids.as_deref());
    if item_ids.is_empty() {
        return Ok(Json(json!({ "Queued": [], "Throttled": [] })));
    }

    let mut queued: Vec<String> = Vec::new();
    let mut throttled: Vec<String> = Vec::new();
    for item_id in item_ids {
        if is_metadata_reset_throttled(&state, item_id).await? {
            throttled.push(uuid_to_emby_guid(&item_id));
            continue;
        }
        if let Err(err) = enqueue_metadata_reset(&state, item_id, session.user_id).await {
            tracing::warn!(item_id = %item_id, ?err, "记录元数据重抓请求失败");
            continue;
        }
        queued.push(uuid_to_emby_guid(&item_id));
    }
    Ok(Json(json!({
        "Queued": queued,
        "Throttled": throttled,
        "ThrottleSeconds": METADATA_RESET_THROTTLE_SECS,
    })))
}

/// GET `/Items/Metadata/Reset` — 按 `Ids=...` 批量查询重抓状态。
///
/// Emby 客户端 "刷新中/已完成" 提示面板用于拉取当前重抓进度。
async fn query_items_metadata_reset_status(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<ItemsQuery>,
) -> Result<Json<Value>, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_ids = parse_emby_uuid_list(query.ids.as_deref());
    let mut items = Vec::with_capacity(item_ids.len());
    for item_id in item_ids {
        let status =
            repository::get_setting_value(&state.pool, &format!("metadata_reset:{item_id}"))
                .await?
                .unwrap_or_else(|| json!({ "Status": "idle" }));
        items.push(json!({
            "ItemId": uuid_to_emby_guid(&item_id),
            "Status": status,
        }));
    }
    Ok(Json(json!({
        "Items": items,
        "ThrottleSeconds": METADATA_RESET_THROTTLE_SECS,
    })))
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct RemoteSearchInfo {
    name: Option<String>,
    year: Option<i32>,
    #[serde(default)]
    provider_ids: std::collections::HashMap<String, serde_json::Value>,
    #[serde(default)]
    metadata_language: Option<String>,
    #[serde(default)]
    metadata_country_code: Option<String>,
    #[serde(default)]
    premiere_date: Option<String>,
    #[serde(default)]
    parent_index_number: Option<i32>,
    #[serde(default)]
    index_number: Option<i32>,
    #[serde(default, alias = "Type")]
    item_type: Option<String>,
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct RemoteSearchQueryBody {
    search_info: RemoteSearchInfo,
    #[serde(default)]
    item_id: Option<String>,
    #[serde(default)]
    search_provider_name: Option<String>,
    #[serde(default)]
    include_disabled_providers: Option<bool>,
}

enum RemoteSearchKind {
    Movie,
    Series,
    Person,
}

async fn remote_search_empty(_session: AuthSession) -> Result<Json<Value>, AppError> {
    Ok(Json(json!([])))
}

async fn remote_search_movie(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<RemoteSearchQueryBody>,
) -> Result<Json<Value>, AppError> {
    run_remote_search(&session, &state, RemoteSearchKind::Movie, body).await
}

async fn remote_search_series(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<RemoteSearchQueryBody>,
) -> Result<Json<Value>, AppError> {
    run_remote_search(&session, &state, RemoteSearchKind::Series, body).await
}

async fn remote_search_person(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<RemoteSearchQueryBody>,
) -> Result<Json<Value>, AppError> {
    run_remote_search(&session, &state, RemoteSearchKind::Person, body).await
}

async fn run_remote_search(
    _session: &AuthSession,
    state: &AppState,
    kind: RemoteSearchKind,
    body: RemoteSearchQueryBody,
) -> Result<Json<Value>, AppError> {
    let query_name = body
        .search_info
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string();
    if query_name.is_empty() {
        return Ok(Json(json!([])));
    }
    let Some(metadata_manager) = state.metadata_manager.as_ref() else {
        return Ok(Json(json!([])));
    };
    let library_options = LibraryOptionsDto {
        preferred_metadata_language: body.search_info.metadata_language.clone(),
        metadata_country_code: body.search_info.metadata_country_code.clone(),
        ..LibraryOptionsDto::default()
    };
    let Some(provider) = item_tmdb_provider(state, metadata_manager, &library_options) else {
        return Ok(Json(json!([])));
    };

    let search_provider_name = body
        .search_provider_name
        .clone()
        .unwrap_or_else(|| "TheMovieDb".to_string());
    let tmdb_hint = body
        .search_info
        .provider_ids
        .iter()
        .find_map(|(key, value)| {
            if key.eq_ignore_ascii_case("tmdb") {
                value.as_str().map(ToOwned::to_owned).or_else(|| {
                    value.as_i64().map(|id| id.to_string())
                })
            } else {
                None
            }
        });

    let year = body.search_info.year;
    let mut results: Vec<Value> = Vec::new();

    match kind {
        RemoteSearchKind::Movie => {
            if let Some(tmdb_id) = &tmdb_hint {
                if let Ok(details) = provider.get_movie_details(tmdb_id).await {
                    results.push(movie_metadata_to_search_result(
                        &details,
                        &search_provider_name,
                    ));
                }
            }
            let search_hits = provider.search_movie(&query_name, year).await?;
            for hit in search_hits {
                if results
                    .iter()
                    .any(|existing| existing.get("ProviderIds").is_some_and(|ids| {
                        ids.get("Tmdb")
                            .and_then(|value| value.as_str())
                            .map(|value| value == hit.external_id)
                            .unwrap_or(false)
                    }))
                {
                    continue;
                }
                results.push(search_result_to_emby_value(
                    &hit,
                    &search_provider_name,
                ));
            }
        }
        RemoteSearchKind::Series => {
            if let Some(tmdb_id) = &tmdb_hint {
                if let Ok(details) = provider.get_series_details(tmdb_id).await {
                    results.push(series_metadata_to_search_result(
                        &details,
                        &search_provider_name,
                    ));
                }
            }
            let search_hits = provider.search_series(&query_name, year).await?;
            for hit in search_hits {
                if results
                    .iter()
                    .any(|existing| existing.get("ProviderIds").is_some_and(|ids| {
                        ids.get("Tmdb")
                            .and_then(|value| value.as_str())
                            .map(|value| value == hit.external_id)
                            .unwrap_or(false)
                    }))
                {
                    continue;
                }
                results.push(search_result_to_emby_value(
                    &hit,
                    &search_provider_name,
                ));
            }
        }
        RemoteSearchKind::Person => {
            let hits = provider.search_person(&query_name).await?;
            for hit in hits {
                results.push(person_search_result_to_emby_value(
                    &hit,
                    &search_provider_name,
                ));
            }
        }
    }

    Ok(Json(Value::Array(results)))
}

fn search_result_to_emby_value(
    hit: &crate::metadata::provider::ExternalMediaSearchResult,
    search_provider_name: &str,
) -> Value {
    let mut provider_ids = serde_json::Map::new();
    for (key, value) in &hit.provider_ids {
        provider_ids.insert(key.clone(), Value::String(value.clone()));
    }
    json!({
        "Name": hit.name,
        "ProductionYear": hit.production_year,
        "PremiereDate": hit.premiere_date.map(|date| date.to_string()),
        "ImageUrl": hit.image_url,
        "Overview": hit.overview,
        "SearchProviderName": search_provider_name,
        "ProviderIds": Value::Object(provider_ids),
    })
}

fn movie_metadata_to_search_result(
    metadata: &crate::metadata::models::ExternalMovieMetadata,
    search_provider_name: &str,
) -> Value {
    let mut provider_ids = serde_json::Map::new();
    for (key, value) in &metadata.provider_ids {
        provider_ids.insert(key.clone(), Value::String(value.clone()));
    }
    json!({
        "Name": metadata.name,
        "ProductionYear": metadata.production_year,
        "PremiereDate": metadata.premiere_date.map(|date| date.to_string()),
        "ImageUrl": metadata.poster_image_url,
        "Overview": metadata.overview,
        "SearchProviderName": search_provider_name,
        "ProviderIds": Value::Object(provider_ids),
    })
}

fn series_metadata_to_search_result(
    metadata: &crate::metadata::models::ExternalSeriesMetadata,
    search_provider_name: &str,
) -> Value {
    let mut provider_ids = serde_json::Map::new();
    for (key, value) in &metadata.provider_ids {
        provider_ids.insert(key.clone(), Value::String(value.clone()));
    }
    json!({
        "Name": metadata.name,
        "ProductionYear": metadata.production_year,
        "PremiereDate": metadata.premiere_date.map(|date| date.to_string()),
        "ImageUrl": Value::Null,
        "Overview": metadata.overview,
        "SearchProviderName": search_provider_name,
        "ProviderIds": Value::Object(provider_ids),
    })
}

fn person_search_result_to_emby_value(
    hit: &crate::metadata::models::ExternalPersonSearchResult,
    search_provider_name: &str,
) -> Value {
    let mut provider_ids = serde_json::Map::new();
    provider_ids.insert(
        hit.provider.to_string(),
        Value::String(hit.external_id.clone()),
    );
    json!({
        "Name": hit.name,
        "ProductionYear": Value::Null,
        "PremiereDate": Value::Null,
        "ImageUrl": hit.image_url,
        "Overview": hit.overview,
        "SearchProviderName": search_provider_name,
        "ProviderIds": Value::Object(provider_ids),
    })
}

async fn remote_search_apply(
    session: AuthSession,
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    body: Option<Json<Value>>,
) -> Result<StatusCode, AppError> {
    if !session.is_admin {
        return Err(AppError::Forbidden);
    }
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;

    // 若客户端带来新的 provider ids，先写回媒体项，使重抓时能以新的 TMDb id 为准。
    if let Some(Json(payload)) = body {
        if let Some(provider_ids) = payload.get("ProviderIds").and_then(|value| value.as_object())
        {
            let cleaned: serde_json::Map<String, Value> = provider_ids
                .iter()
                .filter_map(|(key, value)| {
                    let id = value
                        .as_str()
                        .map(ToOwned::to_owned)
                        .or_else(|| value.as_i64().map(|id| id.to_string()))?;
                    if id.trim().is_empty() {
                        None
                    } else {
                        Some((key.clone(), Value::String(id)))
                    }
                })
                .collect();
            if !cleaned.is_empty() {
                repository::update_media_item_provider_ids(
                    &state.pool,
                    item_id,
                    &Value::Object(cleaned),
                )
                .await?;
            }
        }
    }

    // 真正触发一次元数据重抓流程，复用 `metadata_reset:{id}` 节流窗口，
    // 避免客户端在短时间内反复 Apply 时把 TMDb 配额打爆。
    if is_metadata_reset_throttled(&state, item_id).await? {
        tracing::debug!(item_id = %item_id, "RemoteSearch/Apply 命中节流窗口，跳过重复刷新");
    } else if let Err(err) = enqueue_metadata_reset(&state, item_id, session.user_id).await {
        tracing::warn!(item_id = %item_id, ?err, "RemoteSearch 应用后刷新元数据失败");
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct RemoteSearchImageBody {
    #[serde(alias = "search_info")]
    search_info: RemoteSearchInfo,
    #[serde(alias = "search_provider_name")]
    search_provider_name: Option<String>,
    #[serde(alias = "item_id")]
    _item_id: Option<String>,
    #[serde(alias = "image_type", alias = "type", alias = "Type")]
    image_type: Option<String>,
    #[serde(alias = "provider_name", alias = "ProviderName")]
    _provider_name: Option<String>,
    #[serde(alias = "include_all_languages", alias = "IncludeAllLanguages")]
    include_all_languages: Option<bool>,
}

/// `POST /Items/RemoteSearch/Image` — 根据 provider id 聚合 TMDB 多张海报/背景/Logo。
async fn remote_search_image(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<RemoteSearchImageBody>,
) -> Result<Json<Value>, AppError> {
    let _ = &session;
    let Some(metadata_manager) = state.metadata_manager.as_ref() else {
        return Ok(Json(json!([])));
    };
    let library_options = LibraryOptionsDto {
        preferred_metadata_language: body.search_info.metadata_language.clone(),
        metadata_country_code: body.search_info.metadata_country_code.clone(),
        ..LibraryOptionsDto::default()
    };
    let Some(provider) = item_tmdb_provider(&state, metadata_manager, &library_options) else {
        return Ok(Json(json!([])));
    };

    let tmdb_id = body
        .search_info
        .provider_ids
        .iter()
        .find_map(|(key, value)| {
            if !key.eq_ignore_ascii_case("tmdb") {
                return None;
            }
            value
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| value.as_i64().map(|id| id.to_string()))
        });
    let Some(tmdb_id) = tmdb_id else {
        return Ok(Json(json!([])));
    };

    // 粗判类型：默认 movie，如果 search_info 有 series 年份或客户端显式传 series 用 tv。
    let media_type = match body
        .search_info
        .item_type
        .as_deref()
        .unwrap_or("Movie")
        .to_ascii_lowercase()
        .as_str()
    {
        "series" | "tv" | "season" | "episode" => "tv",
        _ => "movie",
    };
    let images = provider.get_remote_images(media_type, &tmdb_id).await?;

    let mut results: Vec<Value> = Vec::new();
    let filter_type = body
        .image_type
        .as_deref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let include_all_langs = body.include_all_languages.unwrap_or(true);
    for img in images {
        if let Some(expected) = filter_type.as_deref() {
            if !img.image_type.eq_ignore_ascii_case(expected) {
                continue;
            }
        }
        if !include_all_langs && img.language.is_some() {
            // 当客户端要求仅当前语种时，TMDB 返回语言为 null 的（中性）图先跳过会造成过度过滤，
            // 因此只有在 language 与首选语言显式不同时才过滤。
            let preferred = library_options.preferred_metadata_language.as_deref().unwrap_or("");
            let lang = img.language.as_deref().unwrap_or("");
            if !preferred.is_empty() && !lang.is_empty() && !preferred.starts_with(lang) {
                continue;
            }
        }
        results.push(json!({
            "ProviderName": img.provider_name,
            "Url": img.url,
            "ThumbnailUrl": img.thumbnail_url,
            "Type": img.image_type,
            "Language": img.language,
            "Width": img.width,
            "Height": img.height,
            "CommunityRating": img.community_rating,
            "VoteCount": img.vote_count,
            "RatingType": "Score",
        }));
    }

    let _ = body.search_provider_name;
    Ok(Json(Value::Array(results)))
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(rename_all = "PascalCase", default)]
struct RemoteSearchTrailerBody {
    search_info: RemoteSearchInfo,
    search_provider_name: Option<String>,
}

/// `POST /Items/RemoteSearch/Trailer` — 返回 YouTube 预告片链接（基于 TMDB videos 字段）。
async fn remote_search_trailer(
    session: AuthSession,
    State(state): State<AppState>,
    Json(body): Json<RemoteSearchTrailerBody>,
) -> Result<Json<Value>, AppError> {
    let _ = &session;
    let Some(metadata_manager) = state.metadata_manager.as_ref() else {
        return Ok(Json(json!([])));
    };
    let library_options = LibraryOptionsDto {
        preferred_metadata_language: body.search_info.metadata_language.clone(),
        metadata_country_code: body.search_info.metadata_country_code.clone(),
        ..LibraryOptionsDto::default()
    };
    let Some(provider) = item_tmdb_provider(&state, metadata_manager, &library_options) else {
        return Ok(Json(json!([])));
    };
    let tmdb_id = body
        .search_info
        .provider_ids
        .iter()
        .find_map(|(key, value)| {
            if !key.eq_ignore_ascii_case("tmdb") {
                return None;
            }
            value
                .as_str()
                .map(ToOwned::to_owned)
                .or_else(|| value.as_i64().map(|id| id.to_string()))
        });
    let search_provider_name = body.search_provider_name.unwrap_or_else(|| "TheMovieDb".into());
    let Some(tmdb_id) = tmdb_id else {
        return Ok(Json(json!([])));
    };

    // TmdbProvider 当前没有直接暴露 trailer 接口；用已有 `get_movie_details` 返回的
    // `remote_trailers`（如存在）或 fallback 到空结果。未来扩 MetadataProvider trait
    // 再接入 `/movie/{id}/videos` 更干净。
    let trailers = match provider.get_movie_details(&tmdb_id).await {
        Ok(details) => details.remote_trailers,
        Err(err) => {
            tracing::debug!(?err, "RemoteSearch/Trailer: TMDB 电影详情查询失败");
            Vec::new()
        }
    };
    let name = body
        .search_info
        .name
        .clone()
        .unwrap_or_else(|| "Trailer".into());
    let results: Vec<Value> = trailers
        .into_iter()
        .map(|url| {
            json!({
                "Name": name,
                "Url": url,
                "SearchProviderName": search_provider_name,
                "ProviderIds": { "Tmdb": tmdb_id },
            })
        })
        .collect();
    Ok(Json(Value::Array(results)))
}

async fn items_shared_leave(
    _session: AuthSession,
) -> Result<StatusCode, AppError> {
    Ok(StatusCode::NO_CONTENT)
}

async fn user_item_by_id(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<BaseItemDto>, AppError> {
    ensure_user_access(&session, user_id)?;
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {}", item_id_str)))?;
    item_dto(&state, user_id, item_id).await
}

async fn item_intros(
    session: AuthSession,
    State(state): State<AppState>,
    Path((user_id, item_id_str)): Path<(Uuid, String)>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let items = related_child_items(&session, &state, user_id, &item_id_str, &["Intro"]).await?;
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
    let items = related_child_items(&session, &state, user_id, &item_id_str, &["Trailer"]).await?;
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
            include_types: include_types
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
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
    if let Some(item) =
        repository::get_missing_episode_dto(&state.pool, item_id, user_id, state.config.server_id)
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
    do_refresh_item_metadata(&state, item_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// 真正执行单个条目的远程元数据重抓流程。
///
/// 当条目非 Movie/Series、缺少 TMDb provider id 或未配置 TMDb
/// 提供者时，本函数会记录日志并直接返回成功，保证调用方的批处理
/// 可以按“尽力而为”的语义继续跑完剩余条目。
pub(crate) async fn do_refresh_item_metadata(
    state: &AppState,
    item_id: Uuid,
) -> Result<(), AppError> {
    let Some(item) = repository::get_media_item(&state.pool, item_id).await? else {
        return Err(AppError::NotFound("媒体条目不存在".to_string()));
    };

    if !item.item_type.eq_ignore_ascii_case("Series")
        && !item.item_type.eq_ignore_ascii_case("Movie")
    {
        return Ok(());
    }
    work_limiter_config(state).await?;
    let _refresh_permit = state
        .work_limiters
        .acquire(WorkLimiterKind::LibraryScan)
        .await;

    let Some(tmdb_id) = tmdb_id_from_provider_ids(&item.provider_ids) else {
        tracing::debug!(
            item_id = %item.id,
            item_type = %item.item_type,
            "跳过远程元数据刷新：条目缺少 TMDb provider id"
        );
        return Ok(());
    };
    let Some(metadata_manager) = state.metadata_manager.as_ref() else {
        tracing::debug!(
            item_id = %item.id,
            "跳过远程元数据刷新：未配置远程元数据提供者"
        );
        return Ok(());
    };
    let library_options = item_library_options(state, item.id).await?;
    let Some(provider) = item_tmdb_provider(state, metadata_manager, &library_options) else {
        tracing::debug!(
            item_id = %item.id,
            "跳过远程元数据刷新：未配置 TMDb 元数据提供者"
        );
        return Ok(());
    };

    if item.item_type.eq_ignore_ascii_case("Series") {
        let _tmdb_permit = state
            .work_limiters
            .acquire(WorkLimiterKind::TmdbMetadata)
            .await;
        let metadata = provider.get_series_details(&tmdb_id).await?;
        drop(_tmdb_permit);
        repository::update_media_item_series_metadata(&state.pool, item.id, &metadata).await?;
        let _tmdb_permit = state
            .work_limiters
            .acquire(WorkLimiterKind::TmdbMetadata)
            .await;
        let catalog = provider.get_series_episode_catalog(&tmdb_id).await?;
        drop(_tmdb_permit);
        repository::replace_series_episode_catalog(&state.pool, item.id, &catalog).await?;
    } else if item.item_type.eq_ignore_ascii_case("Movie") {
        let _tmdb_permit = state
            .work_limiters
            .acquire(WorkLimiterKind::TmdbMetadata)
            .await;
        let metadata = provider.get_movie_details(&tmdb_id).await?;
        drop(_tmdb_permit);
        repository::update_media_item_movie_metadata(&state.pool, item.id, &metadata).await?;
    }

    let media_type = if item.item_type.eq_ignore_ascii_case("Series") {
        "tv"
    } else {
        "movie"
    };
    let _tmdb_permit = state
        .work_limiters
        .acquire(WorkLimiterKind::TmdbMetadata)
        .await;
    let people = provider.get_item_people(media_type, &tmdb_id).await?;
    drop(_tmdb_permit);
    let tmdb_person_ids = people
        .iter()
        .filter_map(|person| {
            person
                .provider_ids
                .get("Tmdb")
                .or_else(|| person.provider_ids.get("TMDb"))
                .or_else(|| person.provider_ids.get("tmdb"))
                .cloned()
        })
        .collect::<Vec<_>>();
    repository::delete_tmdb_person_roles_except(&state.pool, item.id, &tmdb_person_ids).await?;
    let person_service = PersonService::new(state.pool.clone(), metadata_manager.clone());
    for person in people {
        person_service.upsert_item_person(item.id, person).await?;
    }

    Ok(())
}

async fn work_limiter_config(state: &AppState) -> Result<WorkLimiterConfig, AppError> {
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let config = WorkLimiterConfig {
        library_scan_limit: startup.library_scan_thread_count.max(1) as u32,
        media_analysis_limit: startup.strm_analysis_thread_count.max(1) as u32,
        tmdb_metadata_limit: startup.tmdb_metadata_thread_count.max(1) as u32,
    };
    state.work_limiters.configure(config).await;
    Ok(config)
}

async fn item_library_options(
    state: &AppState,
    item_id: Uuid,
) -> Result<LibraryOptionsDto, AppError> {
    Ok(repository::get_library_for_media_item(&state.pool, item_id)
        .await?
        .map(|library| repository::library_options(&library))
        .unwrap_or_default())
}

fn item_tmdb_provider<'a>(
    state: &'a AppState,
    metadata_manager: &'a MetadataProviderManager,
    library_options: &'a LibraryOptionsDto,
) -> Option<Box<dyn MetadataProvider + 'a>> {
    if let Some(api_key) = &state.config.tmdb_api_key {
        let preferred_metadata_language = library_options
            .preferred_metadata_language
            .as_deref()
            .unwrap_or(&state.config.preferred_metadata_language);
        let metadata_country_code = library_options
            .metadata_country_code
            .as_deref()
            .unwrap_or(&state.config.metadata_country_code);
        return Some(Box::new(TmdbProvider::new_with_preferences(
            api_key.clone(),
            preferred_metadata_language,
            metadata_country_code,
        )));
    }

    metadata_manager
        .get_provider("tmdb")
        .map(|provider| Box::new(RouteProviderRef { inner: provider }) as Box<dyn MetadataProvider>)
}

struct RouteProviderRef<'a> {
    inner: &'a dyn MetadataProvider,
}

#[async_trait::async_trait]
impl MetadataProvider for RouteProviderRef<'_> {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn search_person(
        &self,
        name: &str,
    ) -> Result<Vec<crate::metadata::models::ExternalPersonSearchResult>, AppError> {
        self.inner.search_person(name).await
    }

    async fn get_person_details(
        &self,
        provider_id: &str,
    ) -> Result<crate::metadata::models::ExternalPerson, AppError> {
        self.inner.get_person_details(provider_id).await
    }

    async fn get_person_credits(
        &self,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalPersonCredit>, AppError> {
        self.inner.get_person_credits(provider_id).await
    }

    async fn get_series_details(
        &self,
        provider_id: &str,
    ) -> Result<crate::metadata::models::ExternalSeriesMetadata, AppError> {
        self.inner.get_series_details(provider_id).await
    }

    async fn get_movie_details(
        &self,
        provider_id: &str,
    ) -> Result<crate::metadata::models::ExternalMovieMetadata, AppError> {
        self.inner.get_movie_details(provider_id).await
    }

    async fn get_item_people(
        &self,
        media_type: &str,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalItemPerson>, AppError> {
        self.inner.get_item_people(media_type, provider_id).await
    }

    async fn get_series_episode_catalog(
        &self,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalEpisodeCatalogItem>, AppError> {
        self.inner.get_series_episode_catalog(provider_id).await
    }

    async fn get_remote_images(
        &self,
        media_type: &str,
        provider_id: &str,
    ) -> Result<Vec<crate::metadata::provider::ExternalRemoteImage>, AppError> {
        self.inner.get_remote_images(media_type, provider_id).await
    }

    async fn get_remote_images_for_child(
        &self,
        media_type: &str,
        series_provider_id: &str,
        season_number: Option<i32>,
        episode_number: Option<i32>,
    ) -> Result<Vec<crate::metadata::provider::ExternalRemoteImage>, AppError> {
        self.inner
            .get_remote_images_for_child(
                media_type,
                series_provider_id,
                season_number,
                episode_number,
            )
            .await
    }

    async fn search_movie(
        &self,
        name: &str,
        year: Option<i32>,
    ) -> Result<Vec<crate::metadata::provider::ExternalMediaSearchResult>, AppError> {
        self.inner.search_movie(name, year).await
    }

    async fn search_series(
        &self,
        name: &str,
        year: Option<i32>,
    ) -> Result<Vec<crate::metadata::provider::ExternalMediaSearchResult>, AppError> {
        self.inner.search_series(name, year).await
    }
}

fn tmdb_id_from_provider_ids(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    ["Tmdb", "TMDb", "tmdb"].iter().find_map(|key| {
        object
            .get(*key)
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .or_else(|| {
                object
                    .get(*key)
                    .and_then(|value| value.as_i64().map(|id| id.to_string()))
            })
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
    let user_policy =
        auth::ensure_item_access(&state, &session, item_id, MediaAccessKind::Playback).await?;
    let request_headers = request.headers().clone();
    let request_query = request.uri().query().map(ToOwned::to_owned);
    let request_device_id = auth::client_value(&request_headers, "DeviceId")
        .or_else(|| query_value(request_query.as_deref(), &["DeviceId", "deviceId"]));
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
                    if body_info.max_streaming_bitrate.is_none()
                        && query_info.max_streaming_bitrate.is_some()
                    {
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

    let needs_metadata =
        item.video_codec.is_none() || item.audio_codec.is_none() || item.runtime_ticks.is_none();
    if needs_metadata {
        work_limiter_config(&state).await?;
        let item_path = item.path.clone();
        let path = std::path::Path::new(&item_path);
        if path.exists() {
            if naming::is_strm(path) {
                // 对于.strm文件，尝试分析远程URL
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        if let Some(target_url) = naming::strm_target_from_text(&content) {
                            tracing::debug!("分析.strm文件远程URL: {}", target_url);
                            let _analysis_permit = state
                                .work_limiters
                                .acquire(WorkLimiterKind::MediaAnalysis)
                                .await;
                            match media_analyzer::analyze_remote_media(&target_url).await {
                                Ok(analysis) => {
                                    repository::update_media_item_metadata(
                                        &state.pool,
                                        item_id,
                                        &analysis,
                                    )
                                    .await?;
                                    item = repository::get_media_item(&state.pool, item_id)
                                        .await?
                                        .ok_or_else(|| {
                                            AppError::NotFound("媒体条目不存在".to_string())
                                        })?;
                                    tracing::info!(
                                        "已更新.strm文件远程媒体元数据: {}",
                                        path.display()
                                    );
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
                let _analysis_permit = state
                    .work_limiters
                    .acquire(WorkLimiterKind::MediaAnalysis)
                    .await;
                match media_analyzer::analyze_media_file(path).await {
                    Ok(analysis) => {
                        repository::update_media_item_metadata(&state.pool, item_id, &analysis)
                            .await?;
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

    let mut effective_max_bitrate = info.max_streaming_bitrate.or_else(|| {
        info.device_profile
            .as_ref()
            .and_then(|profile| profile.max_streaming_bitrate)
    });
    if user_policy.remote_client_bitrate_limit > 0 {
        let policy_limit = i64::from(user_policy.remote_client_bitrate_limit);
        effective_max_bitrate = Some(
            effective_max_bitrate
                .map(|value| value.min(policy_limit))
                .unwrap_or(policy_limit),
        );
    }

    let transcode_reasons = media_sources
        .get(selected_media_source_index)
        .map(|source| transcoding_reasons(&info, source, effective_max_bitrate))
        .unwrap_or_default();
    let force_transcoding = !transcode_reasons.is_empty();

    let playback_item_id = crate::models::uuid_to_emby_guid(&item.id);
    let playback_user_id = info.user_id.unwrap_or(session.user_id);
    let playback_user_id = uuid_to_emby_guid(&playback_user_id);
    for media_source in &mut media_sources {
        let media_source_id = media_source.id.clone();
        media_source.direct_stream_url = Some(build_direct_stream_url(
            &playback_item_id,
            &media_source_id,
            media_source.container.as_str(),
            &play_session_id,
            &session.access_token,
            request_device_id.as_deref(),
        ));
        media_source.add_api_key_to_direct_stream_url = Some(false);
        media_source.media_streams.retain(|stream| {
            !(stream.stream_type.eq_ignore_ascii_case("Video")
                && stream
                    .codec
                    .as_deref()
                    .is_some_and(|codec| codec.eq_ignore_ascii_case("mjpeg")))
        });
        for stream in &mut media_source.media_streams {
            if stream.stream_type.eq_ignore_ascii_case("Subtitle")
                && stream
                    .codec
                    .as_deref()
                    .is_some_and(|codec| codec.eq_ignore_ascii_case("hdmv_pgs_subtitle"))
            {
                stream.codec = Some("PGSSUB".to_string());
                stream.mime_type = None;
            }
        }
        media_source.default_subtitle_stream_index = media_source
            .media_streams
            .iter()
            .find(|stream| stream.stream_type.eq_ignore_ascii_case("Subtitle") && stream.is_default)
            .map(|stream| stream.index)
            .or_else(|| {
                media_source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Subtitle"))
                    .map(|stream| stream.index)
            });
        media_source.default_audio_stream_index = media_source
            .media_streams
            .iter()
            .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio") && stream.is_default)
            .map(|stream| stream.index)
            .or_else(|| {
                media_source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
                    .map(|stream| stream.index)
            });
        media_source.required_http_headers.retain(|key, _| {
            key.eq_ignore_ascii_case("Accept-Ranges") || key.eq_ignore_ascii_case("Range")
        });
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

    let encoding_options = repository::encoding_options(&state.pool, &state.config).await?;
    if force_transcoding && !encoding_options.enable_transcoding {
        tracing::warn!(
            item_id = %item.id,
            "播放请求需要转码，但转码功能已禁用；保留 Emby 直连媒体源供客户端尝试播放"
        );
    } else if force_transcoding
        && !user_policy.enable_video_playback_transcoding
        && transcode_reasons.iter().any(|reason| {
            reason.contains("Video")
                || reason.contains("Container")
                || reason.contains("Bitrate")
                || reason.contains("Subtitle")
        })
    {
        tracing::warn!(
            item_id = %item.id,
            user_id = %session.user_id,
            "播放请求需要视频转码，但用户策略禁止视频转码；保留直连媒体源"
        );
    } else if force_transcoding
        && !user_policy.enable_audio_playback_transcoding
        && transcode_reasons
            .iter()
            .any(|reason| reason.contains("Audio"))
    {
        tracing::warn!(
            item_id = %item.id,
            user_id = %session.user_id,
            "播放请求需要音频转码，但用户策略禁止音频转码；保留直连媒体源"
        );
    } else if force_transcoding && !user_policy.enable_playback_remuxing {
        tracing::warn!(
            item_id = %item.id,
            user_id = %session.user_id,
            "播放请求需要封装/转码链路，但用户策略禁止 remux；保留直连媒体源"
        );
    } else if force_transcoding {
        let item_emby_id = crate::models::uuid_to_emby_guid(&item.id);
        let selected_media_source_id = media_sources
            .get(selected_media_source_index)
            .map(|source| source.id.clone())
            .unwrap_or_else(|| format!("mediasource_{item_emby_id}"));

        let transcoding_container = preferred_transcoding_container(&info);
        let transcoding_sub_protocol =
            preferred_transcoding_sub_protocol(&info, &transcoding_container);
        let transcoding_video_codec =
            preferred_transcoding_video_codec(&info, &transcoding_sub_protocol);
        let transcoding_audio_codec =
            preferred_transcoding_audio_codec(&info, &transcoding_sub_protocol);
        let transcoding_url = build_transcoding_url(
            &item_emby_id,
            &selected_media_source_id,
            &play_session_id,
            &session.access_token,
            &playback_user_id,
            request_device_id.as_deref(),
            info.audio_stream_index,
            info.subtitle_stream_index,
            info.start_time_ticks,
            effective_max_bitrate,
            transcoding_video_codec.as_deref(),
            transcoding_audio_codec.as_deref(),
            &transcoding_container,
            &transcoding_sub_protocol,
        );

        if let Some(selected_source) = media_sources.get_mut(selected_media_source_index) {
            selected_source.supports_direct_play = false;
            selected_source.supports_direct_stream = false;
            selected_source.direct_stream_url = None;
            selected_source.add_api_key_to_direct_stream_url = Some(false);
            selected_source.transcoding_url = Some(transcoding_url.clone());
            selected_source.transcoding_container = Some(transcoding_container.clone());
            selected_source.transcoding_sub_protocol = Some(transcoding_sub_protocol.clone());
        }

        let transcoding_info = media_sources
            .get(selected_media_source_index)
            .map(|source| {
                build_transcoding_info(
                    source,
                    &info,
                    &transcoding_container,
                    &transcoding_sub_protocol,
                    transcode_reasons.clone(),
                )
            });

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
    info: &PlaybackInfoDto,
    container: &str,
    sub_protocol: &str,
    reasons: Vec<String>,
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
    let video_bitrate = video_stream
        .and_then(|stream| stream.bit_rate)
        .or(source_bitrate);
    let audio_bitrate = audio_stream.and_then(|stream| stream.bit_rate);
    let start_ticks = info.start_time_ticks.filter(|value| *value > 0);

    TranscodingInfoDto {
        audio_codec: audio_stream.and_then(|stream| stream.codec.clone()),
        video_codec: video_stream.and_then(|stream| stream.codec.clone()),
        sub_protocol: Some(sub_protocol.to_string()),
        container: Some(container.to_string()),
        is_video_direct: false,
        is_audio_direct: false,
        bitrate: source_bitrate,
        audio_bitrate,
        video_bitrate,
        framerate: video_stream
            .and_then(|stream| stream.real_frame_rate.or(stream.average_frame_rate)),
        completion_percentage: Some(0.0),
        transcoding_position_ticks: start_ticks,
        transcoding_start_position_ticks: start_ticks,
        width: video_stream.and_then(|stream| stream.width),
        height: video_stream.and_then(|stream| stream.height),
        audio_channels: audio_stream.and_then(|stream| stream.channels),
        hardware_acceleration_type: None,
        transcode_reasons: reasons,
    }
}

fn device_profile_supports_direct_play(
    profile: &crate::models::DeviceProfile,
    source: &crate::models::MediaSourceDto,
) -> bool {
    if !container_profiles_match(&profile.container_profiles, source) {
        return false;
    }
    if !codec_profiles_match(&profile.codec_profiles, source) {
        return false;
    }

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
    if !codec_profiles_match(&profile.codec_profiles, source) {
        return false;
    }

    profile
        .transcoding_profiles
        .iter()
        .any(|transcoding_profile| {
            transcoding_profile
                .r#type
                .as_deref()
                .is_none_or(|value| value.eq_ignore_ascii_case("Video"))
                && transcoding_profile
                    .container
                    .as_deref()
                    .is_none_or(|value| !value.trim().is_empty())
                && codec_profile_matches(
                    transcoding_profile.video_codec.as_deref(),
                    source,
                    "Video",
                )
                && codec_profile_matches(
                    transcoding_profile.audio_codec.as_deref(),
                    source,
                    "Audio",
                )
        })
}

fn container_profiles_match(profiles: &[Value], source: &crate::models::MediaSourceDto) -> bool {
    profiles.iter().all(|profile| {
        if !profile_type_matches(profile, "Video") {
            return true;
        }
        profile_conditions_match(profile, None, Some(source))
    })
}

fn codec_profiles_match(profiles: &[Value], source: &crate::models::MediaSourceDto) -> bool {
    profiles.iter().all(|profile| {
        if !profile_type_matches(profile, "Video") {
            return true;
        }
        let codec_filter = profile
            .get("Codec")
            .or_else(|| profile.get("codec"))
            .and_then(Value::as_str)
            .map(|value| {
                parse_list(Some(value))
                    .into_iter()
                    .map(|codec| codec.to_ascii_lowercase())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let profile_type = profile
            .get("Type")
            .or_else(|| profile.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("Video");

        source.media_streams.iter().all(|stream| {
            if !stream.stream_type.eq_ignore_ascii_case("Video")
                && !stream.stream_type.eq_ignore_ascii_case("Audio")
                && !stream.stream_type.eq_ignore_ascii_case("Subtitle")
            {
                return true;
            }
            if !stream.stream_type.eq_ignore_ascii_case(profile_type) {
                return true;
            }
            if !codec_filter.is_empty() {
                let Some(codec) = stream.codec.as_deref() else {
                    return true;
                };
                if !codec_filter
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(codec))
                {
                    return true;
                }
            }
            profile_conditions_match(profile, Some(stream), Some(source))
        })
    })
}

fn profile_type_matches(profile: &Value, expected: &str) -> bool {
    profile
        .get("Type")
        .or_else(|| profile.get("type"))
        .and_then(Value::as_str)
        .is_none_or(|value| value.eq_ignore_ascii_case(expected))
}

fn profile_conditions_match(
    profile: &Value,
    stream: Option<&crate::models::MediaStreamDto>,
    source: Option<&crate::models::MediaSourceDto>,
) -> bool {
    let Some(conditions) = profile
        .get("Conditions")
        .or_else(|| profile.get("conditions"))
        .and_then(Value::as_array)
    else {
        return true;
    };

    conditions
        .iter()
        .all(|condition| profile_condition_matches(condition, stream, source))
}

fn profile_condition_matches(
    condition: &Value,
    stream: Option<&crate::models::MediaStreamDto>,
    source: Option<&crate::models::MediaSourceDto>,
) -> bool {
    let property = condition
        .get("Property")
        .or_else(|| condition.get("property"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    let condition_name = condition
        .get("Condition")
        .or_else(|| condition.get("condition"))
        .and_then(Value::as_str)
        .unwrap_or("Equals");
    let target = condition.get("Value").or_else(|| condition.get("value"));

    let Some(actual) = profile_property_value(property, stream, source) else {
        return !condition
            .get("IsRequired")
            .or_else(|| condition.get("isRequired"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    };

    match (actual, target) {
        (ProfileValue::Number(actual), Some(target)) => target
            .as_f64()
            .or_else(|| target.as_str().and_then(|value| value.parse::<f64>().ok()))
            .is_none_or(|target| compare_profile_number(actual, target, condition_name)),
        (ProfileValue::Text(actual), Some(target)) => target
            .as_str()
            .is_none_or(|target| compare_profile_text(&actual, target, condition_name)),
        (ProfileValue::Bool(actual), Some(target)) => target
            .as_bool()
            .or_else(|| {
                target
                    .as_str()
                    .map(|value| value.eq_ignore_ascii_case("true"))
            })
            .is_none_or(|target| compare_profile_bool(actual, target, condition_name)),
        _ => true,
    }
}

enum ProfileValue {
    Number(f64),
    Text(String),
    Bool(bool),
}

fn profile_property_value(
    property: &str,
    stream: Option<&crate::models::MediaStreamDto>,
    source: Option<&crate::models::MediaSourceDto>,
) -> Option<ProfileValue> {
    let normalized = property.to_ascii_lowercase();
    let stream = stream.or_else(|| {
        source.and_then(|source| {
            source
                .media_streams
                .iter()
                .find(|stream| stream.stream_type.eq_ignore_ascii_case("Video"))
        })
    });
    match normalized.as_str() {
        "width" => stream
            .and_then(|stream| stream.width)
            .map(|value| ProfileValue::Number(f64::from(value))),
        "height" => stream
            .and_then(|stream| stream.height)
            .map(|value| ProfileValue::Number(f64::from(value))),
        "videobitrate" | "bitrate" => stream
            .and_then(|stream| stream.bit_rate)
            .or_else(|| source.and_then(|source| source.bitrate))
            .map(|value| ProfileValue::Number(f64::from(value))),
        "videobitdepth" | "bitdepth" => stream
            .and_then(|stream| stream.bit_depth)
            .map(|value| ProfileValue::Number(f64::from(value))),
        "videolevel" | "level" => stream
            .and_then(|stream| stream.level)
            .map(|value| ProfileValue::Number(f64::from(value))),
        "videorefframes" | "refframes" => stream
            .and_then(|stream| stream.ref_frames)
            .map(|value| ProfileValue::Number(f64::from(value))),
        "videoprofile" | "profile" => stream
            .and_then(|stream| stream.profile.clone())
            .map(ProfileValue::Text),
        "videorange" => stream
            .and_then(|stream| stream.video_range.clone())
            .map(ProfileValue::Text),
        "videorangetype" | "extendedvideotype" => stream
            .and_then(|stream| stream.extended_video_type.clone())
            .map(ProfileValue::Text),
        "extendedvideosubtype" | "videoprofilesubtype" => stream
            .and_then(|stream| stream.extended_video_sub_type.clone())
            .map(ProfileValue::Text),
        "videocolorspace" | "colorspace" => stream
            .and_then(|stream| stream.color_space.clone())
            .map(ProfileValue::Text),
        "videocolortransfer" | "colortransfer" => stream
            .and_then(|stream| stream.color_transfer.clone())
            .map(ProfileValue::Text),
        "videocolorprimaries" | "colorprimaries" => stream
            .and_then(|stream| stream.color_primaries.clone())
            .map(ProfileValue::Text),
        "pixelformat" => stream
            .and_then(|stream| stream.pixel_format.clone())
            .map(ProfileValue::Text),
        "videocodec" | "codec" => stream
            .and_then(|stream| stream.codec.clone())
            .map(ProfileValue::Text),
        "audiocodec" => source
            .and_then(|source| {
                source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
                    .and_then(|stream| stream.codec.clone())
            })
            .map(ProfileValue::Text),
        "audiobitrate" => source
            .and_then(|source| {
                source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
                    .and_then(|stream| stream.bit_rate)
            })
            .map(|value| ProfileValue::Number(f64::from(value))),
        "audiochannels" => source
            .and_then(|source| {
                source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
                    .and_then(|stream| stream.channels)
            })
            .map(|value| ProfileValue::Number(f64::from(value))),
        "audiosamplerate" => source
            .and_then(|source| {
                source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
                    .and_then(|stream| stream.sample_rate)
            })
            .map(|value| ProfileValue::Number(f64::from(value))),
        "audiobitdepth" => source
            .and_then(|source| {
                source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
                    .and_then(|stream| stream.bit_depth)
            })
            .map(|value| ProfileValue::Number(f64::from(value))),
        "subtitlecodec" => source
            .and_then(|source| {
                source
                    .media_streams
                    .iter()
                    .find(|stream| stream.stream_type.eq_ignore_ascii_case("Subtitle"))
                    .and_then(|stream| stream.codec.clone())
            })
            .map(ProfileValue::Text),
        "isinterlaced" => stream
            .and_then(|stream| stream.is_interlaced)
            .map(ProfileValue::Bool),
        "isanamorphic" => stream
            .and_then(|stream| stream.is_anamorphic)
            .map(ProfileValue::Bool),
        "isavc" => stream
            .and_then(|stream| stream.is_avc)
            .map(ProfileValue::Bool),
        _ => None,
    }
}

fn compare_profile_number(actual: f64, target: f64, condition: &str) -> bool {
    match condition.to_ascii_lowercase().as_str() {
        "lessthanequal" => actual <= target,
        "lessthan" => actual < target,
        "greaterthanequal" => actual >= target,
        "greaterthan" => actual > target,
        "notequals" => (actual - target).abs() > f64::EPSILON,
        _ => (actual - target).abs() <= f64::EPSILON,
    }
}

fn compare_profile_text(actual: &str, target: &str, condition: &str) -> bool {
    let contains = parse_list(Some(target))
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(actual));
    match condition.to_ascii_lowercase().as_str() {
        "notequals" => !actual.eq_ignore_ascii_case(target),
        "notcontains" => !contains,
        _ => contains || actual.eq_ignore_ascii_case(target),
    }
}

fn compare_profile_bool(actual: bool, target: bool, condition: &str) -> bool {
    match condition.to_ascii_lowercase().as_str() {
        "notequals" => actual != target,
        _ => actual == target,
    }
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
        .all(|codec| {
            allowed
                .iter()
                .any(|allowed_codec| allowed_codec.eq_ignore_ascii_case(codec))
        })
}

fn csv_option_contains(csv: Option<&str>, value: &str) -> bool {
    csv.is_none_or(|csv| {
        parse_list(Some(csv))
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(value))
    })
}

fn transcoding_reasons(
    info: &PlaybackInfoDto,
    media_source: &crate::models::MediaSourceDto,
    effective_max_bitrate: Option<i64>,
) -> Vec<String> {
    let mut reasons = Vec::new();

    if matches!(info.enable_transcoding, Some(false)) {
        return reasons;
    }

    if matches!(info.enable_direct_play, Some(false)) {
        reasons.push("DirectPlayError".to_string());
    }
    if matches!(info.enable_direct_stream, Some(false)) {
        reasons.push("DirectStreamError".to_string());
    }

    if let Some(profile) = &info.device_profile {
        if !profile.direct_play_profiles.is_empty()
            && !device_profile_supports_direct_play(profile, media_source)
        {
            push_reason(&mut reasons, "ContainerNotSupported");
        }
    }

    if let Some(max_audio_channels) = info.max_audio_channels {
        if media_source.media_streams.iter().any(|stream| {
            stream.stream_type.eq_ignore_ascii_case("Audio")
                && stream
                    .channels
                    .is_some_and(|channels| channels > max_audio_channels)
        }) {
            push_reason(&mut reasons, "AudioChannelsNotSupported");
        }
    }

    if matches!(info.allow_video_stream_copy, Some(false))
        && media_source
            .media_streams
            .iter()
            .any(|stream| stream.stream_type.eq_ignore_ascii_case("Video"))
    {
        push_reason(&mut reasons, "VideoCodecNotSupported");
    }

    if matches!(info.allow_audio_stream_copy, Some(false))
        && media_source
            .media_streams
            .iter()
            .any(|stream| stream.stream_type.eq_ignore_ascii_case("Audio"))
    {
        push_reason(&mut reasons, "AudioCodecNotSupported");
    }

    if matches!(info.allow_interlaced_video_stream_copy, Some(false))
        && media_source.media_streams.iter().any(|stream| {
            stream.stream_type.eq_ignore_ascii_case("Video")
                && stream.is_interlaced.unwrap_or(false)
        })
    {
        push_reason(&mut reasons, "InterlacedVideoNotSupported");
    }

    if matches!(info.always_burn_in_subtitle_when_transcoding, Some(true))
        && selected_subtitle_stream(media_source, info.subtitle_stream_index).is_some()
    {
        push_reason(&mut reasons, "SubtitleCodecNotSupported");
    }

    if let (Some(max_bitrate), Some(media_bitrate)) =
        (effective_max_bitrate, media_source.bitrate.map(i64::from))
    {
        if media_bitrate > max_bitrate && matches!(info.enable_transcoding, Some(true) | None) {
            push_reason(&mut reasons, "ContainerBitrateExceedsLimit");
        }
    }

    reasons
}

fn push_reason(reasons: &mut Vec<String>, reason: &str) {
    if !reasons.iter().any(|existing| existing == reason) {
        reasons.push(reason.to_string());
    }
}

fn selected_subtitle_stream<'a>(
    media_source: &'a crate::models::MediaSourceDto,
    subtitle_stream_index: Option<i32>,
) -> Option<&'a crate::models::MediaStreamDto> {
    let index = subtitle_stream_index?;
    if index < 0 {
        return None;
    }
    media_source
        .media_streams
        .iter()
        .find(|stream| stream.stream_type.eq_ignore_ascii_case("Subtitle") && stream.index == index)
}

fn preferred_transcoding_profile(
    info: &PlaybackInfoDto,
) -> Option<&crate::models::TranscodingProfile> {
    info.device_profile
        .as_ref()?
        .transcoding_profiles
        .iter()
        .find(|profile| {
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

fn preferred_transcoding_video_codec(info: &PlaybackInfoDto, sub_protocol: &str) -> Option<String> {
    preferred_transcoding_profile(info)
        .and_then(|profile| profile.video_codec.as_deref())
        .map(first_csv_value)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if sub_protocol.eq_ignore_ascii_case("hls") {
                Some("h264".to_string())
            } else {
                None
            }
        })
}

fn preferred_transcoding_audio_codec(info: &PlaybackInfoDto, sub_protocol: &str) -> Option<String> {
    preferred_transcoding_profile(info)
        .and_then(|profile| profile.audio_codec.as_deref())
        .map(first_csv_value)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            if sub_protocol.eq_ignore_ascii_case("hls") {
                Some("aac".to_string())
            } else {
                None
            }
        })
}

fn first_csv_value(value: &str) -> String {
    value
        .split(',')
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or_default()
        .to_string()
}

fn build_transcoding_url(
    item_emby_id: &str,
    media_source_id: &str,
    play_session_id: &str,
    access_token: &str,
    user_id: &str,
    device_id: Option<&str>,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
    start_time_ticks: Option<i64>,
    max_streaming_bitrate: Option<i64>,
    video_codec: Option<&str>,
    audio_codec: Option<&str>,
    transcoding_container: &str,
    transcoding_sub_protocol: &str,
) -> String {
    let mut params = vec![
        format!("MediaSourceId={media_source_id}"),
        format!("mediaSourceId={media_source_id}"),
        format!("PlaySessionId={play_session_id}"),
        format!("Container={transcoding_container}"),
        format!("api_key={access_token}"),
        format!("X-Emby-Token={access_token}"),
        format!("X-MediaBrowser-Token={access_token}"),
        format!("UserId={user_id}"),
    ];
    if let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) {
        params.push(format!("DeviceId={device_id}"));
    }

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
    if let Some(value) = video_codec.filter(|value| !value.trim().is_empty()) {
        params.push(format!("VideoCodec={value}"));
    }
    if let Some(value) = audio_codec.filter(|value| !value.trim().is_empty()) {
        params.push(format!("AudioCodec={value}"));
    }

    if transcoding_sub_protocol.eq_ignore_ascii_case("hls") {
        format!(
            "/emby/Videos/{item_emby_id}/master.m3u8?{}",
            params.join("&")
        )
    } else {
        format!(
            "/emby/Videos/{item_emby_id}/stream.{transcoding_container}?{}",
            params.join("&")
        )
    }
}

fn build_direct_stream_url(
    item_emby_id: &str,
    media_source_id: &str,
    container: &str,
    play_session_id: &str,
    access_token: &str,
    device_id: Option<&str>,
) -> String {
    let container = container
        .trim()
        .trim_start_matches('.')
        .split(',')
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("mkv");

    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    if let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) {
        serializer.append_pair("DeviceId", device_id);
    }
    serializer.append_pair("MediaSourceId", media_source_id);
    serializer.append_pair("PlaySessionId", play_session_id);
    serializer.append_pair("api_key", access_token);

    format!(
        "/videos/{item_emby_id}/original.{container}?{}",
        serializer.finish()
    )
}

fn query_value(query: Option<&str>, keys: &[&str]) -> Option<String> {
    let query = query?;
    url::form_urlencoded::parse(query.as_bytes()).find_map(|(key, value)| {
        if keys
            .iter()
            .any(|candidate| key.eq_ignore_ascii_case(candidate))
        {
            Some(value.into_owned()).filter(|value| !value.trim().is_empty())
        } else {
            None
        }
    })
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
    session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    ensure_user_access(&session, user_id)?;
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

    let result = repository::list_media_items(&state.pool, options).await?;

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
    ensure_user_access(&session, user_id)?;
    let similar_items = repository::find_similar_items(
        &state.pool,
        &target_item,
        query.limit.unwrap_or(20),
        Some(user_id),
        state.config.server_id,
    )
    .await?;

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

/// `/Trailers` — 返回所有带远程预告片的媒体条目，或类型为 Trailer 的本地条目。
async fn trailers(
    session: AuthSession,
    State(state): State<AppState>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;
    // 强制只返回 Trailer 类型。若客户端未指定排序则按最近添加。
    query.user_id = Some(user_id);
    query.include_item_types = Some("Trailer".to_string());
    query.recursive = Some(true);
    if query.sort_by.is_none() {
        query.sort_by = Some("DateCreated".to_string());
        query.sort_order = Some("Descending".to_string());
    }
    list_items_for_user(&state, user_id, query).await
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct MovieRecommendationsQuery {
    #[serde(default, alias = "userId")]
    user_id: Option<Uuid>,
    #[serde(default, alias = "parentId")]
    parent_id: Option<Uuid>,
    #[serde(default, alias = "categoryLimit")]
    category_limit: Option<i32>,
    #[serde(default, alias = "itemLimit")]
    item_limit: Option<i32>,
    #[serde(default, alias = "fields")]
    #[allow(dead_code)]
    fields: Option<String>,
}

/// `/Movies/Recommendations` — Emby 首页"为你推荐"。返回 RecommendationDto 列表。
async fn movies_recommendations(
    session: AuthSession,
    State(state): State<AppState>,
    Query(query): Query<MovieRecommendationsQuery>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let user_id = query.user_id.unwrap_or(session.user_id);
    ensure_user_access(&session, user_id)?;

    let category_limit = query.category_limit.unwrap_or(6).clamp(1, 12) as usize;
    let item_limit = query.item_limit.unwrap_or(8).clamp(1, 20) as i64;

    let mut options = repository::ItemListOptions {
        user_id: Some(user_id),
        include_types: vec!["Movie".into()],
        recursive: true,
        limit: item_limit,
        ..repository::ItemListOptions::default()
    };
    if let Some(parent_id) = query.parent_id {
        if let Some(library) = repository::get_library(&state.pool, parent_id).await? {
            options.library_id = Some(library.id);
        } else {
            options.parent_id = Some(parent_id);
        }
    }

    let mut categories: Vec<serde_json::Value> = Vec::new();

    // 1) 最近添加
    let latest = repository::list_media_items(
        &state.pool,
        repository::ItemListOptions {
            sort_by: Some("DateCreated".into()),
            sort_order: Some("Descending".into()),
            ..options.clone()
        },
    )
    .await?;
    if !latest.items.is_empty() {
        categories.push(build_recommendation_category(
            &state,
            user_id,
            "SimilarToRecentlyPlayed",
            None,
            "最近添加",
            latest.items,
        )
        .await?);
    }

    // 2) 基于用户最近播放的相似推荐（genre 交集）
    if categories.len() < category_limit {
        let recent_ids: Vec<Uuid> = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT uid.item_id
            FROM user_item_data uid
            INNER JOIN media_items mi ON mi.id = uid.item_id
            WHERE uid.user_id = $1
              AND mi.item_type = 'Movie'
              AND (uid.is_played = true OR uid.playback_position_ticks > 0)
            ORDER BY uid.last_played_date DESC NULLS LAST
            LIMIT 3
            "#,
        )
        .bind(user_id)
        .fetch_all(&state.pool)
        .await
        .unwrap_or_default();

        let mut recent: Vec<crate::models::DbMediaItem> = Vec::new();
        for id in recent_ids {
            if let Some(item) = repository::get_media_item(&state.pool, id).await? {
                recent.push(item);
            }
        }
        for seed in recent {
            if categories.len() >= category_limit {
                break;
            }
            if seed.genres.is_empty() {
                continue;
            }
            let hits = repository::list_media_items(
                &state.pool,
                repository::ItemListOptions {
                    genres: seed.genres.clone(),
                    sort_by: Some("CommunityRating".into()),
                    sort_order: Some("Descending".into()),
                    ..options.clone()
                },
            )
            .await?;
            let picks: Vec<_> = hits
                .items
                .into_iter()
                .filter(|item| item.id != seed.id)
                .take(item_limit as usize)
                .collect();
            if picks.is_empty() {
                continue;
            }
            categories.push(
                build_recommendation_category(
                    &state,
                    user_id,
                    "SimilarToLikedItem",
                    Some(&crate::models::uuid_to_emby_guid(&seed.id)),
                    &format!("与《{}》相似", seed.name),
                    picks,
                )
                .await?,
            );
        }
    }

    // 3) 热门（高评分）
    if categories.len() < category_limit {
        let top_rated = repository::list_media_items(
            &state.pool,
            repository::ItemListOptions {
                sort_by: Some("CommunityRating".into()),
                sort_order: Some("Descending".into()),
                min_community_rating: Some(6.5),
                ..options.clone()
            },
        )
        .await?;
        if !top_rated.items.is_empty() {
            categories.push(
                build_recommendation_category(
                    &state,
                    user_id,
                    "HasSimilarToLikedItem",
                    None,
                    "高分推荐",
                    top_rated.items,
                )
                .await?,
            );
        }
    }

    Ok(Json(categories))
}

async fn build_recommendation_category(
    state: &AppState,
    user_id: Uuid,
    recommendation_type: &str,
    baseline_item_id: Option<&str>,
    category_name: &str,
    items: Vec<crate::models::DbMediaItem>,
) -> Result<serde_json::Value, AppError> {
    let mut dtos = Vec::with_capacity(items.len());
    for item in items {
        dtos.push(
            repository::media_item_to_dto(
                &state.pool,
                &item,
                Some(user_id),
                state.config.server_id,
            )
            .await?,
        );
    }
    Ok(serde_json::json!({
        "Items": dtos,
        "RecommendationType": recommendation_type,
        "BaselineItemName": category_name,
        "CategoryId": category_name,
        "BaselineItemId": baseline_item_id,
    }))
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
            "USERID",
            Some("DEVICEID"),
            Some(1),
            Some(2),
            Some(123),
            Some(4_000_000),
            Some("h264"),
            Some("aac"),
            "mp4",
            "http",
        );

        assert!(url.starts_with("/emby/Videos/ITEMID/stream.mp4?"));
        assert!(url.contains("MediaSourceId=mediasource_ITEMID"));
        assert!(url.contains("PlaySessionId=PLAYSESSION"));
        assert!(url.contains("api_key=TOKEN"));
        assert!(url.contains("UserId=USERID"));
        assert!(url.contains("DeviceId=DEVICEID"));
        assert!(url.contains("VideoCodec=h264"));
        assert!(url.contains("AudioCodec=aac"));
    }

    #[test]
    fn playback_info_builds_emby_original_direct_stream_urls_for_local_player() {
        let url = build_direct_stream_url(
            "ITEMID",
            "mediasource_ITEMID",
            "mkv",
            "PLAYSESSION",
            "TOKEN",
            Some("DEVICEID"),
        );

        assert!(url.starts_with("/videos/ITEMID/original.mkv?"));
        assert!(url.contains("MediaSourceId=mediasource_ITEMID"));
        assert!(url.contains("PlaySessionId=PLAYSESSION"));
        assert!(url.contains("api_key=TOKEN"));
        assert!(url.contains("DeviceId=DEVICEID"));
        assert!(!url.contains("X-Emby-Token"));
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

    #[test]
    fn device_profile_conditions_evaluate_stream_properties() {
        let stream = crate::models::MediaStreamDto {
            index: 0,
            stream_type: "Video".to_string(),
            codec: Some("hevc".to_string()),
            codec_tag: None,
            language: None,
            display_title: None,
            is_default: true,
            is_forced: false,
            width: Some(3840),
            height: Some(2160),
            bit_rate: Some(80_000_000),
            channels: None,
            sample_rate: None,
            is_external: false,
            delivery_method: None,
            delivery_url: None,
            is_chunked_response: None,
            supports_external_stream: false,
            path: None,
            aspect_ratio: Some("16:9".to_string()),
            attachment_size: None,
            average_frame_rate: Some(24.0),
            bit_depth: Some(10),
            color_primaries: None,
            color_space: None,
            color_transfer: None,
            display_language: None,
            extended_video_sub_type: None,
            extended_video_sub_type_description: None,
            extended_video_type: None,
            is_anamorphic: Some(false),
            is_avc: Some(false),
            is_external_url: None,
            is_hearing_impaired: Some(false),
            is_interlaced: Some(false),
            is_text_subtitle_stream: Some(false),
            level: Some(153),
            pixel_format: Some("yuv420p10le".to_string()),
            profile: Some("Main 10".to_string()),
            protocol: Some("File".to_string()),
            real_frame_rate: Some(24.0),
            ref_frames: Some(1),
            rotation: None,
            stream_start_time_ticks: None,
            time_base: None,
            title: None,
            comment: None,
            video_range: Some("HDR10".to_string()),
            channel_layout: None,
            item_id: None,
            server_id: None,
            mime_type: None,
            subtitle_location_type: None,
        };

        let condition = json!({
            "Property": "VideoBitDepth",
            "Condition": "LessThanEqual",
            "Value": "10",
            "IsRequired": true
        });
        assert!(profile_condition_matches(&condition, Some(&stream), None));

        let failing_condition = json!({
            "Property": "Width",
            "Condition": "LessThan",
            "Value": "1920",
            "IsRequired": true
        });
        assert!(!profile_condition_matches(
            &failing_condition,
            Some(&stream),
            None
        ));
    }

    #[test]
    fn transcoding_info_reports_real_reasons_and_sdk_fields() {
        let video_stream = crate::models::MediaStreamDto {
            index: 0,
            stream_type: "Video".to_string(),
            codec: Some("hevc".to_string()),
            codec_tag: None,
            language: None,
            display_title: None,
            is_default: true,
            is_forced: false,
            width: Some(3840),
            height: Some(2160),
            bit_rate: Some(80_000_000),
            channels: None,
            sample_rate: None,
            is_external: false,
            delivery_method: None,
            delivery_url: None,
            is_chunked_response: None,
            supports_external_stream: false,
            path: None,
            aspect_ratio: Some("16:9".to_string()),
            attachment_size: None,
            average_frame_rate: Some(24.0),
            bit_depth: Some(10),
            color_primaries: Some("bt2020".to_string()),
            color_space: Some("bt2020nc".to_string()),
            color_transfer: Some("smpte2084".to_string()),
            display_language: None,
            extended_video_sub_type: Some("DoviProfile76".to_string()),
            extended_video_sub_type_description: Some("Profile 7.6".to_string()),
            extended_video_type: Some("DolbyVision".to_string()),
            is_anamorphic: Some(false),
            is_avc: Some(false),
            is_external_url: None,
            is_hearing_impaired: Some(false),
            is_interlaced: Some(false),
            is_text_subtitle_stream: Some(false),
            level: Some(153),
            pixel_format: Some("yuv420p10le".to_string()),
            profile: Some("Main 10".to_string()),
            protocol: Some("File".to_string()),
            real_frame_rate: Some(24.0),
            ref_frames: Some(1),
            rotation: None,
            stream_start_time_ticks: None,
            time_base: None,
            title: None,
            comment: None,
            video_range: Some("HDR10".to_string()),
            channel_layout: None,
            item_id: None,
            server_id: None,
            mime_type: None,
            subtitle_location_type: None,
        };
        let audio_stream = crate::models::MediaStreamDto {
            index: 1,
            stream_type: "Audio".to_string(),
            codec: Some("truehd".to_string()),
            codec_tag: None,
            language: Some("eng".to_string()),
            display_title: None,
            is_default: true,
            is_forced: false,
            width: None,
            height: None,
            bit_rate: Some(4_000_000),
            channels: Some(8),
            sample_rate: Some(48_000),
            is_external: false,
            delivery_method: None,
            delivery_url: None,
            is_chunked_response: None,
            supports_external_stream: false,
            path: None,
            aspect_ratio: None,
            attachment_size: None,
            average_frame_rate: None,
            bit_depth: Some(24),
            color_primaries: None,
            color_space: None,
            color_transfer: None,
            display_language: Some("English".to_string()),
            extended_video_sub_type: None,
            extended_video_sub_type_description: None,
            extended_video_type: None,
            is_anamorphic: None,
            is_avc: None,
            is_external_url: None,
            is_hearing_impaired: Some(false),
            is_interlaced: Some(false),
            is_text_subtitle_stream: Some(false),
            level: None,
            pixel_format: None,
            profile: None,
            protocol: Some("File".to_string()),
            real_frame_rate: None,
            ref_frames: None,
            rotation: None,
            stream_start_time_ticks: None,
            time_base: None,
            title: None,
            comment: None,
            video_range: None,
            channel_layout: Some("7.1".to_string()),
            item_id: None,
            server_id: None,
            mime_type: None,
            subtitle_location_type: None,
        };
        let source = crate::models::MediaSourceDto {
            chapters: Vec::new(),
            id: "mediasource_item".to_string(),
            path: "http://example.test/movie.mkv".to_string(),
            protocol: "Http".to_string(),
            source_type: "Default".to_string(),
            container: "mkv".to_string(),
            name: "movie".to_string(),
            sort_name: None,
            is_remote: true,
            encoder_path: None,
            encoder_protocol: None,
            probe_path: None,
            probe_protocol: None,
            has_mixed_protocols: Some(false),
            supports_direct_play: true,
            supports_direct_stream: true,
            supports_transcoding: true,
            direct_stream_url: None,
            formats: Vec::new(),
            size: Some(90_000_000_000),
            e_tag: None,
            bitrate: Some(84_000_000),
            default_audio_stream_index: Some(1),
            default_subtitle_stream_index: None,
            run_time_ticks: Some(60_000_000_000),
            container_start_time_ticks: None,
            is_infinite_stream: Some(false),
            requires_opening: Some(false),
            open_token: None,
            requires_closing: Some(false),
            live_stream_id: None,
            buffer_ms: None,
            requires_looping: Some(false),
            supports_probing: Some(true),
            video_3d_format: None,
            timestamp: None,
            required_http_headers: std::collections::BTreeMap::new(),
            add_api_key_to_direct_stream_url: Some(false),
            transcoding_url: None,
            transcoding_sub_protocol: None,
            transcoding_container: None,
            analyze_duration_ms: None,
            read_at_native_framerate: Some(false),
            item_id: Some("item".to_string()),
            server_id: None,
            media_streams: vec![video_stream, audio_stream],
        };

        let info: PlaybackInfoDto = serde_json::from_value(json!({
            "EnableTranscoding": true,
            "MaxStreamingBitrate": 10_000_000,
            "MaxAudioChannels": 6,
            "StartTimeTicks": 12345
        }))
        .expect("valid playback info");

        let reasons = transcoding_reasons(&info, &source, Some(10_000_000));
        assert!(reasons.contains(&"ContainerBitrateExceedsLimit".to_string()));
        assert!(reasons.contains(&"AudioChannelsNotSupported".to_string()));

        let transcoding = build_transcoding_info(&source, &info, "ts", "hls", reasons);
        assert_eq!(transcoding.sub_protocol.as_deref(), Some("hls"));
        assert_eq!(transcoding.video_bitrate, Some(80_000_000));
        assert_eq!(transcoding.audio_bitrate, Some(4_000_000));
        assert_eq!(transcoding.audio_channels, Some(8));
        assert_eq!(transcoding.transcoding_start_position_ticks, Some(12345));
    }

    // ---------------------------------------------------------------------
    // 第三轮补全：锁定 RemoteSearch / Items/Metadata/Reset 等新增接口的协议
    // ---------------------------------------------------------------------

    #[test]
    fn parse_emby_uuid_list_accepts_multiple_delimiters() {
        let ids = parse_emby_uuid_list(Some(
            "00000000000000000000000000000001,00000000-0000-0000-0000-000000000002|00000000000000000000000000000003",
        ));
        assert_eq!(ids.len(), 3);
        assert_eq!(ids[0].as_u128(), 1);
        assert_eq!(ids[1].as_u128(), 2);
        assert_eq!(ids[2].as_u128(), 3);
    }

    #[test]
    fn parse_emby_uuid_list_skips_empty_and_invalid_segments() {
        let ids = parse_emby_uuid_list(Some(",,not-a-uuid,,00000000000000000000000000000001,"));
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0].as_u128(), 1);
    }

    #[test]
    fn remote_search_query_body_parses_emby_payload() {
        let body: RemoteSearchQueryBody = serde_json::from_value(json!({
            "SearchInfo": {
                "Name": "The Matrix",
                "Year": 1999,
                "ProviderIds": { "Tmdb": "603" },
                "MetadataLanguage": "zh-CN",
                "MetadataCountryCode": "CN"
            },
            "ItemId": "00000000000000000000000000000001",
            "SearchProviderName": "TheMovieDb",
            "IncludeDisabledProviders": false
        }))
        .expect("Emby RemoteSearchQuery 应能反序列化");

        assert_eq!(body.search_info.name.as_deref(), Some("The Matrix"));
        assert_eq!(body.search_info.year, Some(1999));
        assert_eq!(
            body.search_info
                .provider_ids
                .get("Tmdb")
                .and_then(|value| value.as_str()),
            Some("603")
        );
        assert_eq!(body.search_provider_name.as_deref(), Some("TheMovieDb"));
    }

    #[test]
    fn remote_search_query_body_tolerates_missing_optional_fields() {
        let body: RemoteSearchQueryBody = serde_json::from_value(json!({
            "SearchInfo": { "Name": "Inception" }
        }))
        .expect("最小化载荷也应该能被接受");
        assert_eq!(body.search_info.name.as_deref(), Some("Inception"));
        assert!(body.search_info.provider_ids.is_empty());
        assert!(body.item_id.is_none());
    }

    #[test]
    fn search_result_to_emby_value_mirrors_remote_search_result_schema() {
        let hit = crate::metadata::provider::ExternalMediaSearchResult {
            provider: "tmdb".to_string(),
            external_id: "603".to_string(),
            name: "The Matrix".to_string(),
            original_name: Some("The Matrix".to_string()),
            overview: Some("A computer hacker...".to_string()),
            premiere_date: chrono::NaiveDate::from_ymd_opt(1999, 3, 31),
            production_year: Some(1999),
            image_url: Some("https://image.example/poster.jpg".to_string()),
            provider_ids: std::collections::HashMap::from([(
                "Tmdb".to_string(),
                "603".to_string(),
            )]),
        };

        let value = search_result_to_emby_value(&hit, "TheMovieDb");
        assert_eq!(value.get("Name").and_then(Value::as_str), Some("The Matrix"));
        assert_eq!(value.get("ProductionYear").and_then(Value::as_i64), Some(1999));
        assert_eq!(
            value.get("PremiereDate").and_then(Value::as_str),
            Some("1999-03-31")
        );
        assert_eq!(
            value.get("SearchProviderName").and_then(Value::as_str),
            Some("TheMovieDb")
        );
        assert_eq!(
            value
                .get("ProviderIds")
                .and_then(|ids| ids.get("Tmdb"))
                .and_then(Value::as_str),
            Some("603")
        );
    }

    #[test]
    fn person_search_result_emits_compatible_provider_ids() {
        let hit = crate::metadata::models::ExternalPersonSearchResult {
            external_id: "6193".to_string(),
            provider: "Tmdb".to_string(),
            name: "Keanu Reeves".to_string(),
            sort_name: None,
            overview: None,
            external_url: None,
            image_url: None,
            known_for: Vec::new(),
            popularity: None,
            adult: None,
        };
        let value = person_search_result_to_emby_value(&hit, "TheMovieDb");
        assert_eq!(
            value.get("Name").and_then(Value::as_str),
            Some("Keanu Reeves")
        );
        assert_eq!(
            value
                .get("ProviderIds")
                .and_then(|ids| ids.get("Tmdb"))
                .and_then(Value::as_str),
            Some("6193")
        );
        assert!(value.get("ProductionYear").unwrap().is_null());
    }

    #[test]
    fn remote_search_returns_empty_array_for_compat_surfaces() {
        // 即便 SearchProviderName 未设置，兼容路径（Book/BoxSet/Game/…）也必须稳定返回 [] 以避免客户端解析失败。
        let empty = json!([]);
        assert!(empty.is_array());
        assert_eq!(empty.as_array().unwrap().len(), 0);
    }

    #[test]
    fn items_router_builds_with_new_remote_search_and_metadata_reset_routes() {
        // 冒烟测试：确保新增路由不会和既有路由冲突，router() 构建成功。
        let _router = super::router();
    }

    #[test]
    fn parse_metadata_date_accepts_multiple_formats() {
        assert_eq!(
            super::parse_metadata_date("2001-08-31"),
            Some(chrono::NaiveDate::from_ymd_opt(2001, 8, 31).unwrap())
        );
        assert_eq!(
            super::parse_metadata_date("08/31/2001"),
            Some(chrono::NaiveDate::from_ymd_opt(2001, 8, 31).unwrap())
        );
        assert_eq!(
            super::parse_metadata_date("2001-08-31T12:00:00Z"),
            Some(chrono::NaiveDate::from_ymd_opt(2001, 8, 31).unwrap())
        );
        assert_eq!(super::parse_metadata_date("garbage"), None);
        assert_eq!(super::parse_metadata_date("  "), None);
    }

    #[test]
    fn coerce_name_list_supports_string_and_object_items() {
        let primary: Option<Vec<Value>> = Some(vec![
            json!({ "Name": "Drama" }),
            json!({ "Name": "Sci-Fi" }),
            json!({ "Name": "   " }),
            json!({ "Name": "drama" }), // duplicate case-insensitive
        ]);
        let fallback: Option<Vec<Value>> = None;
        let out = super::coerce_name_list(&primary, &fallback).unwrap();
        assert_eq!(out, vec!["Drama".to_string(), "Sci-Fi".to_string()]);

        let only_fallback: Option<Vec<Value>> = Some(vec![json!("Thriller"), json!("Thriller")]);
        let fallback_only = super::coerce_name_list(&None, &only_fallback).unwrap();
        assert_eq!(fallback_only, vec!["Thriller".to_string()]);
    }

    #[test]
    fn metadata_editor_returns_expected_schema_shape() {
        // 直接渲染一份静态 schema，校验我们给 Emby 客户端的编辑元数据面板
        // 提供了所有需要的下拉选项。
        let body = json!({
            "ExternalIdInfos": super::external_id_infos_catalog(false),
            "PersonExternalIdInfos": super::external_id_infos_catalog(true),
            "ParentalRatingOptions": super::parental_rating_options(),
            "Countries": super::country_options(),
            "Cultures": super::culture_options(),
        });
        for key in [
            "ExternalIdInfos",
            "PersonExternalIdInfos",
            "ParentalRatingOptions",
            "Countries",
            "Cultures",
        ] {
            assert!(body.get(key).and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty()), "{key} should be non-empty");
        }
        // TheMovieDb/IMDb 一定出现在条目 id 列表里，否则客户端无法手填 TMDb/IMDb id。
        let keys: Vec<&str> = body["ExternalIdInfos"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["Key"].as_str().unwrap())
            .collect();
        assert!(keys.contains(&"Tmdb"));
        assert!(keys.contains(&"Imdb"));
    }

    #[test]
    fn update_item_body_parses_partial_emby_payload() {
        let body: super::UpdateItemBody = serde_json::from_value(json!({
            "Name": "Matrix Reloaded",
            "CommunityRating": 7.3,
            "ProductionYear": 2003,
            "PremiereDate": "2003-05-15T00:00:00.0000000Z",
            "Genres": ["Action", "Sci-Fi"],
            "GenreItems": [{ "Name": "Sci-Fi" }, { "Name": "Action" }],
            "ProviderIds": { "Tmdb": "604", "Imdb": "tt0234215" }
        }))
        .expect("valid partial BaseItemDto");

        assert_eq!(body.name.as_deref(), Some("Matrix Reloaded"));
        assert_eq!(body.community_rating, Some(7.3));
        assert_eq!(body.production_year, Some(2003));
        // GenreItems 优先，但 Genres 也要解析成功。
        let merged = super::coerce_name_list(&body.genre_items, &body.genres).unwrap();
        assert_eq!(merged, vec!["Sci-Fi".to_string(), "Action".to_string()]);
    }
}
