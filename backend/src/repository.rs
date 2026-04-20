use crate::{
    config::Config,
    error::AppError,
    models::{
        ActivityLogEntryDto, AuthSessionRow, BaseItemDto, DbLibrary, DbMediaItem, DbUser,
        DbUserItemData, LibraryOptionsDto, LogFileDto, MediaItemRow, MediaPathInfoDto,
        MediaSourceDto, MediaStreamDto, QueryResult, SessionInfoDto, StartupConfiguration,
        StartupRemoteAccessRequest, UserConfigurationDto, UserDto, UserItemDataDto, UserPolicyDto,
        VirtualFolderInfoDto,
    },
    naming, security,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::{json, Value};
use sqlx::{FromRow, Postgres, QueryBuilder};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};
use url::form_urlencoded;
use uuid::Uuid;

#[derive(Debug, FromRow)]
struct ActivityLogRow {
    id: Uuid,
    event_type: String,
    position_ticks: Option<i64>,
    is_paused: Option<bool>,
    played_to_completion: Option<bool>,
    created_at: DateTime<Utc>,
    user_name: String,
    item_name: Option<String>,
}

pub async fn user_count(pool: &sqlx::PgPool) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?)
}

pub async fn startup_wizard_completed(pool: &sqlx::PgPool) -> Result<bool, AppError> {
    if let Some(value) = get_system_setting(pool, "startup_wizard_completed").await? {
        return Ok(value.as_bool().unwrap_or(false));
    }

    Ok(user_count(pool).await? > 0)
}

pub async fn set_startup_wizard_completed(
    pool: &sqlx::PgPool,
    completed: bool,
) -> Result<(), AppError> {
    set_system_setting(pool, "startup_wizard_completed", json!(completed)).await
}

pub async fn complete_startup_wizard(pool: &sqlx::PgPool) -> Result<(), AppError> {
    if user_count(pool).await? == 0 {
        return Err(AppError::BadRequest("请先创建管理员账户".to_string()));
    }

    set_startup_wizard_completed(pool, true).await
}

pub async fn startup_configuration(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<StartupConfiguration, AppError> {
    if let Some(value) = get_system_setting(pool, "startup_configuration").await? {
        if let Ok(configuration) = serde_json::from_value::<StartupConfiguration>(value) {
            return Ok(configuration);
        }
    }

    Ok(default_startup_configuration(config))
}

pub async fn update_startup_configuration(
    pool: &sqlx::PgPool,
    configuration: &StartupConfiguration,
) -> Result<(), AppError> {
    set_system_setting(pool, "startup_configuration", json!(configuration)).await
}

pub async fn update_remote_access(
    pool: &sqlx::PgPool,
    configuration: &StartupRemoteAccessRequest,
) -> Result<(), AppError> {
    set_system_setting(pool, "startup_remote_access", json!(configuration)).await
}

pub async fn create_initial_admin(
    pool: &sqlx::PgPool,
    name: &str,
    password: &str,
) -> Result<DbUser, AppError> {
    if user_count(pool).await? > 0 {
        return Err(AppError::Forbidden);
    }

    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("管理员名称不能为空".to_string()));
    }
    if password.trim().is_empty() {
        return Err(AppError::BadRequest("管理员密码不能为空".to_string()));
    }

    let id = Uuid::new_v4();
    let password_hash = security::hash_password(password)?;

    sqlx::query(
        r#"
        INSERT INTO users (id, name, password_hash, is_admin)
        VALUES ($1, $2, $3, true)
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(password_hash)
    .execute(pool)
    .await?;

    get_user_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("创建管理员后无法读取用户".to_string()))
}

fn default_startup_configuration(config: &Config) -> StartupConfiguration {
    StartupConfiguration {
        server_name: config.server_name.clone(),
        ui_culture: "zh-CN".to_string(),
        metadata_country_code: "CN".to_string(),
        preferred_metadata_language: "zh".to_string(),
    }
}

async fn get_system_setting(pool: &sqlx::PgPool, key: &str) -> Result<Option<Value>, AppError> {
    Ok(
        sqlx::query_scalar::<_, Value>("SELECT value FROM system_settings WHERE key = $1")
            .bind(key)
            .fetch_optional(pool)
            .await?,
    )
}

async fn set_system_setting(pool: &sqlx::PgPool, key: &str, value: Value) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO system_settings (key, value, updated_at)
        VALUES ($1, $2, now())
        ON CONFLICT (key) DO UPDATE
        SET value = EXCLUDED.value,
            updated_at = now()
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;

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

