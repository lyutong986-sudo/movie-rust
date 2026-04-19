use crate::{
    config::Config,
    error::AppError,
    models::{
        AuthSessionRow, BaseItemDto, DbLibrary, DbMediaItem, DbUser, DbUserItemData, MediaItemRow,
        MediaSourceDto, MediaStreamDto, QueryResult, SessionInfoDto, UserConfigurationDto, UserDto,
        UserItemDataDto, UserPolicyDto,
    },
    security,
};
use chrono::Utc;
use sqlx::{Postgres, QueryBuilder};
use std::{collections::BTreeMap, path::Path};
use uuid::Uuid;

pub async fn ensure_default_admin(pool: &sqlx::PgPool, config: &Config) -> Result<(), AppError> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;

    if count == 0 {
        let password_hash = security::hash_password(&config.default_password)?;
        sqlx::query(
            r#"
            INSERT INTO users (id, name, password_hash, is_admin)
            VALUES ($1, $2, $3, true)
            "#,
        )
        .bind(Uuid::new_v4())
        .bind(&config.default_admin)
        .bind(password_hash)
        .execute(pool)
        .await?;

        tracing::warn!(
            "已创建默认管理员用户 '{}'，请首次登录后修改默认密码",
            config.default_admin
        );
    }

    Ok(())
}

