use crate::{
    auth::AuthSession,
    error::AppError,
    media_analyzer,
    models::{
        emby_id_to_uuid, BaseItemDto, GetSimilarItems, ItemsQuery, PlaybackInfoDto, PlaybackInfoResponse, QueryResult, UpdateUserItemDataRequest,
        UserItemDataDto, UserItemDataQuery,
    },
    naming,
    repository::{self, ItemListOptions, UpdateUserDataInput},
    state::AppState,
};
use axum::{
    body::to_bytes,
    extract::{Path, Query, Request, State},
    http,
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Users/{user_id}/Views", get(user_views))
        .route("/Library/MediaFolders", get(media_folders))
        .route("/Items/Root", get(root_item))
        .route("/Users/{user_id}/Items/Root", get(root_item_for_user))
        .route("/Items", get(items))
        .route("/Users/{user_id}/Items", get(user_items))
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
            "/UserPlayedItems/{item_id}",
            post(mark_played).delete(mark_unplayed),
        )
        .route(
            "/Users/{user_id}/PlayedItems/{item_id}",
            post(legacy_mark_played).delete(legacy_mark_unplayed),
        )
        .route("/Items/{item_id}", get(item_by_id))
        .route("/Users/{user_id}/Items/{item_id}", get(user_item_by_id))
        .route("/Items/{item_id}/Similar", get(get_similar_items))
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
    let recursive = query.recursive.unwrap_or_else(|| {
        query
            .search_term
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
    });

    if let Some(parent_id) = query.parent_id {
        if let Some(library) = repository::get_library(&state.pool, parent_id).await? {
            // 对于电视剧库，如果没有指定包含的类型，默认只显示Series
            let mut include_types = parse_include_types(query.include_item_types.as_deref());
            if include_types.is_empty() && library.collection_type.eq_ignore_ascii_case("tvshows") {
                include_types = vec!["Series".to_string()];
            }
            
            let result = repository::list_media_items(
                &state.pool,
                ItemListOptions {
                    library_id: Some(library.id),
                    parent_id: None,
                    include_types,
                    genres: parse_list(query.genres.as_deref()),
                    recursive,
                    search_term: query.search_term,
                    sort_by: query.sort_by,
                    sort_order: query.sort_order,
                    filters: query.filters,
                    fields: query.fields,
                    start_index: query.start_index.unwrap_or(0),
                    limit: query.limit.unwrap_or(100),
                },
            )
            .await?;
            return media_items_to_dto_result(state, user_id, result).await;
        }
    }

    let result = repository::list_media_items(
        &state.pool,
        ItemListOptions {
            library_id: None,
            parent_id: query.parent_id,
            include_types: parse_include_types(query.include_item_types.as_deref()),
            genres: parse_list(query.genres.as_deref()),
            recursive,
            search_term: query.search_term,
            sort_by: query.sort_by,
            sort_order: query.sort_order,
            filters: query.filters,
            fields: query.fields,
            start_index: query.start_index.unwrap_or(0),
            limit: query.limit.unwrap_or(100),
        },
    )
    .await?;

    media_items_to_dto_result(state, user_id, result).await
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
    repository::get_media_item(&state.pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    Ok(())
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

async fn item_dto(
    state: &AppState,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<Json<BaseItemDto>, AppError> {
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

async fn playback_info(
    _session: AuthSession,
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

    let play_session_id = Uuid::new_v4().simple().to_string();

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

    for media_source in &mut media_sources {
        if let Some(url) = media_source.direct_stream_url.as_mut() {
            url.push_str("&PlaySessionId=");
            url.push_str(&play_session_id);
        }
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
    
    let response_media_source_id = info
        .media_source_id
        .clone()
        .or_else(|| {
            media_sources
                .get(selected_media_source_index)
                .map(|source| source.id.clone())
        });
    let direct_play_protocols = info
        .device_profile
        .as_ref()
        .map(|profile| profile.direct_play_protocols.clone())
        .filter(|protocols| !protocols.is_empty());

    Ok(Json(PlaybackInfoResponse {
        media_sources,
        play_session_id,
        media_source_id: response_media_source_id,
        direct_play_protocols,
        ..Default::default()
    }))
}

fn parse_include_types(value: Option<&str>) -> Vec<String> {
    parse_list(value)
}

fn parse_list(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

async fn user_resume_items(
    _session: AuthSession,
    State(state): State<AppState>,
    Path(user_id): Path<Uuid>,
    Query(mut query): Query<ItemsQuery>,
) -> Result<Json<QueryResult<BaseItemDto>>, AppError> {
    query.user_id = Some(user_id);
    
    // 使用repository中的get_user_resume_items函数
    let limit = query.limit.unwrap_or(50);
    let start_index = query.start_index.unwrap_or(0);
    
    let (items, total_count) = repository::get_user_resume_items(
        &state.pool,
        user_id,
        Some(limit),
        Some(start_index),
        state.config.server_id,
    ).await?;
    
    Ok(Json(QueryResult {
        items,
        total_record_count: total_count,
        start_index: Some(start_index),
    }))
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
    let similar_items = repository::find_similar_items(
        &state.pool,
        &target_item,
        query.limit.unwrap_or(20),
        state.config.server_id,
    ).await?;
    
    let total_record_count = similar_items.len() as i64;
    Ok(Json(QueryResult {
        items: similar_items,
        total_record_count,
        start_index: Some(0),
    }))
}