pub async fn change_user_password(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    new_password: &str,
) -> Result<(), AppError> {
    let password = new_password.trim();
    if password.len() < 4 {
        return Err(AppError::BadRequest("新密码至少需要 4 个字符".to_string()));
    }

    let password_hash = security::hash_password(password)?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2")
        .bind(password_hash)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
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

pub async fn list_server_logs(_pool: &sqlx::PgPool) -> Result<Vec<LogFileDto>, AppError> {
    Ok(Vec::new())
}

pub async fn list_activity_logs(
    pool: &sqlx::PgPool,
    limit: i64,
) -> Result<Vec<ActivityLogEntryDto>, AppError> {
    let rows = sqlx::query_as::<_, ActivityLogRow>(
        r#"
        SELECT
            e.id,
            e.event_type,
            e.position_ticks,
            e.is_paused,
            e.played_to_completion,
            e.created_at,
            u.name AS user_name,
            m.name AS item_name
        FROM playback_events e
        INNER JOIN users u ON u.id = e.user_id
        LEFT JOIN media_items m ON m.id = e.item_id
        ORDER BY e.created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit.clamp(1, 200))
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let entry_type = row.event_type.clone();
            let item_name = row.item_name.unwrap_or_else(|| "未知媒体".to_string());
            let short_overview = Some(format_activity_overview(
                &row.user_name,
                &item_name,
                &row.event_type,
                row.position_ticks,
                row.is_paused,
                row.played_to_completion,
            ));

            ActivityLogEntryDto {
                id: row.id.to_string(),
                name: format!("{} · {}", row.user_name, item_name),
                entry_type,
                short_overview,
                severity: "Info".to_string(),
                date: row.created_at,
            }
        })
        .collect())
}

pub async fn create_library(
    pool: &sqlx::PgPool,
    name: &str,
    collection_type: &str,
    paths: &[String],
    options: LibraryOptionsDto,
) -> Result<DbLibrary, AppError> {
    let name = name.trim();
    let collection_type = normalize_collection_type(collection_type);
    let paths = normalize_library_paths(paths);

    if name.is_empty() {
        return Err(AppError::BadRequest("媒体库名称不能为空".to_string()));
    }
    if paths.is_empty() {
        return Err(AppError::BadRequest("至少需要添加一个媒体路径".to_string()));
    }

    let id = Uuid::new_v4();
    let options = normalize_library_options(options, &paths);
    let options_value = json!(options);

    sqlx::query(
        r#"
        INSERT INTO libraries (id, name, collection_type, path, library_options, date_modified)
        VALUES ($1, $2, $3, $4, $5, now())
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(collection_type.as_str())
    .bind(&paths[0])
    .bind(options_value)
    .execute(pool)
    .await?;

    get_library(pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("创建媒体库后无法读取媒体库".to_string()))
}

pub async fn list_libraries(pool: &sqlx::PgPool) -> Result<Vec<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at
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
        SELECT id, name, collection_type, path, library_options, created_at
        FROM libraries
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_library_by_name(
    pool: &sqlx::PgPool,
    name: &str,
) -> Result<Option<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at
        FROM libraries
        WHERE lower(name) = lower($1)
        "#,
    )
    .bind(name.trim())
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_library(pool: &sqlx::PgPool, id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM libraries WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("媒体库不存在".to_string()));
    }

    Ok(())
}

pub async fn delete_library_by_name(pool: &sqlx::PgPool, name: &str) -> Result<(), AppError> {
    let library = get_library_by_name(pool, name)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;
    delete_library(pool, library.id).await
}

pub async fn rename_library(
    pool: &sqlx::PgPool,
    current_name: &str,
    new_name: &str,
) -> Result<DbLibrary, AppError> {
    let new_name = new_name.trim();
    if new_name.is_empty() {
        return Err(AppError::BadRequest("新媒体库名称不能为空".to_string()));
    }

    let library = get_library_by_name(pool, current_name)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;

    sqlx::query("UPDATE libraries SET name = $1, date_modified = now() WHERE id = $2")
        .bind(new_name)
        .bind(library.id)
        .execute(pool)
        .await?;

    get_library(pool, library.id)
        .await?
        .ok_or_else(|| AppError::Internal("重命名后无法读取媒体库".to_string()))
}

pub async fn update_library_options(
    pool: &sqlx::PgPool,
    id: Uuid,
    options: LibraryOptionsDto,
) -> Result<DbLibrary, AppError> {
    let library = get_library(pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;
    let paths = library_paths(&library);
    let options = normalize_library_options(options, &paths);
    let path = options
        .path_infos
        .first()
        .map(|path| path.path.clone())
        .unwrap_or_else(|| library.path.clone());

    sqlx::query(
        "UPDATE libraries SET path = $1, library_options = $2, date_modified = now() WHERE id = $3",
    )
    .bind(path)
    .bind(json!(options))
    .bind(id)
    .execute(pool)
    .await?;

    get_library(pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("更新媒体库选项后无法读取媒体库".to_string()))
}

pub async fn add_library_path(
    pool: &sqlx::PgPool,
    library_name: &str,
    path: &str,
) -> Result<DbLibrary, AppError> {
    let library = get_library_by_name(pool, library_name)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;
    let mut options = library_options(&library);
    let path = path.trim();
    if path.is_empty() {
        return Err(AppError::BadRequest("媒体路径不能为空".to_string()));
    }
    if !options
        .path_infos
        .iter()
        .any(|candidate| candidate.path.eq_ignore_ascii_case(path))
    {
        options.path_infos.push(MediaPathInfoDto {
            path: path.to_string(),
        });
    }
    update_library_options(pool, library.id, options).await
}

pub async fn update_library_path(
    pool: &sqlx::PgPool,
    library_name: &str,
    path_info: MediaPathInfoDto,
) -> Result<DbLibrary, AppError> {
    let library = get_library_by_name(pool, library_name)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;
    let mut options = library_options(&library);
    let path = path_info.path.trim();
    if path.is_empty() {
        return Err(AppError::BadRequest("媒体路径不能为空".to_string()));
    }
    if options.path_infos.is_empty() {
        options.path_infos.push(MediaPathInfoDto {
            path: path.to_string(),
        });
    } else {
        options.path_infos[0].path = path.to_string();
    }
    update_library_options(pool, library.id, options).await
}

pub async fn remove_library_path(
    pool: &sqlx::PgPool,
    library_name: &str,
    path: &str,
) -> Result<DbLibrary, AppError> {
    let library = get_library_by_name(pool, library_name)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体库不存在".to_string()))?;
    let mut options = library_options(&library);
    options
        .path_infos
        .retain(|candidate| !candidate.path.eq_ignore_ascii_case(path.trim()));

    if options.path_infos.is_empty() {
        return Err(AppError::BadRequest(
            "媒体库至少需要保留一个路径".to_string(),
        ));
    }

    update_library_options(pool, library.id, options).await
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

pub async fn count_item_children(pool: &sqlx::PgPool, parent_id: Uuid) -> Result<i64, AppError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM media_items WHERE parent_id = $1")
            .bind(parent_id)
            .fetch_one(pool)
            .await?,
    )
}