pub async fn get_user_by_name(pool: &sqlx::PgPool, name: &str) -> Result<Option<DbUser>, AppError> {
    Ok(sqlx::query_as::<_, DbUser>(
        r#"
        SELECT id, name, password_hash, is_admin, is_hidden, is_disabled
        FROM users
        WHERE lower(name) = lower($1)
        "#,
    )
    .bind(name)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_user_by_id(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<DbUser>, AppError> {
    Ok(sqlx::query_as::<_, DbUser>(
        r#"
        SELECT id, name, password_hash, is_admin, is_hidden, is_disabled
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_users(pool: &sqlx::PgPool, public_only: bool) -> Result<Vec<DbUser>, AppError> {
    let users = if public_only {
        sqlx::query_as::<_, DbUser>(
            r#"
            SELECT id, name, password_hash, is_admin, is_hidden, is_disabled
            FROM users
            WHERE is_hidden = false AND is_disabled = false
            ORDER BY name
            "#,
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, DbUser>(
            r#"
            SELECT id, name, password_hash, is_admin, is_hidden, is_disabled
            FROM users
            ORDER BY name
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    Ok(users)
}

pub async fn create_session(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    device_id: Option<String>,
    device_name: Option<String>,
    client: Option<String>,
    application_version: Option<String>,
) -> Result<AuthSessionRow, AppError> {
    let token = Uuid::new_v4().simple().to_string();

    sqlx::query(
        r#"
        INSERT INTO sessions (access_token, user_id, device_id, device_name, client, application_version)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(&token)
    .bind(user_id)
    .bind(device_id)
    .bind(device_name)
    .bind(client)
    .bind(application_version)
    .execute(pool)
    .await?;

    get_session(pool, &token)
        .await?
        .ok_or_else(|| AppError::Internal("创建会话后无法读取会话".to_string()))
}

pub async fn get_session(
    pool: &sqlx::PgPool,
    token: &str,
) -> Result<Option<AuthSessionRow>, AppError> {
    let session = sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.access_token = $1 AND u.is_disabled = false
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    if session.is_some() {
        sqlx::query("UPDATE sessions SET last_activity_at = now() WHERE access_token = $1")
            .bind(token)
            .execute(pool)
            .await?;
    }

    Ok(session)
}

pub async fn list_sessions(pool: &sqlx::PgPool) -> Result<Vec<AuthSessionRow>, AppError> {
    Ok(sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        ORDER BY s.last_activity_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn create_library(
    pool: &sqlx::PgPool,
    name: &str,
    collection_type: &str,
    path: &str,
) -> Result<DbLibrary, AppError> {
    let id = Uuid::new_v4();

    sqlx::query(
        r#"
        INSERT INTO libraries (id, name, collection_type, path)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(collection_type)
    .bind(path)
    .execute(pool)
    .await?;

    get_library(pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("创建媒体库后无法读取媒体库".to_string()))
}

pub async fn list_libraries(pool: &sqlx::PgPool) -> Result<Vec<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, created_at
        FROM libraries
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_library(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, created_at
        FROM libraries
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn count_library_children(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<i64, AppError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE library_id = $1")
            .bind(library_id)
            .fetch_one(pool)
            .await?,
    )
}

pub struct ItemListOptions {
    pub library_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub include_types: Vec<String>,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub start_index: i64,
    pub limit: i64,
}

pub async fn list_media_items(
    pool: &sqlx::PgPool,
    options: ItemListOptions,
) -> Result<QueryResult<DbMediaItem>, AppError> {
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            id, parent_id, name, item_type, media_type, path, container,
            overview, production_year, runtime_ticks, premiere_date, image_primary_path,
            backdrop_path, date_created, date_modified, COUNT(*) OVER() AS total_count
        FROM media_items
        WHERE 1 = 1
        "#,
    );

    if let Some(library_id) = options.library_id {
        builder.push(" AND library_id = ").push_bind(library_id);
    }

    if let Some(parent_id) = options.parent_id {
        builder.push(" AND parent_id = ").push_bind(parent_id);
    } else if !options.recursive && options.library_id.is_none() {
        builder.push(" AND parent_id IS NULL");
    }

    if !options.include_types.is_empty() {
        builder
            .push(" AND item_type = ANY(")
            .push_bind(options.include_types)
            .push(")");
    }

    if let Some(search_term) = options.search_term.filter(|value| !value.trim().is_empty()) {
        builder
            .push(" AND name ILIKE ")
            .push_bind(format!("%{}%", search_term.trim()));
    }

    let sort_column = match options.sort_by.as_deref().unwrap_or("SortName") {
        "DateCreated" | "DateLastContentAdded" => "date_created",
        "ProductionYear" => "production_year",
        "Random" => "random()",
        _ => "sort_name",
    };
    let sort_order = if options
        .sort_order
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("Descending"))
    {
        "DESC"
    } else {
        "ASC"
    };

    builder
        .push(" ORDER BY ")
        .push(sort_column)
        .push(" ")
        .push(sort_order)
        .push(" NULLS LAST")
        .push(" OFFSET ")
        .push_bind(options.start_index.max(0))
        .push(" LIMIT ")
        .push_bind(options.limit.clamp(1, 500));

    let rows = builder
        .build_query_as::<MediaItemRow>()
        .fetch_all(pool)
        .await?;
    let total_record_count = rows.first().map(|row| row.total_count).unwrap_or(0);
    let items = rows.into_iter().map(DbMediaItem::from).collect();

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(options.start_index.max(0)),
    })
}

pub async fn get_media_item(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<DbMediaItem>, AppError> {
    Ok(sqlx::query_as::<_, DbMediaItem>(
        r#"
        SELECT
            id, parent_id, name, item_type, media_type, path, container,
            overview, production_year, runtime_ticks, premiere_date, image_primary_path,
            backdrop_path, date_created, date_modified
        FROM media_items
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_user_item_data(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<Option<DbUserItemData>, AppError> {
    Ok(sqlx::query_as::<_, DbUserItemData>(
        r#"
        SELECT playback_position_ticks, play_count, is_favorite, is_played, last_played_date
        FROM user_item_data
        WHERE user_id = $1 AND item_id = $2
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn record_playback_event(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_id: Option<Uuid>,
    session_id: Option<&str>,
    event_type: &str,
    position_ticks: Option<i64>,
    is_paused: Option<bool>,
    played_to_completion: Option<bool>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO playback_events
            (id, user_id, item_id, session_id, event_type, position_ticks, is_paused, played_to_completion)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(user_id)
    .bind(item_id)
    .bind(session_id)
    .bind(event_type)
    .bind(position_ticks)
    .bind(is_paused)
    .bind(played_to_completion)
    .execute(pool)
    .await?;

    if let Some(item_id) = item_id {
        if matches!(event_type, "Progress" | "Stopped") {
            let is_played = played_to_completion.unwrap_or(false);
            sqlx::query(
                r#"
                INSERT INTO user_item_data
                    (user_id, item_id, playback_position_ticks, is_played, play_count, last_played_date, updated_at)
                VALUES ($1, $2, COALESCE($3, 0), $4, CASE WHEN $4 THEN 1 ELSE 0 END, CASE WHEN $4 THEN now() ELSE NULL END, now())
                ON CONFLICT (user_id, item_id)
                DO UPDATE SET
                    playback_position_ticks = COALESCE(EXCLUDED.playback_position_ticks, user_item_data.playback_position_ticks),
                    is_played = CASE WHEN EXCLUDED.is_played THEN true ELSE user_item_data.is_played END,
                    play_count = CASE WHEN EXCLUDED.is_played THEN user_item_data.play_count + 1 ELSE user_item_data.play_count END,
                    last_played_date = CASE WHEN EXCLUDED.is_played THEN now() ELSE user_item_data.last_played_date END,
                    updated_at = now()
                "#,
            )
            .bind(user_id)
            .bind(item_id)
            .bind(position_ticks)
            .bind(is_played)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

pub async fn upsert_media_item(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    name: &str,
    path: &Path,
    container: Option<&str>,
    image_primary_path: Option<&Path>,
) -> Result<(), AppError> {
    let path_text = path.to_string_lossy().to_string();
    let image_text = image_primary_path.map(|value| value.to_string_lossy().to_string());
    let id = Uuid::new_v5(&library_id, path_text.as_bytes());
    let sort_name = name.to_lowercase();

    sqlx::query(
        r#"
        INSERT INTO media_items
            (id, library_id, name, sort_name, item_type, media_type, path, container, image_primary_path, date_modified)
        VALUES ($1, $2, $3, $4, 'Movie', 'Video', $5, $6, $7, now())
        ON CONFLICT (library_id, path)
        DO UPDATE SET
            name = EXCLUDED.name,
            sort_name = EXCLUDED.sort_name,
            container = EXCLUDED.container,
            image_primary_path = EXCLUDED.image_primary_path,
            date_modified = now()
        "#,
    )
    .bind(id)
    .bind(library_id)
    .bind(name)
    .bind(sort_name)
    .bind(path_text)
    .bind(container)
    .bind(image_text)
    .execute(pool)
    .await?;

    Ok(())
}

pub fn user_to_dto(user: &DbUser, server_id: Uuid) -> UserDto {
    UserDto {
        name: user.name.clone(),
        server_id: server_id.to_string(),
        id: user.id.to_string(),
        has_password: true,
        has_configured_password: true,
        has_configured_easy_password: false,
        policy: UserPolicyDto {
            is_administrator: user.is_admin,
            is_hidden: user.is_hidden,
            is_disabled: user.is_disabled,
            enable_remote_access: true,
            enable_media_playback: true,
            enable_content_deletion: user.is_admin,
            enable_downloads: true,
        },
        configuration: UserConfigurationDto {
            play_default_audio_track: true,
            subtitle_mode: "Default".to_string(),
        },
    }
}

pub fn session_to_dto(session: &AuthSessionRow) -> SessionInfoDto {
    SessionInfoDto {
        id: session.access_token.clone(),
        user_id: session.user_id.to_string(),
        user_name: session.user_name.clone(),
        client: session
            .client
            .clone()
            .unwrap_or_else(|| "Movie Rust Client".to_string()),
        device_id: session
            .device_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
        device_name: session
            .device_name
            .clone()
            .unwrap_or_else(|| "Unknown Device".to_string()),
        application_version: session
            .application_version
            .clone()
            .unwrap_or_else(|| "0.1.0".to_string()),
        is_active: true,
        last_activity_date: session.last_activity_at,
    }
}

pub async fn library_to_item_dto(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
    server_id: Uuid,
) -> Result<BaseItemDto, AppError> {
    let child_count = count_library_children(pool, library.id).await?;

    Ok(BaseItemDto {
        name: library.name.clone(),
        server_id: server_id.to_string(),
        id: library.id.to_string(),
        item_type: "CollectionFolder".to_string(),
        is_folder: true,
        collection_type: Some(library.collection_type.clone()),
        media_type: None,
        parent_id: None,
        path: Some(library.path.clone()),
        run_time_ticks: None,
        production_year: None,
        overview: None,
        date_created: Some(library.created_at),
        premiere_date: None,
        image_tags: BTreeMap::new(),
        backdrop_image_tags: Vec::new(),
        user_data: empty_user_data(),
        media_sources: Vec::new(),
        child_count: Some(child_count),
        primary_image_aspect_ratio: None,
    })
}

pub fn root_item_dto(server_id: Uuid) -> BaseItemDto {
    BaseItemDto {
        name: "Root".to_string(),
        server_id: server_id.to_string(),
        id: Uuid::nil().to_string(),
        item_type: "Folder".to_string(),
        is_folder: true,
        collection_type: None,
        media_type: None,
        parent_id: None,
        path: None,
        run_time_ticks: None,
        production_year: None,
        overview: None,
        date_created: Some(Utc::now()),
        premiere_date: None,
        image_tags: BTreeMap::new(),
        backdrop_image_tags: Vec::new(),
        user_data: empty_user_data(),
        media_sources: Vec::new(),
        child_count: None,
        primary_image_aspect_ratio: None,
    }
}

pub async fn media_item_to_dto(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    user_id: Option<Uuid>,
    server_id: Uuid,
) -> Result<BaseItemDto, AppError> {
    let mut image_tags = BTreeMap::new();
    if item.image_primary_path.is_some() {
        image_tags.insert(
            "Primary".to_string(),
            item.date_modified.timestamp().to_string(),
        );
    }

    let backdrop_image_tags = if item.backdrop_path.is_some() {
        vec![item.date_modified.timestamp().to_string()]
    } else {
        Vec::new()
    };

    let user_data = if let Some(user_id) = user_id {
        get_user_item_data(pool, user_id, item.id)
            .await?
            .map(user_item_data_to_dto)
            .unwrap_or_else(empty_user_data)
    } else {
        empty_user_data()
    };

    let media_sources = if item.media_type.eq_ignore_ascii_case("Video") {
        vec![media_source_for_item(item)]
    } else {
        Vec::new()
    };

    Ok(BaseItemDto {
        name: item.name.clone(),
        server_id: server_id.to_string(),
        id: item.id.to_string(),
        item_type: item.item_type.clone(),
        is_folder: false,
        collection_type: None,
        media_type: Some(item.media_type.clone()),
        parent_id: item.parent_id.map(|value| value.to_string()),
        path: Some(item.path.clone()),
        run_time_ticks: item.runtime_ticks,
        production_year: item.production_year,
        overview: item.overview.clone(),
        date_created: Some(item.date_created),
        premiere_date: item.premiere_date,
        image_tags,
        backdrop_image_tags,
        user_data,
        media_sources,
        child_count: None,
        primary_image_aspect_ratio: item.image_primary_path.as_ref().map(|_| 0.666_666_666_7),
    })
}

pub fn media_source_for_item(item: &DbMediaItem) -> MediaSourceDto {
    let container = item
        .container
        .clone()
        .or_else(|| {
            Path::new(&item.path)
                .extension()
                .map(|ext| ext.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "mp4".to_string());

    MediaSourceDto {
        id: item.id.to_string(),
        path: item.path.clone(),
        protocol: "File".to_string(),
        source_type: "Default".to_string(),
        container: container.clone(),
        name: item.name.clone(),
        is_remote: false,
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: false,
        direct_stream_url: format!(
            "/Videos/{}/stream?static=true&mediaSourceId={}",
            item.id, item.id
        ),
        run_time_ticks: item.runtime_ticks,
        media_streams: vec![MediaStreamDto {
            index: 0,
            stream_type: "Video".to_string(),
            codec: None,
            language: None,
            is_default: true,
        }],
    }
}

fn user_item_data_to_dto(data: DbUserItemData) -> UserItemDataDto {
    UserItemDataDto {
        playback_position_ticks: data.playback_position_ticks,
        play_count: data.play_count,
        is_favorite: data.is_favorite,
        played: data.is_played,
        last_played_date: data.last_played_date,
    }
}

fn empty_user_data() -> UserItemDataDto {
    UserItemDataDto {
        playback_position_ticks: 0,
        play_count: 0,
        is_favorite: false,
        played: false,
        last_played_date: None,
    }
}