pub fn library_options(library: &DbLibrary) -> LibraryOptionsDto {
    let mut options = serde_json::from_value::<LibraryOptionsDto>(library.library_options.clone())
        .unwrap_or_default();
    if options.path_infos.is_empty() {
        options.path_infos.push(MediaPathInfoDto {
            path: library.path.clone(),
        });
    }
    normalize_library_options(
        options,
        &library_paths_from_options_or_path(&library.library_options, &library.path),
    )
}

pub fn library_paths(library: &DbLibrary) -> Vec<String> {
    library_paths_from_options_or_path(&library.library_options, &library.path)
}

pub fn library_to_virtual_folder_dto(library: &DbLibrary) -> VirtualFolderInfoDto {
    let options = library_options(library);
    let locations = options
        .path_infos
        .iter()
        .map(|path| path.path.clone())
        .collect::<Vec<_>>();

    VirtualFolderInfoDto {
        name: library.name.clone(),
        collection_type: library.collection_type.clone(),
        item_id: library.id.to_string(),
        locations,
        library_options: options,
    }
}

fn normalize_collection_type(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "tvshows" | "series" | "shows" => "tvshows".to_string(),
        "music" | "audio" => "music".to_string(),
        "homevideos" | "homevideo" | "videos" => "homevideos".to_string(),
        "mixed" => "mixed".to_string(),
        _ => "movies".to_string(),
    }
}

fn normalize_library_paths(paths: &[String]) -> Vec<String> {
    let mut result = Vec::new();
    for path in paths {
        let path = path.trim();
        if path.is_empty() {
            continue;
        }
        if !result
            .iter()
            .any(|candidate: &String| candidate.eq_ignore_ascii_case(path))
        {
            result.push(path.to_string());
        }
    }
    result
}

fn normalize_library_options(
    mut options: LibraryOptionsDto,
    fallback_paths: &[String],
) -> LibraryOptionsDto {
    let option_paths = options
        .path_infos
        .iter()
        .map(|path| path.path.clone())
        .collect::<Vec<_>>();
    let paths = normalize_library_paths(if option_paths.is_empty() {
        fallback_paths
    } else {
        &option_paths
    });

    options.path_infos = paths
        .into_iter()
        .map(|path| MediaPathInfoDto { path })
        .collect();

    if options.season_zero_display_name.trim().is_empty() {
        options.season_zero_display_name = "Specials".to_string();
    }

    options
}

fn library_paths_from_options_or_path(options: &Value, fallback_path: &str) -> Vec<String> {
    let mut paths = serde_json::from_value::<LibraryOptionsDto>(options.clone())
        .ok()
        .map(|options| {
            options
                .path_infos
                .into_iter()
                .map(|path| path.path)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if paths.is_empty() {
        paths.push(fallback_path.to_string());
    }

    normalize_library_paths(&paths)
}

pub struct ItemListOptions {
    pub library_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub include_types: Vec<String>,
    pub genres: Vec<String>,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub start_index: i64,
    pub limit: i64,
}

pub struct ResumeListOptions {
    pub library_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub media_types: Vec<String>,
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
            id, parent_id, name, sort_name, item_type, media_type, path, container,
            overview, production_year, runtime_ticks, premiere_date, series_name, season_name,
            index_number, index_number_end, parent_index_number, provider_ids, genres,
            width, height, video_codec, audio_codec, image_primary_path, backdrop_path,
            date_created, date_modified, COUNT(*) OVER() AS total_count
        FROM media_items
        WHERE 1 = 1
        "#,
    );

    if let Some(library_id) = options.library_id {
        builder.push(" AND library_id = ").push_bind(library_id);
    }

    if let Some(parent_id) = options.parent_id {
        if options.recursive {
            builder.push(
                r#"
                AND id IN (
                    WITH RECURSIVE descendants(id) AS (
                        SELECT id FROM media_items WHERE parent_id =
                "#,
            );
            builder.push_bind(parent_id);
            builder.push(
                r#"
                        UNION ALL
                        SELECT child.id
                        FROM media_items child
                        INNER JOIN descendants d ON child.parent_id = d.id
                    )
                    SELECT id FROM descendants
                )
                "#,
            );
        } else {
            builder.push(" AND parent_id = ").push_bind(parent_id);
        }
    } else if !options.recursive {
        builder.push(" AND parent_id IS NULL");
    }

    if !options.include_types.is_empty() {
        builder
            .push(" AND item_type = ANY(")
            .push_bind(options.include_types)
            .push(")");
    }

    if !options.genres.is_empty() {
        builder.push(" AND genres && ").push_bind(options.genres);
    }

    if let Some(search_term) = options.search_term.filter(|value| !value.trim().is_empty()) {
        builder
            .push(" AND name ILIKE ")
            .push_bind(format!("%{}%", search_term.trim()));
    }

    let sort_column = match options.sort_by.as_deref().unwrap_or("SortName") {
        "DateCreated" | "DateLastContentAdded" => "date_created",
        "IndexNumber" => "index_number",
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

pub async fn list_resume_items(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    options: ResumeListOptions,
) -> Result<QueryResult<DbMediaItem>, AppError> {
    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            mi.id, mi.parent_id, mi.name, mi.sort_name, mi.item_type, mi.media_type,
            mi.path, mi.container, mi.overview, mi.production_year, mi.runtime_ticks,
            mi.premiere_date, mi.series_name, mi.season_name, mi.index_number,
            mi.index_number_end, mi.parent_index_number, mi.provider_ids, mi.genres,
            mi.width, mi.height, mi.video_codec, mi.audio_codec, mi.image_primary_path,
            mi.backdrop_path, mi.date_created, mi.date_modified,
            COUNT(*) OVER() AS total_count
        FROM media_items mi
        INNER JOIN user_item_data uid ON uid.item_id = mi.id
        WHERE uid.user_id =
        "#,
    );
    builder.push_bind(user_id);
    builder.push(
        r#"
            AND uid.playback_position_ticks > 0
            AND uid.is_played = false
            AND mi.item_type NOT IN ('Series', 'Season', 'Folder')
        "#,
    );

    if let Some(library_id) = options.library_id {
        builder.push(" AND mi.library_id = ").push_bind(library_id);
    }

    if let Some(parent_id) = options.parent_id {
        builder.push(
            r#"
            AND mi.id IN (
                WITH RECURSIVE descendants(id) AS (
                    SELECT id FROM media_items WHERE parent_id =
            "#,
        );
        builder.push_bind(parent_id);
        builder.push(
            r#"
                    UNION ALL
                    SELECT child.id
                    FROM media_items child
                    INNER JOIN descendants d ON child.parent_id = d.id
                )
                SELECT id FROM descendants
            )
            "#,
        );
    }

    if !options.media_types.is_empty() {
        builder
            .push(" AND mi.media_type = ANY(")
            .push_bind(options.media_types)
            .push(")");
    }

    builder
        .push(" ORDER BY uid.updated_at DESC NULLS LAST, mi.date_created DESC NULLS LAST")
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
            id, parent_id, name, sort_name, item_type, media_type, path, container,
            overview, production_year, runtime_ticks, premiere_date, series_name, season_name,
            index_number, index_number_end, parent_index_number, provider_ids, genres,
            width, height, video_codec, audio_codec, image_primary_path, backdrop_path,
            date_created, date_modified
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

pub async fn get_user_item_data_dto(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<UserItemDataDto, AppError> {
    let data = get_user_item_data(pool, user_id, item_id)
        .await?
        .map(|data| user_item_data_to_dto_for_item(data, item_id))
        .unwrap_or_else(|| empty_user_data_for_item(item_id));
    Ok(data)
}

pub async fn set_user_favorite(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_id: Uuid,
    is_favorite: bool,
) -> Result<UserItemDataDto, AppError> {
    let data = sqlx::query_as::<_, DbUserItemData>(
        r#"
        INSERT INTO user_item_data (user_id, item_id, is_favorite, updated_at)
        VALUES ($1, $2, $3, now())
        ON CONFLICT (user_id, item_id)
        DO UPDATE SET
            is_favorite = EXCLUDED.is_favorite,
            updated_at = now()
        RETURNING playback_position_ticks, play_count, is_favorite, is_played, last_played_date
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(is_favorite)
    .fetch_one(pool)
    .await?;

    Ok(user_item_data_to_dto_for_item(data, item_id))
}

pub async fn set_user_played(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_id: Uuid,
    is_played: bool,
    date_played: Option<DateTime<Utc>>,
) -> Result<UserItemDataDto, AppError> {
    let data = sqlx::query_as::<_, DbUserItemData>(
        r#"
        INSERT INTO user_item_data
            (user_id, item_id, playback_position_ticks, is_played, play_count, last_played_date, updated_at)
        VALUES
            ($1, $2, 0, $3, CASE WHEN $3 THEN 1 ELSE 0 END, CASE WHEN $3 THEN COALESCE($4, now()) ELSE NULL END, now())
        ON CONFLICT (user_id, item_id)
        DO UPDATE SET
            playback_position_ticks = CASE WHEN $3 THEN 0 ELSE 0 END,
            is_played = $3,
            play_count = CASE WHEN $3 THEN GREATEST(user_item_data.play_count, 1) ELSE 0 END,
            last_played_date = CASE WHEN $3 THEN COALESCE($4, now()) ELSE NULL END,
            updated_at = now()
        RETURNING playback_position_ticks, play_count, is_favorite, is_played, last_played_date
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(is_played)
    .bind(date_played)
    .fetch_one(pool)
    .await?;

    Ok(user_item_data_to_dto_for_item(data, item_id))
}

pub struct UpdateUserDataInput {
    pub playback_position_ticks: Option<i64>,
    pub play_count: Option<i32>,
    pub is_favorite: Option<bool>,
    pub played: Option<bool>,
    pub last_played_date: Option<DateTime<Utc>>,
}

pub async fn update_user_item_data(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_id: Uuid,
    input: UpdateUserDataInput,
) -> Result<UserItemDataDto, AppError> {
    let data = sqlx::query_as::<_, DbUserItemData>(
        r#"
        INSERT INTO user_item_data
            (
                user_id, item_id, playback_position_ticks, play_count, is_favorite,
                is_played, last_played_date, updated_at
            )
        VALUES
            (
                $1, $2, COALESCE($3, 0), COALESCE($4, 0), COALESCE($5, false),
                COALESCE($6, false), $7, now()
            )
        ON CONFLICT (user_id, item_id)
        DO UPDATE SET
            playback_position_ticks = COALESCE($3, user_item_data.playback_position_ticks),
            play_count = COALESCE($4, user_item_data.play_count),
            is_favorite = COALESCE($5, user_item_data.is_favorite),
            is_played = COALESCE($6, user_item_data.is_played),
            last_played_date = COALESCE($7, user_item_data.last_played_date),
            updated_at = now()
        RETURNING playback_position_ticks, play_count, is_favorite, is_played, last_played_date
        "#,
    )
    .bind(user_id)
    .bind(item_id)
    .bind(input.playback_position_ticks)
    .bind(input.play_count)
    .bind(input.is_favorite)
    .bind(input.played)
    .bind(input.last_played_date)
    .fetch_one(pool)
    .await?;

    Ok(user_item_data_to_dto_for_item(data, item_id))
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

pub struct UpsertMediaItem<'a> {
    pub library_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: &'a str,
    pub item_type: &'a str,
    pub media_type: &'a str,
    pub path: &'a Path,
    pub container: Option<&'a str>,
    pub overview: Option<&'a str>,
    pub production_year: Option<i32>,
    pub runtime_ticks: Option<i64>,
    pub premiere_date: Option<NaiveDate>,
    pub genres: &'a [String],
    pub image_primary_path: Option<&'a Path>,
    pub backdrop_path: Option<&'a Path>,
    pub series_name: Option<&'a str>,
    pub season_name: Option<&'a str>,
    pub index_number: Option<i32>,
    pub index_number_end: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub video_codec: Option<&'a str>,
    pub audio_codec: Option<&'a str>,
}

pub async fn upsert_media_item(
    pool: &sqlx::PgPool,
    input: UpsertMediaItem<'_>,
) -> Result<Uuid, AppError> {
    let path_text = input.path.to_string_lossy().to_string();
    let image_text = input
        .image_primary_path
        .map(|value| value.to_string_lossy().to_string());
    let backdrop_text = input
        .backdrop_path
        .map(|value| value.to_string_lossy().to_string());
    let id = Uuid::new_v5(&input.library_id, path_text.as_bytes());
    let sort_name = sort_name_for_item(&input);

    sqlx::query(
        r#"
        INSERT INTO media_items
            (
                id, library_id, parent_id, name, sort_name, item_type, media_type, path,
                container, overview, production_year, runtime_ticks, premiere_date,
                genres, image_primary_path, backdrop_path, series_name, season_name, index_number,
                index_number_end, parent_index_number, width, height, video_codec, audio_codec,
                date_modified
            )
        VALUES
            (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, $10, $11, $12, $13,
                $14, $15, $16, $17, $18, $19,
                $20, $21, $22, $23, $24, $25,
                now()
            )
        ON CONFLICT (library_id, path)
        DO UPDATE SET
            parent_id = EXCLUDED.parent_id,
            name = EXCLUDED.name,
            sort_name = EXCLUDED.sort_name,
            item_type = EXCLUDED.item_type,
            media_type = EXCLUDED.media_type,
            container = EXCLUDED.container,
            overview = EXCLUDED.overview,
            production_year = EXCLUDED.production_year,
            runtime_ticks = EXCLUDED.runtime_ticks,
            premiere_date = EXCLUDED.premiere_date,
            genres = EXCLUDED.genres,
            image_primary_path = EXCLUDED.image_primary_path,
            backdrop_path = EXCLUDED.backdrop_path,
            series_name = EXCLUDED.series_name,
            season_name = EXCLUDED.season_name,
            index_number = EXCLUDED.index_number,
            index_number_end = EXCLUDED.index_number_end,
            parent_index_number = EXCLUDED.parent_index_number,
            width = EXCLUDED.width,
            height = EXCLUDED.height,
            video_codec = EXCLUDED.video_codec,
            audio_codec = EXCLUDED.audio_codec,
            date_modified = now()
        "#,
    )
    .bind(id)
    .bind(input.library_id)
    .bind(input.parent_id)
    .bind(input.name)
    .bind(sort_name)
    .bind(input.item_type)
    .bind(input.media_type)
    .bind(path_text)
    .bind(input.container)
    .bind(input.overview)
    .bind(input.production_year)
    .bind(input.runtime_ticks)
    .bind(input.premiere_date)
    .bind(input.genres)
    .bind(image_text)
    .bind(backdrop_text)
    .bind(input.series_name)
    .bind(input.season_name)
    .bind(input.index_number)
    .bind(input.index_number_end)
    .bind(input.parent_index_number)
    .bind(input.width)
    .bind(input.height)
    .bind(input.video_codec)
    .bind(input.audio_codec)
    .execute(pool)
    .await?;

    Ok(id)
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
    let locations = library_paths(library);

    Ok(BaseItemDto {
        name: library.name.clone(),
        server_id: server_id.to_string(),
        id: library.id.to_string(),
        item_type: "CollectionFolder".to_string(),
        is_folder: true,
        sort_name: Some(library.name.to_lowercase()),
        collection_type: Some(library.collection_type.clone()),
        media_type: None,
        container: None,
        parent_id: None,
        path: locations
            .first()
            .cloned()
            .or_else(|| Some(library.path.clone())),
        run_time_ticks: None,
        production_year: None,
        overview: None,
        date_created: Some(library.created_at),
        premiere_date: None,
        genres: Vec::new(),
        provider_ids: BTreeMap::new(),
        series_name: None,
        season_name: None,
        index_number: None,
        index_number_end: None,
        parent_index_number: None,
        image_tags: BTreeMap::new(),
        backdrop_image_tags: Vec::new(),
        user_data: empty_user_data(),
        media_sources: Vec::new(),
        media_streams: Vec::new(),
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
        sort_name: Some("root".to_string()),
        collection_type: None,
        media_type: None,
        container: None,
        parent_id: None,
        path: None,
        run_time_ticks: None,
        production_year: None,
        overview: None,
        date_created: Some(Utc::now()),
        premiere_date: None,
        genres: Vec::new(),
        provider_ids: BTreeMap::new(),
        series_name: None,
        season_name: None,
        index_number: None,
        index_number_end: None,
        parent_index_number: None,
        image_tags: BTreeMap::new(),
        backdrop_image_tags: Vec::new(),
        user_data: empty_user_data(),
        media_sources: Vec::new(),
        media_streams: Vec::new(),
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
            .map(|data| user_item_data_to_dto_for_item(data, item.id))
            .unwrap_or_else(empty_user_data)
    } else {
        empty_user_data()
    };

    let is_folder = is_folder_item(item);
    let media_sources = if !is_folder {
        vec![media_source_for_item(item, server_id)]
    } else {
        Vec::new()
    };
    let media_streams = media_sources
        .first()
        .map(|source| source.media_streams.clone())
        .unwrap_or_default();
    let child_count = if is_folder {
        Some(count_item_children(pool, item.id).await?)
    } else {
        None
    };

    Ok(BaseItemDto {
        name: item.name.clone(),
        server_id: server_id.to_string(),
        id: item.id.to_string(),
        item_type: item.item_type.clone(),
        is_folder,
        sort_name: Some(item.sort_name.clone()),
        collection_type: None,
        media_type: (!is_folder).then(|| item.media_type.clone()),
        container: item.container.clone(),
        parent_id: item.parent_id.map(|value| value.to_string()),
        path: Some(item.path.clone()),
        run_time_ticks: item.runtime_ticks,
        production_year: item.production_year,
        overview: item.overview.clone(),
        date_created: Some(item.date_created),
        premiere_date: item.premiere_date,
        genres: item.genres.clone(),
        provider_ids: provider_ids_to_map(&item.provider_ids),
        series_name: item.series_name.clone(),
        season_name: item.season_name.clone(),
        index_number: item.index_number,
        index_number_end: item.index_number_end,
        parent_index_number: item.parent_index_number,
        image_tags,
        backdrop_image_tags,
        user_data,
        media_sources,
        media_streams,
        child_count,
        primary_image_aspect_ratio: item.image_primary_path.as_ref().map(|_| 0.666_666_666_7),
    })
}

pub fn media_source_for_item(item: &DbMediaItem, server_id: Uuid) -> MediaSourceDto {
    media_source_for_playback(item, server_id, None, None)
}

pub fn media_source_for_playback(
    item: &DbMediaItem,
    server_id: Uuid,
    play_session_id: Option<&str>,
    access_token: Option<&str>,
) -> MediaSourceDto {
    let local_path = Path::new(&item.path);
    let strm_target = naming::is_strm(local_path)
        .then(|| naming::read_strm_target(local_path))
        .flatten();
    let container = playable_container(item, local_path, strm_target.as_deref());
    let media_streams = media_streams_for_item(item);
    let is_remote = strm_target.is_some();
    let size = if is_remote {
        None
    } else {
        std::fs::metadata(&item.path)
            .ok()
            .and_then(|metadata| i64::try_from(metadata.len()).ok())
    };

    MediaSourceDto {
        id: item.id.to_string(),
        item_id: item.id.to_string(),
        server_id: server_id.to_string(),
        path: strm_target.unwrap_or_else(|| item.path.clone()),
        protocol: if is_remote { "Http" } else { "File" }.to_string(),
        source_type: "Default".to_string(),
        container: container.clone(),
        name: item.name.clone(),
        is_remote,
        is_infinite_stream: false,
        requires_opening: false,
        requires_closing: false,
        requires_looping: false,
        supports_probing: true,
        supports_direct_play: !is_remote,
        supports_direct_stream: true,
        supports_transcoding: false,
        add_api_key_to_direct_stream_url: true,
        direct_stream_url: direct_stream_url(item.id, &container, play_session_id, access_token),
        transcoding_url: None,
        play_session_id: play_session_id.map(ToOwned::to_owned),
        video_type: item
            .media_type
            .eq_ignore_ascii_case("Video")
            .then(|| "VideoFile".to_string()),
        required_http_headers: BTreeMap::new(),
        formats: vec![container.clone()],
        size,
        e_tag: Some(item.date_modified.timestamp().to_string()),
        bitrate: None,
        default_audio_stream_index: Some(if item.media_type.eq_ignore_ascii_case("Audio") {
            0
        } else {
            1
        }),
        default_subtitle_stream_index: None,
        run_time_ticks: item.runtime_ticks,
        media_streams,
    }
}

fn playable_container(item: &DbMediaItem, local_path: &Path, strm_target: Option<&str>) -> String {
    strm_target
        .and_then(naming::extension_from_url)
        .filter(|container| !container.eq_ignore_ascii_case("strm"))
        .or_else(|| {
            item.container
                .clone()
                .filter(|container| !container.eq_ignore_ascii_case("strm"))
        })
        .or_else(|| {
            local_path
                .extension()
                .map(|ext| ext.to_string_lossy().to_string())
                .filter(|container| !container.eq_ignore_ascii_case("strm"))
        })
        .unwrap_or_else(|| "mp4".to_string())
}

fn direct_stream_url(
    item_id: Uuid,
    container: &str,
    play_session_id: Option<&str>,
    access_token: Option<&str>,
) -> String {
    let mut query = form_urlencoded::Serializer::new(String::new());
    query.append_pair("Static", "true");
    query.append_pair("MediaSourceId", &item_id.to_string());
    query.append_pair("mediaSourceId", &item_id.to_string());
    if let Some(play_session_id) = play_session_id {
        query.append_pair("PlaySessionId", play_session_id);
    }
    if let Some(access_token) = access_token {
        query.append_pair("api_key", access_token);
    }

    format!(
        "/Videos/{}/stream.{}?{}",
        item_id,
        container,
        query.finish()
    )
}

pub fn media_streams_for_item(item: &DbMediaItem) -> Vec<MediaStreamDto> {
    if item.media_type.eq_ignore_ascii_case("Audio") {
        return vec![MediaStreamDto {
            index: 0,
            stream_type: "Audio".to_string(),
            codec: item.audio_codec.clone(),
            language: None,
            display_title: item
                .audio_codec
                .clone()
                .or_else(|| Some("Default".to_string())),
            is_default: true,
            is_forced: false,
            width: None,
            height: None,
            bit_rate: None,
            channels: None,
            sample_rate: None,
            is_external: false,
            delivery_method: None,
            delivery_url: None,
            supports_external_stream: false,
            path: None,
        }];
    }

    let mut streams = vec![
        MediaStreamDto {
            index: 0,
            stream_type: "Video".to_string(),
            codec: item.video_codec.clone(),
            language: None,
            display_title: video_display_title(item),
            is_default: true,
            is_forced: false,
            width: item.width,
            height: item.height,
            bit_rate: None,
            channels: None,
            sample_rate: None,
            is_external: false,
            delivery_method: None,
            delivery_url: None,
            supports_external_stream: false,
            path: None,
        },
        MediaStreamDto {
            index: 1,
            stream_type: "Audio".to_string(),
            codec: item.audio_codec.clone(),
            language: None,
            display_title: item
                .audio_codec
                .clone()
                .or_else(|| Some("Default".to_string())),
            is_default: true,
            is_forced: false,
            width: None,
            height: None,
            bit_rate: None,
            channels: None,
            sample_rate: None,
            is_external: false,
            delivery_method: None,
            delivery_url: None,
            supports_external_stream: false,
            path: None,
        },
    ];

    streams.extend(subtitle_streams_for_item(item));
    streams
}

pub fn subtitle_path_for_stream_index(item: &DbMediaItem, stream_index: i32) -> Option<PathBuf> {
    subtitle_streams_for_item(item)
        .into_iter()
        .find(|stream| stream.index == stream_index)
        .and_then(|stream| stream.path)
        .map(PathBuf::from)
}

fn subtitle_streams_for_item(item: &DbMediaItem) -> Vec<MediaStreamDto> {
    let video_path = Path::new(&item.path);
    naming::sidecar_subtitles(video_path)
        .into_iter()
        .enumerate()
        .map(|(offset, subtitle)| {
            let index = 2 + offset as i32;
            MediaStreamDto {
                index,
                stream_type: "Subtitle".to_string(),
                codec: Some(subtitle.format.clone()),
                language: subtitle.language,
                display_title: Some(subtitle.title),
                is_default: false,
                is_forced: false,
                width: None,
                height: None,
                bit_rate: None,
                channels: None,
                sample_rate: None,
                is_external: true,
                delivery_method: Some("External".to_string()),
                delivery_url: Some(format!(
                    "/Videos/{}/{}/Subtitles/{}/Stream.{}",
                    item.id, item.id, index, subtitle.format
                )),
                supports_external_stream: true,
                path: Some(subtitle.path.to_string_lossy().to_string()),
            }
        })
        .collect()
}

fn video_display_title(item: &DbMediaItem) -> Option<String> {
    match (item.width, item.height, item.video_codec.as_deref()) {
        (Some(width), Some(height), Some(codec)) => Some(format!("{width}x{height} {codec}")),
        (Some(width), Some(height), None) => Some(format!("{width}x{height}")),
        (None, Some(height), Some(codec)) => Some(format!("{height}p {codec}")),
        (None, Some(height), None) => Some(format!("{height}p")),
        (_, _, Some(codec)) => Some(codec.to_string()),
        _ => None,
    }
}

fn sort_name_for_item(input: &UpsertMediaItem<'_>) -> String {
    let normalized = input.name.to_lowercase();
    match (input.parent_index_number, input.index_number) {
        (Some(parent_index), Some(index)) if input.item_type.eq_ignore_ascii_case("Episode") => {
            format!("{parent_index:04}-{index:04}-{normalized}")
        }
        (_, Some(index)) => format!("{index:04}-{normalized}"),
        _ => normalized,
    }
}

fn is_folder_item(item: &DbMediaItem) -> bool {
    matches!(
        item.item_type.as_str(),
        "AggregateFolder" | "BoxSet" | "CollectionFolder" | "Folder" | "Season" | "Series"
    )
}

fn provider_ids_to_map(value: &Value) -> BTreeMap<String, String> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .filter_map(|(key, value)| {
                    value
                        .as_str()
                        .map(|text| (key.clone(), text.to_string()))
                        .or_else(|| {
                            value
                                .as_i64()
                                .map(|number| (key.clone(), number.to_string()))
                        })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn user_item_data_to_dto(data: DbUserItemData) -> UserItemDataDto {
    UserItemDataDto {
        rating: None,
        played_percentage: None,
        unplayed_item_count: None,
        playback_position_ticks: data.playback_position_ticks,
        play_count: data.play_count,
        is_favorite: data.is_favorite,
        likes: None,
        played: data.is_played,
        last_played_date: data.last_played_date,
        key: None,
        item_id: None,
    }
}

fn format_activity_overview(
    user_name: &str,
    item_name: &str,
    event_type: &str,
    position_ticks: Option<i64>,
    is_paused: Option<bool>,
    played_to_completion: Option<bool>,
) -> String {
    let position = position_ticks
        .map(|ticks| {
            let total_seconds = ticks / 10_000_000;
            let minutes = total_seconds / 60;
            let seconds = total_seconds % 60;
            format!("{minutes}:{seconds:02}")
        })
        .unwrap_or_else(|| "0:00".to_string());

    match event_type {
        "Started" => format!("{user_name} 开始播放 {item_name}"),
        "Progress" if is_paused.unwrap_or(false) => {
            format!("{user_name} 暂停了 {item_name}，位置 {position}")
        }
        "Progress" => format!("{user_name} 正在观看 {item_name}，位置 {position}"),
        "Stopped" if played_to_completion.unwrap_or(false) => {
            format!("{user_name} 播放完成 {item_name}")
        }
        "Stopped" => format!("{user_name} 停止播放 {item_name}，位置 {position}"),
        _ => format!("{user_name} 发生了播放事件：{item_name}"),
    }
}

fn empty_user_data() -> UserItemDataDto {
    UserItemDataDto {
        rating: None,
        played_percentage: None,
        unplayed_item_count: None,
        playback_position_ticks: 0,
        play_count: 0,
        is_favorite: false,
        likes: None,
        played: false,
        last_played_date: None,
        key: None,
        item_id: None,
    }
}

fn user_item_data_to_dto_for_item(data: DbUserItemData, item_id: Uuid) -> UserItemDataDto {
    let mut dto = user_item_data_to_dto(data);
    dto.key = Some(item_id.to_string());
    dto.item_id = Some(item_id.to_string());
    dto
}

fn empty_user_data_for_item(item_id: Uuid) -> UserItemDataDto {
    let mut dto = empty_user_data();
    dto.key = Some(item_id.to_string());
    dto.item_id = Some(item_id.to_string());
    dto
}
