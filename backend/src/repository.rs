use crate::{
    config::Config,
    error::AppError,
    metadata::models::{ExternalMovieMetadata, ExternalSeriesMetadata},
    metadata::provider::{ExternalEpisodeCatalogItem, ExternalPersonCredit},
    models::{
        emby_id_to_uuid, uuid_to_emby_guid, ActivityLogEntryDto, AuthSessionRow, BaseItemDto,
        BrandingConfiguration, DbLibrary, DbMediaChapter, DbMediaItem, DbMediaStream, DbPerson,
        DbRemoteEmbySource, DbUser, DbUserItemData, EncodingOptionsDto, ExternalUrlDto, GenreDto,
        ItemCountsDto, LibraryOptionsDto, LogFileDto, MediaItemRow, MediaPathInfoDto,
        MediaSourceDto, MediaStreamDto, NameLongIdDto, PersonDto, PublicUserDto, QueryResult,
        SessionInfoDto, StartupConfiguration, StartupRemoteAccessRequest, UserConfigurationDto,
        UserDto, UserItemDataDto, UserPolicyDto, VirtualFolderInfoDto,
    },
    naming, security,
};
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use regex::Regex;
use serde_json::{json, Value};
use sqlx::{FromRow, Postgres, QueryBuilder, Row};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};
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

#[derive(Debug, FromRow)]
struct SessionCommandRow {
    id: Uuid,
    command: String,
    payload: Value,
    created_at: DateTime<Utc>,
}

pub struct SessionRuntimeState {
    pub now_playing_item: BaseItemDto,
    pub play_state: Value,
    pub now_playing_queue: Vec<Value>,
}

pub async fn user_count(pool: &sqlx::PgPool) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?)
}

pub async fn item_counts(pool: &sqlx::PgPool) -> Result<ItemCountsDto, AppError> {
    crate::repo_cache::cached_item_counts(pool).await
}

pub async fn item_counts_uncached(pool: &sqlx::PgPool) -> Result<ItemCountsDto, AppError> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT item_type, COUNT(*)::bigint FROM media_items GROUP BY item_type")
            .fetch_all(pool)
            .await?;

    let mut counts = ItemCountsDto::default();
    for (item_type, count) in rows {
        let count = i64_to_i32_count(count);
        match item_type.as_str() {
            "Movie" => counts.movie_count = count,
            "Series" => counts.series_count = count,
            "Episode" => counts.episode_count = count,
            "Trailer" => counts.trailer_count = count,
            "BoxSet" => counts.box_set_count = count,
            "Book" => counts.book_count = count,
            "MusicVideo" => counts.music_video_count = count,
            "Audio" | "AudioItem" | "Song" => counts.song_count = count,
            "MusicAlbum" | "Album" => counts.album_count = count,
            _ => {}
        }
        counts.item_count = counts.item_count.saturating_add(count);
    }

    Ok(counts)
}

pub fn user_policy_from_value(value: &Value) -> UserPolicyDto {
    serde_json::from_value::<UserPolicyDto>(value.clone()).unwrap_or_default()
}

/// Emby 标准分级映射：将 official_rating 文本转为可比较的整数值
pub fn official_rating_to_value(rating: &str) -> Option<i32> {
    match rating.trim().to_uppercase().as_str() {
        "G" | "TV-Y" | "APPROVED" => Some(1),
        "TV-Y7" | "TV-Y7-FV" => Some(4),
        "PG" | "TV-G" => Some(5),
        "PG-13" | "TV-PG" => Some(7),
        "TV-14" => Some(8),
        "R" | "TV-MA" => Some(9),
        "NC-17" | "X" | "XXX" | "UNRATED" => Some(10),
        "NR" | "NOT RATED" => None,
        _ => None,
    }
}

pub async fn visible_library_ids_for_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<Uuid>, AppError> {
    let user = get_user_by_id(pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let policy = user_policy_from_value(&user.policy);
    let blocked: std::collections::HashSet<Uuid> =
        policy.blocked_media_folders.iter().copied().collect();

    if user.is_admin || policy.enable_all_folders {
        let libraries = list_libraries(pool).await?;
        return Ok(libraries
            .into_iter()
            .map(|library| library.id)
            .filter(|id| !blocked.contains(id))
            .collect());
    }

    let visible: std::collections::BTreeSet<Uuid> = policy.enabled_folders.into_iter().collect();
    let libraries = list_libraries(pool).await?;
    Ok(libraries
        .into_iter()
        .filter(|library| visible.contains(&library.id) && !blocked.contains(&library.id))
        .map(|library| library.id)
        .collect())
}

/// 计算某个用户在 list_media_items 类查询里需要叠加的 library 白名单。
///
/// - 管理员、`EnableAllFolders=true` 用户：返回 `None`（无需收紧，沿用原逻辑）。
/// - 其他用户：返回 `Some(allowed_ids)`，调用方应在 SQL 上追加
///   `AND library_id = ANY(allowed_ids)`；当列表为空时调用方应直接返回空集。
///
/// 这样普通用户在被管理员限定 `EnabledFolders` 后，所有列表型接口（包括
/// `/Items`、`/Genres`、`/Persons`、`/Items/Counts`、Latest/Resume 等）
/// 都能强制裁剪到他可见的库；与 Emby 官方 BaseItemQueryService 行为对齐。
pub async fn effective_library_filter_for_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Option<Vec<Uuid>>, AppError> {
    let Some(user) = get_user_by_id(pool, user_id).await? else {
        return Ok(Some(Vec::new()));
    };
    let policy = user_policy_from_value(&user.policy);
    let blocked: std::collections::HashSet<Uuid> =
        policy.blocked_media_folders.iter().copied().collect();
    if user.is_admin || policy.enable_all_folders {
        if blocked.is_empty() {
            return Ok(None);
        }
        let libraries = list_libraries(pool).await?;
        let allowed: Vec<Uuid> = libraries
            .into_iter()
            .map(|library| library.id)
            .filter(|id| !blocked.contains(id))
            .collect();
        return Ok(Some(allowed));
    }
    let allowed_set: std::collections::BTreeSet<Uuid> =
        policy.enabled_folders.into_iter().collect();
    let libraries = list_libraries(pool).await?;
    let allowed: Vec<Uuid> = libraries
        .into_iter()
        .filter(|library| allowed_set.contains(&library.id) && !blocked.contains(&library.id))
        .map(|library| library.id)
        .collect();
    Ok(Some(allowed))
}

pub async fn item_library_id(pool: &sqlx::PgPool, item_id: Uuid) -> Result<Option<Uuid>, AppError> {
    Ok(
        sqlx::query_scalar("SELECT library_id FROM media_items WHERE id = $1")
            .bind(item_id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn user_can_access_item(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_id: Uuid,
) -> Result<bool, AppError> {
    let Some(library_id) = item_library_id(pool, item_id).await? else {
        return Ok(false);
    };
    let visible_ids = visible_library_ids_for_user(pool, user_id).await?;
    Ok(visible_ids.contains(&library_id))
}

pub async fn visible_libraries_for_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<DbLibrary>, AppError> {
    let user = get_user_by_id(pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    let policy = user_policy_from_value(&user.policy);
    let blocked: std::collections::HashSet<Uuid> =
        policy.blocked_media_folders.iter().copied().collect();

    let libraries = list_libraries(pool).await?;
    if user.is_admin || policy.enable_all_folders {
        return Ok(libraries
            .into_iter()
            .filter(|library| !blocked.contains(&library.id))
            .collect());
    }

    let visible: std::collections::BTreeSet<Uuid> = policy.enabled_folders.into_iter().collect();
    Ok(libraries
        .into_iter()
        .filter(|library| visible.contains(&library.id) && !blocked.contains(&library.id))
        .collect())
}

pub async fn item_counts_for_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<ItemCountsDto, AppError> {
    let library_ids = visible_library_ids_for_user(pool, user_id).await?;
    if library_ids.is_empty() {
        return Ok(ItemCountsDto::default());
    }

    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT item_type, COUNT(*)::bigint FROM media_items WHERE library_id = ANY($1) GROUP BY item_type",
    )
    .bind(&library_ids)
    .fetch_all(pool)
    .await?;

    let mut counts = ItemCountsDto::default();
    for (item_type, count) in rows {
        let count = i64_to_i32_count(count);
        match item_type.as_str() {
            "Movie" => counts.movie_count = count,
            "Series" => counts.series_count = count,
            "Episode" => counts.episode_count = count,
            "Trailer" => counts.trailer_count = count,
            "BoxSet" => counts.box_set_count = count,
            "Book" => counts.book_count = count,
            "MusicVideo" => counts.music_video_count = count,
            "Audio" | "AudioItem" | "Song" => counts.song_count = count,
            "MusicAlbum" | "Album" => counts.album_count = count,
            _ => {}
        }
        counts.item_count = counts.item_count.saturating_add(count);
    }

    Ok(counts)
}

fn i64_to_i32_count(value: i64) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
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
        if let Ok(mut configuration) = serde_json::from_value::<StartupConfiguration>(value) {
            normalize_startup_configuration(&mut configuration);
            return Ok(configuration);
        }
    }

    let mut configuration = default_startup_configuration(config);
    normalize_startup_configuration(&mut configuration);
    Ok(configuration)
}

pub async fn update_startup_configuration(
    pool: &sqlx::PgPool,
    configuration: &StartupConfiguration,
) -> Result<(), AppError> {
    let mut configuration = configuration.clone();
    normalize_startup_configuration(&mut configuration);
    set_system_setting(pool, "startup_configuration", json!(configuration)).await
}

pub async fn branding_configuration(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<BrandingConfiguration, AppError> {
    if let Some(value) = get_system_setting(pool, "branding_configuration").await? {
        if let Ok(configuration) = serde_json::from_value::<BrandingConfiguration>(value) {
            return Ok(configuration);
        }
    }

    Ok(BrandingConfiguration {
        login_disclaimer: config.branding_login_disclaimer.clone(),
        custom_css: config.branding_custom_css.clone(),
        splashscreen_enabled: config.branding_splashscreen_enabled,
    })
}

pub async fn update_branding_configuration(
    pool: &sqlx::PgPool,
    configuration: &BrandingConfiguration,
) -> Result<(), AppError> {
    set_system_setting(pool, "branding_configuration", json!(configuration)).await
}

pub async fn branding_css(pool: &sqlx::PgPool, config: &Config) -> Result<String, AppError> {
    let configuration = branding_configuration(pool, config).await?;
    Ok(configuration.custom_css)
}

pub async fn playback_configuration(
    pool: &sqlx::PgPool,
) -> Result<crate::models::PlaybackConfiguration, AppError> {
    if let Some(value) = get_system_setting(pool, "playback_configuration").await? {
        if let Ok(configuration) =
            serde_json::from_value::<crate::models::PlaybackConfiguration>(value)
        {
            return Ok(configuration);
        }
    }
    Ok(crate::models::PlaybackConfiguration::default())
}

pub async fn update_playback_configuration(
    pool: &sqlx::PgPool,
    configuration: &crate::models::PlaybackConfiguration,
) -> Result<(), AppError> {
    set_system_setting(pool, "playback_configuration", json!(configuration)).await
}

pub async fn network_configuration(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<crate::models::NetworkConfiguration, AppError> {
    if let Some(value) = get_system_setting(pool, "network_configuration").await? {
        if let Ok(configuration) =
            serde_json::from_value::<crate::models::NetworkConfiguration>(value)
        {
            return Ok(configuration);
        }
    }
    let remote_access = startup_remote_access(pool, config).await?;
    Ok(crate::models::NetworkConfiguration {
        local_address: config.host.clone(),
        http_server_port_number: config.port,
        https_port_number: 8920,
        public_http_port: config.port,
        public_https_port: 8920,
        certificate_path: String::new(),
        enable_https: false,
        external_domain: config.public_url.clone().unwrap_or_default(),
        enable_upnp: remote_access.enable_automatic_port_mapping.unwrap_or(false),
    })
}

pub async fn update_network_configuration(
    pool: &sqlx::PgPool,
    configuration: &crate::models::NetworkConfiguration,
) -> Result<(), AppError> {
    set_system_setting(pool, "network_configuration", json!(configuration)).await
}

pub async fn library_display_configuration(
    pool: &sqlx::PgPool,
) -> Result<crate::models::LibraryDisplayConfiguration, AppError> {
    if let Some(value) = get_system_setting(pool, "library_display_configuration").await? {
        if let Ok(configuration) =
            serde_json::from_value::<crate::models::LibraryDisplayConfiguration>(value)
        {
            return Ok(configuration);
        }
    }
    Ok(crate::models::LibraryDisplayConfiguration::default())
}

pub async fn update_library_display_configuration(
    pool: &sqlx::PgPool,
    configuration: &crate::models::LibraryDisplayConfiguration,
) -> Result<(), AppError> {
    set_system_setting(pool, "library_display_configuration", json!(configuration)).await
}

pub async fn subtitle_download_configuration(
    pool: &sqlx::PgPool,
) -> Result<crate::models::SubtitleDownloadConfiguration, AppError> {
    if let Some(value) = get_system_setting(pool, "subtitle_download_configuration").await? {
        if let Ok(configuration) =
            serde_json::from_value::<crate::models::SubtitleDownloadConfiguration>(value)
        {
            return Ok(configuration);
        }
    }
    Ok(crate::models::SubtitleDownloadConfiguration::default())
}

pub async fn update_subtitle_download_configuration(
    pool: &sqlx::PgPool,
    configuration: &crate::models::SubtitleDownloadConfiguration,
) -> Result<(), AppError> {
    set_system_setting(
        pool,
        "subtitle_download_configuration",
        json!(configuration),
    )
    .await
}

pub async fn encoding_options(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<EncodingOptionsDto, AppError> {
    if let Some(value) = get_system_setting(pool, "encoding").await? {
        if let Ok(mut options) = serde_json::from_value::<EncodingOptionsDto>(value) {
            normalize_encoding_options(&mut options, config);
            return Ok(options);
        }
    }

    Ok(EncodingOptionsDto::from_config(config))
}

pub async fn update_encoding_options(
    pool: &sqlx::PgPool,
    config: &Config,
    mut options: EncodingOptionsDto,
) -> Result<EncodingOptionsDto, AppError> {
    normalize_encoding_options(&mut options, config);
    set_system_setting(pool, "encoding", json!(options)).await?;
    Ok(options)
}

pub async fn update_media_encoder_path(
    pool: &sqlx::PgPool,
    config: &Config,
    path: String,
    path_type: String,
) -> Result<EncodingOptionsDto, AppError> {
    let mut options = encoding_options(pool, config).await?;
    let normalized_path_type = if path_type.eq_ignore_ascii_case("Custom") {
        "Custom"
    } else {
        "System"
    };
    options.encoder_location_type = normalized_path_type.to_string();
    options.encoder_app_path = if normalized_path_type == "System" {
        "ffmpeg".to_string()
    } else {
        path.trim().to_string()
    };
    update_encoding_options(pool, config, options).await
}

pub async fn get_session_capabilities(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> Result<Option<Value>, AppError> {
    get_system_setting(pool, &format!("session_capabilities:{session_id}")).await
}

pub async fn set_session_capabilities(
    pool: &sqlx::PgPool,
    session_id: &str,
    value: Value,
) -> Result<(), AppError> {
    set_system_setting(pool, &format!("session_capabilities:{session_id}"), value).await
}

pub async fn delete_session_capabilities(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM system_settings WHERE key = $1")
        .bind(format!("session_capabilities:{session_id}"))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_session_viewing(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> Result<Option<Value>, AppError> {
    get_system_setting(pool, &format!("session_viewing:{session_id}")).await
}

pub async fn set_session_viewing(
    pool: &sqlx::PgPool,
    session_id: &str,
    value: Value,
) -> Result<(), AppError> {
    set_system_setting(pool, &format!("session_viewing:{session_id}"), value).await
}

pub async fn delete_session_viewing(pool: &sqlx::PgPool, session_id: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM system_settings WHERE key = $1")
        .bind(format!("session_viewing:{session_id}"))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_session_state_summary(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> Result<Option<Value>, AppError> {
    get_system_setting(pool, &format!("session_state:{session_id}")).await
}

pub async fn set_session_state_summary(
    pool: &sqlx::PgPool,
    session_id: &str,
    value: Value,
) -> Result<(), AppError> {
    set_system_setting(pool, &format!("session_state:{session_id}"), value).await
}

pub async fn delete_session_state_summary(
    pool: &sqlx::PgPool,
    session_id: &str,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM system_settings WHERE key = $1")
        .bind(format!("session_state:{session_id}"))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn update_remote_access(
    pool: &sqlx::PgPool,
    configuration: &StartupRemoteAccessRequest,
) -> Result<(), AppError> {
    set_system_setting(pool, "startup_remote_access", json!(configuration)).await
}

pub async fn startup_remote_access(
    pool: &sqlx::PgPool,
    config: &Config,
) -> Result<StartupRemoteAccessRequest, AppError> {
    if let Some(value) = get_system_setting(pool, "startup_remote_access").await? {
        if let Ok(configuration) = serde_json::from_value::<StartupRemoteAccessRequest>(value) {
            return Ok(configuration);
        }
    }

    Ok(StartupRemoteAccessRequest {
        enable_remote_access: config.enable_remote_access,
        enable_automatic_port_mapping: Some(config.enable_automatic_port_mapping),
    })
}

pub async fn create_initial_admin(
    pool: &sqlx::PgPool,
    name: &str,
    password: &str,
) -> Result<DbUser, AppError> {
    if user_count(pool).await? > 0 {
        return Err(AppError::Forbidden);
    }

    // 复用注册路径同一套规则。管理员的名字也走 50 字上限 + 拒绝控制 / 零宽字符。
    let name = crate::username::normalize_and_validate(name)
        .map_err(|e| match e {
            AppError::BadRequest(msg) if msg == "用户名不能为空" => {
                AppError::BadRequest("管理员名称不能为空".to_string())
            }
            other => other,
        })?;
    if password.trim().is_empty() {
        return Err(AppError::BadRequest("管理员密码不能为空".to_string()));
    }

    let id = Uuid::new_v4();
    let password_hash = security::hash_password(password)?;

    sqlx::query(
        r#"
        INSERT INTO users (id, name, password_hash, is_admin, policy, configuration, date_modified)
        VALUES ($1, $2, $3, true, $4, $5, now())
        "#,
    )
    .bind(id)
    .bind(&name)
    .bind(password_hash)
    .bind(serde_json::to_value(UserPolicyDto {
        is_administrator: true,
        ..UserPolicyDto::default()
    })?)
    .bind(serde_json::to_value(UserConfigurationDto::default())?)
    .execute(pool)
    .await?;

    get_user_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("创建管理员后无法读取用户".to_string()))
}

fn default_startup_configuration(config: &Config) -> StartupConfiguration {
    StartupConfiguration {
        server_name: config.server_name.clone(),
        ui_culture: config.ui_culture.clone(),
        metadata_country_code: config.metadata_country_code.clone(),
        preferred_metadata_language: config.preferred_metadata_language.clone(),
        library_scan_thread_count: 2,
        strm_analysis_thread_count: 8,
        tmdb_metadata_thread_count: 4,
        translation_thread_count: 4,
        tmdb_api_key: config.tmdb_api_key.clone().unwrap_or_default(),
        tmdb_api_keys: Vec::new(),
        fanart_api_keys: Vec::new(),
        subtitle_api_keys: Vec::new(),
        performance_tier: "medium".to_string(),
        db_max_connections: 20,
        image_download_threads: 8,
        background_task_threads: 4,
    }
}

fn normalize_startup_configuration(configuration: &mut StartupConfiguration) {
    configuration.server_name = configuration.server_name.trim().to_string();
    configuration.ui_culture = configuration.ui_culture.trim().to_string();
    configuration.metadata_country_code = configuration.metadata_country_code.trim().to_string();
    configuration.preferred_metadata_language =
        configuration.preferred_metadata_language.trim().to_string();
    configuration.library_scan_thread_count = configuration.library_scan_thread_count.max(1);
    configuration.strm_analysis_thread_count = configuration.strm_analysis_thread_count.max(1);
    configuration.tmdb_metadata_thread_count = configuration.tmdb_metadata_thread_count.max(1);
    configuration.translation_thread_count = configuration.translation_thread_count.max(1);
    configuration.db_max_connections = configuration.db_max_connections.max(5);
    configuration.image_download_threads = configuration.image_download_threads.max(1);
    configuration.background_task_threads = configuration.background_task_threads.max(1);
    configuration
        .tmdb_api_keys
        .retain(|k| !k.trim().is_empty());
    configuration
        .fanart_api_keys
        .retain(|k| !k.trim().is_empty());
    configuration
        .subtitle_api_keys
        .retain(|k| !k.trim().is_empty());

    apply_performance_tier(configuration);
}

fn apply_performance_tier(configuration: &mut StartupConfiguration) {
    match configuration.performance_tier.as_str() {
        "low" => {
            if configuration.library_scan_thread_count == 0 { configuration.library_scan_thread_count = 1; }
            if configuration.strm_analysis_thread_count == 0 { configuration.strm_analysis_thread_count = 4; }
            if configuration.tmdb_metadata_thread_count == 0 { configuration.tmdb_metadata_thread_count = 2; }
            if configuration.translation_thread_count == 0 { configuration.translation_thread_count = 2; }
            if configuration.db_max_connections == 0 { configuration.db_max_connections = 10; }
            if configuration.image_download_threads == 0 { configuration.image_download_threads = 4; }
            if configuration.background_task_threads == 0 { configuration.background_task_threads = 2; }
        }
        "high" => {
            if configuration.library_scan_thread_count == 0 { configuration.library_scan_thread_count = 8; }
            if configuration.strm_analysis_thread_count == 0 { configuration.strm_analysis_thread_count = 32; }
            if configuration.tmdb_metadata_thread_count == 0 { configuration.tmdb_metadata_thread_count = 16; }
            if configuration.translation_thread_count == 0 { configuration.translation_thread_count = 8; }
            if configuration.db_max_connections == 0 { configuration.db_max_connections = 50; }
            if configuration.image_download_threads == 0 { configuration.image_download_threads = 24; }
            if configuration.background_task_threads == 0 { configuration.background_task_threads = 12; }
        }
        "ultra" => {
            if configuration.library_scan_thread_count == 0 { configuration.library_scan_thread_count = 16; }
            if configuration.strm_analysis_thread_count == 0 { configuration.strm_analysis_thread_count = 64; }
            if configuration.tmdb_metadata_thread_count == 0 { configuration.tmdb_metadata_thread_count = 32; }
            if configuration.translation_thread_count == 0 { configuration.translation_thread_count = 16; }
            if configuration.db_max_connections == 0 { configuration.db_max_connections = 100; }
            if configuration.image_download_threads == 0 { configuration.image_download_threads = 48; }
            if configuration.background_task_threads == 0 { configuration.background_task_threads = 24; }
        }
        "extreme" => {
            if configuration.library_scan_thread_count == 0 { configuration.library_scan_thread_count = 32; }
            if configuration.strm_analysis_thread_count == 0 { configuration.strm_analysis_thread_count = 128; }
            if configuration.tmdb_metadata_thread_count == 0 { configuration.tmdb_metadata_thread_count = 64; }
            if configuration.translation_thread_count == 0 { configuration.translation_thread_count = 32; }
            if configuration.db_max_connections == 0 { configuration.db_max_connections = 200; }
            if configuration.image_download_threads == 0 { configuration.image_download_threads = 96; }
            if configuration.background_task_threads == 0 { configuration.background_task_threads = 48; }
        }
        _ => { // "medium" (default)
            if configuration.library_scan_thread_count == 0 { configuration.library_scan_thread_count = 2; }
            if configuration.strm_analysis_thread_count == 0 { configuration.strm_analysis_thread_count = 8; }
            if configuration.tmdb_metadata_thread_count == 0 { configuration.tmdb_metadata_thread_count = 4; }
            if configuration.translation_thread_count == 0 { configuration.translation_thread_count = 4; }
            if configuration.db_max_connections == 0 { configuration.db_max_connections = 20; }
            if configuration.image_download_threads == 0 { configuration.image_download_threads = 8; }
            if configuration.background_task_threads == 0 { configuration.background_task_threads = 4; }
        }
    }
}

fn normalize_encoding_options(options: &mut EncodingOptionsDto, config: &Config) {
    options.encoder_location_type = if options.encoder_location_type.eq_ignore_ascii_case("Custom")
    {
        "Custom".to_string()
    } else {
        "System".to_string()
    };

    if options.encoder_location_type == "System" {
        options.encoder_app_path = if config.ffmpeg_path.trim().is_empty() {
            "ffmpeg".to_string()
        } else {
            config.ffmpeg_path.clone()
        };
    } else {
        options.encoder_app_path = options.encoder_app_path.trim().to_string();
    }

    if options.transcoding_temp_path.trim().is_empty() {
        options.transcoding_temp_path = config.transcode_dir.to_string_lossy().to_string();
    } else {
        options.transcoding_temp_path = options.transcoding_temp_path.trim().to_string();
    }

    options.hardware_acceleration_type = options.hardware_acceleration_type.trim().to_string();
    options.vaapi_device = options.vaapi_device.trim().to_string();
    options.h264_preset = options.h264_preset.trim().to_string();
    options.encoding_thread_count = options.encoding_thread_count.clamp(-1, 64);
    options.down_mix_audio_boost = options.down_mix_audio_boost.clamp(0.5, 3.0);
    options.h264_crf = options.h264_crf.clamp(0, 51);
    options.max_transcode_sessions = options.max_transcode_sessions.clamp(1, 64);
}

pub async fn get_system_setting(pool: &sqlx::PgPool, key: &str) -> Result<Option<Value>, AppError> {
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

pub async fn get_setting_value(pool: &sqlx::PgPool, key: &str) -> Result<Option<Value>, AppError> {
    get_system_setting(pool, key).await
}

pub async fn set_setting_value(
    pool: &sqlx::PgPool,
    key: &str,
    value: Value,
) -> Result<(), AppError> {
    set_system_setting(pool, key, value).await
}

pub async fn delete_setting_value(pool: &sqlx::PgPool, key: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM system_settings WHERE key = $1")
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

/// 将电影加入 TMDb collection 对应的 BoxSet（系统设置存储）。
/// 如果该 collection 的 BoxSet 不存在则创建。
pub async fn upsert_movie_into_collection(
    pool: &sqlx::PgPool,
    movie_id: Uuid,
    tmdb_collection_id: i32,
    collection_name: &str,
) -> Result<(), AppError> {
    let coll_uuid = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("tmdb_collection:{tmdb_collection_id}").as_bytes(),
    );
    let coll_id_str = uuid_to_emby_guid(&coll_uuid);
    let movie_id_str = uuid_to_emby_guid(&movie_id);
    let key = format!("collection:{coll_id_str}");

    let existing = get_system_setting(pool, &key).await?;
    let mut coll_value = if let Some(val) = existing {
        val
    } else {
        serde_json::json!({
            "Id": coll_id_str,
            "Name": collection_name,
            "TmdbCollectionId": tmdb_collection_id,
            "ItemIds": []
        })
    };

    // 更新名称（TMDb 可能修改）
    if let Some(obj) = coll_value.as_object_mut() {
        obj.insert("Name".to_string(), serde_json::Value::String(collection_name.to_string()));
    }

    if let Some(item_ids) = coll_value.get_mut("ItemIds").and_then(|v| v.as_array_mut()) {
        let already = item_ids.iter().any(|v| v.as_str() == Some(movie_id_str.as_str()));
        if !already {
            item_ids.push(serde_json::Value::String(movie_id_str));
        }
    }

    set_system_setting(pool, &key, coll_value).await
}

fn normalize_connect_guid_str(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c| c == '{' || c == '}')
        .replace('-', "")
        .to_ascii_uppercase()
}

fn connect_link_payload_matches_target(payload: &Value, target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    let keys = [
        "ConnectUserId",
        "connectUserId",
        "UserId",
        "userId",
        "Id",
        "id",
    ];
    match payload {
        Value::Object(map) => {
            for k in keys {
                if let Some(v) = map.get(k) {
                    if let Some(s) = v.as_str() {
                        if normalize_connect_guid_str(s) == *target {
                            return true;
                        }
                    }
                }
            }
            false
        }
        Value::String(s) => normalize_connect_guid_str(s) == *target,
        _ => false,
    }
}

/// 若 `POST /Users/{id}/Connect/Link` 存入了 `ExchangeToken` / `ConnectAccessKey` / `AccessKey`，
/// 必须与请求头里的 Connect AccessKey 一致；未写入时仅要求 AccessKey 非空（与官方云校验相比为宽松模式，适合自托管）。
pub fn connect_exchange_access_key_allowed(payload: &Value, access_key: &str) -> bool {
    let key = access_key.trim();
    if key.is_empty() {
        return false;
    }
    match payload {
        Value::Object(map) => {
            let expected = map
                .get("ExchangeToken")
                .or_else(|| map.get("ConnectAccessKey"))
                .or_else(|| map.get("AccessKey"))
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty());
            match expected {
                Some(exp) => exp == key,
                None => true,
            }
        }
        _ => true,
    }
}

/// 根据 Emby Connect 的 `ConnectUserId` 查找已绑定 `user_connect_link:{local_id}` 的本地用户。
pub async fn find_user_by_connect_user_id(
    pool: &sqlx::PgPool,
    connect_user_id: &str,
) -> Result<Option<(Uuid, Value)>, AppError> {
    let target = normalize_connect_guid_str(connect_user_id);
    if target.is_empty() {
        return Ok(None);
    }
    // 使用 SQL 直接在 JSONB 值中搜索，避免逐用户 O(n) 查询
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT key, value FROM system_settings \
         WHERE key LIKE 'user_connect_link:%' AND value IS NOT NULL AND value != 'null'::jsonb",
    )
    .fetch_all(pool)
    .await?;

    for (key, payload) in rows {
        if connect_link_payload_matches_target(&payload, &target) {
            let id_str = key.strip_prefix("user_connect_link:").unwrap_or_default();
            if let Ok(user_id) = Uuid::parse_str(id_str) {
                return Ok(Some((user_id, payload)));
            }
        }
    }
    Ok(None)
}

/// PB34-2：删除条目前先扫出落盘的图片/章节图路径，DELETE 成功后逐个 fs::remove_file，
/// 避免 `<static_dir>/items/<uuid>/` 目录里的孤儿文件占用磁盘。
///
/// 容错策略：磁盘删除任何失败仅记录 warn，不阻断 DELETE 已经在 DB 完成的事实
/// （若 DB 删除成功后再因 IO 失败回滚，会让 ON DELETE CASCADE 留下不一致）。
/// 远端 URL（http://、https://）跳过，不参与磁盘清理。
pub async fn delete_media_item(pool: &sqlx::PgPool, item_id: Uuid) -> Result<bool, AppError> {
    // 1) 先把要清理的本地文件路径全部 SELECT 出来
    let image_paths_row = sqlx::query(
        r#"
        SELECT image_primary_path, backdrop_path, logo_path, thumb_path,
               art_path, banner_path, disc_path, backdrop_paths
        FROM media_items
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;
    let chapter_image_paths: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT image_path FROM media_chapters WHERE media_item_id = $1",
    )
    .bind(item_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // 2) 真正 DELETE（关联 streams/chapters/person_roles 等由 ON DELETE CASCADE 处理）
    let result = sqlx::query("DELETE FROM media_items WHERE id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;
    let deleted = result.rows_affected() > 0;
    if !deleted {
        return Ok(false);
    }

    // 3) DELETE 成功后清理磁盘
    let mut to_remove: Vec<String> = Vec::new();
    if let Some(row) = image_paths_row {
        for col in [
            "image_primary_path",
            "backdrop_path",
            "logo_path",
            "thumb_path",
            "art_path",
            "banner_path",
            "disc_path",
        ] {
            if let Ok(Some(path)) = row.try_get::<Option<String>, _>(col) {
                to_remove.push(path);
            }
        }
        if let Ok(paths) = row.try_get::<Vec<String>, _>("backdrop_paths") {
            for p in paths {
                if !p.trim().is_empty() {
                    to_remove.push(p);
                }
            }
        }
    }
    for path in chapter_image_paths.into_iter().flatten() {
        to_remove.push(path);
    }
    for raw in to_remove {
        let trimmed = raw.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
        {
            continue;
        }
        let p = std::path::PathBuf::from(trimmed);
        if let Err(error) = tokio::fs::remove_file(&p).await {
            // ENOENT 已经被删过，不算错；其它错误记 warn
            if !matches!(error.kind(), std::io::ErrorKind::NotFound) {
                tracing::warn!(
                    item_id = %item_id,
                    path = %p.display(),
                    error = %error,
                    "PB34-2：删除条目时清理本地图片失败"
                );
            }
        }
    }
    Ok(true)
}

pub async fn add_media_item_tag(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    tag: &str,
) -> Result<bool, AppError> {
    let normalized = tag.trim();
    if normalized.is_empty() {
        return Ok(false);
    }
    let result = sqlx::query(
        r#"
        UPDATE media_items
        SET tags = (
            SELECT ARRAY(
                SELECT DISTINCT value
                FROM unnest(array_append(COALESCE(tags, ARRAY[]::text[]), $2::text)) AS value
                WHERE btrim(value) <> ''
                ORDER BY value
            )
        ),
        date_modified = now()
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(normalized)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn remove_media_item_tag(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    tag: &str,
) -> Result<bool, AppError> {
    let normalized = tag.trim();
    if normalized.is_empty() {
        return Ok(false);
    }
    let result = sqlx::query(
        r#"
        UPDATE media_items
        SET tags = ARRAY(
            SELECT value
            FROM unnest(COALESCE(tags, ARRAY[]::text[])) AS value
            WHERE lower(value) <> lower($2::text)
            ORDER BY value
        ),
        date_modified = now()
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(normalized)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 来自 Emby "编辑元数据" 表单的用户可写字段集合。
///
/// 所有字段都是 `Option` —— `None` 代表该字段未被客户端提交，数据库保留原值；
/// 数组字段用 `Some(vec![])` 代表"清空"。
#[derive(Debug, Default, Clone)]
pub struct MediaItemEditableFields {
    pub name: Option<String>,
    pub original_title: Option<String>,
    pub sort_name: Option<String>,
    pub overview: Option<String>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub official_rating: Option<String>,
    pub production_year: Option<i32>,
    pub premiere_date: Option<chrono::NaiveDate>,
    pub end_date: Option<chrono::NaiveDate>,
    pub status: Option<String>,
    pub genres: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub studios: Option<Vec<String>>,
    pub production_locations: Option<Vec<String>>,
    pub provider_ids: Option<serde_json::Value>,
    /// PB35-1：用户在编辑面板把某个 provider_id（如 `Tmdb: ""`）置空时，
    /// 视作「删除该 key」语义。这里是一份 key 列表（小写），与 `provider_ids` 互斥分配。
    pub provider_ids_to_remove: Option<Vec<String>>,
    // PB32-1：用户编辑面板可设置的「整体锁」+「字段级锁」+ taglines。
    pub taglines: Option<Vec<String>>,
    pub locked_fields: Option<Vec<String>>,
    pub lock_data: Option<bool>,
}

/// 按 `MediaItemEditableFields` 更新媒体项，只有被显式赋值的字段会被写入。
///
/// 与 `update_media_item_movie_metadata` / `series` 版本不同，这里是"用户在编辑
/// 器里主动改的"字段；数组字段按 `Some(vec![])` 表示清空语义。
pub async fn update_media_item_editable_fields(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    updates: &MediaItemEditableFields,
) -> Result<bool, AppError> {
    let parental_value: Option<i32> = updates
        .official_rating
        .as_deref()
        .and_then(official_rating_to_value);

    let result = sqlx::query(
        r#"
        UPDATE media_items
        SET name             = COALESCE($2, name),
            original_title   = COALESCE($3, original_title),
            sort_name        = COALESCE($4, sort_name),
            overview         = COALESCE($5, overview),
            community_rating = COALESCE($6, community_rating),
            critic_rating    = COALESCE($7, critic_rating),
            official_rating  = COALESCE($8, official_rating),
            parental_rating_value = CASE WHEN $8 IS NOT NULL THEN $18::integer ELSE parental_rating_value END,
            production_year  = COALESCE($9, production_year),
            premiere_date    = COALESCE($10, premiere_date),
            end_date         = COALESCE($11, end_date),
            status           = COALESCE($12, status),
            genres           = CASE WHEN $13::text[] IS NULL THEN genres ELSE $13 END,
            tags             = CASE WHEN $14::text[] IS NULL THEN tags ELSE $14 END,
            studios          = CASE WHEN $15::text[] IS NULL THEN studios ELSE $15 END,
            production_locations = CASE WHEN $16::text[] IS NULL THEN production_locations ELSE $16 END,
            provider_ids     = CASE
                                   WHEN $17::jsonb IS NULL AND $22::text[] IS NULL
                                       THEN provider_ids
                                   WHEN $22::text[] IS NULL
                                       THEN COALESCE(provider_ids, '{}'::jsonb) || $17::jsonb
                                   WHEN $17::jsonb IS NULL
                                       THEN COALESCE(provider_ids, '{}'::jsonb) - $22::text[]
                                   ELSE
                                       (COALESCE(provider_ids, '{}'::jsonb) || $17::jsonb) - $22::text[]
                               END,
            taglines         = CASE WHEN $19::text[] IS NULL THEN taglines ELSE $19 END,
            locked_fields    = CASE WHEN $20::text[] IS NULL THEN locked_fields ELSE $20 END,
            lock_data        = COALESCE($21, lock_data),
            date_modified    = now()
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(updates.name.as_deref())
    .bind(updates.original_title.as_deref())
    .bind(updates.sort_name.as_deref())
    .bind(updates.overview.as_deref())
    .bind(updates.community_rating)
    .bind(updates.critic_rating)
    .bind(updates.official_rating.as_deref())
    .bind(updates.production_year)
    .bind(updates.premiere_date)
    .bind(updates.end_date)
    .bind(updates.status.as_deref())
    .bind(updates.genres.as_deref())
    .bind(updates.tags.as_deref())
    .bind(updates.studios.as_deref())
    .bind(updates.production_locations.as_deref())
    .bind(updates.provider_ids.as_ref())
    .bind(parental_value)
    .bind(updates.taglines.as_deref())
    .bind(updates.locked_fields.as_deref())
    .bind(updates.lock_data)
    // PB35-1：要从 provider_ids 删除的 key 列表（用户传空字符串时收集）
    .bind(updates.provider_ids_to_remove.as_deref())
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// 将外部 provider ids 合并到媒体项已有 provider_ids 上。
///
/// 使用 jsonb `||` 运算符让新字段覆盖同名旧字段，避免破坏其他 provider 的 id。
pub async fn update_media_item_provider_ids(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    provider_ids: &serde_json::Value,
) -> Result<bool, AppError> {
    if !provider_ids.is_object() {
        return Ok(false);
    }
    let result = sqlx::query(
        r#"
        UPDATE media_items
        SET provider_ids = COALESCE(provider_ids, '{}'::jsonb) || $2::jsonb,
            date_modified = now()
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(provider_ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_genres(
    pool: &sqlx::PgPool,
    start_index: Option<i32>,
    limit: Option<i32>,
    user_id: Option<Uuid>,
) -> Result<(Vec<GenreDto>, i64), AppError> {
    let offset = start_index.unwrap_or(0).max(0) as i64;
    let cap = limit.unwrap_or(200).clamp(1, 500) as i64;

    let allowed_library_ids = if let Some(uid) = user_id {
        effective_library_filter_for_user(pool, uid).await?
    } else {
        None
    };

    let total: i64 = if let Some(ref lib_ids) = allowed_library_ids {
        if lib_ids.is_empty() {
            return Ok((Vec::new(), 0));
        }
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT DISTINCT unnest(genres) as name FROM media_items WHERE array_length(genres, 1) > 0 AND library_id = ANY($1)) t"
        )
        .bind(lib_ids)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT DISTINCT unnest(genres) as name FROM media_items WHERE array_length(genres, 1) > 0) t"
        )
        .fetch_one(pool)
        .await?
    };

    let rows = if let Some(ref lib_ids) = allowed_library_ids {
        sqlx::query(
            "SELECT DISTINCT unnest(genres) as name FROM media_items WHERE array_length(genres, 1) > 0 AND library_id = ANY($3) ORDER BY name OFFSET $1 LIMIT $2"
        )
        .bind(offset)
        .bind(cap)
        .bind(lib_ids)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT DISTINCT unnest(genres) as name FROM media_items WHERE array_length(genres, 1) > 0 ORDER BY name OFFSET $1 LIMIT $2"
        )
        .bind(offset)
        .bind(cap)
        .fetch_all(pool)
        .await?
    };

    let genres: Vec<GenreDto> = rows
        .iter()
        .map(|row| GenreDto {
            name: row.get::<String, _>("name"),
            id: None,
            image_tags: None,
        })
        .collect();

    Ok((genres, total))
}

pub async fn get_items_by_genre(
    pool: &sqlx::PgPool,
    genre_name: &str,
    server_id: Uuid,
    start_index: Option<i32>,
    limit: Option<i32>,
    user_id: Option<Uuid>,
) -> Result<Vec<BaseItemDto>, AppError> {
    let allowed_library_ids = if let Some(uid) = user_id {
        effective_library_filter_for_user(pool, uid).await?
    } else {
        None
    };

    let (query, use_lib_filter) = if let Some(ref lib_ids) = allowed_library_ids {
        if lib_ids.is_empty() {
            return Ok(Vec::new());
        }
        (
            r#"
            SELECT
                id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
                overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
                premiere_date, status, end_date, air_days, air_time, series_name, season_name,
                index_number, index_number_end, parent_index_number, provider_ids, genres,
                studios, tags, production_locations,
                width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
                logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
                date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
                display_order, 0::bigint AS total_count
            FROM media_items
            WHERE $1 = ANY(genres) AND library_id = ANY($4)
            ORDER BY sort_name
            OFFSET $2 LIMIT $3
            "#,
            true,
        )
    } else {
        (
            r#"
            SELECT
                id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
                overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
                premiere_date, status, end_date, air_days, air_time, series_name, season_name,
                index_number, index_number_end, parent_index_number, provider_ids, genres,
                studios, tags, production_locations,
                width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
                logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
                date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
                display_order, 0::bigint AS total_count
            FROM media_items
            WHERE $1 = ANY(genres)
            ORDER BY sort_name
            OFFSET $2 LIMIT $3
            "#,
            false,
        )
    };

    let items = if use_lib_filter {
        sqlx::query_as::<_, DbMediaItem>(query)
            .bind(genre_name)
            .bind(start_index.unwrap_or(0).max(0) as i64)
            .bind(limit.unwrap_or(100).clamp(1, 200) as i64)
            .bind(&allowed_library_ids.unwrap())
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, DbMediaItem>(query)
            .bind(genre_name)
            .bind(start_index.unwrap_or(0).max(0) as i64)
            .bind(limit.unwrap_or(100).clamp(1, 200) as i64)
            .fetch_all(pool)
            .await?
    };

    let item_dtos: Vec<BaseItemDto> = items
        .iter()
        .map(|item| media_item_to_dto_for_list(item, server_id, None, DtoCountPrefetch::default()))
        .collect();

    Ok(item_dtos)
}

// 人物相关函数

/// 将 `DbPerson` 转换为对外 `PersonDto`，统一处理 image_tags / 出生地 / 主页 等字段。
pub(crate) fn db_person_to_dto(person: DbPerson) -> PersonDto {
    let provider_ids: Option<std::collections::HashMap<String, String>> =
        if person.provider_ids.is_null() {
            None
        } else {
            serde_json::from_value(person.provider_ids).ok()
        };

    let primary_image_tag = person
        .primary_image_path
        .as_ref()
        .map(|_| person.updated_at.timestamp().to_string());
    let backdrop_image_tag = person
        .backdrop_image_path
        .as_ref()
        .map(|_| person.updated_at.timestamp().to_string());
    let image_tags = primary_image_tag.as_ref().map(|tag| {
        let mut tags = std::collections::HashMap::new();
        tags.insert("Primary".to_string(), tag.clone());
        if backdrop_image_tag.is_some() {
            tags.insert("Backdrop".to_string(), backdrop_image_tag.clone().unwrap());
        }
        tags
    });

    let production_locations = person
        .place_of_birth
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .map(|s| vec![s.clone()]);

    PersonDto {
        name: person.name,
        id: uuid_to_emby_guid(&person.id),
        role: None,
        person_type: None,
        primary_image_tag,
        sort_name: person.sort_name,
        overview: person.overview,
        external_url: person.external_url,
        premiere_date: person.premiere_date.map(|dt| dt.to_rfc3339()),
        end_date: person.death_date.map(|dt| dt.to_rfc3339()),
        production_year: person.production_year,
        production_locations,
        homepage_url: person.homepage_url,
        image_tags,
        provider_ids,
        favorite: None,
        backdrop_image_tag,
    }
}

pub async fn get_persons(
    pool: &sqlx::PgPool,
    start_index: Option<i32>,
    limit: Option<i32>,
    name_starts_with: Option<String>,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<(Vec<PersonDto>, i64), AppError> {
    let start_index = start_index.unwrap_or(0);
    let limit = limit.unwrap_or(100).min(200);

    // PB27：受限用户的 /Persons 列表必须只包含「在用户可见库里参演过的人物」，否则
    // 隐藏库参演人物会通过人物列表泄露。`Some(&[])` 直接返回空集（用户当前无任何
    // 可见库时），`None` 走原有「persons 全表 ORDER BY name」快路径。
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok((Vec::new(), 0));
    }
    let allowed_vec: Option<Vec<Uuid>> = allowed_library_ids.map(<[Uuid]>::to_vec);

    let name_pattern_opt = name_starts_with.as_ref().map(|prefix| format!("{}%", prefix));

    let (persons, total) = if let Some(allowed) = allowed_vec.as_ref() {
        // 受限路径：JOIN person_roles + media_items 做 EXISTS 子查询过滤，再按 name 排序。
        // EXISTS 比 IN 在大表 + 部分库白名单时执行计划更稳定。
        let sql_total = r#"
            SELECT COUNT(DISTINCT p.id)
            FROM persons p
            WHERE ($2::text IS NULL OR p.name ILIKE $2)
              AND EXISTS (
                  SELECT 1 FROM person_roles pr
                  INNER JOIN media_items mi ON mi.id = pr.media_item_id
                  WHERE pr.person_id = p.id
                    AND mi.library_id = ANY($1)
              )
        "#;
        let total: i64 = sqlx::query_scalar(sql_total)
            .bind(allowed)
            .bind(name_pattern_opt.as_deref())
            .fetch_one(pool)
            .await?;
        let sql_rows = r#"
            SELECT DISTINCT p.*
            FROM persons p
            WHERE ($2::text IS NULL OR p.name ILIKE $2)
              AND EXISTS (
                  SELECT 1 FROM person_roles pr
                  INNER JOIN media_items mi ON mi.id = pr.media_item_id
                  WHERE pr.person_id = p.id
                    AND mi.library_id = ANY($1)
              )
            ORDER BY p.name
            LIMIT $3 OFFSET $4
        "#;
        let rows = sqlx::query_as::<_, DbPerson>(sql_rows)
            .bind(allowed)
            .bind(name_pattern_opt.as_deref())
            .bind(limit as i64)
            .bind(start_index as i64)
            .fetch_all(pool)
            .await?;
        (rows, total)
    } else if let Some(name_pattern) = name_pattern_opt.as_ref() {
        // admin / 全可见 + name prefix
        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM persons WHERE name ILIKE $1",
        )
        .bind(name_pattern)
        .fetch_one(pool)
        .await?;
        let rows = sqlx::query_as::<_, DbPerson>(
            r#"
            SELECT *
            FROM persons
            WHERE name ILIKE $1
            ORDER BY name
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(name_pattern)
        .bind(limit as i64)
        .bind(start_index as i64)
        .fetch_all(pool)
        .await?;
        (rows, total)
    } else {
        // admin / 全可见 + 无 prefix：维持原有最便宜路径。
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM persons")
            .fetch_one(pool)
            .await?;
        let rows = sqlx::query_as::<_, DbPerson>(
            r#"
            SELECT *
            FROM persons
            ORDER BY name
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit as i64)
        .bind(start_index as i64)
        .fetch_all(pool)
        .await?;
        (rows, total)
    };

    let person_dtos = persons.into_iter().map(db_person_to_dto).collect();

    Ok((person_dtos, total))
}

/// PB27：判定某人物是否在「用户可见库」里有参演条目；admin 路径直接传 `None` 跳过校验。
pub async fn person_visible_to_user(
    pool: &sqlx::PgPool,
    person_id: Uuid,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<bool, AppError> {
    let allowed = match allowed_library_ids {
        None => return Ok(true),
        Some(ids) if ids.is_empty() => return Ok(false),
        Some(ids) => ids,
    };
    let exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM person_roles pr
            INNER JOIN media_items mi ON mi.id = pr.media_item_id
            WHERE pr.person_id = $1
              AND mi.library_id = ANY($2)
        )
        "#,
    )
    .bind(person_id)
    .bind(allowed)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

pub async fn get_person_by_uuid(
    pool: &sqlx::PgPool,
    person_id: Uuid,
) -> Result<Option<PersonDto>, AppError> {
    let person = sqlx::query_as::<_, DbPerson>(
        r#"
        SELECT *
        FROM persons
        WHERE id = $1
        "#,
    )
    .bind(person_id)
    .fetch_optional(pool)
    .await?;

    Ok(person.map(db_person_to_dto))
}

pub async fn get_person_by_name(pool: &sqlx::PgPool, name: &str) -> Result<PersonDto, AppError> {
    let person = sqlx::query_as::<_, DbPerson>(
        r#"
        SELECT *
        FROM persons
        WHERE name = $1
        LIMIT 1
        "#,
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    person
        .map(db_person_to_dto)
        .ok_or_else(|| AppError::NotFound(format!("Person not found: {}", name)))
}

pub async fn get_person_image_path(
    pool: &sqlx::PgPool,
    person_id_or_name: &str,
    image_type: &str,
) -> Result<Option<String>, AppError> {
    #[derive(sqlx::FromRow)]
    struct PersonImagePaths {
        primary_image_path: Option<String>,
        backdrop_image_path: Option<String>,
        logo_image_path: Option<String>,
    }

    let person = if let Ok(person_id) = emby_id_to_uuid(person_id_or_name) {
        sqlx::query_as::<_, PersonImagePaths>(
            "SELECT primary_image_path, backdrop_image_path, logo_image_path FROM persons WHERE id = $1 LIMIT 1",
        )
        .bind(person_id)
        .fetch_optional(pool)
        .await?
    } else {
        sqlx::query_as::<_, PersonImagePaths>(
            "SELECT primary_image_path, backdrop_image_path, logo_image_path FROM persons WHERE name = $1 LIMIT 1",
        )
        .bind(person_id_or_name)
        .fetch_optional(pool)
        .await?
    };

    let Some(person) = person else {
        return Ok(None);
    };

    Ok(match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => person.backdrop_image_path,
        "logo" => person.logo_image_path,
        "thumb" => person.backdrop_image_path.or(person.primary_image_path),
        _ => person.primary_image_path,
    })
}

pub async fn get_genre_image_path(
    pool: &sqlx::PgPool,
    genre_name: &str,
    image_type: &str,
) -> Result<Option<String>, AppError> {
    let image_column = match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => "backdrop_path",
        "logo" => "logo_path",
        "thumb" => "thumb_path",
        _ => "image_primary_path",
    };
    let fallback_column = if image_column == "thumb_path" {
        ", backdrop_path, image_primary_path"
    } else {
        ""
    };
    let query = format!(
        r#"
        SELECT {image_column}{fallback_column}
        FROM media_items
        WHERE $1 = ANY(genres)
          AND ({image_column} IS NOT NULL{fallback_condition})
        ORDER BY date_modified DESC
        LIMIT 1
        "#,
        fallback_condition = if image_column == "thumb_path" {
            " OR backdrop_path IS NOT NULL OR image_primary_path IS NOT NULL"
        } else {
            ""
        },
    );

    let Some(row) = sqlx::query(&query)
        .bind(genre_name)
        .fetch_optional(pool)
        .await?
    else {
        return Ok(None);
    };

    if let Some(path) = row.try_get::<Option<String>, _>(image_column)? {
        return Ok(Some(path));
    }
    if image_column == "thumb_path" {
        if let Some(path) = row.try_get::<Option<String>, _>("backdrop_path")? {
            return Ok(Some(path));
        }
        if let Some(path) = row.try_get::<Option<String>, _>("image_primary_path")? {
            return Ok(Some(path));
        }
    }

    Ok(None)
}

pub async fn get_items_by_person(
    pool: &sqlx::PgPool,
    person_id_or_name: &str,
    server_id: Uuid,
    start_index: Option<i32>,
    limit: Option<i32>,
    user_id: Option<Uuid>,
) -> Result<Vec<BaseItemDto>, AppError> {
    let person_id = if let Ok(uuid) = emby_id_to_uuid(person_id_or_name) {
        uuid
    } else {
        let Some(id) = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT id
            FROM persons
            WHERE name = $1
            LIMIT 1
            "#,
        )
        .bind(person_id_or_name)
        .fetch_optional(pool)
        .await?
        else {
            return Ok(Vec::new());
        };
        id
    };

    let allowed_library_ids = if let Some(uid) = user_id {
        effective_library_filter_for_user(pool, uid).await?
    } else {
        None
    };

    let rows = if let Some(ref lib_ids) = allowed_library_ids {
        if lib_ids.is_empty() {
            return Ok(Vec::new());
        }
        sqlx::query_as::<_, DbMediaItem>(
            r#"
            SELECT
                mi.id, mi.parent_id, mi.name, mi.original_title, mi.sort_name, mi.item_type, mi.media_type, mi.path, mi.container,
                mi.overview, mi.production_year, mi.official_rating, mi.community_rating, mi.critic_rating, mi.runtime_ticks,
                mi.premiere_date, mi.status, mi.end_date, mi.air_days, mi.air_time, mi.series_name, mi.season_name,
                mi.index_number, mi.index_number_end, mi.parent_index_number, mi.provider_ids, mi.genres,
                mi.studios, mi.tags, mi.production_locations,
                mi.width, mi.height, mi.bit_rate, mi.size, mi.video_codec, mi.audio_codec, mi.image_primary_path, mi.backdrop_path,
                mi.logo_path, mi.thumb_path, mi.art_path, mi.banner_path, mi.disc_path, mi.backdrop_paths, mi.remote_trailers,
                mi.date_created, mi.date_modified, mi.image_blur_hashes, mi.series_id, mi.taglines, mi.locked_fields, mi.lock_data,
                mi.display_order, 0::bigint AS total_count
            FROM media_items mi
            WHERE mi.id IN (
                SELECT DISTINCT pr.media_item_id
                FROM person_roles pr
                WHERE pr.person_id = $1
            )
            AND mi.library_id = ANY($4)
            ORDER BY mi.sort_name
            OFFSET $2 LIMIT $3
            "#,
        )
        .bind(person_id)
        .bind(start_index.unwrap_or(0).max(0) as i64)
        .bind(limit.unwrap_or(100).clamp(1, 200) as i64)
        .bind(lib_ids)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, DbMediaItem>(
            r#"
            SELECT
                mi.id, mi.parent_id, mi.name, mi.original_title, mi.sort_name, mi.item_type, mi.media_type, mi.path, mi.container,
                mi.overview, mi.production_year, mi.official_rating, mi.community_rating, mi.critic_rating, mi.runtime_ticks,
                mi.premiere_date, mi.status, mi.end_date, mi.air_days, mi.air_time, mi.series_name, mi.season_name,
                mi.index_number, mi.index_number_end, mi.parent_index_number, mi.provider_ids, mi.genres,
                mi.studios, mi.tags, mi.production_locations,
                mi.width, mi.height, mi.bit_rate, mi.size, mi.video_codec, mi.audio_codec, mi.image_primary_path, mi.backdrop_path,
                mi.logo_path, mi.thumb_path, mi.art_path, mi.banner_path, mi.disc_path, mi.backdrop_paths, mi.remote_trailers,
                mi.date_created, mi.date_modified, mi.image_blur_hashes, mi.series_id, mi.taglines, mi.locked_fields, mi.lock_data,
                mi.display_order, 0::bigint AS total_count
            FROM media_items mi
            WHERE mi.id IN (
                SELECT DISTINCT pr.media_item_id
                FROM person_roles pr
                WHERE pr.person_id = $1
            )
            ORDER BY mi.sort_name
            OFFSET $2 LIMIT $3
            "#,
        )
        .bind(person_id)
        .bind(start_index.unwrap_or(0).max(0) as i64)
        .bind(limit.unwrap_or(100).clamp(1, 200) as i64)
        .fetch_all(pool)
        .await?
    };

    let items: Vec<BaseItemDto> = rows
        .iter()
        .map(|item| media_item_to_dto_for_list(item, server_id, None, DtoCountPrefetch::default()))
        .collect();

    Ok(items)
}

pub async fn get_person_by_external_id(
    pool: &sqlx::PgPool,
    provider: &str,
    external_id: &str,
) -> Result<Option<DbPerson>, AppError> {
    let aliases: &[&str] =
        if provider.eq_ignore_ascii_case("tmdb") || provider.eq_ignore_ascii_case("themoviedb") {
            &["Tmdb", "TMDb", "tmdb", "TheMovieDb"]
        } else if provider.eq_ignore_ascii_case("imdb") {
            &["Imdb", "IMDb", "imdb", "Imdb"]
        } else {
            &[]
        };

    if aliases.is_empty() {
        return Ok(sqlx::query_as::<_, DbPerson>(
            r#"
            SELECT *
            FROM persons
            WHERE provider_ids->>$1 = $2
            LIMIT 1
            "#,
        )
        .bind(provider)
        .bind(external_id)
        .fetch_optional(pool)
        .await?);
    }

    Ok(sqlx::query_as::<_, DbPerson>(
        r#"
        SELECT *
        FROM persons
        WHERE provider_ids->>$1 = $5
           OR provider_ids->>$2 = $5
           OR provider_ids->>$3 = $5
           OR provider_ids->>$4 = $5
        LIMIT 1
        "#,
    )
    .bind(aliases[0])
    .bind(aliases[1])
    .bind(aliases[2])
    .bind(aliases[3])
    .bind(external_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_person(pool: &sqlx::PgPool, person: &DbPerson) -> Result<Uuid, AppError> {
    // 这里之前直接绑定 `person.id`，一旦 id 与既有行冲突（历史数据 / 老版本生成的 v5 UUID）
    // 或者 (name, sort_name) 已经被 upsert 过，就会报 persons_pkey / persons_name_sort_name_key，
    // 从而炸掉外层扫描事务。现在统一走 ON CONFLICT (name, sort_name) 的幂等路径：
    // 新插入让数据库用 gen_random_uuid() 自己产 id，冲突时直接 DO UPDATE 并复用已有的 id。
    let result = sqlx::query(
        r#"
        INSERT INTO persons (
            name, sort_name, overview, external_url,
            provider_ids, premiere_date, production_year,
            primary_image_path, backdrop_image_path, logo_image_path,
            favorite_count, created_at, updated_at,
            death_date, place_of_birth, homepage_url, metadata_synced_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        ON CONFLICT (name, sort_name)
        DO UPDATE SET
            overview = COALESCE(EXCLUDED.overview, persons.overview),
            external_url = COALESCE(EXCLUDED.external_url, persons.external_url),
            provider_ids = CASE
                WHEN persons.provider_ids = '{}'::jsonb THEN EXCLUDED.provider_ids
                ELSE persons.provider_ids || EXCLUDED.provider_ids
            END,
            premiere_date = COALESCE(EXCLUDED.premiere_date, persons.premiere_date),
            production_year = COALESCE(EXCLUDED.production_year, persons.production_year),
            primary_image_path = COALESCE(persons.primary_image_path, EXCLUDED.primary_image_path),
            backdrop_image_path = COALESCE(persons.backdrop_image_path, EXCLUDED.backdrop_image_path),
            logo_image_path = COALESCE(persons.logo_image_path, EXCLUDED.logo_image_path),
            death_date = COALESCE(EXCLUDED.death_date, persons.death_date),
            place_of_birth = COALESCE(EXCLUDED.place_of_birth, persons.place_of_birth),
            homepage_url = COALESCE(EXCLUDED.homepage_url, persons.homepage_url),
            metadata_synced_at = COALESCE(EXCLUDED.metadata_synced_at, persons.metadata_synced_at),
            updated_at = EXCLUDED.updated_at
        RETURNING id
        "#,
    )
    .bind(&person.name)
    .bind(&person.sort_name)
    .bind(&person.overview)
    .bind(&person.external_url)
    .bind(&person.provider_ids)
    .bind(person.premiere_date)
    .bind(person.production_year)
    .bind(&person.primary_image_path)
    .bind(&person.backdrop_image_path)
    .bind(&person.logo_image_path)
    .bind(person.favorite_count)
    .bind(person.created_at)
    .bind(person.updated_at)
    .bind(person.death_date)
    .bind(&person.place_of_birth)
    .bind(&person.homepage_url)
    .bind(person.metadata_synced_at)
    .fetch_one(pool)
    .await?;

    Ok(result.get(0))
}

pub async fn update_person(
    pool: &sqlx::PgPool,
    person_id: Uuid,
    person: &DbPerson,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE persons
        SET name = $2,
            sort_name = $3,
            overview = COALESCE($4, persons.overview),
            external_url = COALESCE($5, persons.external_url),
            provider_ids = CASE
                WHEN persons.provider_ids = '{}'::jsonb THEN $6
                ELSE persons.provider_ids || COALESCE($6, '{}'::jsonb)
            END,
            premiere_date = COALESCE($7, persons.premiere_date),
            production_year = COALESCE($8, persons.production_year),
            primary_image_path = COALESCE($9, persons.primary_image_path),
            backdrop_image_path = COALESCE($10, persons.backdrop_image_path),
            logo_image_path = COALESCE($11, persons.logo_image_path),
            death_date = COALESCE($13, persons.death_date),
            place_of_birth = COALESCE($14, persons.place_of_birth),
            homepage_url = COALESCE($15, persons.homepage_url),
            metadata_synced_at = $16,
            updated_at = $12
        WHERE id = $1
        "#,
    )
    .bind(person_id)
    .bind(&person.name)
    .bind(&person.sort_name)
    .bind(&person.overview)
    .bind(&person.external_url)
    .bind(&person.provider_ids)
    .bind(person.premiere_date)
    .bind(person.production_year)
    .bind(&person.primary_image_path)
    .bind(&person.backdrop_image_path)
    .bind(&person.logo_image_path)
    .bind(chrono::Utc::now())
    .bind(person.death_date)
    .bind(&person.place_of_birth)
    .bind(&person.homepage_url)
    .bind(person.metadata_synced_at)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn upsert_person_from_nfo(
    pool: &sqlx::PgPool,
    name: &str,
    provider_ids: Value,
    primary_image_path: Option<&Path>,
) -> Result<Uuid, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("人物名称不能为空".to_string()));
    }

    let sort_name = name.to_lowercase();
    let image_text = primary_image_path.map(|value| value.to_string_lossy().to_string());
    let result = sqlx::query(
        r#"
        INSERT INTO persons (
            name, sort_name, provider_ids, primary_image_path, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, now(), now())
        ON CONFLICT (name, sort_name)
        DO UPDATE SET
            provider_ids = CASE
                WHEN persons.provider_ids = '{}'::jsonb THEN EXCLUDED.provider_ids
                ELSE persons.provider_ids || EXCLUDED.provider_ids
            END,
            primary_image_path = COALESCE(persons.primary_image_path, EXCLUDED.primary_image_path),
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(sort_name)
    .bind(provider_ids)
    .bind(image_text)
    .fetch_one(pool)
    .await?;

    Ok(result.get(0))
}

pub async fn upsert_person_reference(
    pool: &sqlx::PgPool,
    name: &str,
    provider_ids: Value,
    primary_image_path: Option<&str>,
    external_url: Option<&str>,
) -> Result<Uuid, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("人物名称不能为空".to_string()));
    }

    let sort_name = name.to_lowercase();
    let result = sqlx::query(
        r#"
        INSERT INTO persons (
            name, sort_name, provider_ids, primary_image_path, external_url, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, now(), now())
        ON CONFLICT (name, sort_name)
        DO UPDATE SET
            provider_ids = CASE
                WHEN persons.provider_ids = '{}'::jsonb THEN EXCLUDED.provider_ids
                ELSE persons.provider_ids || EXCLUDED.provider_ids
            END,
            primary_image_path = COALESCE(persons.primary_image_path, EXCLUDED.primary_image_path),
            external_url = COALESCE(persons.external_url, EXCLUDED.external_url),
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(name)
    .bind(sort_name)
    .bind(provider_ids)
    .bind(primary_image_path)
    .bind(external_url)
    .fetch_one(pool)
    .await?;

    Ok(result.get(0))
}

/// PB32-2：编辑面板「整段替换演职员」用：先按 `media_item_id` 删除全部 person_roles，
/// 再逐一 upsert 用户提交的人员列表。`role_type` 默认 `Actor`，传入的 `Type` 大小写不敏感。
pub async fn replace_item_people_from_edit(
    pool: &sqlx::PgPool,
    media_item_id: Uuid,
    people: &[crate::routes::items::UpdateItemPerson],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM person_roles WHERE media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut *tx)
        .await?;
    let mut sort_order = 0i32;
    for entry in people {
        let name = entry
            .name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(name) = name else { continue };
        let role_type = match entry
            .person_type
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            Some(s) => match s.to_ascii_lowercase().as_str() {
                "actor" => "Actor".to_string(),
                "director" => "Director".to_string(),
                "writer" => "Writer".to_string(),
                "producer" => "Producer".to_string(),
                "gueststar" => "GuestStar".to_string(),
                "composer" => "Composer".to_string(),
                _ => s.to_string(),
            },
            None => "Actor".to_string(),
        };
        let provider_ids = entry
            .provider_ids
            .clone()
            .unwrap_or_else(|| Value::Object(serde_json::Map::new()));
        let sort_name = name.to_lowercase();
        let person_row = sqlx::query(
            r#"
            INSERT INTO persons (
                name, sort_name, provider_ids, created_at, updated_at
            )
            VALUES ($1, $2, $3, now(), now())
            ON CONFLICT (name, sort_name)
            DO UPDATE SET
                provider_ids = CASE
                    WHEN persons.provider_ids = '{}'::jsonb THEN EXCLUDED.provider_ids
                    ELSE persons.provider_ids || EXCLUDED.provider_ids
                END,
                updated_at = now()
            RETURNING id
            "#,
        )
        .bind(name)
        .bind(sort_name)
        .bind(provider_ids)
        .fetch_one(&mut *tx)
        .await?;
        let person_id: Uuid = person_row.get(0);

        let normalized_role = entry
            .role
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let role_id = Uuid::new_v5(
            &media_item_id,
            format!(
                "person-role:{person_id}:{role_type}:{}",
                normalized_role.unwrap_or("<none>")
            )
            .as_bytes(),
        );
        sqlx::query(
            r#"
            INSERT INTO person_roles (
                id, person_id, media_item_id, role_type, role, sort_order, created_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, now(), now())
            ON CONFLICT (id)
            DO UPDATE SET
                sort_order = EXCLUDED.sort_order,
                role       = EXCLUDED.role,
                updated_at = now()
            "#,
        )
        .bind(role_id)
        .bind(person_id)
        .bind(media_item_id)
        .bind(role_type.as_str())
        .bind(normalized_role)
        .bind(sort_order)
        .execute(&mut *tx)
        .await?;
        sort_order = sort_order.saturating_add(1);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn upsert_person_role(
    pool: &sqlx::PgPool,
    person_id: Uuid,
    media_item_id: Uuid,
    role_type: &str,
    role: Option<&str>,
    sort_order: i32,
) -> Result<(), AppError> {
    let normalized_role = role.map(str::trim).filter(|value| !value.is_empty());
    let role_key = normalized_role.unwrap_or("<none>");
    // PB35-5 (P3-2)：根据 Emby 习惯，把前 5 位 Actor 视作 "Featured"，
    // 用于客户端"主演 / Top Cast"区块优先排序。
    let is_featured = role_type.eq_ignore_ascii_case("Actor") && sort_order < 5;

    let updated = sqlx::query(
        r#"
        UPDATE person_roles
        SET
            role = $6,
            sort_order = $5,
            is_featured = $7,
            updated_at = now()
        WHERE person_id = $1
          AND media_item_id = $2
          AND role_type = $3
          AND COALESCE(NULLIF(btrim(role), ''), '<none>') = $4
        "#,
    )
    .bind(person_id)
    .bind(media_item_id)
    .bind(role_type)
    .bind(role_key)
    .bind(sort_order)
    .bind(normalized_role)
    .bind(is_featured)
    .execute(pool)
    .await?;

    if updated.rows_affected() > 0 {
        return Ok(());
    }

    let id = Uuid::new_v5(
        &media_item_id,
        format!("person-role:{person_id}:{role_type}:{role_key}").as_bytes(),
    );

    sqlx::query(
        r#"
        INSERT INTO person_roles (
            id, person_id, media_item_id, role_type, role, sort_order, is_featured, created_at, updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())
        ON CONFLICT (id)
        DO UPDATE SET
            sort_order = EXCLUDED.sort_order,
            is_featured = EXCLUDED.is_featured,
            updated_at = now()
        "#,
    )
    .bind(id)
    .bind(person_id)
    .bind(media_item_id)
    .bind(role_type)
    .bind(normalized_role)
    .bind(sort_order)
    .bind(is_featured)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_user_by_name(pool: &sqlx::PgPool, name: &str) -> Result<Option<DbUser>, AppError> {
    // 输入端做和注册一样的轻量规范化（trim Unicode 空白 + NFC），保证：
    // 1. 第三方 client 多带个全角空格 / `é` 的 NFD 形式也能匹配到正常入库用户；
    // 2. 与 `create_user` 入库前的规范化对齐，不会出现"创建成功但登录查不到"的诡异。
    // 不在这里抛错（让登录路径正常 401，而不是返回 400 暴露用户名校验细节）。
    let normalized = crate::username::normalize_for_lookup(name);
    Ok(sqlx::query_as::<_, DbUser>(
        r#"
        SELECT id, name, password_hash, is_admin, is_hidden, is_disabled, policy,
               configuration, primary_image_path, backdrop_image_path, logo_image_path, date_modified,
               easy_password_hash, created_at,
               legacy_password_format, legacy_password_hash
        FROM users
        WHERE lower(name) = lower($1)
        "#,
    )
    .bind(&normalized)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_user_by_id(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<DbUser>, AppError> {
    Ok(sqlx::query_as::<_, DbUser>(
        r#"
        SELECT id, name, password_hash, is_admin, is_hidden, is_disabled, policy,
               configuration, primary_image_path, backdrop_image_path, logo_image_path, date_modified,
               easy_password_hash, created_at,
               legacy_password_format, legacy_password_hash
        FROM users
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_user(
    pool: &sqlx::PgPool,
    name: &str,
    password: Option<&str>,
    copy_from_user_id: Option<Uuid>,
) -> Result<DbUser, AppError> {
    // 集中校验：trim Unicode 空白 + NFC + 拒绝控制 / 零宽 / 双向控制字符 + 长度上限。
    // 失败的 BadRequest 文案给客户端看，Sakura embyboss 等 bot 可直接转发给终端用户。
    let name = crate::username::normalize_and_validate(name)?;

    if get_user_by_name(pool, &name).await?.is_some() {
        return Err(AppError::BadRequest("用户已存在".to_string()));
    }

    let id = Uuid::new_v4();
    let password_hash = security::hash_password(password.map(str::trim).unwrap_or_default())?;
    let (policy, configuration) = if let Some(copy_from_user_id) = copy_from_user_id {
        let source = get_user_by_id(pool, copy_from_user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("要复制的用户不存在".to_string()))?;
        let mut policy = user_policy_from_value(&source.policy);
        policy.is_administrator = false;
        policy.is_hidden = false;
        policy.is_disabled = false;
        policy.invalid_login_attempt_count = 0;
        (
            serde_json::to_value(policy)?,
            if source.configuration.is_null() {
                serde_json::to_value(UserConfigurationDto::default())?
            } else {
                source.configuration
            },
        )
    } else {
        (
            serde_json::to_value(UserPolicyDto::default())?,
            serde_json::to_value(UserConfigurationDto::default())?,
        )
    };

    sqlx::query(
        r#"
        INSERT INTO users (id, name, password_hash, is_admin, is_hidden, is_disabled, policy, configuration, date_modified)
        VALUES ($1, $2, $3, false, false, false, $4, $5, now())
        "#,
    )
    .bind(id)
    .bind(name)
    .bind(password_hash)
    .bind(policy)
    .bind(configuration)
    .execute(pool)
    .await?;

    get_user_by_id(pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("创建用户后无法读取用户".to_string()))
}

pub async fn user_last_activity(pool: &sqlx::PgPool, user_id: Uuid) -> Result<Option<DateTime<Utc>>, AppError> {
    let row: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT MAX(last_activity_at) FROM sessions WHERE user_id = $1",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn delete_user(pool: &sqlx::PgPool, user_id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
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
    // 写新密码时一并清掉 legacy 字段，确保旧版 SHA1 校验路径不会再被触发。
    sqlx::query(
        r#"
        UPDATE users
           SET password_hash = $1,
               legacy_password_format = NULL,
               legacy_password_hash = NULL,
               date_modified = now()
         WHERE id = $2
        "#,
    )
    .bind(password_hash)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

/// 直接覆盖 `password_hash`（不做长度/格式校验，调用方自己保证传的是
/// `security::hash_password` 的输出）。
///
/// 用于"管理员重置密码"等流程：
/// - 写一个永远无法登录的占位 Argon2 hash 作为"清密码"语义；
/// - 同时清掉 legacy 字段，避免老 SHA1 仍然可用。
pub async fn set_user_password_hash(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    new_hash: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE users
           SET password_hash = $1,
               legacy_password_format = NULL,
               legacy_password_hash = NULL,
               date_modified = now()
         WHERE id = $2
        "#,
    )
    .bind(new_hash)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 写入或清除某用户的 legacy 密码（外部用户库导入用）。
///
/// - `format=Some/hash=Some`：覆盖
/// - 任一为 None：清除（`UPDATE … SET legacy_password_format = NULL, legacy_password_hash = NULL`）
pub async fn set_user_legacy_password(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    format: Option<&str>,
    hash: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE users
           SET legacy_password_format = $1,
               legacy_password_hash = $2,
               date_modified = now()
         WHERE id = $3
        "#,
    )
    .bind(format)
    .bind(hash)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 把已知明文密码升级为 Argon2，并清除 legacy 字段。
///
/// 与 `change_user_password` 的差异：
/// - 不做长度限制（emby 老用户允许空密码 / 短 PIN，避免登录态切换时被拒）
/// - 用于"SHA1 命中后透明升级"的内部流程
pub async fn upgrade_legacy_password(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    plaintext: &str,
) -> Result<(), AppError> {
    let password_hash = security::hash_password(plaintext)?;
    sqlx::query(
        r#"
        UPDATE users
           SET password_hash = $1,
               legacy_password_format = NULL,
               legacy_password_hash = NULL,
               date_modified = now()
         WHERE id = $2
        "#,
    )
    .bind(password_hash)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 写入 / 清除 Emby `EasyPassword`（PIN）。传入 `None` 或空白表示移除 PIN。
pub async fn set_user_easy_password(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    new_password: Option<&str>,
) -> Result<(), AppError> {
    let hash = match new_password
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => {
            if value.len() < 4 {
                return Err(AppError::BadRequest(
                    "快速密码至少需要 4 个字符".to_string(),
                ));
            }
            Some(security::hash_password(value)?)
        }
        None => None,
    };

    sqlx::query("UPDATE users SET easy_password_hash = $1, date_modified = now() WHERE id = $2")
        .bind(hash)
        .bind(user_id)
        .execute(pool)
        .await?;

    Ok(())
}

/// 改名：Emby `POST /Users/{Id}` 允许 admin 重命名用户。集中走
/// `username::normalize_and_validate`，与注册同一套规则（控制字符 / 零宽字符 /
/// 双向控制字符 / 50 字上限 / NFC + Unicode trim）+ 唯一性校验。
pub async fn rename_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    new_name: &str,
) -> Result<(), AppError> {
    let normalized = crate::username::normalize_and_validate(new_name)?;
    let current = get_user_by_id(pool, user_id)
        .await?
        .ok_or_else(|| AppError::NotFound("用户不存在".to_string()))?;
    if current.name == normalized {
        return Ok(());
    }
    if let Some(existing) = get_user_by_name(pool, &normalized).await? {
        if existing.id != user_id {
            return Err(AppError::BadRequest("用户名已被占用".to_string()));
        }
    }
    sqlx::query("UPDATE users SET name = $1, date_modified = now() WHERE id = $2")
        .bind(&normalized)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn count_admin_users(pool: &sqlx::PgPool) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE is_admin = true AND is_disabled = false",
    )
    .fetch_one(pool)
    .await?)
}

pub async fn count_enabled_users(pool: &sqlx::PgPool) -> Result<i64, AppError> {
    Ok(
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_disabled = false")
            .fetch_one(pool)
            .await?,
    )
}

pub async fn update_user_policy(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    policy: &UserPolicyDto,
) -> Result<(), AppError> {
    // 将策略转换为JSON
    let policy_json = serde_json::to_value(policy)?;

    // 更新用户策略字段
    sqlx::query(
        r#"
        UPDATE users 
        SET policy = $1,
            is_admin = $2,
            is_hidden = $3,
            is_disabled = $4,
            date_modified = now()
        WHERE id = $5
        "#,
    )
    .bind(policy_json)
    .bind(policy.is_administrator)
    .bind(policy.is_hidden)
    .bind(policy.is_disabled)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn record_failed_login(pool: &sqlx::PgPool, user: &DbUser) -> Result<(), AppError> {
    let mut policy = user_policy_from_value(&user.policy);
    policy.invalid_login_attempt_count = policy.invalid_login_attempt_count.saturating_add(1);
    update_user_policy(pool, user.id, &policy).await
}

pub async fn clear_failed_login_count(pool: &sqlx::PgPool, user: &DbUser) -> Result<(), AppError> {
    let mut policy = user_policy_from_value(&user.policy);
    if policy.invalid_login_attempt_count == 0 {
        return Ok(());
    }
    policy.invalid_login_attempt_count = 0;
    update_user_policy(pool, user.id, &policy).await
}

pub async fn update_user_configuration(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    configuration: &UserConfigurationDto,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE users
        SET configuration = $1,
            date_modified = now()
        WHERE id = $2
        "#,
    )
    .bind(serde_json::to_value(configuration)?)
    .bind(user_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn list_users(pool: &sqlx::PgPool, public_only: bool) -> Result<Vec<DbUser>, AppError> {
    let users = if public_only {
        sqlx::query_as::<_, DbUser>(
            r#"
            SELECT id, name, ''::text AS password_hash, is_admin, is_hidden, is_disabled, policy,
                   configuration, primary_image_path, backdrop_image_path, logo_image_path, date_modified,
                   ''::text AS easy_password_hash, created_at,
                   NULL::text AS legacy_password_format, NULL::text AS legacy_password_hash
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
            SELECT id, name, password_hash, is_admin, is_hidden, is_disabled, policy,
                   configuration, primary_image_path, backdrop_image_path, logo_image_path, date_modified,
                   easy_password_hash, created_at,
                   legacy_password_format, legacy_password_hash
            FROM users
            ORDER BY name
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    Ok(users)
}

pub async fn get_user_image_path(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    image_type: &str,
) -> Result<Option<String>, AppError> {
    let user = get_user_by_id(pool, user_id).await?;
    let Some(user) = user else {
        return Ok(None);
    };

    Ok(match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => user.backdrop_image_path,
        "logo" => user.logo_image_path,
        "thumb" => user.backdrop_image_path.or(user.primary_image_path),
        _ => user.primary_image_path,
    })
}

pub async fn update_user_image_path(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    image_type: &str,
    path: Option<&str>,
) -> Result<(), AppError> {
    let sql = match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => "UPDATE users SET backdrop_image_path = $1, date_modified = now() WHERE id = $2",
        "logo" => "UPDATE users SET logo_image_path = $1, date_modified = now() WHERE id = $2",
        _ => "UPDATE users SET primary_image_path = $1, date_modified = now() WHERE id = $2",
    };

    sqlx::query(sql)
        .bind(path)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_tmdb_person_roles_except(
    pool: &sqlx::PgPool,
    media_item_id: Uuid,
    tmdb_person_ids: &[String],
) -> Result<u64, AppError> {
    let result = sqlx::query(
        r#"
        DELETE FROM person_roles pr
        USING persons p
        WHERE pr.person_id = p.id
          AND pr.media_item_id = $1
          AND (
              p.provider_ids ? 'Tmdb'
              OR p.provider_ids ? 'TMDb'
              OR p.provider_ids ? 'tmdb'
          )
          AND (
              cardinality($2::text[]) = 0
              OR COALESCE(
                    p.provider_ids->>'Tmdb',
                    p.provider_ids->>'TMDb',
                    p.provider_ids->>'tmdb'
                 ) <> ALL($2::text[])
          )
        "#,
    )
    .bind(media_item_id)
    .bind(tmdb_person_ids)
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}

pub async fn update_person_image_path(
    pool: &sqlx::PgPool,
    person_id: Uuid,
    image_type: &str,
    path: Option<&str>,
) -> Result<(), AppError> {
    let column = match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => "backdrop_image_path",
        "logo" => "logo_image_path",
        _ => "primary_image_path",
    };

    let query = format!("UPDATE persons SET {column} = $1, updated_at = now() WHERE id = $2");
    sqlx::query(&query)
        .bind(path)
        .bind(person_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 取某个 media_item 关联的人物 UUID 列表，按 Actor 优先 + sort_order 升序，限制前 `limit` 个。
///
/// 用于 `PersonService::refresh_persons_for_item`：在刷新 Series/Movie 元数据后顺手把
/// cast/director 的简介与头像补完，但只挑前 N 个避免 TMDB 限流。
pub async fn list_item_person_ids(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    limit: usize,
) -> Result<Vec<Uuid>, AppError> {
    let rows = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT pr.person_id
        FROM person_roles pr
        WHERE pr.media_item_id = $1
        ORDER BY
            CASE pr.role_type
                WHEN 'Actor' THEN 0
                WHEN 'GuestStar' THEN 0
                WHEN 'Director' THEN 1
                WHEN 'Writer' THEN 2
                WHEN 'Producer' THEN 3
                ELSE 4
            END,
            pr.sort_order
        LIMIT $2
        "#,
    )
    .bind(item_id)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 仅 patch 人物的元数据（biography/出生地/外部链接），不动 image 路径。
///
/// 与 `update_person` 的区别：
/// - 不要求传入完整 `DbPerson`，省掉外部回填 image 路径的负担
/// - `provider_ids` 走 jsonb 合并而非整体替换，避免覆盖之前由 NFO 写入的 Imdb/Tvdb 等
pub async fn patch_person_metadata(
    pool: &sqlx::PgPool,
    person_id: Uuid,
    overview: Option<&str>,
    external_url: Option<&str>,
    provider_ids: Option<&Value>,
    premiere_date: Option<DateTime<Utc>>,
    death_date: Option<DateTime<Utc>>,
    production_year: Option<i32>,
    place_of_birth: Option<&str>,
    homepage_url: Option<&str>,
    sort_name: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE persons
           SET overview            = COALESCE($2, overview),
               external_url        = COALESCE($3, external_url),
               provider_ids        = CASE
                                       WHEN $4::jsonb IS NULL THEN provider_ids
                                       WHEN provider_ids = '{}'::jsonb THEN $4::jsonb
                                       ELSE provider_ids || $4::jsonb
                                     END,
               premiere_date       = COALESCE($5, premiere_date),
               death_date          = COALESCE($6, death_date),
               production_year     = COALESCE($7, production_year),
               place_of_birth      = COALESCE($8, place_of_birth),
               homepage_url        = COALESCE($9, homepage_url),
               sort_name           = COALESCE($10, sort_name),
               metadata_synced_at  = now(),
               updated_at          = now()
         WHERE id = $1
        "#,
    )
    .bind(person_id)
    .bind(overview)
    .bind(external_url)
    .bind(provider_ids)
    .bind(premiere_date)
    .bind(death_date)
    .bind(production_year)
    .bind(place_of_birth)
    .bind(homepage_url)
    .bind(sort_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// 判断人物元数据是否过时（从未同步或距上次同步 ≥3 天）。
pub async fn is_person_metadata_stale(pool: &sqlx::PgPool, person_id: Uuid) -> Result<bool, AppError> {
    let synced_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT metadata_synced_at FROM persons WHERE id = $1",
    )
    .bind(person_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    Ok(match synced_at {
        None => true,
        Some(ts) => (Utc::now() - ts).num_days() >= 3,
    })
}

/// 标记人物的 metadata_synced_at 为当前时间（即使无 TMDB ID 也标记，避免重复请求）。
pub async fn mark_person_metadata_synced(pool: &sqlx::PgPool, person_id: Uuid) {
    let _ = sqlx::query("UPDATE persons SET metadata_synced_at = now() WHERE id = $1")
        .bind(person_id)
        .execute(pool)
        .await;
}

pub async fn create_session(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    device_id: Option<String>,
    device_name: Option<String>,
    client: Option<String>,
    application_version: Option<String>,
    remote_address: Option<String>,
    expires_at: Option<DateTime<Utc>>,
) -> Result<AuthSessionRow, AppError> {
    create_session_with_type(
        pool,
        user_id,
        device_id,
        device_name,
        client,
        application_version,
        remote_address,
        expires_at,
        "Interactive",
    )
    .await
}

pub async fn create_session_with_type(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    device_id: Option<String>,
    device_name: Option<String>,
    client: Option<String>,
    application_version: Option<String>,
    remote_address: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    session_type: &str,
) -> Result<AuthSessionRow, AppError> {
    let token = Uuid::new_v4().simple().to_string();

    sqlx::query(
        r#"
        INSERT INTO sessions (
            access_token,
            user_id,
            device_id,
            device_name,
            client,
            application_version,
            remote_address,
            expires_at,
            session_type
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(&token)
    .bind(user_id)
    .bind(device_id)
    .bind(device_name)
    .bind(client)
    .bind(application_version)
    .bind(remote_address)
    .bind(expires_at)
    .bind(session_type)
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
            s.session_type,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at,
            s.expires_at,
            s.remote_address
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.access_token = $1
          AND u.is_disabled = false
          AND (s.expires_at IS NULL OR s.expires_at > now())
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    if let Some(ref s) = session {
        if (chrono::Utc::now() - s.last_activity_at).num_seconds() > 60 {
            sqlx::query("UPDATE sessions SET last_activity_at = now() WHERE access_token = $1")
                .bind(token)
                .execute(pool)
                .await?;
        }
    }

    Ok(session)
}

pub async fn list_sessions(pool: &sqlx::PgPool) -> Result<Vec<AuthSessionRow>, AppError> {
    Ok(sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT DISTINCT ON (s.device_id, s.user_id)
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.session_type,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at,
            s.expires_at,
            s.remote_address
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.session_type = 'Interactive'
          AND (s.expires_at IS NULL OR s.expires_at > now())
        ORDER BY s.device_id, s.user_id, s.last_activity_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?)
}

/// 列出所有会话（含已过期），用于 Devices 端点聚合设备信息。
/// Emby 持久化设备表，我们用全量会话历史模拟。
pub async fn list_all_sessions_for_devices(pool: &sqlx::PgPool) -> Result<Vec<AuthSessionRow>, AppError> {
    Ok(sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.session_type,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at,
            s.expires_at,
            s.remote_address
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.device_id IS NOT NULL AND s.device_id != ''
        ORDER BY s.last_activity_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_sessions_for_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<AuthSessionRow>, AppError> {
    Ok(sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT DISTINCT ON (s.device_id)
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.session_type,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at,
            s.expires_at,
            s.remote_address
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.user_id = $1
          AND s.session_type = 'Interactive'
          AND (s.expires_at IS NULL OR s.expires_at > now())
        ORDER BY s.device_id, s.last_activity_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

pub async fn find_active_session(
    pool: &sqlx::PgPool,
    token: &str,
) -> Result<Option<AuthSessionRow>, AppError> {
    Ok(sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.session_type,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at,
            s.expires_at,
            s.remote_address
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.access_token = $1
          AND u.is_disabled = false
          AND (s.expires_at IS NULL OR s.expires_at > now())
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?)
}

/// `session_play_queue` 与 `session_runtime_state` 以 `sessions.access_token` 为维度。
/// Emby 客户端里的 **PlaySessionId**（PlaybackInfo 生成）与登录拿到的 **AccessToken** 不是同一个值；
/// 若把前者误写入 `playback_events.session_id`，则 `record_playback_event` 里
/// `WHERE EXISTS (SELECT 1 FROM sessions WHERE access_token = $1)` 永远不成立，
/// **NowPlayingItem** 无法填充，下游（Sakura / 控制台「在线播放」统计）恒为 0。
///
/// 规则：仅当 `client_session_id` 非空且确实是当前库里的有效 `access_token` 时才采纳；
/// 否则回落到本次请求的 `auth_access_token`。
pub async fn resolve_session_id_for_play_queue(
    pool: &sqlx::PgPool,
    auth_access_token: &str,
    client_session_id: Option<&str>,
) -> Result<String, AppError> {
    if let Some(sid) = client_session_id.map(str::trim).filter(|s| !s.is_empty()) {
        let exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM sessions s
                INNER JOIN users u ON u.id = s.user_id
                WHERE s.access_token = $1
                  AND u.is_disabled = false
                  AND (s.expires_at IS NULL OR s.expires_at > now())
            )
            "#,
        )
        .bind(sid)
        .fetch_one(pool)
        .await?;
        if exists {
            return Ok(sid.to_string());
        }
    }
    Ok(auth_access_token.to_string())
}

pub async fn list_api_key_sessions(pool: &sqlx::PgPool) -> Result<Vec<AuthSessionRow>, AppError> {
    Ok(sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.session_type,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at,
            s.expires_at,
            s.remote_address
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.session_type = 'ApiKey'
        ORDER BY s.last_activity_at DESC
        "#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_api_key_session(
    pool: &sqlx::PgPool,
    token: &str,
) -> Result<Option<AuthSessionRow>, AppError> {
    Ok(sqlx::query_as::<_, AuthSessionRow>(
        r#"
        SELECT
            s.access_token,
            s.user_id,
            u.name AS user_name,
            u.is_admin,
            s.session_type,
            s.device_id,
            s.device_name,
            s.client,
            s.application_version,
            s.last_activity_at,
            s.expires_at,
            s.remote_address
        FROM sessions s
        INNER JOIN users u ON u.id = s.user_id
        WHERE s.access_token = $1
          AND s.session_type = 'ApiKey'
        "#,
    )
    .bind(token)
    .fetch_optional(pool)
    .await?)
}

pub async fn active_session_count_for_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        r#"
        SELECT COUNT(*)::bigint
        FROM sessions
        WHERE user_id = $1
          AND session_type = 'Interactive'
          AND (expires_at IS NULL OR expires_at > now())
        "#,
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?)
}

pub async fn delete_session(pool: &sqlx::PgPool, access_token: &str) -> Result<(), AppError> {
    sqlx::query("DELETE FROM sessions WHERE access_token = $1")
        .bind(access_token)
        .execute(pool)
        .await?;
    delete_session_capabilities(pool, access_token).await?;
    delete_session_viewing(pool, access_token).await?;
    delete_session_state_summary(pool, access_token).await?;
    Ok(())
}

pub async fn delete_sessions_for_user(pool: &sqlx::PgPool, user_id: Uuid) -> Result<(), AppError> {
    let tokens = sqlx::query_scalar::<_, String>(
        r#"
        SELECT access_token
        FROM sessions
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    for token in tokens {
        delete_session(pool, &token).await?;
    }

    Ok(())
}

pub async fn cleanup_stale_sessions(pool: &sqlx::PgPool) -> Result<u64, AppError> {
    let stale_tokens = sqlx::query_scalar::<_, String>(
        r#"
        SELECT access_token FROM sessions
        WHERE (expires_at IS NOT NULL AND expires_at < now())
           OR (last_activity_at < now() - INTERVAL '30 days')
        "#,
    )
    .fetch_all(pool)
    .await?;
    let count = stale_tokens.len() as u64;
    for token in stale_tokens {
        delete_session(pool, &token).await?;
    }
    Ok(count)
}

pub async fn update_media_item_image_path(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    image_type: &str,
    path: Option<&str>,
    backdrop_index: Option<i32>,
) -> Result<(), AppError> {
    let image_type_lc = image_type.to_ascii_lowercase();
    match image_type_lc.as_str() {
        "backdrop" => {
            let idx = backdrop_index.unwrap_or(0);
            if idx == 0 {
                sqlx::query(
                    "UPDATE media_items SET backdrop_path = $1, date_modified = now() WHERE id = $2",
                )
                .bind(path)
                .bind(item_id)
                .execute(pool)
                .await?;
            } else if idx < 0 {
                return Err(AppError::BadRequest("无效的壁纸索引".to_string()));
            } else {
                let Some(item) = get_media_item(pool, item_id).await? else {
                    return Err(AppError::NotFound("媒体条目不存在".to_string()));
                };
                let u = (idx - 1) as usize;
                let mut paths = item.backdrop_paths;
                match path {
                    Some(p) => {
                        if u > paths.len() {
                            return Err(AppError::BadRequest("无效的壁纸索引".to_string()));
                        }
                        if u == paths.len() {
                            paths.push(p.to_string());
                        } else {
                            paths[u] = p.to_string();
                        }
                    }
                    None => {
                        if u < paths.len() {
                            paths.remove(u);
                        }
                    }
                }
                sqlx::query(
                    "UPDATE media_items SET backdrop_paths = $1, date_modified = now() WHERE id = $2",
                )
                .bind(paths)
                .bind(item_id)
                .execute(pool)
                .await?;
            }
        }
        "logo" => {
            sqlx::query(
                "UPDATE media_items SET logo_path = $1, date_modified = now() WHERE id = $2",
            )
            .bind(path)
            .bind(item_id)
            .execute(pool)
            .await?;
        }
        "thumb" => {
            sqlx::query(
                "UPDATE media_items SET thumb_path = $1, date_modified = now() WHERE id = $2",
            )
            .bind(path)
            .bind(item_id)
            .execute(pool)
            .await?;
        }
        "banner" => {
            sqlx::query(
                "UPDATE media_items SET banner_path = $1, date_modified = now() WHERE id = $2",
            )
            .bind(path)
            .bind(item_id)
            .execute(pool)
            .await?;
        }
        "disc" => {
            sqlx::query(
                "UPDATE media_items SET disc_path = $1, date_modified = now() WHERE id = $2",
            )
            .bind(path)
            .bind(item_id)
            .execute(pool)
            .await?;
        }
        "art" => {
            sqlx::query(
                "UPDATE media_items SET art_path = $1, date_modified = now() WHERE id = $2",
            )
            .bind(path)
            .bind(item_id)
            .execute(pool)
            .await?;
        }
        _ => {
            sqlx::query(
                "UPDATE media_items SET image_primary_path = $1, date_modified = now() WHERE id = $2",
            )
            .bind(path)
            .bind(item_id)
            .execute(pool)
            .await?;
        }
    }
    if let Some(image_path) = path {
        tokio::spawn(update_blurhash_for_image(
            pool.clone(),
            item_id,
            image_type.to_string(),
            image_path.to_string(),
        ));
    }

    Ok(())
}

/// PB42：远端 Emby 同步阶段写入的"待下载图片"任务行（仅查询用，不绑定表）。
///
/// 后台 sidecar_image_download_loop 周期性 SELECT 出本结构，
/// 把 `image_primary_path / backdrop_path / logo_path` 中仍是远端 URL 的字段
/// 物理下载到 sidecar 目录，再用 `update_media_item_image_path` 替换为本地绝对路径。
#[derive(Debug, Clone)]
pub struct PendingRemoteImageDownload {
    pub item_id: Uuid,
    pub source_id: Uuid,
    pub item_path: String,
    pub item_type: String,
    pub remote_primary_url: Option<String>,
    pub remote_backdrop_url: Option<String>,
    pub remote_logo_url: Option<String>,
}

/// PB42：扫描 media_items 找出仍指向远端 URL 的图片字段。
///
/// 过滤条件：
/// - `item_type IN ('Movie','Episode')`：只处理 STRM 真实媒体行（Series/Season 行复用同一字段
///   但其 path 不以 .strm 结尾，且本地 sidecar 目录概念不同，由 series detail 流程负责）。
/// - `path LIKE '%.strm'`：与 worker 推导 sidecar_dir 的前提一致——必须有真实落盘的 strm 文件
///   否则 `path.parent()` 没有对应的物理目录。
/// - `provider_ids ? 'RemoteEmbySourceId'`：只挑同步阶段写入的远端绑定行。
/// - 三个 image path 列至少一列以 `http://` / `https://` 开头：还没被 worker 替换为本地。
///
/// 排序：`date_modified DESC` 让最新刚同步进来的条目优先下载（前端用户最可能立刻去看的就是
/// 最近写入的，体感上"刚同步完图片很快补齐"）。
pub async fn find_pending_remote_image_downloads(
    pool: &sqlx::PgPool,
    limit: i64,
) -> Result<Vec<PendingRemoteImageDownload>, AppError> {
    let limit = limit.clamp(1, 1000);
    let rows: Vec<(Uuid, String, String, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(
            r#"
            SELECT
                mi.id,
                mi.provider_ids->>'RemoteEmbySourceId' AS source_id_str,
                mi.path,
                mi.item_type,
                CASE WHEN mi.image_primary_path LIKE 'http://%' OR mi.image_primary_path LIKE 'https://%'
                     THEN mi.image_primary_path ELSE NULL END,
                CASE WHEN mi.backdrop_path      LIKE 'http://%' OR mi.backdrop_path      LIKE 'https://%'
                     THEN mi.backdrop_path      ELSE NULL END,
                CASE WHEN mi.logo_path          LIKE 'http://%' OR mi.logo_path          LIKE 'https://%'
                     THEN mi.logo_path          ELSE NULL END
            FROM media_items mi
            WHERE mi.item_type IN ('Movie', 'Episode')
              AND mi.path LIKE '%.strm'
              AND mi.provider_ids ? 'RemoteEmbySourceId'
              AND (
                   mi.image_primary_path LIKE 'http://%' OR mi.image_primary_path LIKE 'https://%'
                OR mi.backdrop_path      LIKE 'http://%' OR mi.backdrop_path      LIKE 'https://%'
                OR mi.logo_path          LIKE 'http://%' OR mi.logo_path          LIKE 'https://%'
              )
            ORDER BY mi.date_modified DESC NULLS LAST
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (item_id, source_id_str, path, item_type, primary, backdrop, logo) in rows {
        let Ok(source_id) = Uuid::parse_str(source_id_str.trim()) else {
            // RemoteEmbySourceId 写错（理论上不会，但留个跳过路径）—— 跳过避免阻塞 worker
            tracing::warn!(
                item_id = %item_id,
                raw = %source_id_str,
                "PB42：sidecar worker 忽略 RemoteEmbySourceId 不是合法 UUID 的条目"
            );
            continue;
        };
        out.push(PendingRemoteImageDownload {
            item_id,
            source_id,
            item_path: path,
            item_type,
            remote_primary_url: primary,
            remote_backdrop_url: backdrop,
            remote_logo_url: logo,
        });
    }
    Ok(out)
}

async fn update_blurhash_for_image(pool: sqlx::PgPool, item_id: Uuid, image_type: String, image_path: String) {
    let hash = match tokio::task::spawn_blocking(move || generate_blurhash_from_path(&image_path))
        .await
    {
        Ok(Some(hash)) => hash,
        _ => return,
    };
    let image_type_pascal = match image_type.to_ascii_lowercase().as_str() {
        "primary" | "" => "Primary",
        "backdrop" => "Backdrop",
        "logo" => "Logo",
        "thumb" => "Thumb",
        "banner" => "Banner",
        "disc" => "Disc",
        "art" => "Art",
        "box" => "Box",
        "menu" => "Menu",
        _ => "Primary",
    };
    let tag = chrono::Utc::now().timestamp().to_string();
    let _ = sqlx::query(
        r#"UPDATE media_items
           SET image_blur_hashes = jsonb_set(
               image_blur_hashes,
               ARRAY[$1::text],
               jsonb_build_object($2::text, $3::text),
               true
           )
           WHERE id = $4"#,
    )
    .bind(image_type_pascal)
    .bind(&tag)
    .bind(&hash)
    .bind(item_id)
    .execute(&pool)
    .await;
}

fn generate_blurhash_from_path(path: &str) -> Option<String> {
    let img = if path.starts_with("http://") || path.starts_with("https://") {
        return None;
    } else {
        image::open(path).ok()?
    };
    let small = img.thumbnail(32, 32);
    let (w, h) = (small.width(), small.height());
    let rgba = small.to_rgba8();
    let pixels: Vec<[u8; 4]> = rgba.pixels().map(|p| p.0).collect();
    let flat: Vec<u8> = pixels.iter().flat_map(|p| p.iter().copied()).collect();
    let hash = blurhash::encode(4, 3, w, h, &flat).ok()?;
    Some(hash)
}

pub async fn list_server_logs(log_dir: &Path) -> Result<Vec<LogFileDto>, AppError> {
    let mut items = Vec::new();
    if !log_dir.exists() {
        return Ok(items);
    }

    let entries = std::fs::read_dir(log_dir)
        .map_err(|error| AppError::Internal(format!("读取日志目录失败: {error}")))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| AppError::Internal(format!("读取日志文件失败: {error}")))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let metadata = entry
            .metadata()
            .map_err(|error| AppError::Internal(format!("读取日志文件元数据失败: {error}")))?;
        let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let date_modified: DateTime<Utc> = modified.into();
        items.push(LogFileDto {
            name: path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_string(),
            date_modified,
        });
    }

    items.sort_by(|left, right| right.date_modified.cmp(&left.date_modified));
    Ok(items)
}

pub async fn list_activity_logs(
    pool: &sqlx::PgPool,
    limit: i64,
    user_id: Option<uuid::Uuid>,
) -> Result<Vec<ActivityLogEntryDto>, AppError> {
    let rows = if let Some(uid) = user_id {
        sqlx::query_as::<_, ActivityLogRow>(
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
            WHERE e.user_id = $1
            ORDER BY e.created_at DESC
            LIMIT $2
            "#,
        )
        .bind(uid)
        .bind(limit.clamp(1, 200))
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, ActivityLogRow>(
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
        .await?
    };

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
                id: uuid_to_emby_guid(&row.id),
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
    if get_library_by_name(pool, name).await?.is_some() {
        return Err(AppError::BadRequest("媒体库名称已存在".to_string()));
    }
    if paths.is_empty() {
        return Err(AppError::BadRequest("至少需要添加一个媒体路径".to_string()));
    }
    validate_library_paths_available(pool, None, &paths).await?;

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

/// 确保"远端 Emby 中转"虚拟媒体库存在，若不存在则自动创建（幂等）。
/// 用于 separate 显示模式下不强制要求用户手动选择目标媒体库。
pub async fn ensure_remote_transit_library(pool: &sqlx::PgPool) -> Result<DbLibrary, AppError> {
    const TRANSIT_LIB_NAME: &str = "远端 Emby 中转";
    const TRANSIT_LIB_PATH: &str = "__remote_transit__";

    // 先查（大小写不敏感），存在则直接返回
    if let Some(lib) = get_library_by_name(pool, TRANSIT_LIB_NAME).await? {
        return Ok(lib);
    }

    let id = Uuid::new_v4();
    // 远端绑定库默认开启 SaveLocalMetadata：所有 sidecar（图片、NFO、字幕）
    // 直接落到 strm 同目录，与 write_remote_strm_bundle 一致，
    // 同时让 POST /Items/{id}/Refresh 触发的元数据刷新也写真实物理路径。
    let options_value = serde_json::json!({
        "PathInfos": [{"Path": TRANSIT_LIB_PATH}],
        "EnableAutoBoxSets": false,
        "EnableRealtimeMonitor": false,
        "SkipSubtitlesIfEmbeddedSubtitlesPresent": false,
        "SaveLocalMetadata": true
    });
    // 同时处理两个唯一约束：name UNIQUE（精确）和 idx_libraries_name_unique（lower(name)）
    // 任何冲突都静默忽略，之后重查即可拿到已存在的行
    let _ = sqlx::query(
        r#"
        INSERT INTO libraries (id, name, collection_type, path, library_options, date_modified)
        VALUES ($1, $2, 'movies', $3, $4, now())
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(id)
    .bind(TRANSIT_LIB_NAME)
    .bind(TRANSIT_LIB_PATH)
    .bind(&options_value)
    .execute(pool)
    .await;
    // 无论 INSERT 成功还是冲突，都重查一次获取实际行
    get_library_by_name(pool, TRANSIT_LIB_NAME)
        .await?
        .ok_or_else(|| AppError::Internal("无法创建远端 Emby 中转库".to_string()))
}

/// 为远端 Emby 源的单个 View 确保存在对应的本地独立媒体库。
/// - 优先使用 `existing_library_id`（来自 view_library_map）中已有库
/// - 若不存在则以 `view_name` 为名创建（名称冲突时追加源名前缀）
/// 返回库 ID。
pub async fn ensure_view_library(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    source_name: &str,
    view_id: &str,
    view_name: &str,
    collection_type: Option<&str>,
    existing_library_id: Option<Uuid>,
) -> Result<Uuid, AppError> {
    // 1. 如果已有对应库且库仍然存在，直接返回
    if let Some(lib_id) = existing_library_id {
        if get_library(pool, lib_id).await?.is_some() {
            return Ok(lib_id);
        }
    }

    // 2. 根据 view_name 找或建库
    //    优先用 view_name，冲突时用 "{source_name} - {view_name}"
    let preferred_name = view_name.trim().to_string();
    let fallback_name = format!("{} - {}", source_name.trim(), view_name.trim());

    // 解析远端 collection_type → 本地 collection_type
    let local_collection_type = match collection_type {
        Some(t) if t.eq_ignore_ascii_case("tvshows") => "tvshows",
        Some(t) if t.eq_ignore_ascii_case("movies") => "movies",
        Some(t) if t.eq_ignore_ascii_case("homevideos") => "homevideos",
        _ => "movies",
    };

    // 虚拟路径：__remote_view_{source_id}_{view_id}
    let virtual_path = format!("__remote_view_{}_{}", source_id.simple(), view_id.trim());

    // 远端绑定库默认开启 SaveLocalMetadata：所有 sidecar（图片、NFO、字幕）
    // 直接落到 strm 同目录，与 write_remote_strm_bundle 一致。
    let options_value = serde_json::json!({
        "PathInfos": [{"Path": virtual_path}],
        "EnableAutoBoxSets": false,
        "EnableRealtimeMonitor": false,
        "SkipSubtitlesIfEmbeddedSubtitlesPresent": false,
        "SaveLocalMetadata": true
    });

    // 尝试首选名称
    {
        let id = Uuid::new_v4();
        let _ = sqlx::query(
            r#"
            INSERT INTO libraries (id, name, collection_type, path, library_options, date_modified)
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(id)
        .bind(&preferred_name)
        .bind(local_collection_type)
        .bind(&virtual_path)
        .bind(&options_value)
        .execute(pool)
        .await;
        if let Some(lib) = get_library_by_name(pool, &preferred_name).await? {
            return Ok(lib.id);
        }
    }
    // 首选名被其他库占用，用回退名
    {
        let id = Uuid::new_v4();
        let _ = sqlx::query(
            r#"
            INSERT INTO libraries (id, name, collection_type, path, library_options, date_modified)
            VALUES ($1, $2, $3, $4, $5, now())
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(id)
        .bind(&fallback_name)
        .bind(local_collection_type)
        .bind(&virtual_path)
        .bind(&options_value)
        .execute(pool)
        .await;
        if let Some(lib) = get_library_by_name(pool, &fallback_name).await? {
            return Ok(lib.id);
        }
    }
    Err(AppError::Internal(format!(
        "无法为远端 View「{view_name}」创建媒体库"
    )))
}

/// 将远程 View 的虚拟路径注册到已有本地库的 PathInfos 中（合并模式）。
/// 如果路径已存在则跳过。
pub async fn ensure_remote_view_path_in_library(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    source_id: Uuid,
    view_id: &str,
) -> Result<(), AppError> {
    let virtual_path = format!("__remote_view_{}_{}", source_id.simple(), view_id.trim());
    let lib = match get_library(pool, library_id).await? {
        Some(lib) => lib,
        None => return Ok(()),
    };
    let mut options: serde_json::Value = lib.library_options.clone();
    let mut dirty = false;

    // 远端绑定库必须开启 SaveLocalMetadata：所有 sidecar（图片、NFO、字幕）
    // 才能落到 strm 同目录，与 write_remote_strm_bundle 一致。
    // 旧库可能尚未带这个选项，每次同步顺便升级一次。
    if let Some(obj) = options.as_object_mut() {
        let needs_set = obj
            .get("SaveLocalMetadata")
            .and_then(serde_json::Value::as_bool)
            != Some(true);
        if needs_set {
            obj.insert(
                "SaveLocalMetadata".to_string(),
                serde_json::Value::Bool(true),
            );
            dirty = true;
        }
    }

    let path_infos = options
        .as_object_mut()
        .and_then(|obj| obj.entry("PathInfos").or_insert_with(|| serde_json::json!([])).as_array_mut());
    if let Some(arr) = path_infos {
        let already = arr.iter().any(|p| {
            p.get("Path").and_then(|v| v.as_str()).map_or(false, |s| s == virtual_path)
        });
        if !already {
            arr.push(serde_json::json!({"Path": virtual_path}));
            dirty = true;
        }
    }

    if dirty {
        sqlx::query("UPDATE libraries SET library_options = $1, date_modified = now() WHERE id = $2")
            .bind(&options)
            .bind(library_id)
            .execute(pool)
            .await?;
    }
    Ok(())
}

/// PB24：一次性清理「孤儿远端虚拟路径」——`libraries` 表里所有挂着的
/// `__remote_view_<source_id>_*` 路径，但其 `<source_id>` 在 `remote_emby_sources` 表里
/// 已经不存在的那些。这是对应 PB23 修复**之前**已经累积下来的历史残留：用户多次
/// 删除远端源、或迁移过项目，会留下 separate 模式的孤儿独立库 + merge 模式的孤儿
/// PathInfos entry。一次性清理不会触碰当前仍存在的远端源（它们的虚拟路径仍由活跃
/// 的同步流程在维护）。
///
/// 返回 `(deleted_libraries, updated_libraries, scanned_orphan_source_ids)`：分别是被
/// 整体删除的库数 / 被剥离的库数 / 检测到的孤儿 source_id 数量（含未触发实际删除的）。
pub async fn cleanup_orphan_remote_view_paths(
    pool: &sqlx::PgPool,
) -> Result<(u64, u64, u64), AppError> {
    // PB49 (D3)：批量删 standalone library 的安全阀。
    //
    // 这个函数在每次 backend 启动时跑。如果 `remote_emby_sources` 表因为意外
    // （DDL 误操作、备份还原顺序错乱、迁移脚本中途失败、用户把表 truncate 了
    // 想做"软重置"等）暂时为空，naïve 实现会一口气把所有 `__remote_view_*`
    // standalone library 全删——包括用户可能想保留的、对应的 strm 工作区。
    // 启动时跑 → 用户来不及看到日志阻止。
    //
    // 这里加两道闸：
    //   - 绝对上限：单次启动最多删 D3_DELETE_HARD_LIMIT 个 standalone 库，超过
    //     直接 abort 并把"本来要删的 ID 列表"打成 ERROR 日志，要求人工处理。
    //   - 比例上限：如果 standalone 总数 > 0 且要删的占比 >= D3_DELETE_RATIO，
    //     同样 abort——这能防住"备份还原后 sources 表是空的、libraries 表完整"
    //     这种典型脚误场景。
    const D3_DELETE_HARD_LIMIT: usize = 50;
    const D3_DELETE_RATIO: f64 = 0.5;

    // 现有远端源的 simple-uuid 集合（小写、无连字符）。
    let live_source_ids: Vec<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM remote_emby_sources")
            .fetch_all(pool)
            .await?;
    let live_simple: std::collections::HashSet<String> = live_source_ids
        .iter()
        .map(|id| id.simple().to_string().to_ascii_lowercase())
        .collect();

    // 第一遍：扫所有 `path` 形如 `__remote_view_<simple>_*` 的库。
    let standalone: Vec<DbLibrary> = sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at,
               primary_image_path, primary_image_tag
        FROM libraries
        WHERE path LIKE '__remote_view_%'
        "#,
    )
    .fetch_all(pool)
    .await?;

    // 先「干跑」一遍：识别本次会被删的孤儿，但暂不真删，给安全阀打分用。
    struct OrphanLib<'a> {
        lib: &'a DbLibrary,
        simple: String,
    }
    let mut planned: Vec<OrphanLib> = Vec::new();
    for lib in &standalone {
        let rest = lib.path.trim_start_matches("__remote_view_");
        let Some((simple, _)) = rest.split_once('_') else {
            continue;
        };
        let simple_lower = simple.to_ascii_lowercase();
        if !live_simple.contains(&simple_lower) {
            planned.push(OrphanLib {
                lib,
                simple: simple_lower,
            });
        }
    }

    // 安全阀检查
    let total_standalone = standalone.len();
    let plan_count = planned.len();
    let exceeds_hard_limit = plan_count > D3_DELETE_HARD_LIMIT;
    let exceeds_ratio = total_standalone > 0
        && (plan_count as f64 / total_standalone as f64) >= D3_DELETE_RATIO;

    if exceeds_hard_limit || exceeds_ratio {
        // 不删！把待删 ID 列表打成 ERROR 日志，给运维人工排查。
        let plan_preview: Vec<String> = planned
            .iter()
            .take(20)
            .map(|p| format!("{} ({})", p.lib.name, p.lib.path))
            .collect();
        tracing::error!(
            plan_count,
            total_standalone,
            hard_limit = D3_DELETE_HARD_LIMIT,
            ratio_threshold = D3_DELETE_RATIO,
            exceeds_hard_limit,
            exceeds_ratio,
            preview = ?plan_preview,
            "PB49 (D3)：发现疑似批量孤儿远端虚拟库 → 触发安全阀 ABORT。\
             启动清理一次最多删 {hard} 个或 {ratio_pct}% 比例，超阈值直接放行不删。\
             如果远端源表确实被清空且本次删除是预期行为，请手工 DELETE FROM libraries \
             WHERE path LIKE '__remote_view_%' 后重启",
            hard = D3_DELETE_HARD_LIMIT,
            ratio_pct = (D3_DELETE_RATIO * 100.0) as u32,
        );
        // 注意：返回 0 已删 + 0 更新，让上层 startup 日志看出本次没动手。
        return Ok((0, 0, plan_count as u64));
    }

    let mut deleted = 0u64;
    let mut orphan_ids = std::collections::HashSet::<String>::new();
    for orphan in &planned {
        orphan_ids.insert(orphan.simple.clone());
        let res = sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(orphan.lib.id)
            .execute(pool)
            .await?;
        deleted += res.rows_affected();
    }

    // 第二遍：扫所有 library_options.PathInfos 含 `__remote_view_*` 的库，剥掉那些
    // source_id 已不存在的 entry。
    let merge_candidates: Vec<DbLibrary> = sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at,
               primary_image_path, primary_image_tag
        FROM libraries
        WHERE library_options::text LIKE '%__remote_view_%'
        "#,
    )
    .fetch_all(pool)
    .await?;
    let mut updated = 0u64;
    for lib in &merge_candidates {
        let mut options = lib.library_options.clone();
        let mut dirty = false;
        if let Some(arr) = options
            .as_object_mut()
            .and_then(|obj| obj.get_mut("PathInfos"))
            .and_then(|v| v.as_array_mut())
        {
            let before = arr.len();
            arr.retain(|p| {
                let path_str = match p.get("Path").and_then(|v| v.as_str()) {
                    Some(s) => s,
                    None => return true,
                };
                if !path_str.starts_with("__remote_view_") {
                    return true; // 非远端虚拟路径，保留。
                }
                let rest = path_str.trim_start_matches("__remote_view_");
                let simple = match rest.split_once('_') {
                    Some((s, _)) => s.to_ascii_lowercase(),
                    None => return false, // 格式异常的也视为孤儿剥掉。
                };
                if live_simple.contains(&simple) {
                    true
                } else {
                    orphan_ids.insert(simple);
                    false
                }
            });
            if arr.len() != before {
                dirty = true;
            }
        }
        if dirty {
            sqlx::query(
                "UPDATE libraries SET library_options = $1, date_modified = now() WHERE id = $2",
            )
            .bind(&options)
            .bind(lib.id)
            .execute(pool)
            .await?;
            updated += 1;
        }
    }

    Ok((deleted, updated, orphan_ids.len() as u64))
}

/// PB23：删除远端源时配套清理 libraries 表里挂着的虚拟路径。
/// 包含两类清理：
///   1) **Separate 模式**：path 形如 `__remote_view_<source_id>_*` 的整条 library 记录
///      由 `ensure_view_library` 自动创建，删除源时这些独立库失去归属，整条删除（含 media_paths）。
///   2) **Merge 模式**：本地用户原本的 library，其 `library_options.PathInfos` 中混入了
///      `__remote_view_<source_id>_*` 条目；这里只移除这些 entry，保留库本体。
///
/// 返回 `(deleted_libraries, updated_libraries)`：分别是被整体删除的库数 / 被剥离了
/// 远端 view 路径的库数，用于审计日志。
pub async fn cleanup_remote_view_paths_for_source(
    pool: &sqlx::PgPool,
    source_id: Uuid,
) -> Result<(u64, u64), AppError> {
    let prefix = format!("__remote_view_{}_", source_id.simple());
    let standalone_libs: Vec<DbLibrary> = sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at,
               primary_image_path, primary_image_tag
        FROM libraries
        WHERE path LIKE $1
        "#,
    )
    .bind(format!("{prefix}%"))
    .fetch_all(pool)
    .await?;
    let mut deleted = 0u64;
    for lib in &standalone_libs {
        let res = sqlx::query("DELETE FROM libraries WHERE id = $1")
            .bind(lib.id)
            .execute(pool)
            .await?;
        deleted += res.rows_affected();
    }

    // Merge 模式：扫所有「非独立」库，从 library_options.PathInfos 剥掉远端 view 路径。
    let merge_candidates: Vec<DbLibrary> = sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at,
               primary_image_path, primary_image_tag
        FROM libraries
        WHERE library_options::text LIKE $1
        "#,
    )
    .bind(format!("%{prefix}%"))
    .fetch_all(pool)
    .await?;
    let mut updated = 0u64;
    for lib in &merge_candidates {
        let mut options = lib.library_options.clone();
        let mut dirty = false;
        if let Some(arr) = options
            .as_object_mut()
            .and_then(|obj| obj.get_mut("PathInfos"))
            .and_then(|v| v.as_array_mut())
        {
            let before = arr.len();
            arr.retain(|p| {
                p.get("Path")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.starts_with(&prefix))
                    .unwrap_or(true)
            });
            if arr.len() != before {
                dirty = true;
            }
        }
        if dirty {
            sqlx::query(
                "UPDATE libraries SET library_options = $1, date_modified = now() WHERE id = $2",
            )
            .bind(&options)
            .bind(lib.id)
            .execute(pool)
            .await?;
            updated += 1;
        }
    }

    Ok((deleted, updated))
}

/// 将 view_library_map 写回 remote_emby_sources
pub async fn update_source_view_library_map(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    map: &serde_json::Value,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE remote_emby_sources SET view_library_map = $1, updated_at = now() WHERE id = $2",
    )
    .bind(map)
    .bind(source_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_libraries(pool: &sqlx::PgPool) -> Result<Vec<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at,
               primary_image_path, primary_image_tag
        FROM libraries
        WHERE path <> '__remote_transit__'
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await?)
}

/// 返回所有 ExcludeFromSearch=true 的媒体库 ID 列表
pub async fn search_excluded_library_ids(pool: &sqlx::PgPool) -> Result<Vec<Uuid>, AppError> {
    let libs = list_libraries(pool).await?;
    let mut excluded = Vec::new();
    for lib in &libs {
        let opts = library_options(lib);
        if opts.exclude_from_search {
            excluded.push(lib.id);
        }
    }
    Ok(excluded)
}

/// 返回所有 ImportMissingEpisodes=true 的媒体库 ID 列表
pub async fn missing_episodes_enabled_library_ids(pool: &sqlx::PgPool) -> Result<Vec<Uuid>, AppError> {
    let libs = list_libraries(pool).await?;
    let mut enabled = Vec::new();
    for lib in &libs {
        let opts = library_options(lib);
        if opts.import_missing_episodes {
            enabled.push(lib.id);
        }
    }
    Ok(enabled)
}

pub async fn get_library(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at,
               primary_image_path, primary_image_tag
        FROM libraries
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_remote_emby_sources(
    pool: &sqlx::PgPool,
) -> Result<Vec<DbRemoteEmbySource>, AppError> {
    Ok(sqlx::query_as::<_, DbRemoteEmbySource>(
        r#"
        SELECT
            id, name, server_url, username, password, spoofed_user_agent, target_library_id, display_mode, remote_view_ids, remote_views,
            enabled, remote_user_id, access_token, source_secret, last_sync_at, last_sync_error,
            strm_output_path, sync_metadata, sync_subtitles, token_refresh_interval_secs, last_token_refresh_at,
            view_library_map, proxy_mode, auto_sync_interval_minutes,
            page_size, request_interval_ms,
            spoofed_client, spoofed_device_name, spoofed_device_id, spoofed_app_version,
            enable_auto_delete,
            created_at, updated_at
        FROM remote_emby_sources
        ORDER BY name
        "#,
    )
    .fetch_all(pool)
    .await?)
}

/// 查找映射到指定本地库的远端 Emby 源（通过 target_library_id 或 view_library_map）
pub async fn find_remote_sources_for_library(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<Vec<DbRemoteEmbySource>, AppError> {
    let lib_id_str = library_id.to_string();
    Ok(sqlx::query_as::<_, DbRemoteEmbySource>(
        r#"
        SELECT
            id, name, server_url, username, password, spoofed_user_agent, target_library_id, display_mode, remote_view_ids, remote_views,
            enabled, remote_user_id, access_token, source_secret, last_sync_at, last_sync_error,
            strm_output_path, sync_metadata, sync_subtitles, token_refresh_interval_secs, last_token_refresh_at,
            view_library_map, proxy_mode, auto_sync_interval_minutes,
            page_size, request_interval_ms,
            spoofed_client, spoofed_device_name, spoofed_device_id, spoofed_app_version,
            enable_auto_delete,
            created_at, updated_at
        FROM remote_emby_sources
        WHERE enabled = true
          AND (target_library_id = $1 OR view_library_map::text LIKE '%' || $2 || '%')
        ORDER BY name
        "#,
    )
    .bind(library_id)
    .bind(&lib_id_str)
    .fetch_all(pool)
    .await?)
}

pub async fn get_remote_emby_source(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<DbRemoteEmbySource>, AppError> {
    Ok(sqlx::query_as::<_, DbRemoteEmbySource>(
        r#"
        SELECT
            id, name, server_url, username, password, spoofed_user_agent, target_library_id, display_mode, remote_view_ids, remote_views,
            enabled, remote_user_id, access_token, source_secret, last_sync_at, last_sync_error,
            strm_output_path, sync_metadata, sync_subtitles, token_refresh_interval_secs, last_token_refresh_at,
            view_library_map, proxy_mode, auto_sync_interval_minutes,
            page_size, request_interval_ms,
            spoofed_client, spoofed_device_name, spoofed_device_id, spoofed_app_version,
            enable_auto_delete,
            created_at, updated_at
        FROM remote_emby_sources
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn create_remote_emby_source(
    pool: &sqlx::PgPool,
    name: &str,
    server_url: &str,
    username: &str,
    password: &str,
    spoofed_user_agent: &str,
    target_library_id: Uuid,
    display_mode: &str,
    remote_view_ids: &[String],
    remote_views: &Value,
    enabled: bool,
    strm_output_path: Option<&str>,
    sync_metadata: bool,
    sync_subtitles: bool,
    token_refresh_interval_secs: i32,
    proxy_mode: &str,
    view_library_map: Option<&Value>,
    auto_sync_interval_minutes: i32,
    page_size: i32,
    request_interval_ms: i32,
    enable_auto_delete: bool,
    spoofed_client: Option<&str>,
    spoofed_device_name: Option<&str>,
    spoofed_device_id: Option<&str>,
    spoofed_app_version: Option<&str>,
) -> Result<DbRemoteEmbySource, AppError> {
    let name = name.trim();
    let server_url = server_url.trim().trim_end_matches('/');
    let username = username.trim();
    let spoofed_user_agent = spoofed_user_agent.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("远端源名称不能为空".to_string()));
    }
    if server_url.is_empty() {
        return Err(AppError::BadRequest("远端 Emby 地址不能为空".to_string()));
    }
    if username.is_empty() {
        return Err(AppError::BadRequest("远端 Emby 用户名不能为空".to_string()));
    }
    if password.trim().is_empty() {
        return Err(AppError::BadRequest("远端 Emby 密码不能为空".to_string()));
    }
    if spoofed_user_agent.is_empty() {
        return Err(AppError::BadRequest("伪装 User-Agent 不能为空".to_string()));
    }

    if get_library(pool, target_library_id).await?.is_none() {
        return Err(AppError::BadRequest("目标媒体库不存在".to_string()));
    }
    let display_mode = match display_mode.trim().to_ascii_lowercase().as_str() {
        "merge" => "merge",
        _ => "separate",
    };
    let mut sanitized_remote_view_ids = Vec::new();
    let mut selected_remote_view_id_set = HashSet::new();
    for raw in remote_view_ids {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        let lowercase = value.to_ascii_lowercase();
        if selected_remote_view_id_set.insert(lowercase) {
            sanitized_remote_view_ids.push(value.to_string());
        }
    }
    let mut sanitized_remote_views = Vec::new();
    let mut included_view_id_set = HashSet::new();
    if let Some(items) = remote_views.as_array() {
        for item in items {
            let Some(map) = item.as_object() else {
                continue;
            };
            let id = map
                .get("Id")
                .or_else(|| map.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if id.is_empty() {
                continue;
            }
            if !selected_remote_view_id_set.is_empty()
                && !selected_remote_view_id_set.contains(&id.to_ascii_lowercase())
            {
                continue;
            }
            let id_lower = id.to_ascii_lowercase();
            if !included_view_id_set.insert(id_lower) {
                continue;
            }
            let name = map
                .get("Name")
                .or_else(|| map.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(id);
            let collection_type = map
                .get("CollectionType")
                .or_else(|| map.get("collectionType"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            sanitized_remote_views.push(json!({
                "Id": id,
                "Name": name,
                "CollectionType": collection_type,
            }));
        }
    }
    for id in &sanitized_remote_view_ids {
        if included_view_id_set.insert(id.to_ascii_lowercase()) {
            sanitized_remote_views.push(json!({
                "Id": id,
                "Name": id,
                "CollectionType": Value::Null,
            }));
        }
    }
    let strm_output_path = strm_output_path.map(|s| s.trim()).filter(|s| !s.is_empty());
    let token_refresh_interval_secs = token_refresh_interval_secs.clamp(300, 86400 * 30);
    // 0 = 关闭；正值在 [1, 7 天] 区间钳制，避免无效配置
    let auto_sync_interval_minutes = if auto_sync_interval_minutes <= 0 {
        0
    } else {
        auto_sync_interval_minutes.clamp(1, 60 * 24 * 7)
    };
    let proxy_mode = match proxy_mode.trim().to_ascii_lowercase().as_str() {
        "redirect" => "redirect",
        "redirect_direct" => "redirect_direct",
        _ => "proxy",
    };
    // 拉取速率：page_size <= 0 退默认 200，clamp [50, 1000]；request_interval_ms 负数归零，clamp [0, 60_000ms]。
    let page_size = if page_size <= 0 { 200 } else { page_size.clamp(50, 1000) };
    let request_interval_ms = request_interval_ms.max(0).min(60_000);
    let source_secret = Uuid::new_v4();
    let row_id = Uuid::new_v4();
    // remote_views 列类型为 jsonb（单个 JSON 数组），必须 wrap 成 Value::Array
    let remote_views_json = Value::Array(sanitized_remote_views);
    let vlm = view_library_map.cloned().unwrap_or_else(|| serde_json::json!({}));

    // PB39：身份伪装。任一为空时回落到 Infuse-Direct on Apple TV 默认；spoofed_device_id 缺失
    // 时用 `Uuid::new_v4` 派生 32 位 hex（不带 'movie-rust-' 前缀），首次创建即固定。
    let spoofed_client = spoofed_client
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Infuse-Direct");
    let spoofed_device_name = spoofed_device_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Apple TV");
    let spoofed_device_id = spoofed_device_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().simple().to_string());
    let spoofed_app_version = spoofed_app_version
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("8.2.4");

    sqlx::query(
        r#"
        INSERT INTO remote_emby_sources (
            id, name, server_url, username, password, spoofed_user_agent, target_library_id,
            display_mode, remote_view_ids, remote_views, enabled, source_secret,
            strm_output_path, sync_metadata, sync_subtitles, token_refresh_interval_secs, proxy_mode,
            view_library_map, auto_sync_interval_minutes, page_size, request_interval_ms,
            spoofed_client, spoofed_device_name, spoofed_device_id, spoofed_app_version,
            enable_auto_delete
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25, $26)
        "#,
    )
    .bind(row_id)
    .bind(name)
    .bind(server_url)
    .bind(username)
    .bind(password)
    .bind(spoofed_user_agent)
    .bind(target_library_id)
    .bind(display_mode)
    .bind(sanitized_remote_view_ids)
    .bind(&remote_views_json)
    .bind(enabled)
    .bind(source_secret)
    .bind(strm_output_path)
    .bind(sync_metadata)
    .bind(sync_subtitles)
    .bind(token_refresh_interval_secs)
    .bind(proxy_mode)
    .bind(&vlm)
    .bind(auto_sync_interval_minutes)
    .bind(page_size)
    .bind(request_interval_ms)
    .bind(spoofed_client)
    .bind(spoofed_device_name)
    .bind(spoofed_device_id)
    .bind(spoofed_app_version)
    .bind(enable_auto_delete)
    .execute(pool)
    .await?;

    get_remote_emby_source(pool, row_id)
        .await?
        .ok_or_else(|| AppError::Internal("创建远端 Emby 源后无法读取记录".to_string()))
}

pub async fn delete_remote_emby_source(pool: &sqlx::PgPool, id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query("DELETE FROM remote_emby_sources WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("远端 Emby 源不存在".to_string()));
    }
    Ok(())
}

/// 更新远端 Emby 源。`password` 为 `None` 或未填则保留数据库中原密码。
/// PB39：`spoofed_client / spoofed_device_name / spoofed_device_id / spoofed_app_version`
/// 同样为 `Option<&str>`，未传或空字符串则保留 DB 原值（**重要**：不能覆盖为空，
/// 否则远端 Devices 表里这台 device 突然换 ID 会触发 admin 告警）。
pub async fn update_remote_emby_source(
    pool: &sqlx::PgPool,
    id: Uuid,
    name: &str,
    server_url: &str,
    username: &str,
    password: Option<&str>,
    spoofed_user_agent: &str,
    target_library_id: Uuid,
    display_mode: &str,
    remote_view_ids: &[String],
    remote_views: &Value,
    enabled: bool,
    strm_output_path: Option<&str>,
    sync_metadata: bool,
    sync_subtitles: bool,
    token_refresh_interval_secs: i32,
    proxy_mode: &str,
    view_library_map: Option<&Value>,
    auto_sync_interval_minutes: i32,
    page_size: i32,
    request_interval_ms: i32,
    enable_auto_delete: bool,
    spoofed_client: Option<&str>,
    spoofed_device_name: Option<&str>,
    spoofed_device_id: Option<&str>,
    spoofed_app_version: Option<&str>,
) -> Result<DbRemoteEmbySource, AppError> {
    let name = name.trim();
    let server_url = server_url.trim().trim_end_matches('/');
    let username = username.trim();
    let spoofed_user_agent = spoofed_user_agent.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("远端源名称不能为空".to_string()));
    }
    if server_url.is_empty() {
        return Err(AppError::BadRequest("远端 Emby 地址不能为空".to_string()));
    }
    if username.is_empty() {
        return Err(AppError::BadRequest("远端 Emby 用户名不能为空".to_string()));
    }
    if get_remote_emby_source(pool, id).await?.is_none() {
        return Err(AppError::NotFound("远端 Emby 源不存在".to_string()));
    }
    if spoofed_user_agent.is_empty() {
        return Err(AppError::BadRequest("伪装 User-Agent 不能为空".to_string()));
    }
    if get_library(pool, target_library_id).await?.is_none() {
        return Err(AppError::BadRequest("目标媒体库不存在".to_string()));
    }
    let display_mode = match display_mode.trim().to_ascii_lowercase().as_str() {
        "merge" => "merge",
        _ => "separate",
    };
    let mut sanitized_remote_view_ids = Vec::new();
    let mut selected_remote_view_id_set = HashSet::new();
    for raw in remote_view_ids {
        let value = raw.trim();
        if value.is_empty() {
            continue;
        }
        let lowercase = value.to_ascii_lowercase();
        if selected_remote_view_id_set.insert(lowercase) {
            sanitized_remote_view_ids.push(value.to_string());
        }
    }
    let mut sanitized_remote_views = Vec::new();
    let mut included_view_id_set = HashSet::new();
    if let Some(items) = remote_views.as_array() {
        for item in items {
            let Some(map) = item.as_object() else {
                continue;
            };
            let vid = map
                .get("Id")
                .or_else(|| map.get("id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or("");
            if vid.is_empty() {
                continue;
            }
            if !selected_remote_view_id_set.is_empty()
                && !selected_remote_view_id_set.contains(&vid.to_ascii_lowercase())
            {
                continue;
            }
            let id_lower = vid.to_ascii_lowercase();
            if !included_view_id_set.insert(id_lower) {
                continue;
            }
            let vname = map
                .get("Name")
                .or_else(|| map.get("name"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(vid);
            let collection_type = map
                .get("CollectionType")
                .or_else(|| map.get("collectionType"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            sanitized_remote_views.push(json!({
                "Id": vid,
                "Name": vname,
                "CollectionType": collection_type,
            }));
        }
    }
    for vid in &sanitized_remote_view_ids {
        if included_view_id_set.insert(vid.to_ascii_lowercase()) {
            sanitized_remote_views.push(json!({
                "Id": vid,
                "Name": vid,
                "CollectionType": Value::Null,
            }));
        }
    }
    let sanitized_remote_views = Value::Array(sanitized_remote_views);
    let strm_trim = strm_output_path.unwrap_or("").trim();
    let strm_binding: Option<&str> = (!strm_trim.is_empty()).then_some(strm_trim);
    let token_refresh_interval_secs = token_refresh_interval_secs.clamp(300, 86400 * 30);
    let auto_sync_interval_minutes = if auto_sync_interval_minutes <= 0 {
        0
    } else {
        auto_sync_interval_minutes.clamp(1, 60 * 24 * 7)
    };
    let proxy_mode = match proxy_mode.trim().to_ascii_lowercase().as_str() {
        "redirect" => "redirect",
        "redirect_direct" => "redirect_direct",
        _ => "proxy",
    };
    let page_size = if page_size <= 0 { 200 } else { page_size.clamp(50, 1000) };
    let request_interval_ms = request_interval_ms.max(0).min(60_000);

    // PB39：四个 spoofed_* 入参 → Option<&str>，None / 空字符串都按 None 处理，
    // SQL 用 `COALESCE(NULLIF($::text, ''), 列)` 在写入时保留旧值。
    let spoofed_client_param = spoofed_client.map(str::trim).filter(|s| !s.is_empty());
    let spoofed_device_name_param = spoofed_device_name.map(str::trim).filter(|s| !s.is_empty());
    let spoofed_device_id_param = spoofed_device_id.map(str::trim).filter(|s| !s.is_empty());
    let spoofed_app_version_param = spoofed_app_version.map(str::trim).filter(|s| !s.is_empty());

    let vlm = view_library_map.cloned().unwrap_or_else(|| serde_json::json!({}));
    let rows = if let Some(pw) = password.filter(|p| !p.trim().is_empty()) {
        sqlx::query(
            r#"
            UPDATE remote_emby_sources SET
                name = $2, server_url = $3, username = $4, password = $5,
                spoofed_user_agent = $6, target_library_id = $7, display_mode = $8,
                remote_view_ids = $9, remote_views = $10, enabled = $11,
                strm_output_path = $12, sync_metadata = $13, sync_subtitles = $14,
                token_refresh_interval_secs = $15, proxy_mode = $16,
                view_library_map = $17, auto_sync_interval_minutes = $18,
                page_size = $19, request_interval_ms = $20,
                spoofed_client       = COALESCE(NULLIF($21::text, ''), spoofed_client),
                spoofed_device_name  = COALESCE(NULLIF($22::text, ''), spoofed_device_name),
                spoofed_device_id    = COALESCE(NULLIF($23::text, ''), spoofed_device_id),
                spoofed_app_version  = COALESCE(NULLIF($24::text, ''), spoofed_app_version),
                enable_auto_delete   = $25,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(server_url)
        .bind(username)
        .bind(pw.trim())
        .bind(spoofed_user_agent)
        .bind(target_library_id)
        .bind(display_mode)
        .bind(sanitized_remote_view_ids.clone())
        .bind(&sanitized_remote_views)
        .bind(enabled)
        .bind(strm_binding)
        .bind(sync_metadata)
        .bind(sync_subtitles)
        .bind(token_refresh_interval_secs)
        .bind(proxy_mode)
        .bind(&vlm)
        .bind(auto_sync_interval_minutes)
        .bind(page_size)
        .bind(request_interval_ms)
        .bind(spoofed_client_param)
        .bind(spoofed_device_name_param)
        .bind(spoofed_device_id_param)
        .bind(spoofed_app_version_param)
        .bind(enable_auto_delete)
        .execute(pool)
        .await?
    } else {
        sqlx::query(
            r#"
            UPDATE remote_emby_sources SET
                name = $2, server_url = $3, username = $4,
                spoofed_user_agent = $5, target_library_id = $6, display_mode = $7,
                remote_view_ids = $8, remote_views = $9, enabled = $10,
                strm_output_path = $11, sync_metadata = $12, sync_subtitles = $13,
                token_refresh_interval_secs = $14, proxy_mode = $15,
                view_library_map = $16, auto_sync_interval_minutes = $17,
                page_size = $18, request_interval_ms = $19,
                spoofed_client       = COALESCE(NULLIF($20::text, ''), spoofed_client),
                spoofed_device_name  = COALESCE(NULLIF($21::text, ''), spoofed_device_name),
                spoofed_device_id    = COALESCE(NULLIF($22::text, ''), spoofed_device_id),
                spoofed_app_version  = COALESCE(NULLIF($23::text, ''), spoofed_app_version),
                enable_auto_delete   = $24,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(name)
        .bind(server_url)
        .bind(username)
        .bind(spoofed_user_agent)
        .bind(target_library_id)
        .bind(display_mode)
        .bind(sanitized_remote_view_ids.clone())
        .bind(&sanitized_remote_views)
        .bind(enabled)
        .bind(strm_binding)
        .bind(sync_metadata)
        .bind(sync_subtitles)
        .bind(token_refresh_interval_secs)
        .bind(proxy_mode)
        .bind(&vlm)
        .bind(auto_sync_interval_minutes)
        .bind(page_size)
        .bind(request_interval_ms)
        .bind(spoofed_client_param)
        .bind(spoofed_device_name_param)
        .bind(spoofed_device_id_param)
        .bind(spoofed_app_version_param)
        .bind(enable_auto_delete)
        .execute(pool)
        .await?
    };
    if rows.rows_affected() == 0 {
        return Err(AppError::NotFound("远端 Emby 源不存在".to_string()));
    }
    get_remote_emby_source(pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound("远端 Emby 源不存在".to_string()))
}

pub async fn update_remote_emby_source_last_token_refresh(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE remote_emby_sources
        SET last_token_refresh_at = now(), updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_remote_emby_source_auth_state(
    pool: &sqlx::PgPool,
    id: Uuid,
    remote_user_id: &str,
    access_token: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE remote_emby_sources
        SET remote_user_id = $2, access_token = $3, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(remote_user_id.trim())
    .bind(access_token.trim())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_remote_emby_source_auth_state(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        UPDATE remote_emby_sources
        SET remote_user_id = NULL, access_token = NULL, updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 轮换远端源的 spoofed_device_id，同时清空 auth 状态迫使下次请求重新登录。
/// 用于远端设备被封禁导致持续 401/403/连接失败时的自动恢复。
pub async fn rotate_remote_emby_source_device_id(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<String, AppError> {
    let new_device_id = Uuid::new_v4().simple().to_string();
    sqlx::query(
        r#"
        UPDATE remote_emby_sources
        SET spoofed_device_id = $2,
            remote_user_id = NULL,
            access_token = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(&new_device_id)
    .execute(pool)
    .await?;
    Ok(new_device_id)
}

/// PB49：取出指定 (source, view) 的续抓游标。
///
/// 仅当存储的 `incremental_since` 与本次同步语义匹配时才返回 `Some(start_index)`，
/// 否则返回 None。语义：
///   - 本次全量（`incremental_since = None`）⇄ 存储的 incremental_since IS NULL
///   - 本次增量（`incremental_since = Some(t)`）⇄ 存储的 incremental_since = t
///
/// 不匹配的旧游标会在后续 `save_view_progress` 时被 UPSERT 覆盖，无需手动清理。
pub async fn get_view_progress(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    view_id: &str,
    incremental_since: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Option<i64>, AppError> {
    struct Row {
        last_start_index: i64,
        incremental_since: Option<chrono::DateTime<chrono::Utc>>,
    }
    let row_opt: Option<Row> = sqlx::query(
        r#"
        SELECT last_start_index, incremental_since
        FROM remote_emby_source_view_progress
        WHERE source_id = $1 AND view_id = $2
        "#,
    )
    .bind(source_id)
    .bind(view_id)
    .fetch_optional(pool)
    .await?
    .map(|r| {
        use sqlx::Row as _;
        Row {
            last_start_index: r.get("last_start_index"),
            incremental_since: r.get("incremental_since"),
        }
    });

    let Some(row) = row_opt else {
        return Ok(None);
    };
    if row.incremental_since == incremental_since {
        Ok(Some(row.last_start_index))
    } else {
        Ok(None)
    }
}

/// PB49：保存指定 (source, view) 的续抓游标。UPSERT 语义，每对 (source, view) 至多一行。
pub async fn save_view_progress(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    view_id: &str,
    last_start_index: i64,
    total_record_count: i64,
    incremental_since: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO remote_emby_source_view_progress
            (source_id, view_id, last_start_index, total_record_count, incremental_since, updated_at)
        VALUES ($1, $2, $3, $4, $5, now())
        ON CONFLICT (source_id, view_id) DO UPDATE SET
            last_start_index   = EXCLUDED.last_start_index,
            total_record_count = EXCLUDED.total_record_count,
            incremental_since  = EXCLUDED.incremental_since,
            updated_at         = now()
        "#,
    )
    .bind(source_id)
    .bind(view_id)
    .bind(last_start_index)
    .bind(total_record_count)
    .bind(incremental_since)
    .execute(pool)
    .await?;
    Ok(())
}

/// PB49：清空指定 source 的所有续抓游标。在整次 sync 成功完成时调用，
/// 让下次同步从 start_index=0 重新开始（避免 stale 游标累积）。
pub async fn clear_source_view_progress(
    pool: &sqlx::PgPool,
    source_id: Uuid,
) -> Result<u64, AppError> {
    let result = sqlx::query(
        "UPDATE remote_emby_source_view_progress \
         SET last_start_index = 0, incremental_since = NULL, total_record_count = 0, updated_at = now() \
         WHERE source_id = $1",
    )
    .bind(source_id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// PB49 (S1)：批量查询「path 在 DB 里 + 由远端 source 管理」的条目，返回 path → updated_at 映射。
///
/// scanner 在 Phase B 入口前用这把短路：远端 sync 刚刚写完的 STRM 文件，
/// 如果文件 mtime 不晚于 DB 里的 updated_at，就完全没有理由再走一遍
/// `import_tv_file` / `import_movie_file` 整套流水（NFO 解析 + DB upsert ×3
/// + TMDB/OpenSubtitles HTTP 刷元数据 + trickplay/segments 后扫等）。
///
/// 实测对 25 万条目的库，scanner 阶段从 ~60 分钟纯 DB upsert 压到几秒钟（只剩
/// 真正新增的本地物理文件需要 import）。
///
/// 为避免单次 SQL 参数 array 过大（PG 默认 64K 上限够用，但 1M+ paths 也要保险），
/// 这里按 8000 一批分块，结果合并到一个 HashMap。
pub async fn lookup_remote_managed_paths(
    pool: &sqlx::PgPool,
    paths: &[String],
) -> Result<std::collections::HashMap<String, DateTime<Utc>>, AppError> {
    use std::collections::HashMap;
    if paths.is_empty() {
        return Ok(HashMap::new());
    }
    const CHUNK: usize = 8000;
    let mut out: HashMap<String, DateTime<Utc>> = HashMap::with_capacity(paths.len());
    for chunk in paths.chunks(CHUNK) {
        let rows: Vec<(String, DateTime<Utc>)> = sqlx::query_as(
            r#"
            SELECT path, updated_at
            FROM media_items
            WHERE path = ANY($1)
              AND provider_ids->>'RemoteEmbySourceId' IS NOT NULL
              AND provider_ids->>'RemoteEmbySourceId' <> ''
            "#,
        )
        .bind(chunk)
        .fetch_all(pool)
        .await?;
        for (p, ts) in rows {
            out.insert(p, ts);
        }
    }
    Ok(out)
}

/// PB49 (B2)：预热「Series 详情已成功拉过」集合，跨 sync 复用。
///
/// 旧实现里 series_detail_synced 是 per-run DashSet，每次 sync 整个媒体库
/// 都要再为每部剧打一发 /Items/{seriesId} 详情拉取。这里在 sync 启动时把
/// 持久化的标记捞回来，让后续 sync 只为新增 / 用户主动刷新的剧拉详情。
pub async fn preheat_series_detail_synced(
    pool: &sqlx::PgPool,
    source_id: Uuid,
) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT remote_series_id FROM remote_emby_series_detail_synced WHERE source_id = $1",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s,)| s).collect())
}

/// PB49 (B2)：标记某 (source_id, remote_series_id) 的详情已成功落库。
/// 写入失败仅 warn，不影响主链路（下次 sync 重做即可）。
pub async fn mark_series_detail_synced(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    remote_series_id: &str,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO remote_emby_series_detail_synced (source_id, remote_series_id, synced_at)
        VALUES ($1, $2, now())
        ON CONFLICT (source_id, remote_series_id) DO UPDATE
            SET synced_at = excluded.synced_at
        "#,
    )
    .bind(source_id)
    .bind(remote_series_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// PB49 (A2)：序列启动时预热已存在的 Series 行，让本次 sync 的第一个 Episode
/// 直接命中缓存、跳过一次 DB upsert（对几千部剧的库可省 ~5-15 秒同步前置开销）。
///
/// 返回 (view_id, series_name, db_id) 三元组。view_id 取自 provider_ids，
/// 调用方据此组装 `format!("{view_id}::{sanitize(name)}")` key 写入 series_parent_map。
pub async fn preheat_series_for_source(
    pool: &sqlx::PgPool,
    source_id: Uuid,
) -> Result<Vec<(String, String, Uuid)>, AppError> {
    let rows: Vec<(Option<String>, String, Uuid)> = sqlx::query_as(
        r#"
        SELECT
            provider_ids->>'RemoteEmbyViewId' AS view_id,
            name,
            id
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND item_type = 'Series'
        "#,
    )
    .bind(source_id.to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(view_id, name, id)| view_id.filter(|v| !v.is_empty()).map(|v| (v, name, id)))
        .collect())
}

/// PB49 (A2)：序列启动时预热已存在的 Season 行。返回 (series_db_id, season_number, db_id)。
/// 调用方据此组装 `format!("{series_db_id}::{season_number}")` key 写入 season_parent_map。
pub async fn preheat_seasons_for_source(
    pool: &sqlx::PgPool,
    source_id: Uuid,
) -> Result<Vec<(Uuid, i32, Uuid)>, AppError> {
    // Season 行的 parent_id 是其 Series 行的 db id；index_number 是季号。
    let rows: Vec<(Option<Uuid>, Option<i32>, Uuid)> = sqlx::query_as(
        r#"
        SELECT parent_id, index_number, id
        FROM media_items
        WHERE provider_ids->>'RemoteEmbySourceId' = $1
          AND item_type = 'Season'
        "#,
    )
    .bind(source_id.to_string())
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(parent_id, index_number, id)| {
            let parent = parent_id?;
            let number = index_number?;
            Some((parent, number, id))
        })
        .collect())
}

/// PB49 (C4)：清掉指定 source 下「不在当前 view 列表里」的死 cursor。
///
/// 用户在远端管理员侧重建媒体库 / 删 view / 改 view_id 时，旧 view 对应的
/// cursor 行会永远留在表里且永不会再命中。同步入口处先调用此函数清理，
/// 既能让表大小有界，也避免「失败时 cursor 续抓表里有 X 行」给运维带来困惑。
pub async fn prune_source_view_progress_not_in(
    pool: &sqlx::PgPool,
    source_id: Uuid,
    keep_view_ids: &[String],
) -> Result<u64, AppError> {
    if keep_view_ids.is_empty() {
        // 没传当前 view 列表（极端边界），保守起见不动表。
        return Ok(0);
    }
    let result = sqlx::query(
        r#"
        DELETE FROM remote_emby_source_view_progress
        WHERE source_id = $1
          AND NOT (view_id = ANY($2))
        "#,
    )
    .bind(source_id)
    .bind(keep_view_ids)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

pub async fn update_remote_emby_source_sync_state(
    pool: &sqlx::PgPool,
    id: Uuid,
    error_message: Option<&str>,
) -> Result<(), AppError> {
    // 成功（error_message = None）：推进 last_sync_at = now() 并清空 last_sync_error；
    // 失败/中断（error_message = Some）：仅记录错误，**不修改** last_sync_at，
    //   避免上一次成功的增量水位线被失败/中断重写为 now()，导致下次错过远端补全数据。
    //
    // PB49 (B3) 设计决策——「软成功」的代价已经被吸收：
    //   表面上看，永远不更新 last_sync_at 似乎会让一直失败的源永远停在「全量」语义，
    //   听起来很糟糕。但全量语义在 PB49 后已经几乎无代价：
    //     1. local_synced_ids 让主循环对已入库 RemoteEmbyItemId 直接跳过 upsert；
    //     2. remote_emby_source_view_progress 让 fetch 从断点续抓；
    //     3. remote_emby_series_detail_synced 让 series 详情拉取跨 sync 复用。
    //   所以「失败后仍然是全量」的开销主要落在远端 ID 索引枚举（FetchingRemoteIndex）
    //   和「跳过已入库」的廉价 fast path 上，对单源 25 万条目实测 < 1 分钟。
    //
    //   反过来：如果失败也写 last_sync_at = now()，将出现「沉默丢数据」的硬故障——
    //   增量水位线被错误地推进到失败时刻，下一次同步只会拉「失败时刻之后变化的」
    //   远端条目，永远漏掉「上次成功 → 这次失败」窗口内被改动的条目。
    //   软成功的代价（性能）远小于硬故障的代价（正确性）。
    if let Some(message) = error_message {
        sqlx::query(
            r#"
            UPDATE remote_emby_sources
            SET
                last_sync_error = $2,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(message.trim())
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            r#"
            UPDATE remote_emby_sources
            SET
                last_sync_at = now(),
                last_sync_error = NULL,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn get_library_for_media_item(
    pool: &sqlx::PgPool,
    item_id: Uuid,
) -> Result<Option<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT l.id, l.name, l.collection_type, l.path, l.library_options, l.created_at,
               l.primary_image_path, l.primary_image_tag
        FROM media_items mi
        INNER JOIN libraries l ON l.id = mi.library_id
        WHERE mi.id = $1
        LIMIT 1
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_library_by_name(
    pool: &sqlx::PgPool,
    name: &str,
) -> Result<Option<DbLibrary>, AppError> {
    Ok(sqlx::query_as::<_, DbLibrary>(
        r#"
        SELECT id, name, collection_type, path, library_options, created_at,
               primary_image_path, primary_image_tag
        FROM libraries
        WHERE lower(name) = lower($1)
        "#,
    )
    .bind(name.trim())
    .fetch_optional(pool)
    .await?)
}

pub async fn update_library_image_path(
    pool: &sqlx::PgPool,
    id: Uuid,
    image_path: Option<&str>,
) -> Result<(), AppError> {
    let now = Utc::now();
    let new_tag = image_path.map(|_| now.timestamp().to_string());
    let result = sqlx::query(
        r#"
        UPDATE libraries
        SET primary_image_path = $2,
            primary_image_tag  = $3,
            date_modified      = $4
        WHERE id = $1
        "#,
    )
    .bind(id)
    .bind(image_path)
    .bind(new_tag)
    .bind(now)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("媒体库不存在".to_string()));
    }
    Ok(())
}

pub async fn first_library_child_image(
    pool: &sqlx::PgPool,
    library_id: Uuid,
) -> Result<Option<(Uuid, String, DateTime<Utc>)>, AppError> {
    let row: Option<(Uuid, String, DateTime<Utc>)> = sqlx::query_as(
        r#"
        SELECT id, image_primary_path, date_modified
        FROM media_items
        WHERE library_id = $1
          AND image_primary_path IS NOT NULL
          AND length(trim(image_primary_path)) > 0
          AND (image_primary_path NOT LIKE 'http://%' AND image_primary_path NOT LIKE 'https://%')
        ORDER BY
            CASE WHEN item_type IN ('Movie','Series') THEN 0 ELSE 1 END,
            sort_name
        LIMIT 1
        "#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id, path, modified)| (id, path, modified)))
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

    if !library.name.eq_ignore_ascii_case(new_name)
        && get_library_by_name(pool, new_name).await?.is_some()
    {
        return Err(AppError::BadRequest("媒体库名称已存在".to_string()));
    }

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
    let next_paths = options
        .path_infos
        .iter()
        .map(|path| path.path.clone())
        .collect::<Vec<_>>();
    validate_library_paths_available(pool, Some(id), &next_paths).await?;
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

/// 批量版：一次 SQL 拿回多个 parent 的子条目数量，消除列表场景的 N+1。
pub async fn count_item_children_batch(
    pool: &sqlx::PgPool,
    parent_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, AppError> {
    if parent_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT parent_id, COUNT(*)::bigint
        FROM media_items
        WHERE parent_id = ANY($1)
        GROUP BY parent_id
        "#,
    )
    .bind(parent_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().collect())
}

/// 把一批 folder ID 按 item_type 分桶，给递归计数走捷径。
///
/// 审计日志（2026-05-04）显示：列表接口（`IncludeItemTypes=Series` 等）每页都对
/// 每个 folder 跑一次全表 `WITH RECURSIVE descendants ...`，单查询 1.3-1.5s 直接
/// 撑爆 sqlx 连接池（出现 `time to acquire exceeded slow threshold` 2s+ warning）。
///
/// 实际 hierarchy 是 `Series→Season→Episode`，episode 行已经直接挂着 `series_id`
/// 和 `season_id`，对应索引也都有：
///   - `idx_media_items_series` ON media_items(series_id)
///   - `idx_media_items_season_id` ON media_items(season_id)
/// 所以 Series/Season 完全不需要递归 CTE，一条 GROUP BY 索引扫描秒回。
struct FolderBuckets {
    series_ids: Vec<Uuid>,
    season_ids: Vec<Uuid>,
    other_ids: Vec<Uuid>,
}

async fn split_folder_ids_by_type(
    pool: &sqlx::PgPool,
    parent_ids: &[Uuid],
) -> Result<FolderBuckets, AppError> {
    let rows: Vec<(Uuid, String)> =
        sqlx::query_as("SELECT id, item_type FROM media_items WHERE id = ANY($1)")
            .bind(parent_ids)
            .fetch_all(pool)
            .await?;
    let mut series_ids = Vec::new();
    let mut season_ids = Vec::new();
    let mut other_ids = Vec::new();
    let mut known: HashSet<Uuid> = HashSet::with_capacity(rows.len());
    for (id, item_type) in rows {
        known.insert(id);
        match item_type.as_str() {
            "Series" => series_ids.push(id),
            "Season" => season_ids.push(id),
            _ => other_ids.push(id),
        }
    }
    // 表里查不到的 id（理论上不会出现）兜底走通用路径。
    for id in parent_ids {
        if !known.contains(id) {
            other_ids.push(*id);
        }
    }
    Ok(FolderBuckets {
        series_ids,
        season_ids,
        other_ids,
    })
}

/// 批量版的递归子代计数。把每个 root 的递归 CTE 合成一次查询，
/// 避免 `count_recursive_children` 在列表里被 N 次独立执行（递归 CTE 很贵）。
///
/// Series/Season 直接走 `series_id` / `season_id` 索引；其余类型才落回递归 CTE。
pub async fn count_recursive_children_batch(
    pool: &sqlx::PgPool,
    parent_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, AppError> {
    if parent_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let buckets = split_folder_ids_by_type(pool, parent_ids).await?;

    let mut result: HashMap<Uuid, i64> = HashMap::with_capacity(parent_ids.len());

    if !buckets.series_ids.is_empty() {
        // Series 的递归子代 = 该 series 下所有 Season + Episode。media_items 中所有
        // Episode/Season 行都有 series_id 直接外键，索引扫描即可。
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            r#"
            SELECT series_id, COUNT(*)::bigint
            FROM media_items
            WHERE series_id = ANY($1)
              AND item_type IN ('Season', 'Episode')
            GROUP BY series_id
            "#,
        )
        .bind(&buckets.series_ids)
        .fetch_all(pool)
        .await?;
        for (id, count) in rows {
            result.insert(id, count);
        }
        for id in &buckets.series_ids {
            result.entry(*id).or_insert(0);
        }
    }

    if !buckets.season_ids.is_empty() {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            r#"
            SELECT season_id, COUNT(*)::bigint
            FROM media_items
            WHERE season_id = ANY($1)
              AND item_type = 'Episode'
            GROUP BY season_id
            "#,
        )
        .bind(&buckets.season_ids)
        .fetch_all(pool)
        .await?;
        for (id, count) in rows {
            result.insert(id, count);
        }
        for id in &buckets.season_ids {
            result.entry(*id).or_insert(0);
        }
    }

    if !buckets.other_ids.is_empty() {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            r#"
            WITH RECURSIVE descendants AS (
                SELECT id, parent_id, parent_id AS root_id
                FROM media_items
                WHERE parent_id = ANY($1)
                UNION ALL
                SELECT m.id, m.parent_id, d.root_id
                FROM media_items m
                INNER JOIN descendants d ON m.parent_id = d.id
            )
            SELECT root_id, COUNT(*)::bigint
            FROM descendants
            GROUP BY root_id
            "#,
        )
        .bind(&buckets.other_ids)
        .fetch_all(pool)
        .await?;
        for (id, count) in rows {
            result.insert(id, count);
        }
    }

    Ok(result)
}

/// 批量版：对一批 folder（Series/Season）统计该用户的未看子集数。
/// 返回 HashMap<parent_id, unplayed_count>。
///
/// 同样按 Series/Season/Other 分桶，前两类用索引扫，第三类才走递归 CTE。
pub async fn count_unplayed_children_batch(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    parent_ids: &[Uuid],
) -> Result<HashMap<Uuid, i64>, AppError> {
    if parent_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let buckets = split_folder_ids_by_type(pool, parent_ids).await?;

    let mut result: HashMap<Uuid, i64> = HashMap::with_capacity(parent_ids.len());

    if !buckets.series_ids.is_empty() {
        // Emby 的 UnplayedItemCount 语义只算可消费的 Episode（Season 不算 leaf）。
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            r#"
            SELECT m.series_id,
                   COUNT(*) FILTER (
                       WHERE NOT EXISTS (
                           SELECT 1 FROM user_item_data uid
                           WHERE uid.user_id = $1
                             AND uid.item_id = m.id
                             AND uid.is_played = true
                       )
                   )::bigint
            FROM media_items m
            WHERE m.series_id = ANY($2)
              AND m.item_type = 'Episode'
            GROUP BY m.series_id
            "#,
        )
        .bind(user_id)
        .bind(&buckets.series_ids)
        .fetch_all(pool)
        .await?;
        for (id, count) in rows {
            result.insert(id, count);
        }
        for id in &buckets.series_ids {
            result.entry(*id).or_insert(0);
        }
    }

    if !buckets.season_ids.is_empty() {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            r#"
            SELECT m.season_id,
                   COUNT(*) FILTER (
                       WHERE NOT EXISTS (
                           SELECT 1 FROM user_item_data uid
                           WHERE uid.user_id = $1
                             AND uid.item_id = m.id
                             AND uid.is_played = true
                       )
                   )::bigint
            FROM media_items m
            WHERE m.season_id = ANY($2)
              AND m.item_type = 'Episode'
            GROUP BY m.season_id
            "#,
        )
        .bind(user_id)
        .bind(&buckets.season_ids)
        .fetch_all(pool)
        .await?;
        for (id, count) in rows {
            result.insert(id, count);
        }
        for id in &buckets.season_ids {
            result.entry(*id).or_insert(0);
        }
    }

    if !buckets.other_ids.is_empty() {
        let rows: Vec<(Uuid, i64)> = sqlx::query_as(
            r#"
            WITH RECURSIVE descendants AS (
                SELECT id, parent_id, parent_id AS root_id
                FROM media_items
                WHERE parent_id = ANY($2)
                UNION ALL
                SELECT m.id, m.parent_id, d.root_id
                FROM media_items m
                INNER JOIN descendants d ON m.parent_id = d.id
            )
            SELECT d.root_id,
                   COUNT(*) FILTER (
                       WHERE NOT EXISTS (
                           SELECT 1 FROM user_item_data uid
                           WHERE uid.user_id = $1
                             AND uid.item_id = d.id
                             AND uid.is_played = true
                       )
                   )::bigint
            FROM descendants d
            GROUP BY d.root_id
            "#,
        )
        .bind(user_id)
        .bind(&buckets.other_ids)
        .fetch_all(pool)
        .await?;
        for (id, count) in rows {
            result.insert(id, count);
        }
    }

    Ok(result)
}

/// 批量版：一次 SQL 拿回多部剧的季数。
pub async fn count_series_seasons_batch(
    pool: &sqlx::PgPool,
    series_ids: &[Uuid],
) -> Result<HashMap<Uuid, i32>, AppError> {
    if series_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"
        SELECT parent_id, COUNT(*)::bigint
        FROM media_items
        WHERE parent_id = ANY($1)
          AND item_type = 'Season'
        GROUP BY parent_id
        "#,
    )
    .bind(series_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, count)| (id, i32::try_from(count).unwrap_or(i32::MAX)))
        .collect())
}

pub async fn count_library_items_by_type(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    item_type: &str,
) -> Result<i32, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM media_items WHERE library_id = $1 AND item_type = $2",
    )
    .bind(library_id)
    .bind(item_type)
    .fetch_one(pool)
    .await?;
    Ok(i32::try_from(count).unwrap_or(i32::MAX))
}

pub async fn count_recursive_children(
    pool: &sqlx::PgPool,
    parent_id: Uuid,
) -> Result<i64, AppError> {
    Ok(sqlx::query_scalar(
        r#"
            WITH RECURSIVE descendants AS (
                SELECT id FROM media_items WHERE parent_id = $1
                UNION ALL
                SELECT m.id
                FROM media_items m
                INNER JOIN descendants d ON m.parent_id = d.id
            )
            SELECT COUNT(*) FROM descendants
            "#,
    )
    .bind(parent_id)
    .fetch_one(pool)
    .await?)
}

pub async fn count_series_seasons(pool: &sqlx::PgPool, series_id: Uuid) -> Result<i32, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM media_items WHERE parent_id = $1 AND item_type = 'Season'",
    )
    .bind(series_id)
    .fetch_one(pool)
    .await?;
    Ok(i32::try_from(count).unwrap_or(i32::MAX))
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

/// 扫描时使用的路径 = 库 `PathInfos` 中真实磁盘路径 + **映射到本库的远端 STRM 物理子目录**，
/// 便于混合库在用户删除 `.strm`/侧车后由计划任务/实时监控扫到。
pub async fn library_scan_paths_union_remote_strm(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
) -> Result<Vec<String>, AppError> {
    let mut merged = library_paths(library);
    let sources = find_remote_sources_for_library(pool, library.id).await?;
    for pb in crate::remote_emby::strm_watch_directories_for_sources(&sources, library.id) {
        let s = pb.to_string_lossy().into_owned();
        if s.trim().is_empty() {
            continue;
        }
        let has = merged
            .iter()
            .any(|x| x.trim().eq_ignore_ascii_case(s.trim()));
        if !has {
            merged.push(s);
        }
    }
    Ok(normalize_library_paths(&merged))
}

pub fn library_to_virtual_folder_dto(library: &DbLibrary) -> VirtualFolderInfoDto {
    let options = library_options(library);
    let locations = options
        .path_infos
        .iter()
        .map(|path| path.path.clone())
        .collect::<Vec<_>>();

    let item_id = uuid_to_emby_guid(&library.id);
    VirtualFolderInfoDto {
        name: library.name.clone(),
        collection_type: library.collection_type.clone(),
        guid: item_id.clone(),
        item_id,
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

    if options
        .preferred_image_language
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
    {
        options.preferred_image_language = options.preferred_metadata_language.clone();
    }

    if options.min_collection_items < 2 {
        options.min_collection_items = 2;
    }

    options
}

async fn validate_library_paths_available(
    pool: &sqlx::PgPool,
    exclude_library_id: Option<Uuid>,
    requested_paths: &[String],
) -> Result<(), AppError> {
    let normalized_requested = requested_paths
        .iter()
        .map(|path| normalize_path_for_compare(path))
        .collect::<Vec<_>>();

    let libraries = list_libraries(pool).await?;
    for library in libraries {
        if exclude_library_id == Some(library.id) {
            continue;
        }

        for existing_path in library_paths(&library) {
            let normalized_existing = normalize_path_for_compare(&existing_path);
            for requested_path in &normalized_requested {
                if paths_conflict(&normalized_existing, requested_path) {
                    return Err(AppError::BadRequest(format!(
                        "媒体路径与现有媒体库“{}”冲突: {}",
                        library.name, existing_path
                    )));
                }
            }
        }
    }

    Ok(())
}

fn normalize_path_for_compare(path: &str) -> String {
    let replaced = path.trim().replace('\\', "/");
    let trimmed = replaced.trim_end_matches('/').to_string();
    trimmed.to_ascii_lowercase()
}

fn paths_conflict(left: &str, right: &str) -> bool {
    left == right || is_parent_path(left, right) || is_parent_path(right, left)
}

fn is_parent_path(parent: &str, child: &str) -> bool {
    if parent.is_empty() || child.is_empty() || parent == child {
        return false;
    }

    let mut prefix = parent.to_string();
    prefix.push('/');
    child.starts_with(&prefix)
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

#[derive(Clone)]
pub struct ItemListOptions {
    pub user_id: Option<Uuid>,
    pub library_id: Option<Uuid>,
    pub parent_id: Option<Uuid>,
    pub item_ids: Vec<Uuid>,
    pub include_types: Vec<String>,
    pub exclude_types: Vec<String>,
    pub media_types: Vec<String>,
    pub video_types: Vec<String>,
    pub image_types: Vec<String>,
    pub genres: Vec<String>,
    pub official_ratings: Vec<String>,
    pub tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub years: Vec<i32>,
    pub person_ids: Vec<Uuid>,
    pub person_types: Vec<String>,
    pub artists: Vec<String>,
    pub artist_ids: Vec<Uuid>,
    pub albums: Vec<String>,
    pub studios: Vec<String>,
    pub studio_ids: Vec<String>,
    pub containers: Vec<String>,
    pub audio_codecs: Vec<String>,
    pub video_codecs: Vec<String>,
    pub subtitle_codecs: Vec<String>,
    pub any_provider_id_equals: Vec<String>,
    pub is_played: Option<bool>,
    pub is_favorite: Option<bool>,
    pub is_folder: Option<bool>,
    pub is_hd: Option<bool>,
    pub is_3d: Option<bool>,
    pub is_locked: Option<bool>,
    pub is_place_holder: Option<bool>,
    pub has_overview: Option<bool>,
    pub has_subtitles: Option<bool>,
    pub has_trailer: Option<bool>,
    pub has_theme_song: Option<bool>,
    pub has_theme_video: Option<bool>,
    pub has_special_feature: Option<bool>,
    pub has_tmdb_id: Option<bool>,
    pub has_imdb_id: Option<bool>,
    pub series_status: Vec<String>,
    pub min_community_rating: Option<f64>,
    pub min_critic_rating: Option<f64>,
    pub min_premiere_date: Option<DateTime<Utc>>,
    pub max_premiere_date: Option<DateTime<Utc>>,
    pub min_start_date: Option<DateTime<Utc>>,
    pub max_start_date: Option<DateTime<Utc>>,
    pub min_end_date: Option<DateTime<Utc>>,
    pub max_end_date: Option<DateTime<Utc>>,
    pub min_date_last_saved: Option<DateTime<Utc>>,
    pub max_date_last_saved: Option<DateTime<Utc>>,
    pub min_date_last_saved_for_user: Option<DateTime<Utc>>,
    pub max_date_last_saved_for_user: Option<DateTime<Utc>>,
    pub aired_during_season: Option<i32>,
    pub project_to_media: bool,
    pub resume_only: bool,
    pub recursive: bool,
    pub search_term: Option<String>,
    pub name_starts_with: Option<String>,
    pub name_starts_with_or_greater: Option<String>,
    pub name_less_than: Option<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub filters: Option<String>,
    pub fields: Option<String>,
    pub start_index: i64,
    pub limit: i64,
    pub group_items_into_collections: bool,
    pub collapse_box_set_items: bool,
    pub enable_total_record_count: bool,
    /// 由 list_media_items 入口自动注入：当请求来自普通用户且其 `EnableAllFolders=false`
    /// 时，这里会被填成"该用户允许访问的 library_id 白名单"；为空表示用户什么都看不到，
    /// 应当直接返回 0 行。`None` 表示无需叠加白名单（admin / 全可见）。
    pub allowed_library_ids: Option<Vec<Uuid>>,
    /// 由 list_media_items 根据用户策略自动注入：排除超过此分级的内容
    pub policy_max_parental_rating: Option<i32>,
    /// 由 list_media_items 根据用户策略自动注入：排除含有这些标签的内容
    pub policy_blocked_tags: Vec<String>,
    /// 由 list_media_items 根据用户策略自动注入：排除未分级的特定类型
    pub policy_block_unrated_items: Vec<String>,
    /// 当搜索时自动注入：exclude_from_search=true 的媒体库 ID 列表
    pub excluded_library_ids: Vec<Uuid>,
}

impl Default for ItemListOptions {
    fn default() -> Self {
        Self {
            user_id: None,
            library_id: None,
            parent_id: None,
            item_ids: Vec::new(),
            include_types: Vec::new(),
            exclude_types: Vec::new(),
            media_types: Vec::new(),
            video_types: Vec::new(),
            image_types: Vec::new(),
            genres: Vec::new(),
            official_ratings: Vec::new(),
            tags: Vec::new(),
            exclude_tags: Vec::new(),
            years: Vec::new(),
            person_ids: Vec::new(),
            person_types: Vec::new(),
            artists: Vec::new(),
            artist_ids: Vec::new(),
            albums: Vec::new(),
            studios: Vec::new(),
            studio_ids: Vec::new(),
            containers: Vec::new(),
            audio_codecs: Vec::new(),
            video_codecs: Vec::new(),
            subtitle_codecs: Vec::new(),
            any_provider_id_equals: Vec::new(),
            is_played: None,
            is_favorite: None,
            is_folder: None,
            is_hd: None,
            is_3d: None,
            is_locked: None,
            is_place_holder: None,
            has_overview: None,
            has_subtitles: None,
            has_trailer: None,
            has_theme_song: None,
            has_theme_video: None,
            has_special_feature: None,
            has_tmdb_id: None,
            has_imdb_id: None,
            series_status: Vec::new(),
            min_community_rating: None,
            min_critic_rating: None,
            min_premiere_date: None,
            max_premiere_date: None,
            min_start_date: None,
            max_start_date: None,
            min_end_date: None,
            max_end_date: None,
            min_date_last_saved: None,
            max_date_last_saved: None,
            min_date_last_saved_for_user: None,
            max_date_last_saved_for_user: None,
            aired_during_season: None,
            project_to_media: false,
            resume_only: false,
            recursive: false,
            search_term: None,
            name_starts_with: None,
            name_starts_with_or_greater: None,
            name_less_than: None,
            sort_by: None,
            sort_order: None,
            filters: None,
            fields: None,
            start_index: 0,
            limit: 100,
            group_items_into_collections: true,
            collapse_box_set_items: false,
            enable_total_record_count: true,
            allowed_library_ids: None,
            policy_max_parental_rating: None,
            policy_blocked_tags: Vec::new(),
            policy_block_unrated_items: Vec::new(),
            excluded_library_ids: Vec::new(),
        }
    }
}

/// 把"是否需要直接短路返回空集"判断与"白名单 ANY 子句"封装在一处。
fn check_allowed_library_short_circuit(options: &ItemListOptions) -> bool {
    let Some(allowed) = options.allowed_library_ids.as_ref() else {
        return false;
    };
    if allowed.is_empty() {
        return true;
    }
    if let Some(library_id) = options.library_id {
        if !allowed.contains(&library_id) {
            return true;
        }
    }
    false
}

/// 在 `WHERE 1 = 1` 之后追加 `AND library_id = ANY(allowed_library_ids)`。
///
/// PB10 防御加固：即便上游显式给了 `options.library_id`（路由层拿到的客户端 ParentId
/// 或 library 参数），只要用户绑定了非空白名单，依旧把 `library_id = ANY(allowed)`
/// 注入；与 `check_allowed_library_short_circuit` 一起形成 "客户端越权读取隐藏库
/// COUNT" 的双重校验：一处早返空，一处即便绕过 short-circuit 也由 SQL 层过滤掉。
fn push_allowed_library_filter<'a>(
    builder: &mut QueryBuilder<'a, Postgres>,
    options: &ItemListOptions,
) {
    if let Some(allowed) = options.allowed_library_ids.as_ref() {
        if !allowed.is_empty() {
            builder
                .push(" AND library_id = ANY(")
                .push_bind(allowed.clone())
                .push(")");
        }
    }
}

async fn fast_count_media_items(
    pool: &sqlx::PgPool,
    options: &ItemListOptions,
) -> Result<i64, AppError> {
    if check_allowed_library_short_circuit(options) {
        return Ok(0);
    }

    // PB10：白名单非空就一律视为"有库过滤"，即便上游显式传 library_id，也强制走通用路径
    // 让 push_allowed_library_filter 真正生效，避免快速路径绕过 allowed_library_ids。
    let has_user_library_filter = options
        .allowed_library_ids
        .as_ref()
        .is_some_and(|list| !list.is_empty());

    let has_type_filter = !options.include_types.is_empty();
    let has_no_conditions = options.library_id.is_none()
        && options.parent_id.is_none()
        && options.item_ids.is_empty()
        && options.include_types.is_empty()
        && options.exclude_types.is_empty()
        && options.genres.is_empty()
        && options.tags.is_empty()
        && options.search_term.as_ref().map_or(true, |s| s.trim().is_empty())
        && options.years.is_empty()
        && options.person_ids.is_empty()
        && options.is_played.is_none()
        && options.is_favorite.is_none()
        && !options.resume_only
        && options.min_community_rating.is_none()
        && options.min_premiere_date.is_none()
        && options.max_premiere_date.is_none()
        && options.studios.is_empty()
        && options.official_ratings.is_empty()
        && options.containers.is_empty()
        && options.audio_codecs.is_empty()
        && options.video_codecs.is_empty()
        && options.has_overview.is_none()
        && options.has_subtitles.is_none()
        && options.has_trailer.is_none()
        && options.has_tmdb_id.is_none()
        && options.has_imdb_id.is_none()
        && options.series_status.is_empty()
        && options.name_starts_with.as_ref().map_or(true, |s| s.trim().is_empty())
        && options.is_folder.is_none()
        && options.is_hd.is_none()
        && options.filters.is_none()
        && options.any_provider_id_equals.is_empty()
        && !has_user_library_filter;

    if has_no_conditions && options.recursive {
        let est: Option<f32> = sqlx::query_scalar(
            "SELECT reltuples::real FROM pg_class WHERE relname = 'media_items'"
        )
        .fetch_optional(pool)
        .await?;
        if let Some(est) = est {
            if est > 0.0 {
                return Ok(est as i64);
            }
        }
    }

    let simple_filter = options.search_term.as_ref().map_or(true, |s| s.trim().is_empty())
        && options.genres.is_empty()
        && options.tags.is_empty()
        && options.years.is_empty()
        && options.person_ids.is_empty()
        && options.person_types.is_empty()
        && options.is_played.is_none()
        && options.is_favorite.is_none()
        && !options.resume_only
        && options.studios.is_empty()
        && options.official_ratings.is_empty()
        && options.min_community_rating.is_none()
        && options.min_premiere_date.is_none()
        && options.max_premiere_date.is_none()
        && options.containers.is_empty()
        && options.audio_codecs.is_empty()
        && options.video_codecs.is_empty()
        && options.has_overview.is_none()
        && options.has_subtitles.is_none()
        && options.has_trailer.is_none()
        && options.has_tmdb_id.is_none()
        && options.has_imdb_id.is_none()
        && options.series_status.is_empty()
        && options.name_starts_with.as_ref().map_or(true, |s| s.trim().is_empty())
        && options.is_folder.is_none()
        && options.is_hd.is_none()
        && options.filters.is_none()
        && options.any_provider_id_equals.is_empty()
        && options.exclude_types.is_empty()
        && !has_user_library_filter;

    if simple_filter && options.parent_id.is_none() {
        if let Some(library_id) = options.library_id {
            if has_type_filter {
                let types = lowercase_list(&options.include_types);
                let count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM media_items WHERE library_id = $1 AND lower(item_type) = ANY($2)"
                )
                .bind(library_id)
                .bind(&types)
                .fetch_one(pool)
                .await?;
                return Ok(count);
            } else {
                let count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM media_items WHERE library_id = $1"
                )
                .bind(library_id)
                .fetch_one(pool)
                .await?;
                return Ok(count);
            }
        } else if has_type_filter {
            let types = lowercase_list(&options.include_types);
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM media_items WHERE lower(item_type) = ANY($1)"
            )
            .bind(&types)
            .fetch_one(pool)
            .await?;
            return Ok(count);
        }
    }

    let has_search = options.search_term.as_ref().map_or(false, |s| !s.trim().is_empty());
    if has_search && !has_user_library_filter && options.excluded_library_ids.is_empty() {
        let search_term = options.search_term.as_ref().unwrap().trim();
        let pattern = format!("%{}%", search_term);
        let probe_limit = options.start_index.max(0) + options.limit.clamp(1, 200) + 1;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT 1 FROM media_items WHERE name ILIKE $1 OR sort_name ILIKE $1 LIMIT $2) t"
        )
        .bind(&pattern)
        .bind(probe_limit)
        .fetch_one(pool)
        .await?;
        return Ok(count);
    }

    // Resume 列表的 COUNT 通用路径会扫整张 media_items（旧实现实测 1.0+ s），
    // 因为 fallback 的 `SELECT COUNT(*) FROM media_items WHERE 1 = 1` 既没把 resume_only
    // 的 EXISTS 子句加上、也没接 user_item_data 上的索引。
    // user_item_data PRIMARY KEY (user_id, item_id) 让"按 user_id 拉所有 in-progress 行"
    // 是直接的 index range scan，再用 EXISTS(media_items 同条件) 过滤剩下的 item / library / 类型，
    // 通常落在 <50ms。
    if options.resume_only {
        if let Some(user_id) = options.user_id {
            let mut builder = QueryBuilder::<Postgres>::new(
                "SELECT COUNT(*) FROM user_item_data uid WHERE uid.user_id = ",
            );
            builder.push_bind(user_id);
            builder.push(" AND uid.playback_position_ticks > 0");
            builder.push(" AND EXISTS (SELECT 1 FROM media_items WHERE id = uid.item_id");
            apply_item_where_conditions(&mut builder, options);
            push_allowed_library_filter(&mut builder, options);
            builder.push(")");
            let count: i64 = builder.build_query_scalar().fetch_one(pool).await?;
            return Ok(count);
        }
    }

    let mut builder = QueryBuilder::<Postgres>::new(
        "SELECT COUNT(*) FROM media_items WHERE 1 = 1"
    );
    apply_item_where_conditions(&mut builder, options);
    push_allowed_library_filter(&mut builder, options);
    let count: i64 = builder.build_query_scalar().fetch_one(pool).await?;
    Ok(count)
}

fn apply_item_where_conditions(
    builder: &mut QueryBuilder<'_, Postgres>,
    options: &ItemListOptions,
) {
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
            .push(" AND lower(item_type) = ANY(")
            .push_bind(lowercase_list(&options.include_types))
            .push(")");
    }

    if !options.exclude_types.is_empty() {
        builder
            .push(" AND NOT (lower(item_type) = ANY(")
            .push_bind(lowercase_list(&options.exclude_types))
            .push("))");
    }

    if !options.genres.is_empty() {
        builder.push(" AND genres && ").push_bind(options.genres.clone());
    }

    if !options.tags.is_empty() {
        builder.push(" AND tags && ").push_bind(options.tags.clone());
    }

    if !options.years.is_empty() {
        builder
            .push(" AND production_year = ANY(")
            .push_bind(options.years.clone())
            .push(")");
    }

    if !options.studios.is_empty() {
        builder.push(" AND studios && ").push_bind(options.studios.clone());
    }

    if !options.official_ratings.is_empty() {
        builder
            .push(" AND official_rating = ANY(")
            .push_bind(options.official_ratings.clone())
            .push(")");
    }

    if let Some(search_term) = options.search_term.as_ref().filter(|s| !s.trim().is_empty()) {
        let pattern = format!("%{}%", search_term.trim());
        builder
            .push(" AND (name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(original_title, '') ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR sort_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR COALESCE(series_name, '') ILIKE ")
            .push_bind(pattern)
            .push(")");
    }

    if let Some(min_rating) = options.min_community_rating {
        builder.push(" AND community_rating >= ").push_bind(min_rating);
    }

    if let Some(min_date) = options.min_premiere_date {
        builder.push(" AND premiere_date >= ").push_bind(min_date).push("::date");
    }
    if let Some(max_date) = options.max_premiere_date {
        builder.push(" AND premiere_date <= ").push_bind(max_date).push("::date");
    }

    if options.project_to_media {
        builder.push(" AND NOT (item_type = ANY(ARRAY['CollectionFolder','Folder','BoxSet']))");
    }

    if let Some(is_folder) = options.is_folder {
        if is_folder {
            builder.push(" AND item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder'])");
        } else {
            builder.push(" AND NOT (item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder']))");
        }
    }

    if let Some(is_hd) = options.is_hd {
        if is_hd {
            builder.push(" AND (COALESCE(width, 0) >= 1280 OR COALESCE(height, 0) >= 720)");
        } else {
            builder.push(" AND (COALESCE(width, 0) < 1280 AND COALESCE(height, 0) < 720)");
        }
    }

    if let Some(has_tmdb_id) = options.has_tmdb_id {
        if has_tmdb_id {
            builder.push(" AND (provider_ids ? 'Tmdb' OR provider_ids ? 'TMDb' OR provider_ids ? 'tmdb')");
        } else {
            builder.push(" AND NOT (provider_ids ? 'Tmdb' OR provider_ids ? 'TMDb' OR provider_ids ? 'tmdb')");
        }
    }

    if let Some(has_imdb_id) = options.has_imdb_id {
        if has_imdb_id {
            builder.push(" AND (provider_ids ? 'Imdb' OR provider_ids ? 'IMDb' OR provider_ids ? 'imdb')");
        } else {
            builder.push(" AND NOT (provider_ids ? 'Imdb' OR provider_ids ? 'IMDb' OR provider_ids ? 'imdb')");
        }
    }

    if !options.series_status.is_empty() {
        builder
            .push(" AND lower(COALESCE(status, '')) = ANY(")
            .push_bind(lowercase_list(&options.series_status))
            .push(")");
    }

    if let Some(max_rating) = options.policy_max_parental_rating {
        builder
            .push(" AND (parental_rating_value IS NULL OR parental_rating_value <= ")
            .push_bind(max_rating)
            .push(")");
    }

    if !options.policy_blocked_tags.is_empty() {
        builder
            .push(" AND NOT (tags && ")
            .push_bind(options.policy_blocked_tags.clone())
            .push(")");
    }

    if !options.policy_block_unrated_items.is_empty() {
        builder
            .push(" AND NOT (parental_rating_value IS NULL AND lower(item_type) = ANY(")
            .push_bind(lowercase_list(&options.policy_block_unrated_items))
            .push("))");
    }

    if !options.excluded_library_ids.is_empty() {
        builder
            .push(" AND (library_id IS NULL OR NOT (library_id = ANY(")
            .push_bind(options.excluded_library_ids.clone())
            .push(")))");
    }
}

pub async fn list_media_items(
    pool: &sqlx::PgPool,
    mut options: ItemListOptions,
) -> Result<QueryResult<DbMediaItem>, AppError> {
    if let Some(user_id) = options.user_id {
        if options.allowed_library_ids.is_none() {
            options.allowed_library_ids = effective_library_filter_for_user(pool, user_id).await?;
        }
        if options.policy_max_parental_rating.is_none() && options.policy_blocked_tags.is_empty() {
            if let Some(user) = get_user_by_id(pool, user_id).await? {
                if !user.is_admin {
                    let policy = user_policy_from_value(&user.policy);
                    options.policy_max_parental_rating = policy.max_parental_rating;
                    if !policy.blocked_tags.is_empty() {
                        options.policy_blocked_tags = policy.blocked_tags;
                    }
                    if !policy.block_unrated_items.is_empty() {
                        options.policy_block_unrated_items = policy.block_unrated_items;
                    }
                }
            }
        }
    }

    // 搜索请求时自动排除 exclude_from_search=true 的媒体库
    if options.excluded_library_ids.is_empty()
        && options.search_term.as_ref().is_some_and(|s| !s.trim().is_empty())
    {
        options.excluded_library_ids = search_excluded_library_ids(pool).await?;
    }

    if check_allowed_library_short_circuit(&options) {
        return Ok(QueryResult {
            items: Vec::new(),
            total_record_count: 0,
            start_index: Some(options.start_index),
        });
    }

    // 部分 push_bind 后续会 move 走 options 字段，这里先克隆一份兜底用作 SQL 末端追加。
    let outer_allowed_libs = options.allowed_library_ids.clone();
    let outer_library_id = options.library_id;

    let total_record_count = if options.enable_total_record_count {
        fast_count_media_items(pool, &options).await?
    } else {
        0i64
    };

    let mut builder = QueryBuilder::<Postgres>::new(
        r#"
        SELECT
            id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
            overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
            premiere_date, status, end_date, air_days, air_time, series_name, season_name,
            index_number, index_number_end, parent_index_number, provider_ids, genres,
            studios, tags, production_locations,
            width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
            logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
            date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
            display_order, 0::bigint AS total_count
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

    if !options.item_ids.is_empty() {
        builder
            .push(" AND id = ANY(")
            .push_bind(options.item_ids)
            .push(")");
    }

    if !options.include_types.is_empty() {
        builder
            .push(" AND lower(item_type) = ANY(")
            .push_bind(lowercase_list(&options.include_types))
            .push(")");
    }

    if !options.exclude_types.is_empty() {
        builder
            .push(" AND NOT (lower(item_type) = ANY(")
            .push_bind(lowercase_list(&options.exclude_types))
            .push("))");
    }

    if !options.media_types.is_empty() {
        builder
            .push(" AND lower(media_type) = ANY(")
            .push_bind(lowercase_list(&options.media_types))
            .push(")");
    }

    if !options.video_types.is_empty() {
        let video_types = lowercase_list(&options.video_types);
        if video_types
            .iter()
            .any(|value| matches!(value.as_str(), "videofile" | "video"))
        {
            builder.push(" AND media_type = 'Video'");
        } else {
            builder.push(" AND false");
        }
    }

    if !options.genres.is_empty() {
        builder.push(" AND genres && ").push_bind(options.genres);
    }

    if !options.official_ratings.is_empty() {
        builder
            .push(" AND official_rating = ANY(")
            .push_bind(options.official_ratings)
            .push(")");
    }

    if !options.tags.is_empty() {
        builder.push(" AND tags && ").push_bind(options.tags);
    }

    if !options.exclude_tags.is_empty() {
        builder
            .push(" AND NOT (tags && ")
            .push_bind(options.exclude_tags)
            .push(")");
    }

    if !options.years.is_empty() {
        builder
            .push(" AND production_year = ANY(")
            .push_bind(options.years)
            .push(")");
    }

    if !options.containers.is_empty() {
        builder
            .push(" AND lower(container) = ANY(")
            .push_bind(lowercase_list(&options.containers))
            .push(")");
    }

    if !options.audio_codecs.is_empty() {
        builder
            .push(
                " AND (lower(audio_codec) = ANY(",
            )
            .push_bind(lowercase_list(&options.audio_codecs))
            .push(
                ") OR EXISTS (SELECT 1 FROM media_streams ms WHERE ms.media_item_id = media_items.id AND ms.stream_type = 'Audio' AND lower(ms.codec) = ANY(",
            )
            .push_bind(lowercase_list(&options.audio_codecs))
            .push(")))");
    }

    if !options.video_codecs.is_empty() {
        builder
            .push(" AND (lower(video_codec) = ANY(")
            .push_bind(lowercase_list(&options.video_codecs))
            .push(
                ") OR EXISTS (SELECT 1 FROM media_streams ms WHERE ms.media_item_id = media_items.id AND ms.stream_type = 'Video' AND lower(ms.codec) = ANY(",
            )
            .push_bind(lowercase_list(&options.video_codecs))
            .push(")))");
    }

    if !options.subtitle_codecs.is_empty() {
        builder
            .push(
                " AND EXISTS (SELECT 1 FROM media_streams ms WHERE ms.media_item_id = media_items.id AND ms.stream_type = 'Subtitle' AND lower(ms.codec) = ANY(",
            )
            .push_bind(lowercase_list(&options.subtitle_codecs))
            .push("))");
    }

    if !options.image_types.is_empty() {
        let image_types = lowercase_list(&options.image_types);
        let mut has_known_image_type = false;
        builder.push(" AND (false");
        if image_types.iter().any(|value| value == "primary") {
            has_known_image_type = true;
            builder.push(" OR image_primary_path IS NOT NULL");
        }
        if image_types.iter().any(|value| value == "backdrop") {
            has_known_image_type = true;
            builder.push(" OR backdrop_path IS NOT NULL");
        }
        if image_types.iter().any(|value| value == "logo") {
            has_known_image_type = true;
            builder.push(" OR logo_path IS NOT NULL");
        }
        if image_types.iter().any(|value| value == "thumb") {
            has_known_image_type = true;
            builder.push(" OR thumb_path IS NOT NULL");
        }
        builder.push(")");
        if !has_known_image_type {
            builder.push(" AND false");
        }
    }

    if !options.person_ids.is_empty() {
        builder
            .push(
                " AND EXISTS (SELECT 1 FROM person_roles pr WHERE pr.media_item_id = media_items.id AND pr.person_id = ANY(",
            )
            .push_bind(options.person_ids)
            .push(")");
        if !options.person_types.is_empty() {
            builder
                .push(" AND pr.role_type = ANY(")
                .push_bind(options.person_types)
                .push(")");
        }
        builder.push(")");
    } else if !options.person_types.is_empty() {
        builder
            .push(
                " AND EXISTS (SELECT 1 FROM person_roles pr WHERE pr.media_item_id = media_items.id AND pr.role_type = ANY(",
            )
            .push_bind(options.person_types)
            .push("))");
    }

    if !options.artists.is_empty() {
        builder
            .push(
                " AND EXISTS (SELECT 1 FROM person_roles pr INNER JOIN persons p ON p.id = pr.person_id WHERE pr.media_item_id = media_items.id AND lower(p.name) = ANY(",
            )
            .push_bind(lowercase_list(&options.artists))
            .push(") AND lower(pr.role_type) = ANY(ARRAY['artist','albumartist','musicartist','composer']))");
    }

    if !options.artist_ids.is_empty() {
        builder
            .push(
                " AND EXISTS (SELECT 1 FROM person_roles pr WHERE pr.media_item_id = media_items.id AND pr.person_id = ANY(",
            )
            .push_bind(options.artist_ids)
            .push(") AND lower(pr.role_type) = ANY(ARRAY['artist','albumartist','musicartist','composer']))");
    }

    if !options.albums.is_empty() {
        builder
            .push(
                " AND (lower(item_type) = ANY(ARRAY['album','musicalbum']) AND lower(name) = ANY(",
            )
            .push_bind(lowercase_list(&options.albums))
            .push(
                ") OR EXISTS (SELECT 1 FROM media_items parent WHERE parent.id = media_items.parent_id AND lower(parent.name) = ANY(",
            )
            .push_bind(lowercase_list(&options.albums))
            .push(")))");
    }

    if !options.studios.is_empty() {
        builder.push(" AND studios && ").push_bind(options.studios);
    }

    if !options.studio_ids.is_empty() {
        builder
            .push(" AND (studios && ")
            .push_bind(options.studio_ids)
            .push(")");
    }

    if !options.any_provider_id_equals.is_empty() {
        builder
            .push(
                " AND EXISTS (SELECT 1 FROM jsonb_each_text(provider_ids) provider WHERE provider.value = ANY(",
            )
            .push_bind(options.any_provider_id_equals)
            .push("))");
    }

    if let Some(has_overview) = options.has_overview {
        if has_overview {
            builder.push(" AND overview IS NOT NULL AND btrim(overview) <> ''");
        } else {
            builder.push(" AND (overview IS NULL OR btrim(overview) = '')");
        }
    }

    if let Some(has_subtitles) = options.has_subtitles {
        if has_subtitles {
            builder.push(" AND EXISTS (SELECT 1 FROM media_streams ms WHERE ms.media_item_id = media_items.id AND ms.stream_type = 'Subtitle')");
        } else {
            builder.push(" AND NOT EXISTS (SELECT 1 FROM media_streams ms WHERE ms.media_item_id = media_items.id AND ms.stream_type = 'Subtitle')");
        }
    }

    if let Some(has_trailer) = options.has_trailer {
        if has_trailer {
            builder.push(" AND COALESCE(cardinality(remote_trailers), 0) > 0");
        } else {
            builder.push(" AND COALESCE(cardinality(remote_trailers), 0) = 0");
        }
    }

    if matches!(options.has_theme_song, Some(true))
        || matches!(options.has_theme_video, Some(true))
        || matches!(options.has_special_feature, Some(true))
    {
        builder.push(" AND false");
    }

    if let Some(has_tmdb_id) = options.has_tmdb_id {
        if has_tmdb_id {
            builder.push(
                " AND (provider_ids ? 'Tmdb' OR provider_ids ? 'TMDb' OR provider_ids ? 'tmdb')",
            );
        } else {
            builder.push(" AND NOT (provider_ids ? 'Tmdb' OR provider_ids ? 'TMDb' OR provider_ids ? 'tmdb')");
        }
    }

    if let Some(has_imdb_id) = options.has_imdb_id {
        if has_imdb_id {
            builder.push(
                " AND (provider_ids ? 'Imdb' OR provider_ids ? 'IMDb' OR provider_ids ? 'imdb')",
            );
        } else {
            builder.push(" AND NOT (provider_ids ? 'Imdb' OR provider_ids ? 'IMDb' OR provider_ids ? 'imdb')");
        }
    }

    if options.project_to_media {
        builder.push(" AND NOT (item_type = ANY(ARRAY['CollectionFolder','Folder','BoxSet']))");
    }

    if let Some(is_folder) = options.is_folder {
        if is_folder {
            builder.push(" AND item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder'])");
        } else {
            builder.push(" AND NOT (item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder']))");
        }
    }

    if let Some(is_hd) = options.is_hd {
        if is_hd {
            builder.push(" AND (COALESCE(width, 0) >= 1280 OR COALESCE(height, 0) >= 720)");
        } else {
            builder.push(" AND (COALESCE(width, 0) < 1280 AND COALESCE(height, 0) < 720)");
        }
    }

    if matches!(options.is_3d, Some(true))
        || matches!(options.is_locked, Some(true))
        || matches!(options.is_place_holder, Some(true))
    {
        builder.push(" AND false");
    }

    if !options.series_status.is_empty() {
        builder
            .push(" AND lower(COALESCE(status, '')) = ANY(")
            .push_bind(lowercase_list(&options.series_status))
            .push(")");
    }

    if let Some(min_rating) = options.min_community_rating {
        builder
            .push(" AND community_rating >= ")
            .push_bind(min_rating);
    }

    if let Some(min_rating) = options.min_critic_rating {
        builder.push(" AND critic_rating >= ").push_bind(min_rating);
    }

    if let Some(min_date) = options.min_premiere_date {
        builder
            .push(" AND premiere_date >= ")
            .push_bind(min_date)
            .push("::date");
    }

    if let Some(max_date) = options.max_premiere_date {
        builder
            .push(" AND premiere_date <= ")
            .push_bind(max_date)
            .push("::date");
    }

    if let Some(min_date) = options.min_start_date {
        builder
            .push(" AND premiere_date >= ")
            .push_bind(min_date)
            .push("::date");
    }

    if let Some(max_date) = options.max_start_date {
        builder
            .push(" AND premiere_date <= ")
            .push_bind(max_date)
            .push("::date");
    }

    if let Some(min_date) = options.min_end_date {
        builder
            .push(" AND end_date >= ")
            .push_bind(min_date)
            .push("::date");
    }

    if let Some(max_date) = options.max_end_date {
        builder
            .push(" AND end_date <= ")
            .push_bind(max_date)
            .push("::date");
    }

    if let Some(min_date) = options.min_date_last_saved {
        builder.push(" AND date_modified >= ").push_bind(min_date);
    }

    if let Some(max_date) = options.max_date_last_saved {
        builder.push(" AND date_modified <= ").push_bind(max_date);
    }

    if let Some(min_date) = options.min_date_last_saved_for_user {
        builder.push(" AND date_modified >= ").push_bind(min_date);
    }

    if let Some(max_date) = options.max_date_last_saved_for_user {
        builder.push(" AND date_modified <= ").push_bind(max_date);
    }

    if let Some(season_number) = options.aired_during_season {
        builder
            .push(" AND (parent_index_number = ")
            .push_bind(season_number)
            .push(" OR (item_type = 'Season' AND index_number = ")
            .push_bind(season_number)
            .push("))");
    }

    if let Some(user_id) = options.user_id {
        if let Some(is_played) = options.is_played {
            if is_played {
                builder
                    .push(" AND EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                    .push_bind(user_id)
                    .push(" AND uid.item_id = media_items.id AND uid.is_played = true)");
            } else {
                builder
                    .push(" AND NOT EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                    .push_bind(user_id)
                    .push(" AND uid.item_id = media_items.id AND uid.is_played = true)");
            }
        }

        if let Some(is_favorite) = options.is_favorite {
            if is_favorite {
                builder
                    .push(" AND EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                    .push_bind(user_id)
                    .push(" AND uid.item_id = media_items.id AND uid.is_favorite = true)");
            } else {
                builder
                    .push(" AND NOT EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                    .push_bind(user_id)
                    .push(" AND uid.item_id = media_items.id AND uid.is_favorite = true)");
            }
        }

        if options.resume_only {
            builder
                .push(" AND EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                .push_bind(user_id)
                .push(" AND uid.item_id = media_items.id AND uid.playback_position_ticks > 0)");
        }

        let filters = parse_option_filters(options.filters.as_deref());
        for filter in filters {
            match filter.as_str() {
                "isplayed" => {
                    builder
                        .push(" AND EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                        .push_bind(user_id)
                        .push(" AND uid.item_id = media_items.id AND uid.is_played = true)");
                }
                "isunplayed" => {
                    builder
                        .push(" AND NOT EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                        .push_bind(user_id)
                        .push(" AND uid.item_id = media_items.id AND uid.is_played = true)");
                }
                "isfavorite" => {
                    builder
                        .push(" AND EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                        .push_bind(user_id)
                        .push(" AND uid.item_id = media_items.id AND uid.is_favorite = true)");
                }
                "isresumable" => {
                    builder
                        .push(" AND EXISTS (SELECT 1 FROM user_item_data uid WHERE uid.user_id = ")
                        .push_bind(user_id)
                        .push(" AND uid.item_id = media_items.id AND uid.playback_position_ticks > 0)");
                }
                "isfolder" => {
                    builder.push(" AND item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder'])");
                }
                "isnotfolder" => {
                    builder.push(" AND NOT (item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder']))");
                }
                _ => {}
            }
        }
    } else if options.resume_only {
        builder.push(" AND false");
    } else {
        for filter in parse_option_filters(options.filters.as_deref()) {
            match filter.as_str() {
                "isfolder" => {
                    builder.push(" AND item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder'])");
                }
                "isnotfolder" => {
                    builder.push(" AND NOT (item_type = ANY(ARRAY['Series','Season','BoxSet','Folder','CollectionFolder']))");
                }
                "isplayed" | "isunplayed" | "isfavorite" | "isresumable" => {
                    builder.push(" AND false");
                }
                _ => {}
            }
        }
    }

    if let Some(search_term) = options.search_term.filter(|value| !value.trim().is_empty()) {
        let search_pattern = format!("%{}%", search_term.trim());
        builder
            .push(" AND (name ILIKE ")
            .push_bind(search_pattern.clone())
            .push(" OR sort_name ILIKE ")
            .push_bind(search_pattern)
            .push(")");
    }

    if let Some(prefix) = options
        .name_starts_with
        .filter(|value| !value.trim().is_empty())
    {
        let pattern = format!("{}%", prefix.trim());
        builder
            .push(" AND (sort_name ILIKE ")
            .push_bind(pattern.clone())
            .push(" OR name ILIKE ")
            .push_bind(pattern)
            .push(")");
    }

    if let Some(lower_bound) = options
        .name_starts_with_or_greater
        .filter(|value| !value.trim().is_empty())
    {
        builder
            .push(" AND lower(COALESCE(sort_name, name)) >= lower(")
            .push_bind(lower_bound.trim().to_string())
            .push(")");
    }

    if let Some(upper_bound) = options
        .name_less_than
        .filter(|value| !value.trim().is_empty())
    {
        builder
            .push(" AND lower(COALESCE(sort_name, name)) < lower(")
            .push_bind(upper_bound.trim().to_string())
            .push(")");
    }

    // PB10：白名单非空就一律注入 SQL 过滤，与 push_allowed_library_filter 形成对称。
    // outer_library_id 已在 check_allowed_library_short_circuit 校验过包含关系，这里
    // 不再以 `outer_library_id.is_none()` 短路，避免上游显式 library_id 时绕过白名单。
    let _ = outer_library_id;
    if let Some(allowed) = outer_allowed_libs.as_ref() {
        if !allowed.is_empty() {
            builder
                .push(" AND library_id = ANY(")
                .push_bind(allowed.clone())
                .push(")");
        }
    }

    let sort_keys: Vec<&str> = options
        .sort_by
        .as_deref()
        .unwrap_or("SortName")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect();
    let sort_order = if options
        .sort_order
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("Descending"))
    {
        "DESC"
    } else {
        "ASC"
    };

    builder.push(" ORDER BY ");

    let effective_sort_keys = if sort_keys.is_empty() {
        vec!["SortName"]
    } else {
        sort_keys
    };

    for (index, sort_key) in effective_sort_keys.iter().enumerate() {
        if index > 0 {
            builder.push(", ");
        }

        match *sort_key {
            "DatePlayed" => {
                if let Some(user_id) = options.user_id {
                    builder
                        .push(
                            "COALESCE((SELECT uid.last_played_date FROM user_item_data uid WHERE uid.user_id = ",
                        )
                        .push_bind(user_id)
                        .push(" AND uid.item_id = media_items.id), to_timestamp(0))");
                } else {
                    builder.push("date_created");
                }
            }
            "DateCreated" | "DateLastContentAdded" => {
                builder.push("date_created");
            }
            "ParentIndexNumber" => {
                builder.push("parent_index_number");
            }
            "IndexNumber" => {
                builder.push("index_number");
            }
            "PremiereDate" => {
                builder.push("premiere_date");
            }
            "ProductionYear" => {
                builder.push("production_year");
            }
            "CommunityRating" => {
                builder.push("community_rating");
            }
            "OfficialRating" => {
                builder.push("official_rating");
            }
            "Runtime" => {
                builder.push("runtime_ticks");
            }
            "Random" => {
                builder.push("random()");
            }
            _ => {
                builder.push("sort_name");
            }
        }

        builder.push(" ").push(sort_order).push(" NULLS LAST");
    }

    let effective_limit = options.limit.max(0).min(1_000);
    if effective_limit == 0 {
        return Ok(QueryResult {
            items: Vec::new(),
            total_record_count,
            start_index: Some(options.start_index.max(0)),
        });
    }

    builder
        .push(" OFFSET ")
        .push_bind(options.start_index.max(0))
        .push(" LIMIT ")
        .push_bind(effective_limit);

    let start_index = options.start_index;
    let rows = builder
        .build_query_as::<MediaItemRow>()
        .fetch_all(pool)
        .await?;
    let items = if options.group_items_into_collections {
        deduplicate_media_items(rows.into_iter().map(DbMediaItem::from).collect())
    } else {
        rows.into_iter().map(DbMediaItem::from).collect()
    };

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index.max(0)),
    })
}

pub async fn media_stream_codecs_for_items(
    pool: &sqlx::PgPool,
    item_ids: &[Uuid],
) -> Result<Vec<(String, String)>, AppError> {
    if item_ids.is_empty() {
        return Ok(Vec::new());
    }

    Ok(sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT DISTINCT stream_type, codec
        FROM media_streams
        WHERE media_item_id = ANY($1)
          AND codec IS NOT NULL
          AND btrim(codec) <> ''
        ORDER BY stream_type, codec
        "#,
    )
    .bind(item_ids)
    .fetch_all(pool)
    .await?)
}

/// PB18：聚合 endpoint 的统一思路——
/// `allowed_library_ids = None` 表示「无库可见性约束」（admin 或没启用隐藏库的场景），
/// 走 `repo_cache` 全局缓存路径；`Some(&[])` 表示「显式 zero 可见」直接返回空；
/// `Some(&[..])` 走未缓存的 SQL，并把库白名单注入到 `library_id = ANY(...)` 谓词，
/// 防止隐藏库里的 studios/tags/genres/years/codecs 通过聚合接口泄露给受限用户。
pub async fn aggregate_text_values(
    pool: &sqlx::PgPool,
    field: &str,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<String>, AppError> {
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok(Vec::new());
    }
    let column = match field {
        "container" => "container",
        "official_rating" => "official_rating",
        _ => return Ok(Vec::new()),
    };
    let sql = format!(
        r#"
        SELECT DISTINCT {col} AS value
        FROM media_items
        WHERE {col} IS NOT NULL AND btrim({col}) <> ''
          AND ($1::uuid[] IS NULL OR library_id = ANY($1))
        ORDER BY {col}
        "#,
        col = column
    );
    Ok(sqlx::query_scalar::<_, String>(&sql)
        .bind(allowed_library_ids.map(<[Uuid]>::to_vec))
        .fetch_all(pool)
        .await?)
}

pub async fn aggregate_array_values(
    pool: &sqlx::PgPool,
    field: &str,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<String>, AppError> {
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok(Vec::new());
    }
    // 仅 admin / 全可见情况下命中全局缓存；受限用户走 uncached 的 library_id 过滤路径。
    if allowed_library_ids.is_none() {
        return crate::repo_cache::cached_aggregate_array_values(pool, field).await;
    }
    aggregate_array_values_uncached(pool, field, allowed_library_ids).await
}

pub async fn aggregate_array_values_uncached(
    pool: &sqlx::PgPool,
    field: &str,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<String>, AppError> {
    let (column, limit) = match field {
        "tags" => ("tags", 1000),
        "studios" => ("studios", 1000),
        "genres" => ("genres", 500),
        _ => return Ok(Vec::new()),
    };
    let sql = format!(
        r#"
        SELECT DISTINCT value
        FROM media_items, unnest({col}) AS value
        WHERE btrim(value) <> ''
          AND ($1::uuid[] IS NULL OR library_id = ANY($1))
        ORDER BY value
        LIMIT {lim}
        "#,
        col = column,
        lim = limit
    );
    Ok(sqlx::query_scalar::<_, String>(&sql)
        .bind(allowed_library_ids.map(<[Uuid]>::to_vec))
        .fetch_all(pool)
        .await?)
}

pub async fn aggregate_years(
    pool: &sqlx::PgPool,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<i32>, AppError> {
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok(Vec::new());
    }
    if allowed_library_ids.is_none() {
        return crate::repo_cache::cached_aggregate_years(pool).await;
    }
    aggregate_years_uncached(pool, allowed_library_ids).await
}

pub async fn aggregate_years_uncached(
    pool: &sqlx::PgPool,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<i32>, AppError> {
    Ok(sqlx::query_scalar::<_, i32>(
        r#"
        SELECT DISTINCT production_year
        FROM media_items
        WHERE production_year IS NOT NULL
          AND ($1::uuid[] IS NULL OR library_id = ANY($1))
        ORDER BY production_year DESC
        "#,
    )
    .bind(allowed_library_ids.map(<[Uuid]>::to_vec))
    .fetch_all(pool)
    .await?)
}

pub async fn aggregate_stream_codecs(
    pool: &sqlx::PgPool,
    stream_type: &str,
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<String>, AppError> {
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_scalar::<_, String>(
        r#"
        SELECT DISTINCT ms.codec
        FROM media_streams ms
        INNER JOIN media_items mi ON mi.id = ms.media_item_id
        WHERE ms.stream_type = $1
          AND ms.codec IS NOT NULL
          AND btrim(ms.codec) <> ''
          AND ($2::uuid[] IS NULL OR mi.library_id = ANY($2))
        ORDER BY ms.codec
        "#,
    )
    .bind(stream_type)
    .bind(allowed_library_ids.map(<[Uuid]>::to_vec))
    .fetch_all(pool)
    .await?)
}

/// Scoped aggregation: aggregate array values (genres/tags/studios) filtered by
/// library_id and include_types, plus user visibility constraints.
pub async fn aggregate_array_values_scoped(
    pool: &sqlx::PgPool,
    field: &str,
    library_id: Option<Uuid>,
    include_types: &[String],
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<String>, AppError> {
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok(Vec::new());
    }
    if library_id.is_none() && include_types.is_empty() {
        return aggregate_array_values(pool, field, allowed_library_ids).await;
    }
    let (column, limit) = match field {
        "tags" => ("tags", 1000),
        "studios" => ("studios", 1000),
        "genres" => ("genres", 500),
        _ => return Ok(Vec::new()),
    };
    let types_lower: Vec<String> = include_types.iter().map(|t| t.to_lowercase()).collect();
    let sql = format!(
        r#"
        SELECT DISTINCT value
        FROM media_items, unnest({col}) AS value
        WHERE btrim(value) <> ''
          AND ($1::uuid IS NULL OR library_id = $1)
          AND ($2::text[] IS NULL OR lower(item_type) = ANY($2))
          AND ($3::uuid[] IS NULL OR library_id = ANY($3))
        ORDER BY value
        LIMIT {lim}
        "#,
        col = column,
        lim = limit
    );
    Ok(sqlx::query_scalar::<_, String>(&sql)
        .bind(library_id)
        .bind(if types_lower.is_empty() { None } else { Some(&types_lower) })
        .bind(allowed_library_ids.map(<[Uuid]>::to_vec))
        .fetch_all(pool)
        .await?)
}

/// Scoped aggregation: aggregate text values (official_rating/container) filtered by
/// library_id and include_types.
pub async fn aggregate_text_values_scoped(
    pool: &sqlx::PgPool,
    field: &str,
    library_id: Option<Uuid>,
    include_types: &[String],
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<String>, AppError> {
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok(Vec::new());
    }
    if library_id.is_none() && include_types.is_empty() {
        return aggregate_text_values(pool, field, allowed_library_ids).await;
    }
    let column = match field {
        "container" => "container",
        "official_rating" => "official_rating",
        _ => return Ok(Vec::new()),
    };
    let types_lower: Vec<String> = include_types.iter().map(|t| t.to_lowercase()).collect();
    let sql = format!(
        r#"
        SELECT DISTINCT {col} AS value
        FROM media_items
        WHERE {col} IS NOT NULL AND btrim({col}) <> ''
          AND ($1::uuid IS NULL OR library_id = $1)
          AND ($2::text[] IS NULL OR lower(item_type) = ANY($2))
          AND ($3::uuid[] IS NULL OR library_id = ANY($3))
        ORDER BY {col}
        "#,
        col = column
    );
    Ok(sqlx::query_scalar::<_, String>(&sql)
        .bind(library_id)
        .bind(if types_lower.is_empty() { None } else { Some(&types_lower) })
        .bind(allowed_library_ids.map(<[Uuid]>::to_vec))
        .fetch_all(pool)
        .await?)
}

/// Scoped aggregation: aggregate years filtered by library_id and include_types.
pub async fn aggregate_years_scoped(
    pool: &sqlx::PgPool,
    library_id: Option<Uuid>,
    include_types: &[String],
    allowed_library_ids: Option<&[Uuid]>,
) -> Result<Vec<i32>, AppError> {
    if matches!(allowed_library_ids, Some(ids) if ids.is_empty()) {
        return Ok(Vec::new());
    }
    if library_id.is_none() && include_types.is_empty() {
        return aggregate_years(pool, allowed_library_ids).await;
    }
    let types_lower: Vec<String> = include_types.iter().map(|t| t.to_lowercase()).collect();
    Ok(sqlx::query_scalar::<_, i32>(
        r#"
        SELECT DISTINCT production_year
        FROM media_items
        WHERE production_year IS NOT NULL
          AND ($1::uuid IS NULL OR library_id = $1)
          AND ($2::text[] IS NULL OR lower(item_type) = ANY($2))
          AND ($3::uuid[] IS NULL OR library_id = ANY($3))
        ORDER BY production_year DESC
        "#,
    )
    .bind(library_id)
    .bind(if types_lower.is_empty() { None } else { Some(&types_lower) })
    .bind(allowed_library_ids.map(<[Uuid]>::to_vec))
    .fetch_all(pool)
    .await?)
}

pub async fn aggregate_artists(pool: &sqlx::PgPool) -> Result<Vec<(Uuid, String)>, AppError> {
    Ok(sqlx::query_as::<_, (Uuid, String)>(
        r#"
        SELECT DISTINCT p.id, p.name
        FROM persons p
        INNER JOIN person_roles pr ON pr.person_id = p.id
        WHERE lower(pr.role_type) = ANY(ARRAY['artist','albumartist','musicartist','composer'])
          AND btrim(p.name) <> ''
        ORDER BY p.name
        "#,
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_next_up_episodes(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    parent_id: Option<Uuid>,
    server_id: Uuid,
    start_index: i64,
    limit: i64,
    enable_total_record_count: bool,
) -> Result<QueryResult<BaseItemDto>, AppError> {
    let allowed_library_ids = effective_library_filter_for_user(pool, user_id).await?;
    if let Some(ref ids) = allowed_library_ids {
        if ids.is_empty() {
            return Ok(QueryResult {
                items: Vec::new(),
                total_record_count: 0,
                start_index: Some(start_index.max(0)),
            });
        }
    }
    let library_filter_sql = if allowed_library_ids.is_some() {
        "AND mi.library_id = ANY($5)"
    } else {
        "AND ($5::uuid[] IS NULL OR TRUE)"
    };
    let allowed_ids_param: Option<Vec<Uuid>> = allowed_library_ids;

    // 当客户端传入具体 scope_id 时，命中以下任一条件即可：
    //  1. mi.series_id (scanner / backfill 已写入)
    //  2. mi.parent_id (传入的是 Season ID，Episode 直接挂在它下面)
    //  3. mi.library_id (传入的是 Library ID)
    //
    // 之前还有一条 `EXISTS (SELECT 1 FROM media_items season WHERE season.id = mi.parent_id
    // AND season.parent_id = $2)`：这是在 series_id 还没回填之前的兜底链路。
    // 现在 ensure_schema_compatibility 启动时会强制回填 Episode.series_id，
    // 该 EXISTS 等价于 `mi.series_id = $2`，但会让 Postgres planner 在 BitmapOr
    // 之外多跑一个嵌套子查询（实测把 NextUp 由 ~10ms 拖到 1.0–1.4s），故移除。
    // 留下的 3 路 OR 都直接落到 b-tree 索引（series_id / parent_id / library_id），
    // BitmapOr 后过滤 Episode 是 sub-100ms 路径。
    let (scope_sql_fragment, use_series_fast_path) = if parent_id.is_some() {
        (
            r#"AND (
                mi.series_id = $2
                OR mi.parent_id = $2
                OR mi.library_id = $2
            )"#,
            true,
        )
    } else {
        (
            r#"AND (
                $2::uuid IS NULL
            )"#,
            false,
        )
    };

    let total_record_count = if enable_total_record_count {
        let count_sql = if use_series_fast_path {
            format!(
                r#"
                SELECT COUNT(DISTINCT mi.parent_id) FROM media_items mi
                LEFT JOIN user_item_data uid ON uid.item_id = mi.id AND uid.user_id = $1
                WHERE mi.item_type = 'Episode'
                  AND COALESCE(uid.is_played, false) = false
                  {scope}
                  {lib_filter}
                "#,
                scope = scope_sql_fragment,
                lib_filter = library_filter_sql
            )
        } else {
            format!(
                r#"
                WITH ranked AS (
                    SELECT
                        mi.id,
                        row_number() OVER (
                            PARTITION BY COALESCE(mi.series_id, mi.parent_id, mi.id)
                            ORDER BY mi.parent_index_number NULLS LAST,
                                     mi.index_number NULLS LAST,
                                     mi.sort_name
                        ) AS next_rank
                    FROM media_items mi
                    LEFT JOIN user_item_data uid ON uid.item_id = mi.id AND uid.user_id = $1
                    WHERE mi.item_type = 'Episode'
                      AND COALESCE(uid.is_played, false) = false
                      {scope}
                      {lib_filter}
                )
                SELECT COUNT(*) FROM ranked WHERE next_rank = 1
                "#,
                scope = scope_sql_fragment,
                lib_filter = library_filter_sql
            )
        };
        sqlx::query_scalar(&count_sql)
            .bind(user_id)
            .bind(parent_id)
            .bind(start_index.max(0))
            .bind(limit.clamp(1, 200))
            .bind(allowed_ids_param.as_deref())
            .fetch_one(pool)
            .await?
    } else {
        0i64
    };

    let data_sql = format!(
        r#"
        WITH ranked AS (
            SELECT
                mi.id, mi.parent_id, mi.name, mi.original_title, mi.sort_name,
                mi.item_type, mi.media_type, mi.path, mi.container, mi.overview,
                mi.production_year, mi.official_rating, mi.community_rating,
                mi.critic_rating, mi.runtime_ticks, mi.premiere_date,
                mi.status, mi.end_date, mi.air_days, mi.air_time,
                mi.series_name, mi.season_name,
                mi.index_number, mi.index_number_end, mi.parent_index_number,
                mi.provider_ids, mi.genres, mi.studios, mi.tags,
                mi.production_locations, mi.width, mi.height,
                mi.bit_rate, mi.size, mi.video_codec, mi.audio_codec,
                mi.image_primary_path, mi.backdrop_path, mi.logo_path,
                mi.thumb_path, mi.art_path, mi.banner_path, mi.disc_path,
                mi.backdrop_paths, mi.remote_trailers,
                mi.date_created, mi.date_modified, mi.image_blur_hashes,
                mi.series_id, mi.taglines, mi.locked_fields, mi.lock_data,
                mi.display_order,
                row_number() OVER (
                    PARTITION BY COALESCE(mi.series_id, mi.parent_id, mi.id)
                    ORDER BY
                        mi.parent_index_number NULLS LAST,
                        mi.index_number NULLS LAST,
                        mi.sort_name
                ) AS next_rank
            FROM media_items mi
            LEFT JOIN user_item_data uid ON uid.item_id = mi.id AND uid.user_id = $1
            WHERE mi.item_type = 'Episode'
              AND COALESCE(uid.is_played, false) = false
              {scope}
              {lib_filter}
        )
        SELECT
            id, parent_id, name, original_title, sort_name, item_type,
            media_type, path, container, overview, production_year,
            official_rating, community_rating, critic_rating, runtime_ticks, premiere_date,
            status, end_date, air_days, air_time, series_name, season_name,
            index_number, index_number_end, parent_index_number, provider_ids,
            genres, studios, tags, production_locations, width, height,
            bit_rate, size, video_codec, audio_codec, image_primary_path,
            backdrop_path, logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
            date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
            display_order
        FROM ranked
        WHERE next_rank = 1
        ORDER BY series_name NULLS LAST,
                 parent_index_number NULLS LAST,
                 index_number NULLS LAST,
                 sort_name
        OFFSET $3 LIMIT $4
        "#,
        scope = scope_sql_fragment,
        lib_filter = library_filter_sql
    );

    let rows = sqlx::query_as::<_, DbMediaItem>(&data_sql)
        .bind(user_id)
        .bind(parent_id)
        .bind(start_index.max(0))
        .bind(limit.clamp(1, 200))
        .bind(allowed_ids_param.as_deref())
        .fetch_all(pool)
        .await?;

    let row_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let user_data_map = get_user_item_data_batch(pool, user_id, &row_ids).await?;

    let items: Vec<BaseItemDto> = rows
        .iter()
        .map(|row| {
            let prefetched = Some(user_data_map.get(&row.id).cloned());
            media_item_to_dto_for_list(row, server_id, prefetched, DtoCountPrefetch::default())
        })
        .collect();

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index.max(0)),
    })
}

pub async fn get_upcoming_episodes(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    parent_id: Option<Uuid>,
    server_id: Uuid,
    start_index: i64,
    limit: i64,
) -> Result<QueryResult<BaseItemDto>, AppError> {
    let allowed_library_ids = effective_library_filter_for_user(pool, user_id).await?;
    if let Some(ref ids) = allowed_library_ids {
        if ids.is_empty() {
            return Ok(QueryResult {
                items: Vec::new(),
                total_record_count: 0,
                start_index: Some(start_index.max(0)),
            });
        }
    }
    let library_filter_sql = if allowed_library_ids.is_some() {
        "AND mi.library_id = ANY($5)"
    } else {
        "AND ($5::uuid[] IS NULL OR TRUE)"
    };

    let today = Utc::now().date_naive();
    let count_sql = format!(
        r#"
        SELECT COUNT(*)
        FROM media_items mi
        WHERE mi.item_type = 'Episode'
          AND mi.premiere_date >= $1
          AND (
              $2::uuid IS NULL
              OR mi.parent_id = $2
              OR mi.library_id = $2
              OR EXISTS (
                  SELECT 1
                  FROM media_items season
                  WHERE season.id = mi.parent_id
                    AND (season.parent_id = $2 OR season.library_id = $2)
              )
          )
          {lib_filter}
        "#,
        lib_filter = library_filter_sql
    );
    let total_record_count: i64 = sqlx::query_scalar(&count_sql)
        .bind(today)
        .bind(parent_id)
        .bind(start_index.max(0))
        .bind(limit.clamp(1, 200))
        .bind(allowed_library_ids.as_deref())
        .fetch_one(pool)
        .await?;

    let data_sql = format!(
        r#"
        SELECT
            mi.id, mi.parent_id, mi.name, mi.original_title, mi.sort_name, mi.item_type,
            mi.media_type, mi.path, mi.container, mi.overview, mi.production_year,
            mi.official_rating, mi.community_rating, mi.critic_rating, mi.runtime_ticks, mi.premiere_date,
            mi.status, mi.end_date, mi.air_days, mi.air_time, mi.series_name, mi.season_name,
            mi.index_number, mi.index_number_end, mi.parent_index_number, mi.provider_ids,
            mi.genres, mi.studios, mi.tags, mi.production_locations, mi.width, mi.height,
            mi.bit_rate, mi.size, mi.video_codec, mi.audio_codec, mi.image_primary_path,
            mi.backdrop_path, mi.logo_path, mi.thumb_path, mi.art_path, mi.banner_path, mi.disc_path, mi.backdrop_paths, mi.remote_trailers,
            mi.date_created, mi.date_modified, mi.image_blur_hashes, mi.series_id, mi.taglines, mi.locked_fields, mi.lock_data,
            mi.display_order
        FROM media_items mi
        WHERE mi.item_type = 'Episode'
          AND mi.premiere_date >= $1
          AND (
              $2::uuid IS NULL
              OR mi.parent_id = $2
              OR mi.library_id = $2
              OR EXISTS (
                  SELECT 1
                  FROM media_items season
                  WHERE season.id = mi.parent_id
                    AND (season.parent_id = $2 OR season.library_id = $2)
              )
          )
          {lib_filter}
        ORDER BY mi.premiere_date,
                 mi.series_name NULLS LAST,
                 mi.parent_index_number NULLS LAST,
                 mi.index_number NULLS LAST,
                 mi.sort_name
        OFFSET $3 LIMIT $4
        "#,
        lib_filter = library_filter_sql
    );
    let rows = sqlx::query_as::<_, DbMediaItem>(&data_sql)
        .bind(today)
        .bind(parent_id)
        .bind(start_index.max(0))
        .bind(limit.clamp(1, 200))
        .bind(allowed_library_ids.as_deref())
    .fetch_all(pool)
    .await?;

    let row_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let user_data_map = get_user_item_data_batch(pool, user_id, &row_ids).await?;

    let items: Vec<BaseItemDto> = rows
        .iter()
        .map(|row| {
            let prefetched = Some(user_data_map.get(&row.id).cloned());
            media_item_to_dto_for_list(row, server_id, prefetched, DtoCountPrefetch::default())
        })
        .collect();

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index.max(0)),
    })
}

#[derive(Debug, FromRow)]
struct MissingEpisodeRow {
    id: Uuid,
    series_id: Uuid,
    provider: String,
    external_series_id: String,
    external_season_id: Option<String>,
    external_episode_id: Option<String>,
    season_number: i32,
    episode_number: i32,
    episode_number_end: Option<i32>,
    name: String,
    overview: Option<String>,
    premiere_date: Option<NaiveDate>,
    image_path: Option<String>,
    series_name: String,
    series_sort_name: String,
    series_overview: Option<String>,
    series_production_year: Option<i32>,
    series_provider_ids: Value,
    series_image_primary_path: Option<String>,
    series_backdrop_path: Option<String>,
    series_logo_path: Option<String>,
    series_thumb_path: Option<String>,
    series_date_modified: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct MissingEpisodeDetailRow {
    id: Uuid,
    series_id: Uuid,
    provider: String,
    external_series_id: String,
    external_season_id: Option<String>,
    external_episode_id: Option<String>,
    season_number: i32,
    episode_number: i32,
    episode_number_end: Option<i32>,
    name: String,
    overview: Option<String>,
    premiere_date: Option<NaiveDate>,
    image_path: Option<String>,
    series_name: String,
    series_sort_name: String,
    series_overview: Option<String>,
    series_production_year: Option<i32>,
    series_provider_ids: Value,
    series_image_primary_path: Option<String>,
    series_backdrop_path: Option<String>,
    series_logo_path: Option<String>,
    series_thumb_path: Option<String>,
    series_date_modified: DateTime<Utc>,
}

pub async fn replace_series_episode_catalog(
    pool: &sqlx::PgPool,
    series_id: Uuid,
    items: &[ExternalEpisodeCatalogItem],
) -> Result<(), AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM series_episode_catalog WHERE series_id = $1")
        .bind(series_id)
        .execute(&mut *tx)
        .await?;

    for item in items {
        sqlx::query(
            r#"
            INSERT INTO series_episode_catalog (
                id, series_id, provider, external_series_id, external_season_id, external_episode_id,
                season_number, episode_number, episode_number_end, name, overview, premiere_date, image_path,
                created_at, updated_at
            )
            VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10, $11, $12, $13,
                now(), now()
            )
            "#,
        )
        .bind(Uuid::new_v5(
            &series_id,
            format!(
                "catalog:{}:{}:{}",
                item.provider, item.season_number, item.episode_number
            )
            .as_bytes(),
        ))
        .bind(series_id)
        .bind(&item.provider)
        .bind(&item.external_series_id)
        .bind(&item.external_season_id)
        .bind(&item.external_episode_id)
        .bind(item.season_number)
        .bind(item.episode_number)
        .bind(item.episode_number_end)
        .bind(&item.name)
        .bind(&item.overview)
        .bind(item.premiere_date)
        .bind(&item.image_path)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn get_missing_episodes(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    parent_id: Option<Uuid>,
    server_id: Uuid,
    start_index: i64,
    limit: i64,
) -> Result<QueryResult<BaseItemDto>, AppError> {
    // 收集启用了 import_missing_episodes 的媒体库 ID
    let enabled_lib_ids = missing_episodes_enabled_library_ids(pool).await?;
    if enabled_lib_ids.is_empty() {
        return Ok(QueryResult {
            items: Vec::new(),
            total_record_count: 0,
            start_index: Some(start_index),
        });
    }

    let total_record_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM series_episode_catalog sec
        INNER JOIN media_items series ON series.id = sec.series_id
        WHERE (sec.premiere_date IS NULL OR sec.premiere_date <= CURRENT_DATE)
          AND series.library_id = ANY($2)
          AND (
              $1::uuid IS NULL
              OR sec.series_id = $1
              OR series.library_id = $1
              OR EXISTS (
                  SELECT 1
                  FROM media_items season_scope
                  WHERE season_scope.id = $1
                    AND season_scope.item_type = 'Season'
                    AND season_scope.parent_id = sec.series_id
                    AND season_scope.index_number = sec.season_number
              )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM media_items season
              INNER JOIN media_items episode
                  ON episode.parent_id = season.id
                 AND episode.item_type = 'Episode'
                 AND episode.index_number = sec.episode_number
              WHERE season.parent_id = sec.series_id
                AND season.item_type = 'Season'
                AND season.index_number = sec.season_number
          )
        "#,
    )
    .bind(parent_id)
    .bind(&enabled_lib_ids)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query_as::<_, MissingEpisodeRow>(
        r#"
        SELECT
            sec.id,
            sec.series_id,
            sec.provider,
            sec.external_series_id,
            sec.external_season_id,
            sec.external_episode_id,
            sec.season_number,
            sec.episode_number,
            sec.episode_number_end,
            sec.name,
            sec.overview,
            sec.premiere_date,
            sec.image_path,
            series.name AS series_name,
            series.sort_name AS series_sort_name,
            series.overview AS series_overview,
            series.production_year AS series_production_year,
            series.provider_ids AS series_provider_ids,
            series.image_primary_path AS series_image_primary_path,
            series.backdrop_path AS series_backdrop_path,
            series.logo_path AS series_logo_path,
            series.thumb_path AS series_thumb_path,
            series.date_modified AS series_date_modified
        FROM series_episode_catalog sec
        INNER JOIN media_items series ON series.id = sec.series_id
        WHERE (sec.premiere_date IS NULL OR sec.premiere_date <= CURRENT_DATE)
          AND series.library_id = ANY($4)
          AND (
              $1::uuid IS NULL
              OR sec.series_id = $1
              OR series.library_id = $1
              OR EXISTS (
                  SELECT 1
                  FROM media_items season_scope
                  WHERE season_scope.id = $1
                    AND season_scope.item_type = 'Season'
                    AND season_scope.parent_id = sec.series_id
                    AND season_scope.index_number = sec.season_number
              )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM media_items season
              INNER JOIN media_items episode
                  ON episode.parent_id = season.id
                 AND episode.item_type = 'Episode'
                 AND episode.index_number = sec.episode_number
              WHERE season.parent_id = sec.series_id
                AND season.item_type = 'Season'
                AND season.index_number = sec.season_number
          )
        ORDER BY
            COALESCE(sec.premiere_date, DATE '1900-01-01'),
            series.sort_name,
            sec.season_number,
            sec.episode_number
        OFFSET $2 LIMIT $3
        "#,
    )
    .bind(parent_id)
    .bind(start_index.max(0))
    .bind(limit.clamp(1, 200))
    .bind(&enabled_lib_ids)
    .fetch_all(pool)
    .await?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(missing_episode_row_to_dto(pool, row, user_id, server_id).await?);
    }

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index.max(0)),
    })
}

pub async fn get_missing_episode_dto(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    user_id: Uuid,
    server_id: Uuid,
) -> Result<Option<BaseItemDto>, AppError> {
    let row = sqlx::query_as::<_, MissingEpisodeDetailRow>(
        r#"
        SELECT
            sec.id,
            sec.series_id,
            sec.provider,
            sec.external_series_id,
            sec.external_season_id,
            sec.external_episode_id,
            sec.season_number,
            sec.episode_number,
            sec.episode_number_end,
            sec.name,
            sec.overview,
            sec.premiere_date,
            sec.image_path,
            series.name AS series_name,
            series.sort_name AS series_sort_name,
            series.overview AS series_overview,
            series.production_year AS series_production_year,
            series.provider_ids AS series_provider_ids,
            series.image_primary_path AS series_image_primary_path,
            series.backdrop_path AS series_backdrop_path,
            series.logo_path AS series_logo_path,
            series.thumb_path AS series_thumb_path,
            series.date_modified AS series_date_modified
        FROM series_episode_catalog sec
        INNER JOIN media_items series ON series.id = sec.series_id
        WHERE sec.id = $1
          AND (sec.premiere_date IS NULL OR sec.premiere_date <= CURRENT_DATE)
          AND NOT EXISTS (
              SELECT 1
              FROM media_items season
              INNER JOIN media_items episode
                  ON episode.parent_id = season.id
                 AND episode.item_type = 'Episode'
                 AND episode.index_number = sec.episode_number
              WHERE season.parent_id = sec.series_id
                AND season.item_type = 'Season'
                AND season.index_number = sec.season_number
          )
        LIMIT 1
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;

    match row {
        Some(row) => Ok(Some(
            missing_episode_row_to_dto(pool, row, user_id, server_id).await?,
        )),
        None => Ok(None),
    }
}

pub async fn get_missing_episode_image_path(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    image_type: &str,
) -> Result<Option<String>, AppError> {
    let row = sqlx::query_as::<_, MissingEpisodeDetailRow>(
        r#"
        SELECT
            sec.id,
            sec.series_id,
            sec.provider,
            sec.external_series_id,
            sec.external_season_id,
            sec.external_episode_id,
            sec.season_number,
            sec.episode_number,
            sec.episode_number_end,
            sec.name,
            sec.overview,
            sec.premiere_date,
            sec.image_path,
            series.name AS series_name,
            series.sort_name AS series_sort_name,
            series.overview AS series_overview,
            series.production_year AS series_production_year,
            series.provider_ids AS series_provider_ids,
            series.image_primary_path AS series_image_primary_path,
            series.backdrop_path AS series_backdrop_path,
            series.logo_path AS series_logo_path,
            series.thumb_path AS series_thumb_path,
            series.date_modified AS series_date_modified
        FROM series_episode_catalog sec
        INNER JOIN media_items series ON series.id = sec.series_id
        WHERE sec.id = $1
        LIMIT 1
        "#,
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    Ok(match image_type.to_ascii_lowercase().as_str() {
        "backdrop" => row.series_backdrop_path,
        "logo" => row.series_logo_path,
        "thumb" => row.series_thumb_path.or(row.series_backdrop_path),
        _ => row.image_path.or(row.series_image_primary_path),
    })
}

pub async fn get_boxsets_for_item_ids(
    pool: &sqlx::PgPool,
    _user_id: Uuid,
    item_ids: &[Uuid],
    server_id: Uuid,
    start_index: i64,
    limit: i64,
) -> Result<QueryResult<BaseItemDto>, AppError> {
    let mut results = Vec::new();

    for item_id in item_ids {
        let Some(item) = get_media_item(pool, *item_id).await? else {
            continue;
        };
        let grouped_items = version_group_items_for_item(pool, &item).await?;
        if grouped_items.len() <= 1 {
            continue;
        }

        let providers = provider_ids_for_item(&item);
        let boxset_id = uuid_to_emby_guid(&Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("boxset:{}", presentation_unique_key(&item, &providers)).as_bytes(),
        ));
        let primary_tag = item
            .image_primary_path
            .as_ref()
            .map(|_| item.date_modified.timestamp().to_string());
        let mut image_tags = BTreeMap::new();
        if let Some(tag) = primary_tag.clone() {
            image_tags.insert("Primary".to_string(), tag);
        }

        results.push(BaseItemDto {
            name: item.name.clone(),
            original_title: item.original_title.clone(),
            server_id: uuid_to_emby_guid(&server_id),
            id: boxset_id.clone(),
            etag: Some(item.date_modified.timestamp().to_string()),
            date_modified: Some(item.date_modified),
            guid: None,
            can_delete: Some(false),
            can_download: false,
            can_edit_items: Some(false),
            supports_resume: Some(false),
            presentation_unique_key: Some(format!("{boxset_id}_")),
            supports_sync: Some(true),
            item_type: "BoxSet".to_string(),
            is_folder: true,
            sort_name: Some(item.sort_name.clone()),
            forced_sort_name: Some(item.sort_name.clone()),
            primary_image_tag: primary_tag,
            collection_type: Some("movies".to_string()),
            media_type: Some(item.media_type.clone()),
            container: None,
            parent_id: None,
            path: None,
            location_type: Some("Virtual".to_string()),
            run_time_ticks: item.runtime_ticks,
            production_year: item.production_year,
            overview: item.overview.clone(),
            date_created: Some(item.date_created),
            premiere_date: premiere_date_to_utc(item.premiere_date),
            video_codec: None,
            audio_codec: None,
            average_frame_rate: None,
            real_frame_rate: None,
            genres: item.genres.clone(),
            genre_items: name_long_id_items_from_names(&item.genres),
            provider_ids: providers.clone(),
            external_urls: external_urls_from_provider_map(&providers, "Movie"),
            production_locations: item.production_locations.clone(),
            size: None,
            file_name: None,
            bitrate: None,
            official_rating: item.official_rating.clone(),
            community_rating: item.community_rating,
            critic_rating: None,
            taglines: Vec::new(),
            remote_trailers: remote_trailers_from_urls(&item.remote_trailers),
            people: Vec::new(),
            studios: name_long_id_items_from_names(&item.studios),
            tag_items: name_long_id_items_from_names(&item.tags),
            local_trailer_count: Some(0),
            display_preferences_id: Some(boxset_id.clone()),
            playlist_item_id: None,
            recursive_item_count: Some(grouped_items.len() as i64),
            season_count: None,
            series_count: None,
            movie_count: Some(grouped_items.len() as i32),
            status: None,
            air_days: Vec::new(),
            air_time: None,
            end_date: None,
            width: item.width,
            height: item.height,
            is_movie: Some(false),
            is_series: Some(false),
            is_live: Some(false),
            is_news: Some(false),
            is_kids: Some(false),
            is_sports: Some(false),
            is_premiere: Some(false),
            is_new: Some(false),
            is_repeat: Some(false),
            disabled: Some(false),
            series_name: None,
            series_id: None,
            season_name: None,
            season_id: None,
            index_number: None,
            index_number_end: None,
            parent_index_number: None,
            image_tags,
            image_blur_hashes: parse_blur_hashes(&item.image_blur_hashes),
            backdrop_image_tags: item
                .backdrop_path
                .as_ref()
                .map(|_| vec![item.date_modified.timestamp().to_string()])
                .unwrap_or_default(),
            parent_logo_item_id: None,
            parent_logo_image_tag: None,
            parent_backdrop_item_id: None,
            parent_backdrop_image_tags: Vec::new(),
            parent_thumb_item_id: None,
            parent_thumb_image_tag: None,
            thumb_image_tag: None,
            series_primary_image_tag: None,
            primary_image_item_id: None,
            series_studio: None,
            user_data: empty_user_data_for_item(*item_id),
            media_sources: Vec::new(),
            media_streams: Vec::new(),
            part_count: Some(grouped_items.len() as i32),
            chapters: Vec::new(),
            locked_fields: Vec::new(),
            lock_data: Some(false),
            special_feature_count: None,
            child_count: Some(grouped_items.len() as i64),
            display_order: None,
            primary_image_aspect_ratio: None,
            completion_percentage: None,
            tags: item.tags.clone(),
            extra_fields: BTreeMap::new(),
        });
    }

    let total_record_count = results.len() as i64;
    let start = start_index.max(0) as usize;
    let end = (start + limit.clamp(1, 200) as usize).min(results.len());
    let items = if start >= total_record_count as usize {
        Vec::new()
    } else {
        results
            .into_iter()
            .skip(start)
            .take(end.saturating_sub(start))
            .collect()
    };

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index.max(0)),
    })
}

pub async fn create_playlist(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    name: &str,
    media_type: &str,
    overview: Option<&str>,
) -> Result<crate::models::DbPlaylist, AppError> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO playlists (id, user_id, name, media_type, overview)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(id)
    .bind(user_id)
    .bind(name)
    .bind(media_type)
    .bind(overview)
    .execute(pool)
    .await?;
    get_playlist(pool, id)
        .await?
        .ok_or_else(|| AppError::Internal("创建播放列表后无法读取".to_string()))
}

pub async fn get_playlist(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<crate::models::DbPlaylist>, AppError> {
    Ok(sqlx::query_as::<_, crate::models::DbPlaylist>(
        r#"
        SELECT id, user_id, name, media_type, overview, image_primary_path, created_at, updated_at
        FROM playlists
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

pub async fn list_playlists_for_user(
    pool: &sqlx::PgPool,
    user_id: Uuid,
) -> Result<Vec<crate::models::DbPlaylist>, AppError> {
    Ok(sqlx::query_as::<_, crate::models::DbPlaylist>(
        r#"
        SELECT id, user_id, name, media_type, overview, image_primary_path, created_at, updated_at
        FROM playlists
        WHERE user_id = $1
        ORDER BY updated_at DESC, name ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?)
}

pub async fn update_playlist(
    pool: &sqlx::PgPool,
    id: Uuid,
    name: Option<&str>,
    overview: Option<Option<&str>>,
) -> Result<(), AppError> {
    if let Some(name) = name {
        sqlx::query("UPDATE playlists SET name = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(name)
            .execute(pool)
            .await?;
    }
    if let Some(overview) = overview {
        sqlx::query("UPDATE playlists SET overview = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(overview)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn delete_playlist(pool: &sqlx::PgPool, id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM playlists WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_playlist_items(
    pool: &sqlx::PgPool,
    playlist_id: Uuid,
) -> Result<Vec<crate::models::DbPlaylistItem>, AppError> {
    Ok(sqlx::query_as::<_, crate::models::DbPlaylistItem>(
        r#"
        SELECT id, playlist_id, media_item_id, playlist_item_id, sort_index, created_at
        FROM playlist_items
        WHERE playlist_id = $1
        ORDER BY sort_index ASC, created_at ASC
        "#,
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await?)
}

pub async fn add_playlist_items(
    pool: &sqlx::PgPool,
    playlist_id: Uuid,
    media_item_ids: &[Uuid],
) -> Result<(), AppError> {
    if media_item_ids.is_empty() {
        return Ok(());
    }
    let current_max: Option<i64> = sqlx::query_scalar(
        "SELECT MAX(sort_index)::bigint FROM playlist_items WHERE playlist_id = $1",
    )
    .bind(playlist_id)
    .fetch_optional(pool)
    .await?
    .flatten();
    let mut next_index = current_max.map(|value| value + 1).unwrap_or(0);
    for media_item_id in media_item_ids {
        sqlx::query(
            r#"
            INSERT INTO playlist_items (playlist_id, media_item_id, sort_index)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(playlist_id)
        .bind(media_item_id)
        .bind(next_index as i32)
        .execute(pool)
        .await?;
        next_index += 1;
    }
    sqlx::query("UPDATE playlists SET updated_at = now() WHERE id = $1")
        .bind(playlist_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn remove_playlist_items(
    pool: &sqlx::PgPool,
    playlist_id: Uuid,
    entry_ids: &[String],
) -> Result<(), AppError> {
    if entry_ids.is_empty() {
        return Ok(());
    }
    sqlx::query(
        r#"
        DELETE FROM playlist_items
        WHERE playlist_id = $1 AND playlist_item_id = ANY($2)
        "#,
    )
    .bind(playlist_id)
    .bind(entry_ids)
    .execute(pool)
    .await?;
    sqlx::query("UPDATE playlists SET updated_at = now() WHERE id = $1")
        .bind(playlist_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn move_playlist_item(
    pool: &sqlx::PgPool,
    playlist_id: Uuid,
    entry_id: &str,
    new_index: i32,
) -> Result<(), AppError> {
    let items = list_playlist_items(pool, playlist_id).await?;
    let mut remaining: Vec<_> = items
        .iter()
        .filter(|item| item.playlist_item_id != entry_id)
        .collect();
    let Some(target) = items
        .iter()
        .find(|item| item.playlist_item_id == entry_id)
        .cloned()
    else {
        return Err(AppError::NotFound("播放列表条目不存在".to_string()));
    };
    let insert_index = new_index.clamp(0, remaining.len() as i32);
    remaining.insert(insert_index as usize, &target);
    for (index, item) in remaining.iter().enumerate() {
        sqlx::query("UPDATE playlist_items SET sort_index = $2 WHERE id = $1")
            .bind(item.id)
            .bind(index as i32)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn get_media_item(
    pool: &sqlx::PgPool,
    id: Uuid,
) -> Result<Option<DbMediaItem>, AppError> {
    Ok(sqlx::query_as::<_, DbMediaItem>(
        r#"
        SELECT
            id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
            overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
            premiere_date, status, end_date, air_days, air_time, series_name, season_name,
            index_number, index_number_end, parent_index_number, provider_ids, genres,
            studios, tags, production_locations,
            width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
            logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
            date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
            display_order
        FROM media_items
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?)
}

/// 把上游 Emby 的原始数字 / hex itemId 映射回本地 media_items.id。
///
/// 远程同步阶段会把 upstream 的 ItemId 写到 `provider_ids.RemoteEmbyItemId`
/// （Episodes/Movies）和 `provider_ids.RemoteEmbySeriesId`（Series 详情同步），
/// 客户端如果是从 upstream Emby 切过来的、UI 缓存里还残留 upstream 原 ID，
/// 直接用那个 ID 来访问 `/Items/{id}/Images/...` 就会被 emby_id_to_uuid 解析失败。
///
/// 这里在解析失败时按 RemoteEmbyItemId → RemoteEmbySeriesId 的顺序查一下，
/// 命中就把请求当作本地 UUID 走通常路径，避免出现"明明是有数据的项目，却 400"。
///
/// 之所以两个键都查：
/// - Episode/Movie 行存在 RemoteEmbyItemId（同步主路径里 ensure_remote_episode 写入）
/// - Series 行存在 RemoteEmbySeriesId（详情同步路径里写入），不存在 RemoteEmbyItemId
pub async fn find_item_id_by_remote_emby_id(
    pool: &sqlx::PgPool,
    remote_id: &str,
) -> Result<Option<Uuid>, AppError> {
    let trimmed = remote_id.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let row: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id
        FROM media_items
        WHERE provider_ids->>'RemoteEmbyItemId' = $1
           OR provider_ids->>'RemoteEmbySeriesId' = $1
        ORDER BY
            CASE WHEN provider_ids->>'RemoteEmbyItemId' = $1 THEN 0 ELSE 1 END,
            date_modified DESC
        LIMIT 1
        "#,
    )
    .bind(trimmed)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(id,)| id))
}

pub async fn list_media_item_children(
    pool: &sqlx::PgPool,
    parent_id: Uuid,
) -> Result<Vec<DbMediaItem>, AppError> {
    Ok(sqlx::query_as::<_, DbMediaItem>(
        r#"
        SELECT
            id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
            overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
            premiere_date, status, end_date, air_days, air_time, series_name, season_name,
            index_number, index_number_end, parent_index_number, provider_ids, genres,
            studios, tags, production_locations,
            width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
            logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
            date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
            display_order
        FROM media_items
        WHERE parent_id = $1
        ORDER BY index_number, sort_name
        "#,
    )
    .bind(parent_id)
    .fetch_all(pool)
    .await?)
}

pub async fn find_items_for_external_person_credit(
    pool: &sqlx::PgPool,
    credit: &ExternalPersonCredit,
) -> Result<Vec<DbMediaItem>, AppError> {
    let item_type = if credit.media_type.eq_ignore_ascii_case("tv") {
        "Series"
    } else {
        "Movie"
    };
    let title_pattern = format!("%{}%", credit.title.trim());

    Ok(sqlx::query_as::<_, DbMediaItem>(
        r#"
        SELECT
            id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
            overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
            premiere_date, status, end_date, air_days, air_time, series_name, season_name,
            index_number, index_number_end, parent_index_number, provider_ids, genres,
            studios, tags, production_locations,
            width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
            logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
            date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
            display_order
        FROM media_items
        WHERE item_type = $1
          AND (
              provider_ids->>'Tmdb' = $2 OR provider_ids->>'TMDb' = $2 OR provider_ids->>'tmdb' = $2
              OR (
                  name ILIKE $3
                  AND ($4::integer IS NULL OR production_year = $4)
              )
              OR (
                  original_title ILIKE $3
                  AND ($4::integer IS NULL OR production_year = $4)
              )
          )
        ORDER BY
            CASE
                WHEN provider_ids->>'Tmdb' = $2 OR provider_ids->>'TMDb' = $2 OR provider_ids->>'tmdb' = $2 THEN 0
                WHEN production_year = $4 THEN 1
                ELSE 2
            END,
            date_created ASC
        LIMIT 20
        "#,
    )
    .bind(item_type)
    .bind(&credit.external_id)
    .bind(&title_pattern)
    .bind(credit.year)
    .fetch_all(pool)
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

/// 列表接口专用：一次拉回 N 个条目的 UserItemData，消除逐条 N+1。
/// Key 为 item_id，找不到的条目会在调用方退化为空 UserData（已有 `empty_user_data_for_item`）。
pub async fn get_user_item_data_batch(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    item_ids: &[Uuid],
) -> Result<HashMap<Uuid, DbUserItemData>, AppError> {
    if item_ids.is_empty() {
        return Ok(HashMap::new());
    }
    #[derive(sqlx::FromRow)]
    struct Row {
        item_id: Uuid,
        playback_position_ticks: i64,
        play_count: i32,
        is_favorite: bool,
        is_played: bool,
        last_played_date: Option<DateTime<Utc>>,
    }
    let rows = sqlx::query_as::<_, Row>(
        r#"
        SELECT item_id, playback_position_ticks, play_count, is_favorite, is_played, last_played_date
        FROM user_item_data
        WHERE user_id = $1 AND item_id = ANY($2)
        "#,
    )
    .bind(user_id)
    .bind(item_ids)
    .fetch_all(pool)
    .await?;

    let mut out = HashMap::with_capacity(rows.len());
    for row in rows {
        out.insert(
            row.item_id,
            DbUserItemData {
                playback_position_ticks: row.playback_position_ticks,
                play_count: row.play_count,
                is_favorite: row.is_favorite,
                is_played: row.is_played,
                last_played_date: row.last_played_date,
            },
        );
    }
    Ok(out)
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
                $1,
                $2,
                CASE WHEN COALESCE($6, false) THEN 0 ELSE COALESCE($3, 0) END,
                COALESCE($4, CASE WHEN COALESCE($6, false) THEN 1 ELSE 0 END),
                COALESCE($5, false),
                COALESCE($6, false),
                CASE WHEN COALESCE($6, false) THEN COALESCE($7, now()) ELSE $7 END,
                now()
            )
        ON CONFLICT (user_id, item_id)
        DO UPDATE SET
            playback_position_ticks = CASE
                WHEN $6 = true THEN 0
                ELSE COALESCE($3, user_item_data.playback_position_ticks)
            END,
            play_count = COALESCE(
                $4,
                CASE WHEN $6 = true THEN GREATEST(user_item_data.play_count, 1) ELSE user_item_data.play_count END
            ),
            is_favorite = COALESCE($5, user_item_data.is_favorite),
            is_played = COALESCE($6, user_item_data.is_played),
            last_played_date = CASE
                WHEN $6 = true THEN COALESCE($7, now())
                WHEN $6 = false THEN NULL
                ELSE COALESCE($7, user_item_data.last_played_date)
            END,
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

pub struct PlaybackEventExtras {
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub play_method: Option<String>,
    pub media_source_id: Option<String>,
    pub volume_level: Option<i32>,
    pub repeat_mode: Option<String>,
    pub playback_rate: Option<f64>,
    /// PB29：客户端在 PlaybackReport 里带的 PlaySessionId，会随 INSERT 写入
    /// `playback_events.play_session_id` 独立列；与 `session_id`（access_token 维度）
    /// 解耦，让 Stop/Progress 等回调能按 PlaySessionId 反查最初 PlaybackInfo。
    pub play_session_id: Option<String>,
}

/// PB29：按 PlaySessionId 反查最近一次 `Started` 事件，常用于 `/Sessions/Playing/{id}/Stop`
/// 这种"靠 PlaySessionId 不靠 itemId 也能识别"的回调路径。当前只返回最新一行用作识别。
pub async fn get_latest_event_by_play_session_id(
    pool: &sqlx::PgPool,
    play_session_id: &str,
) -> Result<Option<(Uuid, Uuid, Option<Uuid>)>, AppError> {
    let row: Option<(Uuid, Uuid, Option<Uuid>)> = sqlx::query_as(
        r#"
        SELECT id, user_id, item_id
        FROM playback_events
        WHERE play_session_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(play_session_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
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
    extras: &PlaybackEventExtras,
) -> Result<(), AppError> {
    // PB29：把客户端在 PlaybackReport 里上报的 PlaySessionId 写进独立列。
    // `session_id` 仍然是「access_token / 队列归属维度」（兼容旧逻辑），新加的
    // `play_session_id` 字段对应 Emby PlaybackInfo handler 生成的 PlaySessionId，
    // 让 Stop/Progress 等回调能通过 PlaySessionId 反查最初的 PlaybackInfo。
    sqlx::query(
        r#"
        INSERT INTO playback_events
            (id, user_id, item_id, session_id, event_type, position_ticks, is_paused, played_to_completion, play_session_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
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
    .bind(extras.play_session_id.as_deref())
    .execute(pool)
    .await?;

    if let Some(item_id) = item_id {
        if let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) {
            if matches!(event_type, "Started" | "Progress" | "Ping") {
                sqlx::query(
                    r#"
                    INSERT INTO session_play_queue
                        (session_id, item_id, sort_index, position_ticks, is_paused, play_state,
                         audio_stream_index, subtitle_stream_index, play_method,
                         media_source_id, volume_level, repeat_mode, playback_rate, updated_at)
                    SELECT $1, $2, 0, $3, $4,
                           CASE WHEN COALESCE($4, false) THEN 'Paused' ELSE 'Playing' END,
                           $5, $6, $7, $8, $9, $10, $11, now()
                    WHERE EXISTS (SELECT 1 FROM sessions WHERE access_token = $1)
                    ON CONFLICT (session_id, item_id)
                    DO UPDATE SET
                        position_ticks = COALESCE(EXCLUDED.position_ticks, session_play_queue.position_ticks),
                        is_paused = COALESCE(EXCLUDED.is_paused, session_play_queue.is_paused),
                        play_state = COALESCE(EXCLUDED.play_state, session_play_queue.play_state),
                        audio_stream_index = COALESCE(EXCLUDED.audio_stream_index, session_play_queue.audio_stream_index),
                        subtitle_stream_index = COALESCE(EXCLUDED.subtitle_stream_index, session_play_queue.subtitle_stream_index),
                        play_method = COALESCE(EXCLUDED.play_method, session_play_queue.play_method),
                        media_source_id = COALESCE(EXCLUDED.media_source_id, session_play_queue.media_source_id),
                        volume_level = COALESCE(EXCLUDED.volume_level, session_play_queue.volume_level),
                        repeat_mode = COALESCE(EXCLUDED.repeat_mode, session_play_queue.repeat_mode),
                        playback_rate = COALESCE(EXCLUDED.playback_rate, session_play_queue.playback_rate),
                        updated_at = now()
                    "#,
                )
                .bind(session_id)
                .bind(item_id)
                .bind(position_ticks)
                .bind(is_paused)
                .bind(&extras.audio_stream_index)
                .bind(&extras.subtitle_stream_index)
                .bind(&extras.play_method)
                .bind(&extras.media_source_id)
                .bind(&extras.volume_level)
                .bind(&extras.repeat_mode)
                .bind(&extras.playback_rate)
                .execute(pool)
                .await?;
            } else if event_type == "Stopped" {
                sqlx::query(
                    r#"
                    DELETE FROM session_play_queue
                    WHERE session_id = $1 AND item_id = $2
                    "#,
                )
                .bind(session_id)
                .bind(item_id)
                .execute(pool)
                .await?;
            }
        }

        if matches!(event_type, "Started" | "Progress" | "Stopped") {
            // Emby/Jellyfin: 服务端根据进度比例自动判定是否已看完（>= 90%）
            let client_says_completed = played_to_completion.unwrap_or(false);
            let auto_completed = if !client_says_completed {
                if let Some(pos) = position_ticks.filter(|&p| p > 0) {
                    let runtime: Option<i64> = sqlx::query_scalar(
                        "SELECT runtime_ticks FROM media_items WHERE id = $1",
                    )
                    .bind(item_id)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
                    runtime
                        .filter(|&rt| rt > 0)
                        .map(|rt| (pos as f64 / rt as f64) >= 0.9)
                        .unwrap_or(false)
                } else {
                    false
                }
            } else {
                false
            };
            let is_completed = client_says_completed || auto_completed;

            // PlayCount 去重：仅当 is_played 从 false 转为 true 时 +1
            // 使用 SQL 条件确保已经是 played 状态时不再累加
            sqlx::query(
                r#"
                INSERT INTO user_item_data
                    (user_id, item_id, playback_position_ticks, is_played, play_count, last_played_date, updated_at)
                VALUES ($1, $2, $3, $4, CASE WHEN $4 THEN 1 ELSE 0 END, now(), now())
                ON CONFLICT (user_id, item_id)
                DO UPDATE SET
                    playback_position_ticks = COALESCE(EXCLUDED.playback_position_ticks, user_item_data.playback_position_ticks),
                    is_played = CASE WHEN EXCLUDED.is_played THEN true ELSE user_item_data.is_played END,
                    play_count = CASE
                        WHEN EXCLUDED.is_played AND NOT user_item_data.is_played
                        THEN user_item_data.play_count + 1
                        ELSE user_item_data.play_count
                    END,
                    last_played_date = now(),
                    updated_at = now()
                "#,
            )
            .bind(user_id)
            .bind(item_id)
            .bind(position_ticks)
            .bind(is_completed)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// 清理超过 `timeout_minutes` 分钟未更新的 session_play_queue 条目（空闲会话检测）。
/// 返回被清理的行数。
pub async fn cleanup_stale_play_queue(pool: &sqlx::PgPool, timeout_minutes: i64) -> Result<u64, AppError> {
    let result = sqlx::query(
        r#"
        DELETE FROM session_play_queue
        WHERE updated_at < now() - ($1::bigint || ' minutes')::interval
        "#,
    )
    .bind(timeout_minutes)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

fn deduplicate_media_items(items: Vec<DbMediaItem>) -> Vec<DbMediaItem> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::with_capacity(items.len());

    for item in items {
        let providers = provider_ids_for_item(&item);
        let key =
            item_identity_key(&item, &providers).unwrap_or_else(|| format!("item:{}", item.id));
        if seen.insert(key) {
            unique.push(item);
        }
    }

    unique
}

pub async fn session_play_queue(
    pool: &sqlx::PgPool,
    session_id: Option<&str>,
    device_id: Option<&str>,
    user_id: Uuid,
    server_id: Uuid,
) -> Result<QueryResult<BaseItemDto>, AppError> {
    let rows = sqlx::query_as::<_, DbMediaItem>(
        r#"
        SELECT
            mi.id, mi.parent_id, mi.name, mi.original_title, mi.sort_name, mi.item_type,
            mi.media_type, mi.path, mi.container, mi.overview, mi.production_year,
            mi.official_rating, mi.community_rating, mi.critic_rating, mi.runtime_ticks,
            mi.premiere_date, mi.status, mi.end_date, mi.air_days, mi.air_time,
            mi.series_name, mi.season_name,
            mi.index_number, mi.index_number_end, mi.parent_index_number, mi.provider_ids,
            mi.genres, mi.studios, mi.tags, mi.production_locations,
            mi.width, mi.height, mi.bit_rate, mi.size, mi.video_codec, mi.audio_codec,
            mi.image_primary_path, mi.backdrop_path, mi.logo_path, mi.thumb_path,
            mi.art_path, mi.banner_path, mi.disc_path, mi.backdrop_paths, mi.remote_trailers,
            mi.date_created, mi.date_modified, mi.image_blur_hashes, mi.series_id, mi.taglines, mi.locked_fields, mi.lock_data,
            mi.display_order
        FROM session_play_queue q
        INNER JOIN sessions s ON s.access_token = q.session_id
        INNER JOIN media_items mi ON mi.id = q.item_id
        WHERE s.user_id = $1
          AND ($2::text IS NULL OR q.session_id = $2)
          AND ($3::text IS NULL OR s.device_id = $3)
        ORDER BY q.sort_index, q.updated_at DESC
        "#,
    )
    .bind(user_id)
    .bind(session_id)
    .bind(device_id)
    .fetch_all(pool)
    .await?;

    let total_record_count = rows.len() as i64;
    let row_ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let user_data_map = get_user_item_data_batch(pool, user_id, &row_ids).await?;
    let items: Vec<BaseItemDto> = rows
        .iter()
        .map(|row| {
            let prefetched = Some(user_data_map.get(&row.id).cloned());
            media_item_to_dto_for_list(row, server_id, prefetched, DtoCountPrefetch::default())
        })
        .collect();

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(0),
    })
}

pub async fn session_runtime_state(
    pool: &sqlx::PgPool,
    session_id: &str,
    user_id: Uuid,
    server_id: Uuid,
) -> Result<Option<SessionRuntimeState>, AppError> {
    let row = sqlx::query(
        r#"
        SELECT
            q.position_ticks AS queue_position_ticks,
            q.is_paused AS queue_is_paused,
            q.play_state AS queue_play_state,
            q.updated_at AS queue_updated_at,
            q.audio_stream_index AS queue_audio_stream_index,
            q.subtitle_stream_index AS queue_subtitle_stream_index,
            q.play_method AS queue_play_method,
            q.media_source_id AS queue_media_source_id,
            q.volume_level AS queue_volume_level,
            q.repeat_mode AS queue_repeat_mode,
            q.playback_rate AS queue_playback_rate,
            q.playlist_item_id AS queue_playlist_item_id,
            mi.id, mi.parent_id, mi.name, mi.original_title, mi.sort_name, mi.item_type,
            mi.media_type, mi.path, mi.container, mi.overview, mi.production_year,
            mi.official_rating, mi.community_rating, mi.critic_rating, mi.runtime_ticks,
            mi.premiere_date, mi.status, mi.end_date, mi.air_days, mi.air_time,
            mi.series_name, mi.season_name,
            mi.index_number, mi.index_number_end, mi.parent_index_number, mi.provider_ids,
            mi.genres, mi.studios, mi.tags, mi.production_locations,
            mi.width, mi.height, mi.bit_rate, mi.size, mi.video_codec, mi.audio_codec,
            mi.image_primary_path, mi.backdrop_path, mi.logo_path, mi.thumb_path,
            mi.art_path, mi.banner_path, mi.disc_path, mi.backdrop_paths, mi.remote_trailers,
            mi.date_created, mi.date_modified, mi.image_blur_hashes, mi.series_id, mi.taglines, mi.locked_fields, mi.lock_data,
            mi.display_order
        FROM session_play_queue q
        INNER JOIN sessions s ON s.access_token = q.session_id
        INNER JOIN media_items mi ON mi.id = q.item_id
        WHERE s.user_id = $1
          AND q.session_id = $2
        ORDER BY q.sort_index, q.updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let item = DbMediaItem::from_row(&row)?;
    let dto = media_item_to_dto(pool, &item, Some(user_id), server_id).await?;
    let position_ticks: Option<i64> = row.try_get("queue_position_ticks")?;
    let is_paused: Option<bool> = row.try_get("queue_is_paused")?;
    let _play_state_str: Option<String> = row.try_get("queue_play_state")?;
    let audio_stream_index: Option<i32> = row.try_get("queue_audio_stream_index")?;
    let subtitle_stream_index: Option<i32> = row.try_get("queue_subtitle_stream_index")?;
    let stored_play_method: Option<String> = row.try_get("queue_play_method")?;
    let stored_media_source_id: Option<String> = row.try_get("queue_media_source_id")?;
    let volume_level: Option<i32> = row.try_get("queue_volume_level")?;
    let stored_repeat_mode: Option<String> = row.try_get("queue_repeat_mode")?;
    let playback_rate: Option<f64> = row.try_get("queue_playback_rate")?;
    let playlist_item_id: Option<String> = row.try_get("queue_playlist_item_id")?;

    let media_source_id = stored_media_source_id
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            dto.media_sources
                .first()
                .map(|source| source.id.clone())
        })
        .unwrap_or_else(|| format!("mediasource_{}", dto.id));

    let play_method = stored_play_method
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| {
            dto.media_sources
                .first()
                .map(|source| {
                    if source.transcoding_url.is_some() {
                        "Transcode".to_string()
                    } else if source.is_remote {
                        "DirectStream".to_string()
                    } else {
                        "DirectPlay".to_string()
                    }
                })
                .unwrap_or_else(|| "DirectPlay".to_string())
        });

    let repeat_mode = stored_repeat_mode
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "RepeatNone".to_string());

    let mut play_state_json = json!({
        "PositionTicks": position_ticks.unwrap_or(0),
        "CanSeek": true,
        "IsPaused": is_paused.unwrap_or(false),
        "IsMuted": false,
        "VolumeLevel": volume_level.unwrap_or(100),
        "PlayMethod": play_method,
        "RepeatMode": repeat_mode,
        "MediaSourceId": media_source_id,
        "PlaybackRate": playback_rate.unwrap_or(1.0),
    });
    if let Some(idx) = audio_stream_index {
        play_state_json["AudioStreamIndex"] = json!(idx);
    }
    if let Some(idx) = subtitle_stream_index {
        play_state_json["SubtitleStreamIndex"] = json!(idx);
    }

    let item_emby_id = dto.id.clone();
    let now_playing_queue = vec![json!({
        "Id": item_emby_id,
        "PlaylistItemId": playlist_item_id.unwrap_or_else(|| item_emby_id.clone()),
    })];

    Ok(Some(SessionRuntimeState {
        now_playing_item: dto,
        play_state: play_state_json,
        now_playing_queue,
    }))
}

pub async fn record_session_command(
    pool: &sqlx::PgPool,
    session_id: &str,
    command: &str,
    payload: Value,
) -> Result<(), AppError> {
    sqlx::query(
        r#"
        INSERT INTO session_commands (id, session_id, command, payload)
        SELECT $1, $2, $3, $4
        WHERE EXISTS (SELECT 1 FROM sessions WHERE access_token = $2)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(session_id)
    .bind(command)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn apply_session_command_state(
    pool: &sqlx::PgPool,
    session_id: &str,
    command: &str,
    payload: &Value,
) -> Result<(), AppError> {
    let normalized = command.trim().to_ascii_lowercase();
    let mut summary = get_session_state_summary(pool, session_id)
        .await?
        .unwrap_or_else(|| json!({}));
    let summary_object = summary.as_object_mut();
    match normalized.as_str() {
        "pause" => {
            sqlx::query(
                r#"
                UPDATE session_play_queue
                SET is_paused = true,
                    play_state = 'Paused',
                    updated_at = now()
                WHERE session_id = $1
                "#,
            )
            .bind(session_id)
            .execute(pool)
            .await?;
            if let Some(object) = summary_object {
                object.insert("IsPaused".to_string(), json!(true));
                object.insert("State".to_string(), json!("Paused"));
            }
        }
        "unpause" | "play" => {
            sqlx::query(
                r#"
                UPDATE session_play_queue
                SET is_paused = false,
                    play_state = 'Playing',
                    updated_at = now()
                WHERE session_id = $1
                "#,
            )
            .bind(session_id)
            .execute(pool)
            .await?;
            if let Some(object) = summary_object {
                object.insert("IsPaused".to_string(), json!(false));
                object.insert("State".to_string(), json!("Playing"));
            }
        }
        "stop" => {
            sqlx::query("DELETE FROM session_play_queue WHERE session_id = $1")
                .bind(session_id)
                .execute(pool)
                .await?;
            if let Some(object) = summary_object {
                object.insert("State".to_string(), json!("Stopped"));
                object.insert("PositionTicks".to_string(), json!(0));
            }
        }
        "seek" => {
            let position_ticks = payload
                .get("SeekPositionTicks")
                .or_else(|| payload.get("PositionTicks"))
                .or_else(|| payload.get("seekPositionTicks"))
                .or_else(|| payload.get("positionTicks"))
                .and_then(Value::as_i64);
            if let Some(position_ticks) = position_ticks {
                sqlx::query(
                    r#"
                    UPDATE session_play_queue
                    SET position_ticks = $2,
                        updated_at = now()
                    WHERE session_id = $1
                    "#,
                )
                .bind(session_id)
                .bind(position_ticks)
                .execute(pool)
                .await?;
                if let Some(object) = summary_object {
                    object.insert("PositionTicks".to_string(), json!(position_ticks));
                }
            }
        }
        "playpause" | "togglepause" => {
            sqlx::query(
                r#"
                UPDATE session_play_queue
                SET is_paused = NOT COALESCE(is_paused, false),
                    play_state = CASE
                        WHEN COALESCE(is_paused, false) THEN 'Playing'
                        ELSE 'Paused'
                    END,
                    updated_at = now()
                WHERE session_id = $1
                "#,
            )
            .bind(session_id)
            .execute(pool)
            .await?;
            if let Some(object) = summary_object {
                let next_is_paused = object
                    .get("IsPaused")
                    .and_then(Value::as_bool)
                    .map(|value| !value)
                    .unwrap_or(true);
                object.insert("IsPaused".to_string(), json!(next_is_paused));
                object.insert(
                    "State".to_string(),
                    json!(if next_is_paused { "Paused" } else { "Playing" }),
                );
            }
        }
        "setaudiostreamindex" => {
            if let Some(index) = payload
                .get("Index")
                .or_else(|| payload.get("AudioStreamIndex"))
                .or_else(|| payload.get("index"))
                .or_else(|| payload.get("audioStreamIndex"))
                .and_then(Value::as_i64)
            {
                if let Some(object) = summary_object {
                    object.insert("AudioStreamIndex".to_string(), json!(index));
                }
            }
        }
        "setsubtitlestreamindex" => {
            if let Some(index) = payload
                .get("Index")
                .or_else(|| payload.get("SubtitleStreamIndex"))
                .or_else(|| payload.get("index"))
                .or_else(|| payload.get("subtitleStreamIndex"))
                .and_then(Value::as_i64)
            {
                if let Some(object) = summary_object {
                    object.insert("SubtitleStreamIndex".to_string(), json!(index));
                }
            }
        }
        "setvolume" => {
            if let Some(level) = payload
                .get("Volume")
                .or_else(|| payload.get("VolumeLevel"))
                .or_else(|| payload.get("volume"))
                .or_else(|| payload.get("volumeLevel"))
                .and_then(Value::as_i64)
            {
                if let Some(object) = summary_object {
                    object.insert("VolumeLevel".to_string(), json!(level.clamp(0, 100)));
                    object.insert("IsMuted".to_string(), json!(level <= 0));
                }
            }
        }
        "displaymessage" => {
            if let Some(object) = summary_object {
                if let Some(header) = payload
                    .get("Header")
                    .or_else(|| payload.get("header"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                {
                    object.insert("LastMessageHeader".to_string(), json!(header));
                }
                if let Some(text) = payload
                    .get("Text")
                    .or_else(|| payload.get("text"))
                    .or_else(|| payload.get("Message"))
                    .or_else(|| payload.get("message"))
                    .and_then(Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                {
                    object.insert("LastMessageText".to_string(), json!(text));
                }
                object.insert("LastMessageDate".to_string(), json!(Utc::now()));
            }
        }
        "setadditionaluser" => {
            if let Some(user_id) = payload
                .get("UserId")
                .or_else(|| payload.get("userId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if let Some(object) = summary_object {
                    let mut users = object
                        .get("AdditionalUsers")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    if users.iter().all(|entry| entry.as_str() != Some(user_id)) {
                        users.push(json!(user_id));
                    }
                    object.insert("AdditionalUsers".to_string(), Value::Array(users));
                }
            }
        }
        _ => {}
    }

    set_session_state_summary(pool, session_id, summary).await?;

    Ok(())
}

pub async fn list_session_commands(
    pool: &sqlx::PgPool,
    session_id: &str,
    user_id: Uuid,
    is_admin: bool,
    start_index: i64,
    limit: i64,
    consume: bool,
) -> Result<QueryResult<Value>, AppError> {
    let session_owner =
        sqlx::query_scalar::<_, Uuid>("SELECT user_id FROM sessions WHERE access_token = $1")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;

    let Some(session_owner) = session_owner else {
        return Ok(QueryResult {
            items: Vec::new(),
            total_record_count: 0,
            start_index: Some(start_index.max(0)),
        });
    };

    if !is_admin && session_owner != user_id {
        return Err(AppError::Unauthorized);
    }

    let start_index = start_index.max(0);
    let limit = limit.clamp(1, 200);
    let total_record_count = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT COUNT(*)
        FROM session_commands
        WHERE session_id = $1
          AND consumed_at IS NULL
        "#,
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query_as::<_, SessionCommandRow>(
        r#"
        SELECT id, command, payload, created_at
        FROM session_commands
        WHERE session_id = $1
          AND consumed_at IS NULL
        ORDER BY created_at
        OFFSET $2
        LIMIT $3
        "#,
    )
    .bind(session_id)
    .bind(start_index)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    if consume && !rows.is_empty() {
        let command_ids: Vec<Uuid> = rows.iter().map(|row| row.id).collect();
        sqlx::query(
            r#"
            UPDATE session_commands
            SET consumed_at = now()
            WHERE session_id = $1
              AND id = ANY($2)
            "#,
        )
        .bind(session_id)
        .bind(command_ids)
        .execute(pool)
        .await?;
    }

    let items = rows.into_iter().map(session_command_to_json).collect();

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index),
    })
}

fn session_command_to_json(row: SessionCommandRow) -> Value {
    let payload_object = row.payload.as_object();
    let name = payload_object
        .and_then(|payload| payload.get("Name").or_else(|| payload.get("name")))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&row.command);
    let arguments = payload_object
        .and_then(|payload| {
            payload
                .get("Arguments")
                .or_else(|| payload.get("arguments"))
        })
        .cloned()
        .unwrap_or_else(|| json!({}));

    json!({
        "Id": row.id.to_string(),
        "Name": name,
        "Command": name,
        "Arguments": arguments,
        "Payload": row.payload,
        "DateCreated": row.created_at,
        "CreatedAt": row.created_at
    })
}

pub async fn get_display_preferences(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    display_preferences_id: &str,
    client: &str,
) -> Result<Option<Value>, AppError> {
    Ok(sqlx::query_scalar::<_, Value>(
        r#"
        SELECT preferences
        FROM display_preferences
        WHERE user_id = $1
          AND display_preferences_id = $2
          AND client = $3
        "#,
    )
    .bind(user_id)
    .bind(display_preferences_id)
    .bind(client)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_display_preferences_template(
    pool: &sqlx::PgPool,
    client: &str,
) -> Result<Option<Value>, AppError> {
    get_system_setting(
        pool,
        &format!(
            "display_preferences_defaults:{}",
            client.trim().to_ascii_lowercase()
        ),
    )
    .await
}

pub async fn upsert_display_preferences(
    pool: &sqlx::PgPool,
    user_id: Uuid,
    display_preferences_id: &str,
    client: &str,
    preferences: Value,
) -> Result<Value, AppError> {
    Ok(sqlx::query_scalar::<_, Value>(
        r#"
        INSERT INTO display_preferences (user_id, display_preferences_id, client, preferences, updated_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (user_id, display_preferences_id, client)
        DO UPDATE SET preferences = EXCLUDED.preferences, updated_at = now()
        RETURNING preferences
        "#,
    )
    .bind(user_id)
    .bind(display_preferences_id)
    .bind(client)
    .bind(preferences)
    .fetch_one(pool)
    .await?)
}

pub struct UpsertMediaItem<'a> {
    pub library_id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: &'a str,
    pub item_type: &'a str,
    pub media_type: &'a str,
    pub path: &'a Path,
    pub container: Option<&'a str>,
    pub original_title: Option<&'a str>,
    pub overview: Option<&'a str>,
    pub production_year: Option<i32>,
    pub official_rating: Option<&'a str>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub runtime_ticks: Option<i64>,
    pub premiere_date: Option<NaiveDate>,
    pub status: Option<&'a str>,
    pub end_date: Option<NaiveDate>,
    pub air_days: &'a [String],
    pub air_time: Option<&'a str>,
    pub provider_ids: Value,
    pub genres: &'a [String],
    pub studios: &'a [String],
    pub tags: &'a [String],
    pub production_locations: &'a [String],
    pub image_primary_path: Option<&'a Path>,
    pub backdrop_path: Option<&'a Path>,
    pub logo_path: Option<&'a Path>,
    pub thumb_path: Option<&'a Path>,
    pub art_path: Option<&'a Path>,
    pub banner_path: Option<&'a Path>,
    pub disc_path: Option<&'a Path>,
    /// 除 `backdrop_path` 外的额外壁纸（Emby `Backdrop/1`…）
    pub backdrop_paths: &'a [String],
    pub remote_trailers: &'a [String],
    pub series_name: Option<&'a str>,
    pub season_name: Option<&'a str>,
    pub index_number: Option<i32>,
    pub index_number_end: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub video_codec: Option<&'a str>,
    pub audio_codec: Option<&'a str>,
    pub series_id: Option<Uuid>,
    /// 当为 `true` 时，ON CONFLICT UPDATE 无条件用 EXCLUDED 的图片路径覆盖已有值，
    /// 跳过"已有本地路径 → 不被 HTTP URL 覆盖"的保护逻辑。
    /// 适用于增量同步 `force_refresh_sidecar = true` 场景：本地 jpg 已被物理删除，
    /// 需要把远端 HTTP URL 写回 DB 以便 sidecar worker 按新 ImageTag 重新下载。
    pub force_overwrite_images: bool,
}

/// 返回 `(item_id, was_inserted)`：
/// - `was_inserted=true` 表示这是一次"全新入库"（INSERT 走通），调用方可借此触发
///   `webhooks::events::ITEM_ADDED` 等"上线"类事件；
/// - `was_inserted=false` 表示这是 ON CONFLICT 触发的 UPDATE（已有 item 被刷新），
///   不应当二次推送 ITEM_ADDED。
///
/// 实现使用 PostgreSQL 的 `xmax = 0` 技巧：插入产生新行时 xmax = 0，更新已有行时 xmax 非零。
pub async fn upsert_media_item(
    pool: &sqlx::PgPool,
    input: UpsertMediaItem<'_>,
) -> Result<(Uuid, bool), AppError> {
    let path_text = input.path.to_string_lossy().to_string();
    let image_text = input
        .image_primary_path
        .map(|value| value.to_string_lossy().to_string());
    let backdrop_text = input
        .backdrop_path
        .map(|value| value.to_string_lossy().to_string());
    let logo_text = input
        .logo_path
        .map(|value| value.to_string_lossy().to_string());
    let thumb_text = input
        .thumb_path
        .map(|value| value.to_string_lossy().to_string());
    let art_text = input
        .art_path
        .map(|value| value.to_string_lossy().to_string());
    let banner_text = input
        .banner_path
        .map(|value| value.to_string_lossy().to_string());
    let disc_text = input
        .disc_path
        .map(|value| value.to_string_lossy().to_string());
    let backdrop_paths_vec: Vec<String> = input.backdrop_paths.to_vec();
    let sort_name = sort_name_for_item(&input);
    // PB45：id 由 `Uuid::new_v5(library_id, path)` 派生，本来想让 `(library_id, path)` 与
    // PK `(id)` 在新数据上一一对应；ON CONFLICT 改成 PK arbiter 来避开并发下 PK 先冲突
    // 没法被 (lib, path) arbiter 接住的 race。
    //
    // PB47：但 PB45 漏了一种"id 漂移"老行——库里历史上写过的某些 media_items 行，其 id
    // 不等于 `Uuid::new_v5(library_id, path)`。可能来源：
    //   1) 之前某个版本的 scanner 用过 Uuid::new_v4()
    //   2) 手动 INSERT / 数据导入脚本/迁移
    //   3) 同步路径中曾经短暂用过别的派生方式
    // PB45 之前 ON CONFLICT (library_id, path) 路径下，这些漂移行被 UPDATE 吞掉（SET 子句
    // 不修改 id 列），从未暴露。PB45 改 (id) 之后，新算的 v5 id ≠ 老行 id，PG 看到：
    //   - PK (id=v5_new): 不冲突（库里没有这个 id）
    //   - UNIQUE (library_id, path): 冲突
    //   - arbiter 是 (id), 不匹配 (lib, path) UNIQUE
    //   → 抛 `duplicate key value violates unique constraint "media_items_library_id_path_key"`
    //
    // PB47 修复：SELECT-first 拿到现存行的 id 优先复用——这样
    //   - 老漂移行：用其原 id INSERT → PK 命中 (id) arbiter → UPDATE 老行（保留 id 不变）
    //   - 全新行：SELECT 返回 None → 用 v5 → INSERT 成功
    //   - 并发同 (lib, path) 全新行：两 task 都 SELECT None，都用相同 v5 id；
    //     第一个 INSERT 成功，第二个 PK 冲突 → ON CONFLICT (id) → UPDATE
    //   - 并发同 (lib, path) 漂移老行：两 task 都 SELECT 拿到同一 existing_id；
    //     第一个 INSERT id=existing_id → PK 冲突 → UPDATE；
    //     第二个 INSERT id=existing_id → PK 冲突 → UPDATE
    //
    // PB48：PB47 仍漏一种极端 race——漂移老行 R0(id=X) 在 SELECT 之后被并发删除
    // （prune / cascade / 用户手动删），随后两个并发任务一个用 X 一个用 v5：
    //     T0 A SELECT(L, P) → X
    //     T1 R0 被删
    //     T2 B SELECT(L, P) → None（R0 已没）
    //     T3 A INSERT id=X → 行不存在 → 成功 → 新 R1(id=X, lib=L, path=P)
    //     T4 B INSERT id=v5 → PK 不冲突，但 (lib, path) 撞 R1 → 抛 (lib, path) 违例
    // 这是 PG ON CONFLICT 单仲裁器（只能一个）的固有局限：副 UNIQUE 索引冲突时
    // arbiter 接不住。
    //
    // 修复方案：finite retry。捕获到 `media_items_library_id_path_key` 时
    // 重新跑一次 SELECT-first——这次 R1 已可见，新一轮拿到 R1.id=X，重新 INSERT
    // id=X 走 ON CONFLICT (id) → DO UPDATE，收敛成功。
    //
    // 重试 3 次保底；超出说明热点异常密集（同一 (lib, path) 上有 ≥3 个并发删除+插入），
    // 这种情况已经不是同步路径正常工作流，让原 error 抛出去由上层处理。
    //
    // 性能：SELECT 走 UNIQUE (library_id, path) 索引，O(log n) 单点查询，几 μs 级；
    // 重试只在真冲突时发生（正常路径 0 重试），无稳态开销。
    const UPSERT_MAX_ATTEMPTS: usize = 3;
    for attempt in 0..UPSERT_MAX_ATTEMPTS {
        let existing_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM media_items WHERE library_id = $1 AND path = $2",
        )
        .bind(input.library_id)
        .bind(path_text.as_str())
        .fetch_optional(pool)
        .await?;
        let id = existing_id
            .unwrap_or_else(|| Uuid::new_v5(&input.library_id, path_text.as_bytes()));

        let insert_result = sqlx::query(
            r#"
            INSERT INTO media_items
                (
                    id, library_id, parent_id, name, original_title, sort_name, item_type, media_type, path,
                    container, overview, production_year, official_rating, community_rating, critic_rating,
                    runtime_ticks, premiere_date, status, end_date, air_days, air_time,
                    provider_ids, genres, studios, tags, production_locations,
                    image_primary_path, backdrop_path, logo_path, thumb_path,
                    art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
                    series_name, season_name, index_number,
                    index_number_end, parent_index_number, width, height, video_codec, audio_codec,
                    series_id, season_id,
                    date_modified
                )
            VALUES
                (
                    $1, $2, $3, $4, $5, $6, $7, $8,
                    $9, $10, $11, $12, $13, $14,
                    $15, $16, $17, $18, $19, $20,
                    $21, $22, $23, $24, $25,
                    $26, $27, $28, $29, $30, $31, $32, $33, $34,
                    $35, $36, $37, $38, $39, $40,
                    $41, $42, $43, $44,
                    $45,
                    -- season_id：Episode 的父就是 Season，直接复用 parent_id 自动推导，
                    -- 让 count_recursive_children_batch 的 `WHERE season_id = ANY($1)` 立刻能命中。
                    -- Season / Series / 其它类型留 NULL，避免污染索引扫描。
                    CASE WHEN $7 = 'Episode' THEN $3 ELSE NULL END,
                    now()
                )
            ON CONFLICT (id)
            DO UPDATE SET
                parent_id = EXCLUDED.parent_id,
                name = EXCLUDED.name,
                original_title = EXCLUDED.original_title,
                sort_name = EXCLUDED.sort_name,
                item_type = EXCLUDED.item_type,
                media_type = EXCLUDED.media_type,
                container = EXCLUDED.container,
                overview = EXCLUDED.overview,
                production_year = EXCLUDED.production_year,
                official_rating = EXCLUDED.official_rating,
                community_rating = EXCLUDED.community_rating,
                critic_rating = EXCLUDED.critic_rating,
                runtime_ticks = EXCLUDED.runtime_ticks,
                premiere_date = EXCLUDED.premiere_date,
                status = EXCLUDED.status,
                end_date = EXCLUDED.end_date,
                air_days = EXCLUDED.air_days,
                air_time = EXCLUDED.air_time,
                provider_ids = EXCLUDED.provider_ids,
                genres = EXCLUDED.genres,
                studios = EXCLUDED.studios,
                tags = EXCLUDED.tags,
                production_locations = EXCLUDED.production_locations,
                image_primary_path = CASE
                    WHEN $46 THEN EXCLUDED.image_primary_path
                    WHEN EXCLUDED.image_primary_path IS NULL THEN media_items.image_primary_path
                    WHEN media_items.image_primary_path IS NOT NULL
                         AND media_items.image_primary_path NOT LIKE 'http://%'
                         AND media_items.image_primary_path NOT LIKE 'https://%'
                         AND (EXCLUDED.image_primary_path LIKE 'http://%' OR EXCLUDED.image_primary_path LIKE 'https://%')
                    THEN media_items.image_primary_path
                    ELSE EXCLUDED.image_primary_path
                END,
                backdrop_path = CASE
                    WHEN $46 THEN EXCLUDED.backdrop_path
                    WHEN EXCLUDED.backdrop_path IS NULL THEN media_items.backdrop_path
                    WHEN media_items.backdrop_path IS NOT NULL
                         AND media_items.backdrop_path NOT LIKE 'http://%'
                         AND media_items.backdrop_path NOT LIKE 'https://%'
                         AND (EXCLUDED.backdrop_path LIKE 'http://%' OR EXCLUDED.backdrop_path LIKE 'https://%')
                    THEN media_items.backdrop_path
                    ELSE EXCLUDED.backdrop_path
                END,
                logo_path = CASE
                    WHEN $46 THEN EXCLUDED.logo_path
                    WHEN EXCLUDED.logo_path IS NULL THEN media_items.logo_path
                    WHEN media_items.logo_path IS NOT NULL
                         AND media_items.logo_path NOT LIKE 'http://%'
                         AND media_items.logo_path NOT LIKE 'https://%'
                         AND (EXCLUDED.logo_path LIKE 'http://%' OR EXCLUDED.logo_path LIKE 'https://%')
                    THEN media_items.logo_path
                    ELSE EXCLUDED.logo_path
                END,
                thumb_path = EXCLUDED.thumb_path,
                art_path = EXCLUDED.art_path,
                banner_path = EXCLUDED.banner_path,
                disc_path = EXCLUDED.disc_path,
                backdrop_paths = EXCLUDED.backdrop_paths,
                remote_trailers = EXCLUDED.remote_trailers,
                series_name = EXCLUDED.series_name,
                season_name = EXCLUDED.season_name,
                index_number = EXCLUDED.index_number,
                index_number_end = EXCLUDED.index_number_end,
                parent_index_number = EXCLUDED.parent_index_number,
                width = EXCLUDED.width,
                height = EXCLUDED.height,
                video_codec = EXCLUDED.video_codec,
                audio_codec = EXCLUDED.audio_codec,
                series_id = COALESCE(EXCLUDED.series_id, media_items.series_id),
                season_id = COALESCE(EXCLUDED.season_id, media_items.season_id),
                date_modified = now()
            "#,
        )
        .bind(id)
        .bind(input.library_id)
        .bind(input.parent_id)
        .bind(input.name)
        .bind(input.original_title)
        .bind(sort_name.as_str())
        .bind(input.item_type)
        .bind(input.media_type)
        .bind(path_text.as_str())
        .bind(input.container)
        .bind(input.overview)
        .bind(input.production_year)
        .bind(input.official_rating)
        .bind(input.community_rating)
        .bind(input.critic_rating)
        .bind(input.runtime_ticks)
        .bind(input.premiere_date)
        .bind(input.status)
        .bind(input.end_date)
        .bind(input.air_days)
        .bind(input.air_time)
        .bind(&input.provider_ids)
        .bind(input.genres)
        .bind(input.studios)
        .bind(input.tags)
        .bind(input.production_locations)
        .bind(image_text.as_deref())
        .bind(backdrop_text.as_deref())
        .bind(logo_text.as_deref())
        .bind(thumb_text.as_deref())
        .bind(art_text.as_deref())
        .bind(banner_text.as_deref())
        .bind(disc_text.as_deref())
        .bind(&backdrop_paths_vec)
        .bind(input.remote_trailers)
        .bind(input.series_name)
        .bind(input.season_name)
        .bind(input.index_number)
        .bind(input.index_number_end)
        .bind(input.parent_index_number)
        .bind(input.width)
        .bind(input.height)
        .bind(input.video_codec)
        .bind(input.audio_codec)
        .bind(input.series_id)
        .bind(input.force_overwrite_images)
        .execute(pool)
        .await;

        match insert_result {
            Ok(_) => {
                // INSERT 路径：xmax=0 → 此前 0 行影响转 1 行；ON CONFLICT UPDATE 路径会改 xmax≠0，
                // 但 affected row 也是 1。所以单看 affected 区分不了，要再补一个 SELECT。
                let was_inserted: bool = sqlx::query_scalar(
                    "SELECT (xmax = 0) FROM media_items WHERE id = $1",
                )
                .bind(id)
                .fetch_optional(pool)
                .await?
                .unwrap_or(false);
                return Ok((id, was_inserted));
            }
            Err(error) => {
                if let sqlx::Error::Database(ref db_err) = error {
                    let constraint = db_err.constraint();
                    // PB48：(library_id, path) 漂移行 race，仍有重试余量则继续。
                    if constraint == Some("media_items_library_id_path_key")
                        && attempt + 1 < UPSERT_MAX_ATTEMPTS
                    {
                        tracing::warn!(
                            library_id = %input.library_id,
                            path = %path_text,
                            attempt = attempt + 1,
                            "PB48：检测到 (library_id, path) 唯一约束冲突（漂移行 race），重试 SELECT-first"
                        );
                        continue;
                    }
                    // PB49：父行外键违例的诊断埋点。
                    //
                    // 这是用户报告的 `media_items_parent_id_fkey` 报错的「现场告警」：
                    // 在故障真正发生的那一行附上 (库, 路径, 类型, 父行 UUID) 四元组，
                    // 配合 PB49 的 per-source 互斥 + DetailHandlesGuard，能立即定位到
                    // 究竟是哪个 Series/Season 父行被并发删了。日志只在出错时记录一次，
                    // 没有稳态开销。
                    if constraint == Some("media_items_parent_id_fkey") {
                        tracing::error!(
                            library_id = %input.library_id,
                            parent_id = ?input.parent_id,
                            item_type = %input.item_type,
                            path = %path_text,
                            name = %input.name,
                            series_id = ?input.series_id,
                            db_message = %db_err.message(),
                            "PB49：父行外键违例（media_items_parent_id_fkey）—— \
                             疑似 Series/Season 父行被并发任务删除后才 INSERT 子行；\
                             若仍频繁出现，请检查同源是否有未屏蔽的并发同步入口"
                        );
                        // PB49：FK 违例不要被 enqueue_library_scan 的 sqlx-retry 守卫
                        // 反复吞 3 次——重试是治标不治本，每次都打同样的 FK error，
                        // 上层只会看到「3 次都失败」而看不到第一现场。
                        // 转成 AppError::Internal（不被 sqlx-retry 捕获）让错误立刻
                        // 浮到 UI，配合上面的 error! 日志一击命中根因。
                        return Err(AppError::Internal(format!(
                            "父行外键违例 (parent_id={:?}, library_id={}, path={}): {}",
                            input.parent_id, input.library_id, path_text, db_err.message()
                        )));
                    }
                }
                return Err(error.into());
            }
        }
    }
    // 上面 for 循环要么 return Ok / return Err，要么 continue；走到这里说明
    // 用尽了重试次数都没成功——这条分支由最后一次循环里的 `attempt + 1 < MAX` 守卫
    // 决定不重试而直接 return Err，所以理论上不可达。
    unreachable!("PB48 retry loop exited without resolution")
}

pub fn user_to_dto(user: &DbUser, server_id: Uuid) -> UserDto {
    // 尝试从数据库的policy字段反序列化UserPolicyDto
    let mut policy = if !user.policy.is_null() {
        match serde_json::from_value::<UserPolicyDto>(user.policy.clone()) {
            Ok(p) => p,
            Err(_) => UserPolicyDto::default(),
        }
    } else {
        UserPolicyDto::default()
    };

    // 覆盖关键字段以确保与数据库状态一致
    policy.is_administrator = user.is_admin;
    policy.is_hidden = user.is_hidden;
    policy.is_disabled = user.is_disabled;

    let configuration = if !user.configuration.is_null() {
        serde_json::from_value::<UserConfigurationDto>(user.configuration.clone())
            .unwrap_or_default()
    } else {
        UserConfigurationDto::default()
    };

    let has_password = !user.password_hash.trim().is_empty();
    let has_easy_password = user
        .easy_password_hash
        .as_deref()
        .is_some_and(|hash| !hash.trim().is_empty());

    let primary_image_tag = user
        .primary_image_path
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(|_| user.date_modified.timestamp().to_string());

    UserDto {
        name: user.name.clone(),
        server_id: uuid_to_emby_guid(&server_id),
        id: uuid_to_emby_guid(&user.id),
        has_password,
        has_configured_password: has_password,
        has_configured_easy_password: has_easy_password,
        primary_image_tag,
        last_login_date: None,
        last_activity_date: None,
        date_created: user.created_at,
        policy,
        configuration,
    }
}

pub fn user_to_public_dto(user: &DbUser, server_id: Uuid) -> PublicUserDto {
    let has_password = !user.password_hash.trim().is_empty();
    let has_easy_password = user
        .easy_password_hash
        .as_deref()
        .is_some_and(|hash| !hash.trim().is_empty());

    PublicUserDto {
        name: user.name.clone(),
        server_id: uuid_to_emby_guid(&server_id),
        id: uuid_to_emby_guid(&user.id),
        has_password,
        has_configured_password: has_password,
        has_configured_easy_password: has_easy_password,
    }
}

pub fn session_to_dto(session: &AuthSessionRow, server_id: Uuid) -> SessionInfoDto {
    SessionInfoDto {
        id: session.access_token.clone(),
        user_id: uuid_to_emby_guid(&session.user_id),
        user_name: session.user_name.clone(),
        server_id: uuid_to_emby_guid(&server_id),
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
        is_active: session.expires_at.is_none_or(|value| value > Utc::now()),
        last_activity_date: session.last_activity_at,
        remote_end_point: session.remote_address.clone(),
        supports_remote_control: true,
        playable_media_types: vec!["Audio".to_string(), "Video".to_string()],
        supported_commands: vec![
            "MoveUp".to_string(),
            "MoveDown".to_string(),
            "MoveLeft".to_string(),
            "MoveRight".to_string(),
            "PageUp".to_string(),
            "PageDown".to_string(),
            "PreviousLetter".to_string(),
            "NextLetter".to_string(),
            "ToggleOsd".to_string(),
            "ToggleContextMenu".to_string(),
            "Select".to_string(),
            "Back".to_string(),
            "SendKey".to_string(),
            "SendString".to_string(),
            "GoHome".to_string(),
            "GoToSettings".to_string(),
            "VolumeUp".to_string(),
            "VolumeDown".to_string(),
            "Mute".to_string(),
            "Unmute".to_string(),
            "ToggleMute".to_string(),
            "SetVolume".to_string(),
            "SetAudioStreamIndex".to_string(),
            "SetSubtitleStreamIndex".to_string(),
            "DisplayContent".to_string(),
            "GoToSearch".to_string(),
            "DisplayMessage".to_string(),
            "SetRepeatMode".to_string(),
            "ChannelUp".to_string(),
            "ChannelDown".to_string(),
            "PlayMediaSource".to_string(),
            "PlayTrailers".to_string(),
            "SetShuffleQueue".to_string(),
            "PlayState".to_string(),
            "PlayNext".to_string(),
            "ToggleFullscreen".to_string(),
        ],
        now_playing_item: None,
        now_viewing_item: None,
        play_state: None,
        additional_users: vec![],
        now_playing_queue: vec![],
        user_primary_image_tag: None,
    }
}

/// Batch library statistics: one query for all libraries
///
/// 字段语义按 EmbySDK CollectionFolder 对齐：
/// - `child_count`：库内**顶层**条目数（Series/Movie/Folder/BoxSet 之和，
///   不含 Season/Episode 这类嵌套子节点）。Sidebar 上显示的就是这个数字。
/// - `recursive_item_count`：库内全量 media_items 数（含所有后代）。
/// - `movie_count` / `series_count`：分别精确到顶层电影/剧集数，用于
///   `MovieCount` / `SeriesCount` 字段输出。
pub struct LibraryStats {
    pub child_count: i64,
    pub recursive_item_count: i64,
    pub movie_count: i32,
    pub series_count: i32,
}

pub async fn batch_library_stats(
    pool: &sqlx::PgPool,
    library_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, LibraryStats>, AppError> {
    if library_ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let rows: Vec<(Uuid, Option<String>, i64)> = sqlx::query_as(
        "SELECT library_id, item_type, COUNT(*)::bigint \
         FROM media_items WHERE library_id = ANY($1) \
         GROUP BY library_id, item_type",
    )
    .bind(library_ids)
    .fetch_all(pool)
    .await?;

    let mut stats: std::collections::HashMap<Uuid, LibraryStats> = library_ids
        .iter()
        .map(|id| {
            (
                *id,
                LibraryStats {
                    child_count: 0,
                    recursive_item_count: 0,
                    movie_count: 0,
                    series_count: 0,
                },
            )
        })
        .collect();

    for (lib_id, item_type, count) in rows {
        if let Some(s) = stats.get_mut(&lib_id) {
            s.recursive_item_count += count;
            // top-level item types: Movie / Series / BoxSet / Folder。
            // Season / Episode / Audio / Video 等是嵌套子节点，不计入 ChildCount。
            match item_type.as_deref() {
                Some("Movie") => {
                    s.movie_count = count as i32;
                    s.child_count += count;
                }
                Some("Series") => {
                    s.series_count = count as i32;
                    s.child_count += count;
                }
                Some("BoxSet") | Some("Folder") => {
                    s.child_count += count;
                }
                _ => {}
            }
        }
    }
    Ok(stats)
}

pub async fn library_to_item_dto_with_stats(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
    server_id: Uuid,
    stats: Option<&LibraryStats>,
) -> Result<BaseItemDto, AppError> {
    let (child_count, recursive_item_count, movie_count, series_count) = if let Some(s) = stats {
        (s.child_count, s.recursive_item_count, s.movie_count, s.series_count)
    } else {
        // 兜底：单独 SQL 一次性算齐顶层（Movie/Series/BoxSet/Folder）+ 全量 + 分类统计。
        // 走到这里说明 batch_library_stats 没把这库放进 map，理论上不会发生。
        let m = count_library_items_by_type(pool, library.id, "Movie").await?;
        let s = count_library_items_by_type(pool, library.id, "Series").await?;
        let total = count_library_children(pool, library.id).await?;
        let top = (m as i64) + (s as i64);
        (top, total, m, s)
    };

    library_to_item_dto_inner(
        pool,
        library,
        server_id,
        child_count,
        recursive_item_count,
        movie_count,
        series_count,
    )
    .await
}

pub async fn library_to_item_dto(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
    server_id: Uuid,
) -> Result<BaseItemDto, AppError> {
    let recursive_item_count = count_library_children(pool, library.id).await?;
    let movie_count = count_library_items_by_type(pool, library.id, "Movie").await?;
    let series_count = count_library_items_by_type(pool, library.id, "Series").await?;
    let child_count = (movie_count as i64) + (series_count as i64);
    library_to_item_dto_inner(
        pool,
        library,
        server_id,
        child_count,
        recursive_item_count,
        movie_count,
        series_count,
    )
    .await
}

async fn library_to_item_dto_inner(
    pool: &sqlx::PgPool,
    library: &DbLibrary,
    server_id: Uuid,
    child_count: i64,
    recursive_item_count: i64,
    movie_count: i32,
    series_count: i32,
) -> Result<BaseItemDto, AppError> {
    let locations = library_paths(library);

    let mut image_tags: BTreeMap<String, String> = BTreeMap::new();

    if let Some(path) = library.primary_image_path.as_ref() {
        if !path.trim().is_empty() {
            let tag = library
                .primary_image_tag
                .clone()
                .unwrap_or_else(|| library.created_at.timestamp().to_string());
            image_tags.insert("Primary".to_string(), tag);
        }
    }

    if image_tags.is_empty() {
        if let Some((_child_id, _path, child_modified)) =
            first_library_child_image(pool, library.id).await?
        {
            let tag = child_modified.timestamp().to_string();
            image_tags.insert("Primary".to_string(), tag);
        }
    }

    Ok(BaseItemDto {
        name: library.name.clone(),
        original_title: None,
        server_id: uuid_to_emby_guid(&server_id),
        id: uuid_to_emby_guid(&library.id),
        guid: Some(uuid_to_emby_guid(&library.id)),
        etag: None,
        date_modified: None,
        can_delete: None,
        can_download: false,
        can_edit_items: None,
        supports_resume: None,
        presentation_unique_key: None,
        supports_sync: None,
        item_type: "CollectionFolder".to_string(),
        is_folder: true,
        sort_name: None,
        forced_sort_name: None,
        primary_image_tag: None,
        collection_type: Some(library.collection_type.clone()),
        media_type: None,
        container: None,
        parent_id: None,
        path: None,
        location_type: None,
        run_time_ticks: None,
        production_year: None,
        overview: None,
        date_created: None,
        premiere_date: None,
        video_codec: None,
        audio_codec: None,
        average_frame_rate: None,
        real_frame_rate: None,
        genres: Vec::new(),
        genre_items: Vec::new(),
        provider_ids: BTreeMap::new(),
        external_urls: Vec::new(),
        production_locations: Vec::new(),
        size: None,
        file_name: None,
        bitrate: None,
        official_rating: None,
        community_rating: None,
        critic_rating: None,
        taglines: Vec::new(),
        remote_trailers: Vec::new(),
        people: Vec::new(),
        studios: Vec::new(),
        tag_items: Vec::new(),
        local_trailer_count: None,
        display_preferences_id: None,
        playlist_item_id: None,
        recursive_item_count: if recursive_item_count > 0 { Some(recursive_item_count) } else { None },
        season_count: None,
        series_count: if series_count > 0 { Some(series_count) } else { None },
        movie_count: if movie_count > 0 { Some(movie_count) } else { None },
        status: None,
        air_days: Vec::new(),
        air_time: None,
        end_date: None,
        width: None,
        height: None,
        is_movie: None,
        is_series: None,
        is_live: None,
        is_news: None,
        is_kids: None,
        is_sports: None,
        is_premiere: None,
        is_new: None,
        is_repeat: None,
        disabled: None,
        series_name: None,
        series_id: None,
        season_name: None,
        season_id: None,
        index_number: None,
        index_number_end: None,
        parent_index_number: None,
        image_tags,
        image_blur_hashes: None,
        backdrop_image_tags: Vec::new(),
        parent_logo_item_id: None,
        parent_logo_image_tag: None,
        parent_backdrop_item_id: None,
        parent_backdrop_image_tags: Vec::new(),
        parent_thumb_item_id: None,
        parent_thumb_image_tag: None,
        thumb_image_tag: None,
        series_primary_image_tag: None,
        primary_image_item_id: None,
        series_studio: None,
        user_data: empty_user_data(),
        media_sources: Vec::new(),
        media_streams: Vec::new(),
        part_count: None,
        chapters: Vec::new(),
        locked_fields: Vec::new(),
        lock_data: None,
        special_feature_count: None,
        // EmbySDK CollectionFolder.ChildCount = 顶层直接子项数（不含 Season/Episode）。
        // 旧版本误把 `locations.len()`（库扫描路径数）当成 ChildCount，导致 sidebar
        // 永远显示 1 / 3 而不是真实数。这里用 batch_library_stats 已算出来的
        // Movie+Series+BoxSet+Folder 之和。
        child_count: Some(child_count.max(0)),
        display_order: None,
        primary_image_aspect_ratio: None,
        completion_percentage: None,
        tags: Vec::new(),
        extra_fields: BTreeMap::new(),
    })
}

fn parse_blur_hashes(
    value: &serde_json::Value,
) -> Option<BTreeMap<String, BTreeMap<String, String>>> {
    if value.is_null() || value.as_object().map_or(true, |o| o.is_empty()) {
        return None;
    }
    serde_json::from_value(value.clone()).ok()
}

pub fn root_item_dto(server_id: Uuid) -> BaseItemDto {
    BaseItemDto {
        name: "Root".to_string(),
        original_title: None,
        server_id: uuid_to_emby_guid(&server_id),
        id: uuid_to_emby_guid(&Uuid::nil()),
        guid: Some(uuid_to_emby_guid(&Uuid::nil())),
        etag: None,
        date_modified: Some(Utc::now()),
        can_delete: Some(false),
        can_download: false,
        can_edit_items: Some(false),
        supports_resume: Some(false),
        presentation_unique_key: Some("root_".to_string()),
        supports_sync: Some(true),
        item_type: "Folder".to_string(),
        is_folder: true,
        sort_name: Some("root".to_string()),
        forced_sort_name: Some("root".to_string()),
        primary_image_tag: None,
        collection_type: None,
        media_type: None,
        container: None,
        parent_id: None,
        path: None,
        location_type: Some("Virtual".to_string()),
        run_time_ticks: None,
        production_year: None,
        overview: None,
        date_created: Some(Utc::now()),
        premiere_date: None,
        video_codec: None,
        audio_codec: None,
        average_frame_rate: None,
        real_frame_rate: None,
        genres: Vec::new(),
        genre_items: Vec::new(),
        provider_ids: BTreeMap::new(),
        external_urls: Vec::new(),
        production_locations: Vec::new(),
        size: Some(0),
        file_name: None,
        bitrate: None,
        official_rating: None,
        community_rating: None,
        critic_rating: None,
        taglines: Vec::new(),
        remote_trailers: Vec::new(),
        people: Vec::new(),
        studios: Vec::new(),
        tag_items: Vec::new(),
        local_trailer_count: Some(0),
        display_preferences_id: Some("root".to_string()),
        playlist_item_id: None,
        recursive_item_count: None,
        season_count: None,
        series_count: None,
        movie_count: None,
        status: None,
        air_days: Vec::new(),
        air_time: None,
        end_date: None,
        width: None,
        height: None,
        is_movie: Some(false),
        is_series: Some(false),
        is_live: Some(false),
        is_news: Some(false),
        is_kids: Some(false),
        is_sports: Some(false),
        is_premiere: Some(false),
        is_new: Some(false),
        is_repeat: Some(false),
        disabled: Some(false),
        series_name: None,
        series_id: None,
        season_name: None,
        season_id: None,
        index_number: None,
        index_number_end: None,
        parent_index_number: None,
        image_tags: BTreeMap::new(),
        image_blur_hashes: None,
        backdrop_image_tags: Vec::new(),
        parent_logo_item_id: None,
        parent_logo_image_tag: None,
        parent_backdrop_item_id: None,
        parent_backdrop_image_tags: Vec::new(),
        parent_thumb_item_id: None,
        parent_thumb_image_tag: None,
        thumb_image_tag: None,
        series_primary_image_tag: None,
        primary_image_item_id: None,
        series_studio: None,
        user_data: {
            let mut value = empty_user_data();
            value.server_id = Some(uuid_to_emby_guid(&server_id));
            value
        },
        media_sources: Vec::new(),
        media_streams: Vec::new(),
        part_count: Some(0),
        chapters: Vec::new(),
        locked_fields: Vec::new(),
        lock_data: Some(false),
        special_feature_count: Some(0),
        child_count: None,
        display_order: None,
        primary_image_aspect_ratio: None,
        completion_percentage: None,
        tags: Vec::new(),
        extra_fields: BTreeMap::new(),
    }
}

pub async fn media_item_to_dto(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    user_id: Option<Uuid>,
    server_id: Uuid,
) -> Result<BaseItemDto, AppError> {
    media_item_to_dto_inner(
        pool,
        item,
        user_id,
        server_id,
        None,
        DtoCountPrefetch::default(),
    )
    .await
}

/// 列表专用：调用方已经批量预取好 UserItemData 就通过 `prefetched_user_data` 传入，
/// 避免在 `media_item_to_dto` 内部按条重复 SELECT（N+1）。
/// - `Some(Some(data))` —— 已知 DB 存在该行；
/// - `Some(None)`       —— 已知 DB 不存在（走 empty 默认值）；
/// - `None`             —— 未提供预取，走老逻辑内部查询。
pub async fn media_item_to_dto_with_user_data(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    user_id: Option<Uuid>,
    server_id: Uuid,
    prefetched_user_data: Option<Option<DbUserItemData>>,
) -> Result<BaseItemDto, AppError> {
    media_item_to_dto_inner(
        pool,
        item,
        user_id,
        server_id,
        prefetched_user_data,
        DtoCountPrefetch::default(),
    )
    .await
}

/// 列表接口给 Folder 类型条目预取子代计数，避免每条 item 重复跑
/// `count_item_children` / `count_recursive_children` / `count_series_seasons` 三次独立 SQL。
#[derive(Debug, Default, Clone, Copy)]
pub struct DtoCountPrefetch {
    pub child_count: Option<i64>,
    pub recursive_item_count: Option<i64>,
    pub season_count: Option<i32>,
}

/// 列表专用零查询 DTO 构建：仅使用已有 DbMediaItem 字段 + 预取的 user_data + 预取的 counts，
/// 不做任何额外 DB 查询（跳过 media_sources、people、parent/series lookup、metadata_preferences）。
/// 百万级列表从 N+1 → 0 额外查询，200 条列表从 ~53s 降到 <1s。
pub fn media_item_to_dto_for_list(
    item: &DbMediaItem,
    server_id: Uuid,
    prefetched_user_data: Option<Option<DbUserItemData>>,
    counts: DtoCountPrefetch,
) -> BaseItemDto {
    let is_folder = is_folder_item(item);

    let mut image_tags = BTreeMap::new();
    if item.image_primary_path.is_some() {
        image_tags.insert("Primary".to_string(), item.date_modified.timestamp().to_string());
    }
    if item.logo_path.is_some() {
        image_tags.insert("Logo".to_string(), item.date_modified.timestamp().to_string());
    }
    if item.thumb_path.is_some() {
        image_tags.insert("Thumb".to_string(), item.date_modified.timestamp().to_string());
    }
    if item.banner_path.is_some() {
        image_tags.insert("Banner".to_string(), item.date_modified.timestamp().to_string());
    }
    if item.disc_path.is_some() {
        image_tags.insert("Disc".to_string(), item.date_modified.timestamp().to_string());
    }
    if item.art_path.is_some() {
        image_tags.insert("Art".to_string(), item.date_modified.timestamp().to_string());
    }

    let backdrop_tag = item.date_modified.timestamp().to_string();
    let backdrop_count = item.backdrop_path.is_some() as usize + item.backdrop_paths.len();
    let backdrop_image_tags: Vec<String> = vec![backdrop_tag.clone(); backdrop_count];

    let mut user_data = match prefetched_user_data {
        Some(Some(data)) => {
            let mut dto = user_item_data_to_dto_with_runtime(data, item.runtime_ticks);
            let emby_id = uuid_to_emby_guid(&item.id);
            dto.key = Some(emby_id.clone());
            dto.item_id = Some(emby_id);
            dto
        }
        _ => empty_user_data_for_item(item.id),
    };
    user_data.server_id = Some(uuid_to_emby_guid(&server_id));

    let primary_image_aspect_ratio = infer_primary_image_aspect_ratio(item, item.width, item.height);
    let primary_image_tag = item.image_primary_path.as_ref().map(|_| item.date_modified.timestamp().to_string());

    let child_count = if is_folder { Some(counts.child_count.unwrap_or(0)) } else { None };
    let recursive_item_count = if is_folder { Some(counts.recursive_item_count.unwrap_or(0)) } else { None };
    let season_count = if item.item_type.eq_ignore_ascii_case("Series") { Some(counts.season_count.unwrap_or(0)) } else { None };

    let series_id = match item.item_type.as_str() {
        "Season" => item.series_id.or(item.parent_id).map(|id| uuid_to_emby_guid(&id)),
        "Episode" => item.series_id.map(|id| uuid_to_emby_guid(&id)),
        _ => None,
    };
    let season_id = match item.item_type.as_str() {
        "Episode" => item.parent_id.map(|id| uuid_to_emby_guid(&id)),
        _ => None,
    };
    // 为 Episode/Season 填充继承的图片字段（列表模式无 DB 查询，从已知字段推导）
    let is_ep_or_season = item.item_type.eq_ignore_ascii_case("Episode")
        || item.item_type.eq_ignore_ascii_case("Season");
    let resolved_series_uuid = if is_ep_or_season {
        item.series_id.or_else(|| {
            if item.item_type.eq_ignore_ascii_case("Season") { item.parent_id } else { None }
        })
    } else {
        None
    };
    let series_primary_image_tag = if is_ep_or_season {
        resolved_series_uuid.map(|_| item.date_modified.timestamp().to_string())
    } else {
        None
    };
    let parent_backdrop_item_id = if is_ep_or_season {
        resolved_series_uuid.map(|id| uuid_to_emby_guid(&id))
    } else {
        None
    };
    let parent_backdrop_image_tags = if is_ep_or_season && resolved_series_uuid.is_some() {
        vec![item.date_modified.timestamp().to_string()]
    } else {
        Vec::new()
    };

    let completion_percentage = if is_folder {
        user_data.played_percentage
    } else {
        match (item.runtime_ticks, user_data.playback_position_ticks) {
            (Some(runtime_ticks), position) if runtime_ticks > 0 && position > 0 => {
                Some(((position as f64 / runtime_ticks as f64) * 100.0).min(100.0))
            }
            _ => user_data.played_percentage,
        }
    };

    BaseItemDto {
        name: item.name.clone(),
        original_title: item.original_title.clone(),
        server_id: uuid_to_emby_guid(&server_id),
        id: uuid_to_emby_guid(&item.id),
        guid: Some(uuid_to_emby_guid(&item.id)),
        etag: Some(item_etag(item)),
        date_modified: Some(item.date_modified),
        can_delete: Some(true),
        can_download: !is_folder,
        can_edit_items: Some(true),
        supports_resume: Some(!is_folder),
        presentation_unique_key: Some(presentation_unique_key(
            item,
            &provider_ids_to_map(&item.provider_ids),
        )),
        supports_sync: Some(true),
        item_type: item.item_type.clone(),
        is_folder,
        sort_name: Some(item.sort_name.clone()),
        forced_sort_name: Some(item.sort_name.clone()),
        primary_image_tag: primary_image_tag.clone(),
        collection_type: None,
        media_type: (!is_folder).then(|| item.media_type.clone()),
        container: item.container.clone(),
        parent_id: item.parent_id.map(|value| uuid_to_emby_guid(&value)),
        path: sanitize_item_path(item),
        location_type: Some(if item.path.starts_with("http://") || item.path.starts_with("https://") { "Remote".to_string() } else { "FileSystem".to_string() }),
        run_time_ticks: item.runtime_ticks,
        production_year: item.production_year,
        overview: item.overview.clone(),
        date_created: Some(item.date_created),
        premiere_date: premiere_date_to_utc(item.premiere_date),
        video_codec: item.video_codec.clone(),
        audio_codec: item.audio_codec.clone(),
        average_frame_rate: None,
        real_frame_rate: None,
        genres: item.genres.clone(),
        genre_items: genre_items_from_names(&item.genres),
        provider_ids: provider_ids_to_map(&item.provider_ids),
        // PB35-3：list DTO 也带回 ExternalUrls（IMDb / TMDb / TVDb 跳转链接）。
        external_urls: external_urls_from_provider_map(
            &provider_ids_to_map(&item.provider_ids),
            &item.item_type,
        ),
        production_locations: item.production_locations.clone(),
        size: None,
        file_name: None,
        bitrate: None,
        official_rating: item.official_rating.clone(),
        community_rating: item.community_rating,
        critic_rating: item.critic_rating,
        taglines: item.taglines.clone(),
        remote_trailers: remote_trailers_from_urls(&item.remote_trailers),
        people: Vec::new(),
        studios: name_long_id_items_from_names(&item.studios),
        tag_items: name_long_id_items_from_names(&item.tags),
        local_trailer_count: Some(0),
        display_preferences_id: None,
        playlist_item_id: None,
        recursive_item_count,
        season_count,
        series_count: None,
        movie_count: None,
        status: item.status.clone(),
        air_days: item.air_days.clone(),
        air_time: item.air_time.clone(),
        end_date: premiere_date_to_utc(item.end_date),
        width: item.width,
        height: item.height,
        is_movie: Some(item.item_type.eq_ignore_ascii_case("Movie")),
        is_series: Some(item.item_type.eq_ignore_ascii_case("Series")),
        is_live: Some(false),
        is_news: Some(false),
        is_kids: Some(false),
        is_sports: Some(false),
        is_premiere: Some(false),
        is_new: Some(false),
        is_repeat: Some(false),
        disabled: Some(false),
        series_name: item.series_name.clone(),
        series_id,
        season_name: item.season_name.clone(),
        season_id,
        index_number: item.index_number,
        index_number_end: item.index_number_end,
        parent_index_number: item.parent_index_number,
        thumb_image_tag: image_tags.get("Thumb").cloned(),
        image_tags,
        image_blur_hashes: parse_blur_hashes(&item.image_blur_hashes),
        backdrop_image_tags,
        parent_logo_item_id: None,
        parent_logo_image_tag: None,
        parent_backdrop_item_id,
        parent_backdrop_image_tags,
        parent_thumb_item_id: None,
        parent_thumb_image_tag: None,
        series_primary_image_tag,
        primary_image_item_id: if primary_image_tag.is_some() {
            Some(uuid_to_emby_guid(&item.id))
        } else if is_ep_or_season {
            resolved_series_uuid.map(|id| uuid_to_emby_guid(&id))
        } else {
            None
        },
        series_studio: item.studios.first().cloned(),
        user_data,
        media_sources: Vec::new(),
        media_streams: Vec::new(),
        part_count: Some(if is_folder { 0 } else { 1 }),
        chapters: Vec::new(),
        locked_fields: item.locked_fields.clone(),
        lock_data: if item.lock_data { Some(true) } else { Some(false) },
        special_feature_count: Some(0),
        child_count,
        display_order: item.display_order.clone(),
        primary_image_aspect_ratio,
        completion_percentage,
        tags: item.tags.clone(),
        extra_fields: BTreeMap::new(),
    }
}

async fn media_item_to_dto_inner(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    user_id: Option<Uuid>,
    server_id: Uuid,
    prefetched_user_data: Option<Option<DbUserItemData>>,
    prefetched_counts: DtoCountPrefetch,
) -> Result<BaseItemDto, AppError> {
    let mut image_tags = BTreeMap::new();
    if item.image_primary_path.is_some() {
        image_tags.insert(
            "Primary".to_string(),
            item.date_modified.timestamp().to_string(),
        );
    }
    if item.logo_path.is_some() {
        image_tags.insert(
            "Logo".to_string(),
            item.date_modified.timestamp().to_string(),
        );
    }
    if item.thumb_path.is_some() {
        image_tags.insert(
            "Thumb".to_string(),
            item.date_modified.timestamp().to_string(),
        );
    }
    if item.banner_path.is_some() {
        image_tags.insert(
            "Banner".to_string(),
            item.date_modified.timestamp().to_string(),
        );
    }
    if item.disc_path.is_some() {
        image_tags.insert(
            "Disc".to_string(),
            item.date_modified.timestamp().to_string(),
        );
    }
    if item.art_path.is_some() {
        image_tags.insert(
            "Art".to_string(),
            item.date_modified.timestamp().to_string(),
        );
    }

    let backdrop_tag = item.date_modified.timestamp().to_string();
    let backdrop_count = item.backdrop_path.is_some() as usize + item.backdrop_paths.len();
    let backdrop_image_tags: Vec<String> = vec![backdrop_tag.clone(); backdrop_count];

    let mut user_data = match (user_id, prefetched_user_data) {
        (_, Some(Some(data))) => {
            let mut dto = user_item_data_to_dto_with_runtime(data, item.runtime_ticks);
            let emby_id = uuid_to_emby_guid(&item.id);
            dto.key = Some(emby_id.clone());
            dto.item_id = Some(emby_id);
            dto
        }
        (_, Some(None)) => empty_user_data_for_item(item.id),
        (Some(user_id), None) => get_user_item_data(pool, user_id, item.id)
            .await?
            .map(|data| {
                let mut dto = user_item_data_to_dto_with_runtime(data, item.runtime_ticks);
                let emby_id = uuid_to_emby_guid(&item.id);
                dto.key = Some(emby_id.clone());
                dto.item_id = Some(emby_id);
                dto
            })
            .unwrap_or_else(|| empty_user_data_for_item(item.id)),
        (None, None) => empty_user_data_for_item(item.id),
    };
    user_data.server_id = Some(uuid_to_emby_guid(&server_id));

    let is_folder = is_folder_item(item);
    let is_series = item.item_type.eq_ignore_ascii_case("Series");

    // PB30：详情页 DTO 的 8 个独立查询此前**串行 await**，warm cache 命中也要绕一圈
    // 数据库 round-trip 多次（实测 30-50ms）。同一 connection pool 内并发跑彼此独立
    // 的只读 SELECT 完全安全；用 tokio::try_join! 一次拉齐 7 个查询：
    //   - media_sources_for_item（非 Folder 才做）
    //   - count_item_children / count_recursive_children（Folder 才做，且 prefetch 优先）
    //   - count_series_seasons（Series 才做，且 prefetch 优先）
    //   - resolve_series_and_season_ids（Episode/Season 拓父）
    //   - parent_item lookup（按 parent_id）
    //   - get_item_people（演职人员）
    //   - metadata_preferences_from_settings（全局偏好，pool 内共享行）
    // series_item 依赖 resolve_series_and_season_ids 的结果，先放在 join 之后单独跑。
    let media_sources_fut = async {
        if !is_folder {
            media_sources_for_item(pool, item, server_id).await
        } else {
            Ok::<_, AppError>(Vec::new())
        }
    };
    let child_count_fut = async {
        if is_folder {
            match prefetched_counts.child_count {
                Some(value) => Ok::<_, AppError>(Some(value)),
                None => count_item_children(pool, item.id).await.map(Some),
            }
        } else {
            Ok(None)
        }
    };
    let recursive_item_count_fut = async {
        if is_folder {
            match prefetched_counts.recursive_item_count {
                Some(value) => Ok::<_, AppError>(Some(value)),
                None => count_recursive_children(pool, item.id).await.map(Some),
            }
        } else {
            Ok(None)
        }
    };
    let season_count_fut = async {
        if is_series {
            match prefetched_counts.season_count {
                Some(value) => Ok::<_, AppError>(Some(value)),
                None => count_series_seasons(pool, item.id).await.map(Some),
            }
        } else {
            Ok(None)
        }
    };
    let unplayed_count_fut = async {
        if is_folder {
            if let Some(uid) = user_id {
                let map = count_unplayed_children_batch(pool, uid, &[item.id]).await?;
                Ok::<_, AppError>(map.get(&item.id).copied())
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    };
    let series_and_season_fut = resolve_series_and_season_ids(pool, item);
    let parent_item_fut = async {
        match item.parent_id {
            Some(parent_id) => get_media_item(pool, parent_id).await,
            None => Ok::<_, AppError>(None),
        }
    };
    let people_fut = get_item_people(pool, item.id);
    let metadata_preferences_fut = metadata_preferences_from_settings(pool);

    let (
        media_sources,
        child_count,
        recursive_item_count,
        season_count,
        unplayed_count,
        (series_id, season_id),
        parent_item,
        people,
        metadata_preferences,
    ) = tokio::try_join!(
        media_sources_fut,
        child_count_fut,
        recursive_item_count_fut,
        season_count_fut,
        unplayed_count_fut,
        series_and_season_fut,
        parent_item_fut,
        people_fut,
        metadata_preferences_fut,
    )?;

    let media_streams = media_sources
        .iter()
        .find(|source| source.source_type == "Default")
        .or_else(|| media_sources.first())
        .map(|source| source.media_streams.clone())
        .unwrap_or_default();
    let average_frame_rate = media_streams
        .iter()
        .find(|stream| stream.stream_type == "Video")
        .and_then(|stream| stream.average_frame_rate);
    let real_frame_rate = media_streams
        .iter()
        .find(|stream| stream.stream_type == "Video")
        .and_then(|stream| stream.real_frame_rate);
    let chapters = media_sources
        .iter()
        .find(|source| source.source_type == "Default")
        .or_else(|| media_sources.first())
        .map(|source| source.chapters.clone())
        .unwrap_or_default();
    let video_stream = media_streams
        .iter()
        .find(|stream| stream.stream_type.eq_ignore_ascii_case("Video"));
    let width = item
        .width
        .or_else(|| video_stream.and_then(|stream| stream.width));
    let height = item
        .height
        .or_else(|| video_stream.and_then(|stream| stream.height));
    let primary_image_aspect_ratio = infer_primary_image_aspect_ratio(item, width, height);
    if is_folder {
        if let Some(unplayed) = unplayed_count {
            user_data.unplayed_item_count = Some(unplayed as i32);
        }
    }

    let item_detail_media_sources = sanitize_media_sources_for_item_detail(media_sources);
    let provider_ids = provider_ids_for_item(item);
    let primary_image_tag = item
        .image_primary_path
        .as_ref()
        .map(|_| item.date_modified.timestamp().to_string());
    let series_item = if let Some(series_id_value) = &series_id {
        if let Ok(series_uuid) = emby_id_to_uuid(series_id_value) {
            get_media_item(pool, series_uuid).await?
        } else {
            None
        }
    } else {
        None
    };
    let inherited_logo_source = parent_item
        .as_ref()
        .filter(|parent| parent.logo_path.is_some())
        .or_else(|| {
            series_item
                .as_ref()
                .filter(|series| series.logo_path.is_some())
        });
    let inherited_backdrop_source = parent_item
        .as_ref()
        .filter(|parent| parent.backdrop_path.is_some())
        .or_else(|| {
            series_item
                .as_ref()
                .filter(|series| series.backdrop_path.is_some())
        });
    let inherited_thumb_source = parent_item
        .as_ref()
        .filter(|parent| parent.thumb_path.is_some())
        .or_else(|| {
            series_item
                .as_ref()
                .filter(|series| series.thumb_path.is_some())
        });
    let parent_logo_item_id = inherited_logo_source.map(|parent| uuid_to_emby_guid(&parent.id));
    let parent_logo_image_tag = inherited_logo_source.and_then(|parent| {
        parent
            .logo_path
            .as_ref()
            .map(|_| parent.date_modified.timestamp().to_string())
    });
    let parent_backdrop_item_id =
        inherited_backdrop_source.map(|parent| uuid_to_emby_guid(&parent.id));
    let parent_backdrop_image_tags = inherited_backdrop_source
        .map(|parent| {
            let tag = parent.date_modified.timestamp().to_string();
            let mut tags = Vec::new();
            if parent.backdrop_path.is_some() {
                tags.push(tag.clone());
            }
            for _ in &parent.backdrop_paths {
                tags.push(tag.clone());
            }
            tags
        })
        .unwrap_or_default();
    let parent_thumb_item_id = inherited_thumb_source.map(|parent| uuid_to_emby_guid(&parent.id));
    let parent_thumb_image_tag = inherited_thumb_source.and_then(|parent| {
        parent
            .thumb_path
            .as_ref()
            .map(|_| parent.date_modified.timestamp().to_string())
    });
    let series_primary_image_tag = series_item.as_ref().and_then(|series_item| {
        series_item
            .image_primary_path
            .as_ref()
            .map(|_| series_item.date_modified.timestamp().to_string())
    });
    let genre_items = genre_items_from_names(&item.genres);
    // PB30：people / metadata_preferences 已经由上方 try_join! 一并并发取回，这里直接使用。
    let extra_fields = emby_extra_fields(
        item,
        &people,
        parent_item.as_ref(),
        metadata_preferences.as_ref(),
    );
    let resolved_series_name = item
        .series_name
        .clone()
        .or_else(|| series_item.as_ref().map(|series| series.name.clone()));
    let resolved_season_name = item.season_name.clone().or_else(|| {
        parent_item
            .as_ref()
            .filter(|parent| parent.item_type.eq_ignore_ascii_case("Season"))
            .map(|parent| parent.name.clone())
    });
    let completion_percentage = if is_folder {
        user_data.played_percentage
    } else {
        match (item.runtime_ticks, user_data.playback_position_ticks) {
            (Some(runtime_ticks), position) if runtime_ticks > 0 && position > 0 => {
                Some(((position as f64 / runtime_ticks as f64) * 100.0).min(100.0))
            }
            _ => user_data.played_percentage,
        }
    };

    Ok(BaseItemDto {
        name: item.name.clone(),
        original_title: item.original_title.clone(),
        server_id: uuid_to_emby_guid(&server_id),
        id: uuid_to_emby_guid(&item.id),
        guid: Some(uuid_to_emby_guid(&item.id)),
        etag: Some(item_etag(item)),
        date_modified: Some(item.date_modified),
        can_delete: Some(true),
        can_download: !is_folder,
        can_edit_items: Some(true),
        supports_resume: Some(!is_folder),
        presentation_unique_key: Some(presentation_unique_key(item, &provider_ids)),
        supports_sync: Some(true),
        item_type: item.item_type.clone(),
        is_folder,
        sort_name: Some(item.sort_name.clone()),
        forced_sort_name: Some(item.sort_name.clone()),
        primary_image_tag: primary_image_tag.clone(),
        collection_type: None,
        media_type: (!is_folder).then(|| item.media_type.clone()),
        container: effective_container(item),
        parent_id: item.parent_id.map(|value| uuid_to_emby_guid(&value)),
        path: sanitize_item_path(item),
        location_type: Some(
            if item.path.starts_with("http://") || item.path.starts_with("https://") {
                "Remote".to_string()
            } else {
                "FileSystem".to_string()
            },
        ),
        run_time_ticks: item.runtime_ticks,
        production_year: item.production_year,
        overview: item.overview.clone(),
        date_created: Some(item.date_created),
        premiere_date: premiere_date_to_utc(item.premiere_date),
        video_codec: item.video_codec.clone(),
        audio_codec: item.audio_codec.clone(),
        average_frame_rate,
        real_frame_rate,
        genres: item.genres.clone(),
        genre_items,
        provider_ids: provider_ids.clone(),
        external_urls: external_urls_from_provider_map(&provider_ids, &item.item_type),
        production_locations: item.production_locations.clone(),
        size: Some(item_size(item, is_folder)),
        file_name: item_file_name(item),
        bitrate: item.bit_rate,
        official_rating: item.official_rating.clone(),
        community_rating: item.community_rating,
        critic_rating: item.critic_rating,
        taglines: item.taglines.clone(),
        remote_trailers: remote_trailers_from_urls(&item.remote_trailers),
        people,
        studios: name_long_id_items_from_names(&item.studios),
        tag_items: name_long_id_items_from_names(&item.tags),
        local_trailer_count: Some(0),
        display_preferences_id: Some(display_preferences_id(item, &provider_ids)),
        playlist_item_id: None,
        recursive_item_count,
        season_count,
        series_count: None,
        movie_count: None,
        status: item.status.clone(),
        air_days: item.air_days.clone(),
        air_time: item.air_time.clone(),
        end_date: premiere_date_to_utc(item.end_date),
        width,
        height,
        is_movie: Some(item.item_type.eq_ignore_ascii_case("Movie")),
        is_series: Some(item.item_type.eq_ignore_ascii_case("Series")),
        is_live: Some(false),
        is_news: Some(false),
        is_kids: Some(false),
        is_sports: Some(false),
        is_premiere: Some(false),
        is_new: Some(false),
        is_repeat: Some(false),
        disabled: Some(false),
        series_name: resolved_series_name,
        series_id,
        season_name: resolved_season_name,
        season_id,
        index_number: item.index_number,
        index_number_end: item.index_number_end,
        parent_index_number: item.parent_index_number,
        thumb_image_tag: image_tags.get("Thumb").cloned(),
        image_tags,
        image_blur_hashes: parse_blur_hashes(&item.image_blur_hashes),
        backdrop_image_tags,
        parent_logo_item_id,
        parent_logo_image_tag,
        parent_backdrop_item_id,
        parent_backdrop_image_tags,
        parent_thumb_item_id,
        parent_thumb_image_tag,
        series_primary_image_tag,
        primary_image_item_id: primary_image_tag
            .as_ref()
            .map(|_| uuid_to_emby_guid(&item.id)),
        series_studio: item.studios.first().cloned(),
        user_data,
        media_sources: item_detail_media_sources,
        media_streams,
        part_count: Some(if is_folder { 0 } else { 1 }),
        chapters,
        locked_fields: item.locked_fields.clone(),
        lock_data: if item.lock_data { Some(true) } else { Some(false) },
        special_feature_count: Some(0),
        child_count,
        display_order: item.display_order.clone(),
        primary_image_aspect_ratio,
        completion_percentage,
        tags: item.tags.clone(),
        extra_fields,
    })
}

fn sanitize_media_sources_for_item_detail(
    media_sources: Vec<MediaSourceDto>,
) -> Vec<MediaSourceDto> {
    media_sources
        .into_iter()
        .map(|mut source| {
            source.direct_stream_url = None;
            source.transcoding_url = None;
            source.transcoding_sub_protocol = None;
            source.transcoding_container = None;
            source.required_http_headers = BTreeMap::new();
            source.add_api_key_to_direct_stream_url = Some(false);
            source
        })
        .collect()
}

fn emby_extra_fields(
    item: &DbMediaItem,
    people: &[PersonDto],
    parent_item: Option<&DbMediaItem>,
    metadata_preferences: Option<&StartupConfiguration>,
) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();

    if let Some(preferences) = metadata_preferences {
        if !preferences.preferred_metadata_language.trim().is_empty() {
            fields.insert(
                "PreferredMetadataLanguage".to_string(),
                json!(preferences.preferred_metadata_language),
            );
        }
        if !preferences.metadata_country_code.trim().is_empty() {
            fields.insert(
                "PreferredMetadataCountryCode".to_string(),
                json!(preferences.metadata_country_code),
            );
        }
    }

    if let Some(index) = item.index_number {
        fields.insert("SortIndexNumber".to_string(), json!(index));
    }
    if let Some(index) = item.parent_index_number {
        fields.insert("SortParentIndexNumber".to_string(), json!(index));
    }
    if let Some(rating) = item
        .official_rating
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        fields.insert("CustomRating".to_string(), json!(rating));
    }
    if let Some(video_3d_format) = item.tags.iter().find_map(|tag| {
        let normalized = tag.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "3d" | "sbs" | "hsbs" => Some("HalfSideBySide"),
            "tab" | "htab" => Some("HalfTopAndBottom"),
            _ => None,
        }
    }) {
        fields.insert("Video3DFormat".to_string(), json!(video_3d_format));
    }

    let artists = people
        .iter()
        .filter(|person| {
            person.person_type.as_deref().is_some_and(|kind| {
                matches!(
                    kind.to_ascii_lowercase().as_str(),
                    "artist" | "albumartist" | "musicartist"
                )
            })
        })
        .collect::<Vec<_>>();
    if !artists.is_empty() {
        fields.insert(
            "Artists".to_string(),
            json!(artists
                .iter()
                .map(|person| person.name.clone())
                .collect::<Vec<_>>()),
        );
        fields.insert(
            "ArtistItems".to_string(),
            json!(artists
                .iter()
                .map(|person| json!({ "Name": person.name, "Id": person.id }))
                .collect::<Vec<_>>()),
        );
    }

    let composers = people
        .iter()
        .filter(|person| {
            person
                .person_type
                .as_deref()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("composer"))
        })
        .collect::<Vec<_>>();
    if !composers.is_empty() {
        fields.insert(
            "Composers".to_string(),
            json!(composers
                .iter()
                .map(|person| json!({ "Name": person.name, "Id": person.id }))
                .collect::<Vec<_>>()),
        );
    }

    if let Some(album) = parent_item.filter(|parent| {
        matches!(
            parent.item_type.to_ascii_lowercase().as_str(),
            "album" | "musicalbum"
        )
    }) {
        fields.insert("Album".to_string(), json!(album.name));
        fields.insert("AlbumId".to_string(), json!(uuid_to_emby_guid(&album.id)));
        if album.image_primary_path.is_some() {
            fields.insert(
                "AlbumPrimaryImageTag".to_string(),
                json!(album.date_modified.timestamp().to_string()),
            );
        }
        let album_artists = artists
            .iter()
            .filter(|person| {
                person
                    .person_type
                    .as_deref()
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("AlbumArtist"))
            })
            .collect::<Vec<_>>();
        if let Some(first_album_artist) = album_artists.first() {
            fields.insert("AlbumArtist".to_string(), json!(first_album_artist.name));
        }
        if !album_artists.is_empty() {
            fields.insert(
                "AlbumArtists".to_string(),
                json!(album_artists
                    .iter()
                    .map(|person| json!({ "Name": person.name, "Id": person.id }))
                    .collect::<Vec<_>>()),
            );
        }
    }

    fields.insert("CanMakePublic".to_string(), json!(false));
    fields.insert("CanManageAccess".to_string(), json!(false));
    fields.insert("CanLeaveContent".to_string(), json!(false));
    fields
}

async fn metadata_preferences_from_settings(
    pool: &sqlx::PgPool,
) -> Result<Option<StartupConfiguration>, AppError> {
    let Some(value) = crate::repo_cache::cached_system_setting(pool, "startup_configuration").await? else {
        return Ok(None);
    };
    Ok(serde_json::from_value::<StartupConfiguration>(value).ok())
}

async fn resolve_series_and_season_ids(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
) -> Result<(Option<String>, Option<String>), AppError> {
    match item.item_type.as_str() {
        "Season" => Ok((item.parent_id.map(|id| uuid_to_emby_guid(&id)), None)),
        "Episode" => {
            let season_id = item.parent_id.map(|id| uuid_to_emby_guid(&id));
            let Some(parent_id) = item.parent_id else {
                return Ok((None, season_id));
            };

            let series_id = match get_media_item(pool, parent_id).await? {
                Some(parent) if parent.item_type == "Season" => {
                    parent.parent_id.map(|id| uuid_to_emby_guid(&id))
                }
                Some(parent) if parent.item_type == "Series" => Some(uuid_to_emby_guid(&parent.id)),
                _ => None,
            };

            Ok((series_id, season_id))
        }
        _ => Ok((None, None)),
    }
}

async fn get_season_id_by_number(
    pool: &sqlx::PgPool,
    series_id: Uuid,
    season_number: i32,
) -> Result<Option<String>, AppError> {
    let season_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        SELECT id
        FROM media_items
        WHERE parent_id = $1
          AND item_type = 'Season'
          AND index_number = $2
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .bind(season_number)
    .fetch_optional(pool)
    .await?;

    Ok(season_id.map(|value| uuid_to_emby_guid(&value)))
}

async fn missing_episode_row_to_dto<T>(
    pool: &sqlx::PgPool,
    row: T,
    _user_id: Uuid,
    server_id: Uuid,
) -> Result<BaseItemDto, AppError>
where
    T: Into<MissingEpisodeDtoSource>,
{
    let row = row.into();
    let virtual_id = row.id;
    let providers = provider_ids_to_map(&row.series_provider_ids);
    let mut item_provider_ids = providers.clone();
    item_provider_ids.insert("Tmdb".to_string(), row.external_series_id.clone());
    if let Some(value) = &row.external_episode_id {
        item_provider_ids.insert("TmdbEpisode".to_string(), value.clone());
    }
    if let Some(value) = &row.external_season_id {
        item_provider_ids.insert("TmdbSeason".to_string(), value.clone());
    }
    let mut image_tags = BTreeMap::new();
    if row.image_path.is_some() {
        image_tags.insert(
            "Primary".to_string(),
            row.series_date_modified.timestamp().to_string(),
        );
    }
    let backdrop_image_tags = row
        .series_backdrop_path
        .as_ref()
        .map(|_| vec![row.series_date_modified.timestamp().to_string()])
        .unwrap_or_default();
    let season_id = get_season_id_by_number(pool, row.series_id, row.season_number).await?;
    let overview = row.overview.clone().or(row.series_overview.clone());
    let people = get_item_people(pool, row.series_id).await?;
    let parent_backdrop_item_id = Some(uuid_to_emby_guid(&row.series_id));
    let parent_backdrop_image_tags = row
        .series_backdrop_path
        .as_ref()
        .map(|_| vec![row.series_date_modified.timestamp().to_string()])
        .unwrap_or_default();
    let parent_thumb_item_id = Some(uuid_to_emby_guid(&row.series_id));
    let parent_thumb_image_tag = row
        .series_thumb_path
        .as_ref()
        .map(|_| row.series_date_modified.timestamp().to_string());
    let parent_logo_item_id = Some(uuid_to_emby_guid(&row.series_id));
    let parent_logo_image_tag = row
        .series_logo_path
        .as_ref()
        .map(|_| row.series_date_modified.timestamp().to_string());

    Ok(BaseItemDto {
        name: row.name,
        original_title: None,
        server_id: uuid_to_emby_guid(&server_id),
        id: uuid_to_emby_guid(&virtual_id),
        guid: None,
        etag: Some(row.series_date_modified.timestamp().to_string()),
        date_modified: Some(row.series_date_modified),
        can_delete: Some(false),
        can_download: false,
        can_edit_items: Some(false),
        supports_resume: Some(false),
        presentation_unique_key: Some(format!("{}_missing", uuid_to_emby_guid(&virtual_id))),
        supports_sync: Some(true),
        item_type: "Episode".to_string(),
        is_folder: false,
        sort_name: Some(format!(
            "{:04}-{:04}-{}",
            row.season_number, row.episode_number, row.series_sort_name
        )),
        forced_sort_name: None,
        primary_image_tag: image_tags.get("Primary").cloned(),
        collection_type: None,
        media_type: Some("Video".to_string()),
        container: None,
        parent_id: season_id.clone(),
        path: None,
        location_type: Some("Virtual".to_string()),
        run_time_ticks: None,
        production_year: row
            .premiere_date
            .map(|date| date.year())
            .or(row.series_production_year),
        overview,
        date_created: None,
        premiere_date: premiere_date_to_utc(row.premiere_date),
        video_codec: None,
        audio_codec: None,
        average_frame_rate: None,
        real_frame_rate: None,
        genres: Vec::new(),
        genre_items: Vec::new(),
        provider_ids: item_provider_ids.clone(),
        external_urls: external_urls_from_provider_map(&item_provider_ids, "Series"),
        production_locations: Vec::new(),
        size: None,
        file_name: None,
        bitrate: None,
        official_rating: None,
        community_rating: None,
        critic_rating: None,
        taglines: Vec::new(),
        remote_trailers: Vec::new(),
        people,
        studios: Vec::new(),
        tag_items: Vec::new(),
        local_trailer_count: Some(0),
        display_preferences_id: Some(uuid_to_emby_guid(&virtual_id)),
        playlist_item_id: None,
        recursive_item_count: None,
        season_count: None,
        series_count: None,
        movie_count: None,
        status: None,
        air_days: Vec::new(),
        air_time: None,
        end_date: None,
        width: None,
        height: None,
        is_movie: Some(false),
        is_series: Some(false),
        is_live: Some(false),
        is_news: Some(false),
        is_kids: Some(false),
        is_sports: Some(false),
        is_premiere: row
            .premiere_date
            .map(|date| date == Utc::now().date_naive()),
        is_new: None,
        is_repeat: Some(false),
        disabled: Some(false),
        series_name: Some(row.series_name),
        series_id: Some(uuid_to_emby_guid(&row.series_id)),
        season_name: Some(format!("Season {}", row.season_number)),
        season_id,
        index_number: Some(row.episode_number),
        index_number_end: row.episode_number_end,
        parent_index_number: Some(row.season_number),
        thumb_image_tag: image_tags.get("Thumb").cloned(),
        image_tags,
        image_blur_hashes: None,
        backdrop_image_tags,
        parent_logo_item_id,
        parent_logo_image_tag,
        parent_backdrop_item_id,
        parent_backdrop_image_tags,
        parent_thumb_item_id,
        parent_thumb_image_tag,
        series_primary_image_tag: row
            .series_image_primary_path
            .as_ref()
            .map(|_| row.series_date_modified.timestamp().to_string()),
        primary_image_item_id: Some(uuid_to_emby_guid(&row.series_id)),
        series_studio: None,
        user_data: empty_user_data_for_item(virtual_id),
        media_sources: Vec::new(),
        media_streams: Vec::new(),
        part_count: Some(0),
        chapters: Vec::new(),
        locked_fields: Vec::new(),
        lock_data: Some(false),
        special_feature_count: None,
        child_count: None,
        display_order: None,
        primary_image_aspect_ratio: None,
        completion_percentage: None,
        tags: Vec::new(),
        extra_fields: BTreeMap::new(),
    })
}

struct MissingEpisodeDtoSource {
    id: Uuid,
    series_id: Uuid,
    provider: String,
    external_series_id: String,
    external_season_id: Option<String>,
    external_episode_id: Option<String>,
    season_number: i32,
    episode_number: i32,
    episode_number_end: Option<i32>,
    name: String,
    overview: Option<String>,
    premiere_date: Option<NaiveDate>,
    image_path: Option<String>,
    series_name: String,
    series_sort_name: String,
    series_overview: Option<String>,
    series_production_year: Option<i32>,
    series_provider_ids: Value,
    series_image_primary_path: Option<String>,
    series_backdrop_path: Option<String>,
    series_logo_path: Option<String>,
    series_thumb_path: Option<String>,
    series_date_modified: DateTime<Utc>,
}

impl From<MissingEpisodeRow> for MissingEpisodeDtoSource {
    fn from(value: MissingEpisodeRow) -> Self {
        Self {
            id: value.id,
            series_id: value.series_id,
            provider: value.provider,
            external_series_id: value.external_series_id,
            external_season_id: value.external_season_id,
            external_episode_id: value.external_episode_id,
            season_number: value.season_number,
            episode_number: value.episode_number,
            episode_number_end: value.episode_number_end,
            name: value.name,
            overview: value.overview,
            premiere_date: value.premiere_date,
            image_path: value.image_path,
            series_name: value.series_name,
            series_sort_name: value.series_sort_name,
            series_overview: value.series_overview,
            series_production_year: value.series_production_year,
            series_provider_ids: value.series_provider_ids,
            series_image_primary_path: value.series_image_primary_path,
            series_backdrop_path: value.series_backdrop_path,
            series_logo_path: value.series_logo_path,
            series_thumb_path: value.series_thumb_path,
            series_date_modified: value.series_date_modified,
        }
    }
}

impl From<MissingEpisodeDetailRow> for MissingEpisodeDtoSource {
    fn from(value: MissingEpisodeDetailRow) -> Self {
        Self {
            id: value.id,
            series_id: value.series_id,
            provider: value.provider,
            external_series_id: value.external_series_id,
            external_season_id: value.external_season_id,
            external_episode_id: value.external_episode_id,
            season_number: value.season_number,
            episode_number: value.episode_number,
            episode_number_end: value.episode_number_end,
            name: value.name,
            overview: value.overview,
            premiere_date: value.premiere_date,
            image_path: value.image_path,
            series_name: value.series_name,
            series_sort_name: value.series_sort_name,
            series_overview: value.series_overview,
            series_production_year: value.series_production_year,
            series_provider_ids: value.series_provider_ids,
            series_image_primary_path: value.series_image_primary_path,
            series_backdrop_path: value.series_backdrop_path,
            series_logo_path: value.series_logo_path,
            series_thumb_path: value.series_thumb_path,
            series_date_modified: value.series_date_modified,
        }
    }
}

pub fn media_source_for_item(item: &DbMediaItem) -> MediaSourceDto {
    let local_path = Path::new(&item.path);
    // 兼容旧虚拟路径数据：未触发再同步前 DB 中可能仍有 `REMOTE_EMBY/...` 行
    let normalized_path = item.path.replace('\\', "/");
    let legacy_virtual_remote = normalized_path
        .to_ascii_uppercase()
        .starts_with("REMOTE_EMBY/");
    let strm_target = naming::is_strm(local_path)
        .then(|| naming::read_strm_target(local_path))
        .flatten();
    let container = effective_container_from_target(item, strm_target.as_deref());
    let provider_remote = item
        .provider_ids
        .get("RemoteEmbySourceId")
        .and_then(serde_json::Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let is_remote = strm_target.is_some() || legacy_virtual_remote || provider_remote;
    let size = media_source_size(item, is_remote);
    let item_emby_id = uuid_to_emby_guid(&item.id);
    let media_source_id = format!("mediasource_{item_emby_id}");

    let sanitized_path = if is_remote {
        format!(
            "/Videos/{}/stream.{}?Static=true&MediaSourceId={}",
            item_emby_id, container, media_source_id
        )
    } else {
        item.path.clone()
    };

    MediaSourceDto {
        chapters: Vec::new(),
        id: media_source_id.clone(),
        path: sanitized_path,
        protocol: if is_remote { "Http" } else { "File" }.to_string(),
        source_type: "Default".to_string(),
        container: container.clone(),
        name: media_source_name(item, strm_target.as_deref()),
        is_remote,
        sort_name: None,
        encoder_path: None,
        encoder_protocol: None,
        probe_path: None,
        probe_protocol: None,
        has_mixed_protocols: Some(false),
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: true,
        direct_stream_url: Some(format!(
            "/Videos/{}/stream.{}?Static=true&MediaSourceId={}&mediaSourceId={}",
            item_emby_id, container, media_source_id, media_source_id
        )),
        formats: vec![container.clone()],
        size,
        e_tag: Some(item.date_modified.timestamp().to_string()),
        bitrate: item.bit_rate,
        default_audio_stream_index: None,
        default_subtitle_stream_index: None,
        run_time_ticks: item.runtime_ticks,
        container_start_time_ticks: None,
        is_infinite_stream: Some(false),
        requires_opening: Some(false),
        open_token: None,
        requires_closing: Some(false),
        live_stream_id: None,
        buffer_ms: None,
        requires_looping: Some(false),
        supports_probing: Some(true),
        video_type: Some("VideoFile".to_string()),
        iso_type: None,
        video_3d_format: None,
        timestamp: infer_timestamp(&container),
        ignore_dts: false,
        ignore_index: false,
        gen_pts_input: false,
        required_http_headers: BTreeMap::new(),
        add_api_key_to_direct_stream_url: Some(false),
        transcoding_url: None,
        transcoding_sub_protocol: None,
        transcoding_container: None,
        analyze_duration_ms: None,
        read_at_native_framerate: Some(false),
        item_id: Some(item_emby_id),
        server_id: None,
        media_streams: Vec::new(),
        media_attachments: Vec::new(),
    }
}

async fn media_source_for_item_with_type(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    source_type: &str,
    server_id: Uuid,
) -> Result<MediaSourceDto, AppError> {
    let mut source = get_media_source_with_streams(pool, item, server_id).await?;
    source.source_type = source_type.to_string();
    source.server_id = None;
    Ok(source)
}

pub async fn media_sources_for_item(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    server_id: Uuid,
) -> Result<Vec<MediaSourceDto>, AppError> {
    let mut sources =
        vec![media_source_for_item_with_type(pool, item, "Default", server_id).await?];
    let grouped_sources = version_group_items_for_item(pool, item).await?;
    if grouped_sources.len() <= 1 {
        return Ok(sources);
    }
    let mut hydrated_sources = Vec::new();
    for source in &grouped_sources {
        let source_type = if source.id == item.id {
            "Default"
        } else {
            "Grouping"
        };
        hydrated_sources
            .push(media_source_for_item_with_type(pool, source, source_type, server_id).await?);
    }
    sources = hydrated_sources;

    Ok(sources)
}

async fn version_group_items_for_item(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
) -> Result<Vec<DbMediaItem>, AppError> {
    if !matches!(item.item_type.as_str(), "Movie" | "Series" | "Episode") {
        return Ok(vec![item.clone()]);
    }

    let providers = provider_ids_for_item(item);
    let tmdb = providers
        .get("Tmdb")
        .or_else(|| providers.get("TMDb"))
        .or_else(|| providers.get("tmdb"))
        .cloned();
    let imdb = providers
        .get("Imdb")
        .or_else(|| providers.get("IMDb"))
        .or_else(|| providers.get("imdb"))
        .cloned();
    let tmdb_path_token = tmdb.as_ref().map(|value| format!("%{{tmdbid={value}}}%"));
    let imdb_path_token = imdb.as_ref().map(|value| format!("%{{imdbid={value}}}%"));

    let mut grouped_items = if item.item_type == "Episode" {
        sqlx::query_as::<_, DbMediaItem>(
            r#"
            SELECT
                id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
                overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
                premiere_date, status, end_date, air_days, air_time, series_name, season_name,
                index_number, index_number_end, parent_index_number, provider_ids, genres,
                studios, tags, production_locations,
                width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
                logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
                date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
                display_order
            FROM media_items
            WHERE item_type = 'Episode'
              AND media_type = $1
              AND parent_index_number = $2
              AND index_number = $3
              AND COALESCE(index_number_end, -1) = COALESCE($4, -1)
              AND (
                  (
                      $5::text IS NOT NULL AND (
                          provider_ids->>'Tmdb' = $5 OR provider_ids->>'TMDb' = $5 OR provider_ids->>'tmdb' = $5
                          OR path ILIKE $7
                      )
                  )
                  OR
                  (
                      $6::text IS NOT NULL AND (
                          provider_ids->>'Imdb' = $6 OR provider_ids->>'IMDb' = $6 OR provider_ids->>'imdb' = $6
                          OR path ILIKE $8
                      )
                  )
                  OR
                  (
                      ($5::text IS NULL AND $6::text IS NULL)
                      AND lower(COALESCE(series_name, '')) = lower(COALESCE($9, ''))
                  )
              )
            ORDER BY date_created ASC
            LIMIT 20
            "#,
        )
        .bind(&item.media_type)
        .bind(item.parent_index_number)
        .bind(item.index_number)
        .bind(item.index_number_end)
        .bind(tmdb.as_deref())
        .bind(imdb.as_deref())
        .bind(tmdb_path_token.as_deref())
        .bind(imdb_path_token.as_deref())
        .bind(item.series_name.as_deref())
        .fetch_all(pool)
        .await?
    } else {
        if tmdb.is_none() && imdb.is_none() {
            return Ok(vec![item.clone()]);
        }

        sqlx::query_as::<_, DbMediaItem>(
            r#"
            SELECT
                id, parent_id, name, original_title, sort_name, item_type, media_type, path, container,
                overview, production_year, official_rating, community_rating, critic_rating, runtime_ticks,
                premiere_date, status, end_date, air_days, air_time, series_name, season_name,
                index_number, index_number_end, parent_index_number, provider_ids, genres,
                studios, tags, production_locations,
                width, height, bit_rate, size, video_codec, audio_codec, image_primary_path, backdrop_path,
                logo_path, thumb_path, art_path, banner_path, disc_path, backdrop_paths, remote_trailers,
                date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data,
                display_order
            FROM media_items
            WHERE item_type = $1
              AND media_type = $2
              AND (
                  ($3::text IS NOT NULL AND (
                      provider_ids->>'Tmdb' = $3 OR provider_ids->>'TMDb' = $3 OR provider_ids->>'tmdb' = $3
                      OR path ILIKE $5
                  ))
                  OR
                  ($4::text IS NOT NULL AND (
                      provider_ids->>'Imdb' = $4 OR provider_ids->>'IMDb' = $4 OR provider_ids->>'imdb' = $4
                      OR path ILIKE $6
                  ))
              )
            ORDER BY date_created ASC
            LIMIT 20
            "#,
        )
        .bind(&item.item_type)
        .bind(&item.media_type)
        .bind(tmdb.as_deref())
        .bind(imdb.as_deref())
        .bind(tmdb_path_token.as_deref())
        .bind(imdb_path_token.as_deref())
        .fetch_all(pool)
        .await?
    };

    if !grouped_items
        .iter()
        .any(|candidate| candidate.id == item.id)
    {
        grouped_items.push(item.clone());
    }
    grouped_items.sort_by_key(|candidate| candidate.date_created);
    grouped_items.dedup_by_key(|candidate| candidate.id);
    Ok(grouped_items)
}

fn lowercase_list(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn parse_option_filters(value: Option<&str>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split([',', '|', ';'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .collect()
}

pub async fn get_additional_parts_for_item(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    user_id: Uuid,
    server_id: Uuid,
    start_index: i64,
    limit: i64,
) -> Result<QueryResult<BaseItemDto>, AppError> {
    let item = get_media_item(pool, item_id)
        .await?
        .ok_or_else(|| AppError::NotFound("媒体条目不存在".to_string()))?;
    let mut grouped_items = version_group_items_for_item(pool, &item).await?;
    grouped_items.retain(|candidate| candidate.id != item.id);

    let total_record_count = grouped_items.len() as i64;
    let start = start_index.max(0) as usize;
    let take = limit.clamp(1, 200) as usize;
    let page_items: Vec<_> = grouped_items.into_iter().skip(start).take(take).collect();

    let row_ids: Vec<Uuid> = page_items.iter().map(|r| r.id).collect();
    let user_data_map = get_user_item_data_batch(pool, user_id, &row_ids).await?;
    let items: Vec<BaseItemDto> = page_items
        .iter()
        .map(|item| {
            let prefetched = Some(user_data_map.get(&item.id).cloned());
            media_item_to_dto_for_list(item, server_id, prefetched, DtoCountPrefetch::default())
        })
        .collect();

    Ok(QueryResult {
        items,
        total_record_count,
        start_index: Some(start_index.max(0)),
    })
}

pub async fn get_media_sources_for_item(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    server_id: Uuid,
) -> Result<Vec<MediaSourceDto>, AppError> {
    media_sources_for_item(pool, item, server_id).await
}
pub async fn subtitle_path_for_stream_index(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    stream_index: i32,
) -> Option<PathBuf> {
    let max_db_index: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(index), 0) FROM media_streams WHERE media_item_id = $1",
    )
    .bind(item.id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let sidecar_start_index = max_db_index + 1;
    let sidecars = naming::sidecar_subtitles(Path::new(&item.path));
    let offset = stream_index - sidecar_start_index;
    if offset >= 0 && (offset as usize) < sidecars.len() {
        Some(sidecars[offset as usize].path.clone())
    } else {
        None
    }
}

/// 对于 DB 中 is_external=true 的字幕流（index 范围在 DB 流内），
/// 查询其 is_external_url 字段（远端 Emby 的 DeliveryUrl）。
/// 返回远端相对 URL（如 /Videos/.../Subtitles/.../Stream.srt）。
pub async fn subtitle_external_url_for_stream_index(
    pool: &sqlx::PgPool,
    media_item_id: uuid::Uuid,
    stream_index: i32,
) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT is_external_url FROM media_streams
           WHERE media_item_id = $1 AND index = $2
             AND stream_type = 'subtitle' AND is_external = true
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .bind(stream_index)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
    .filter(|u| !u.trim().is_empty())
}

/// 根据已写入磁盘的 sidecar 字幕路径，反查它在 Emby `MediaStreams` 序列中的
/// 索引（视频路径同目录扫描，按完整 path 匹配）。供字幕下载完成后回填
/// `SubtitleDownloadResult.NewIndex` 使用。
pub async fn sidecar_subtitle_stream_index(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    sub_path: &Path,
) -> Option<i32> {
    let max_db_index: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(index), 0) FROM media_streams WHERE media_item_id = $1",
    )
    .bind(item.id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let sidecar_start_index = max_db_index + 1;
    let canonical = sub_path.to_string_lossy().to_string();
    naming::sidecar_subtitles(Path::new(&item.path))
        .into_iter()
        .enumerate()
        .find(|(_, s)| s.path.to_string_lossy() == canonical)
        .map(|(offset, _)| sidecar_start_index + offset as i32)
}


fn subtitle_mime_type(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "srt" | "subrip" => "application/x-subrip".to_string(),
        "ass" | "ssa" => "text/x-ssa".to_string(),
        "vtt" | "webvtt" => "text/vtt".to_string(),
        "sub" => "text/x-microdvd".to_string(),
        "smi" => "application/smil".to_string(),
        "ttml" => "application/ttml+xml".to_string(),
        "lrc" => "application/x-lyrics".to_string(),
        _ => "application/octet-stream".to_string(),
    }
}

fn infer_primary_image_aspect_ratio(
    item: &DbMediaItem,
    width: Option<i32>,
    height: Option<i32>,
) -> Option<f64> {
    match item.item_type.as_str() {
        "Movie" | "Series" | "Season" | "BoxSet" | "CollectionFolder" => {
            if item.image_primary_path.is_some() {
                Some(2.0 / 3.0)
            } else {
                None
            }
        }
        "Episode" => {
            if item.image_primary_path.is_some() || item.thumb_path.is_some() {
                Some(16.0 / 9.0)
            } else {
                None
            }
        }
        _ => match (width, height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => {
                Some(width as f64 / height as f64)
            }
            _ => None,
        },
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

fn sanitize_item_path(item: &DbMediaItem) -> Option<String> {
    let normalized = item.path.replace('\\', "/");
    let is_strm = naming::is_strm(Path::new(&item.path));
    // 兼容旧虚拟路径数据
    let legacy_virtual_remote = normalized.to_ascii_uppercase().starts_with("REMOTE_EMBY/");
    // 任何被远端 Emby 源标记的条目（包括 Series/Season）都做 desensitize，
    // 客户端不应看到本地落盘的物理路径，只看到 emby 风格的虚拟展示路径。
    let provider_remote = item
        .provider_ids
        .get("RemoteEmbySourceId")
        .and_then(serde_json::Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    if is_strm || legacy_virtual_remote || provider_remote {
        let container = effective_container(item);
        let name = &item.name;
        let year = item
            .production_year
            .map(|y| format!(" ({y})"))
            .unwrap_or_default();
        Some(format!(
            "/{name}{year}/{name}{year}.{}",
            container.unwrap_or_else(|| "mkv".to_string())
        ))
    } else {
        Some(item.path.clone())
    }
}

fn is_folder_item(item: &DbMediaItem) -> bool {
    matches!(
        item.item_type.as_str(),
        "AggregateFolder" | "BoxSet" | "CollectionFolder" | "Folder" | "Season" | "Series"
    )
}

/// 公开版本：列表接口预取阶段判断是否需要 Folder counts 时使用。
pub fn is_folder_item_public(item: &DbMediaItem) -> bool {
    is_folder_item(item)
}

pub fn provider_ids_to_map(value: &Value) -> BTreeMap<String, String> {
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

fn provider_ids_for_item(item: &DbMediaItem) -> BTreeMap<String, String> {
    let mut providers = provider_ids_to_map(&item.provider_ids);
    for (key, value) in provider_ids_from_local_nfo(item) {
        providers.entry(key).or_insert(value);
    }
    for (key, value) in provider_ids_from_path_text(&item.path) {
        providers.entry(key).or_insert(value);
    }
    providers
}

fn provider_ids_from_local_nfo(item: &DbMediaItem) -> BTreeMap<String, String> {
    let mut providers = BTreeMap::new();
    let media_path = Path::new(&item.path);

    for candidate in nfo_candidates_for_item(item, media_path) {
        let Ok(xml) = std::fs::read_to_string(&candidate) else {
            continue;
        };

        for (key, value) in provider_ids_from_nfo_text(&xml) {
            providers.entry(key).or_insert(value);
        }

        if !providers.is_empty() {
            break;
        }
    }

    providers
}

fn nfo_candidates_for_item(item: &DbMediaItem, media_path: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    match item.item_type.as_str() {
        "Movie" | "Trailer" | "Video" => {
            if let Some(parent) = media_path.parent() {
                if let Some(stem) = media_path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                {
                    candidates.push(parent.join(format!("{stem}.nfo")));
                }
                candidates.push(parent.join("movie.nfo"));
            }
        }
        "Series" => {
            candidates.push(media_path.join("tvshow.nfo"));
            candidates.push(media_path.join("series.nfo"));
            if let Some(parent) = media_path.parent() {
                candidates.push(parent.join("tvshow.nfo"));
                candidates.push(parent.join("series.nfo"));
            }
        }
        "Season" => {
            candidates.push(media_path.join("season.nfo"));
            candidates.push(media_path.join("tvshow.nfo"));
            candidates.push(media_path.join("series.nfo"));
            if let Some(parent) = media_path.parent() {
                candidates.push(parent.join("tvshow.nfo"));
                candidates.push(parent.join("series.nfo"));
            }
        }
        "Episode" => {
            if let Some(mut dir) = media_path.parent() {
                if let Some(stem) = media_path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                {
                    candidates.push(dir.join(format!("{stem}.nfo")));
                }
                candidates.push(dir.join("episodedetails.nfo"));
                candidates.push(dir.join("episode.nfo"));
                candidates.push(dir.join("season.nfo"));
                // 向上遍历每一级父目录的 tvshow.nfo（多季/子文件夹结构与虚拟路径不一致时仍需命中剧集根 NFO）。
                for _ in 0..48 {
                    candidates.push(dir.join("tvshow.nfo"));
                    candidates.push(dir.join("series.nfo"));
                    if let Some(p) = dir.parent() {
                        dir = p;
                    } else {
                        break;
                    }
                }
            }
        }
        _ => {
            if let Some(parent) = media_path.parent() {
                if let Some(stem) = media_path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                {
                    candidates.push(parent.join(format!("{stem}.nfo")));
                }
            }
        }
    }

    candidates
}

fn provider_ids_from_nfo_text(xml: &str) -> BTreeMap<String, String> {
    let mut providers = BTreeMap::new();

    if let Some(value) = first_tag_value(xml, &["imdbid"]) {
        providers.insert("Imdb".to_string(), value);
    }
    if let Some(value) = first_tag_value(xml, &["tmdbid"]) {
        providers.insert("Tmdb".to_string(), value);
    }
    if let Some(value) = first_tag_value(xml, &["tvdbid"]) {
        providers.insert("Tvdb".to_string(), value);
    }
    if let Some(value) = first_tag_value(xml, &["traktid"]) {
        providers.insert("Trakt".to_string(), value);
    }

    if let Ok(regex) = Regex::new(r#"(?is)<uniqueid\b([^>]*)>(.*?)</uniqueid>"#) {
        for captures in regex.captures_iter(xml) {
            let attrs = captures
                .get(1)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let raw_value = captures
                .get(2)
                .map(|value| value.as_str())
                .unwrap_or_default();
            let value = strip_xml_tags(raw_value).trim().to_string();
            if value.is_empty() {
                continue;
            }

            let provider = uniqueid_provider_name(attrs).unwrap_or_else(|| "Unknown".to_string());
            providers.entry(provider).or_insert(value);
        }
    }

    providers
}

fn first_tag_value(xml: &str, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        let pattern = format!(
            r"(?is)<{}\b[^>]*>(.*?)</{}>",
            regex::escape(name),
            regex::escape(name)
        );
        let regex = Regex::new(&pattern).ok()?;
        let capture = regex.captures(xml)?;
        let raw = capture.get(1)?.as_str();
        let value = strip_xml_tags(raw).trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn strip_xml_tags(value: &str) -> String {
    if let Ok(regex) = Regex::new(r"(?is)<[^>]+>") {
        regex.replace_all(value, "").to_string()
    } else {
        value.to_string()
    }
}

fn uniqueid_provider_name(attrs: &str) -> Option<String> {
    let regex = Regex::new(r#"(?i)\b(?:type|provider)\s*=\s*["']([^"']+)["']"#).ok()?;
    let raw = regex
        .captures(attrs)?
        .get(1)?
        .as_str()
        .trim()
        .to_ascii_lowercase();
    let provider = match raw.as_str() {
        "imdb" => "Imdb",
        "tmdb" | "themoviedb" => "Tmdb",
        "tvdb" | "thetvdb" => "Tvdb",
        "trakt" => "Trakt",
        other if !other.is_empty() => return Some(other.to_string()),
        _ => return None,
    };
    Some(provider.to_string())
}

fn provider_ids_from_path_text(path: &str) -> BTreeMap<String, String> {
    let mut providers = BTreeMap::new();
    let lower = path.to_lowercase();
    for (marker, provider_name) in [
        ("{tmdbid=", "Tmdb"),
        ("{imdbid=", "Imdb"),
        ("{tvdbid=", "Tvdb"),
        ("{traktid=", "Trakt"),
    ] {
        let Some(start) = lower.find(marker) else {
            continue;
        };
        let value_start = start + marker.len();
        let Some(relative_end) = path[value_start..].find('}') else {
            continue;
        };
        let value = path[value_start..value_start + relative_end].trim();
        if !value.is_empty() {
            providers.insert(provider_name.to_string(), value.to_string());
        }
    }
    providers
}

fn item_identity_scope(item: &DbMediaItem) -> &'static str {
    match item.item_type.as_str() {
        "Series" | "Season" | "Episode" => "series",
        "Movie" | "Trailer" | "Video" => "movie",
        "Audio" | "AudioBook" | "MusicAlbum" | "MusicArtist" => "audio",
        _ => "item",
    }
}

fn provider_value<'a>(providers: &'a BTreeMap<String, String>, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| providers.get(*key).map(String::as_str))
}

fn item_identity_key(item: &DbMediaItem, providers: &BTreeMap<String, String>) -> Option<String> {
    let scope = item_identity_scope(item);
    let base = provider_value(providers, &["Tmdb", "TMDb", "tmdb"])
        .map(|value| format!("{scope}:tmdb:{value}"))
        .or_else(|| {
            provider_value(providers, &["Imdb", "IMDb", "imdb"])
                .map(|value| format!("{scope}:imdb:{value}"))
        })
        .or_else(|| {
            provider_value(providers, &["Tvdb", "TVDb", "tvdb"])
                .map(|value| format!("{scope}:tvdb:{value}"))
        });
    let fallback = fallback_item_identity_base(item);
    let identity_base = base.or(fallback);

    match item.item_type.as_str() {
        "Season" => identity_base
            .map(|value| format!("{value}:season:{}", item.index_number.unwrap_or_default())),
        "Episode" => identity_base.map(|value| {
            if item.index_number.is_none() && item.index_number_end.is_none() {
                if let Some(date) = item.premiere_date {
                    return format!("{value}:aired:{date}");
                }
            }
            format!(
                "{value}:season:{}:episode:{}:{}",
                item.parent_index_number.unwrap_or_default(),
                item.index_number.unwrap_or_default(),
                item.index_number_end.unwrap_or_default()
            )
        }),
        _ => identity_base,
    }
}

fn fallback_item_identity_base(item: &DbMediaItem) -> Option<String> {
    let scope = item_identity_scope(item);
    let parent_folder_name = parent_folder_name(&item.path);
    let normalized_name = normalized_identity_text(
        item.series_name
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                if item.item_type == "Series" {
                    Some(item.name.as_str())
                } else {
                    None
                }
            })
            .or(parent_folder_name.as_deref()),
    )?;
    let year = item.production_year.unwrap_or_default();
    Some(format!("{scope}:name:{normalized_name}:year:{year}"))
}

fn normalized_identity_text(value: Option<&str>) -> Option<String> {
    value
        .map(naming::clean_display_name)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
}

fn parent_folder_name(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .and_then(Path::file_name)
        .and_then(|value| value.to_str())
        .map(str::to_string)
}

fn presentation_unique_key(item: &DbMediaItem, providers: &BTreeMap<String, String>) -> String {
    let source =
        item_identity_key(item, providers).unwrap_or_else(|| match item.item_type.as_str() {
            "Season" => format!(
                "item:{}:season:{}",
                item.id,
                item.index_number.unwrap_or_default()
            ),
            "Episode" => format!(
                "item:{}:season:{}:episode:{}:{}",
                item.id,
                item.parent_index_number.unwrap_or_default(),
                item.index_number.unwrap_or_default(),
                item.index_number_end.unwrap_or_default()
            ),
            _ => format!("item:{}", item.id),
        });
    format!(
        "{}_",
        uuid_to_emby_guid(&Uuid::new_v5(&Uuid::NAMESPACE_URL, source.as_bytes()))
    )
}

fn display_preferences_id(item: &DbMediaItem, providers: &BTreeMap<String, String>) -> String {
    let source =
        item_identity_key(item, providers).unwrap_or_else(|| match item.item_type.as_str() {
            "Season" => format!(
                "item:{}:season:{}",
                item.id,
                item.index_number.unwrap_or_default()
            ),
            "Episode" => format!(
                "item:{}:season:{}:episode:{}:{}",
                item.id,
                item.parent_index_number.unwrap_or_default(),
                item.index_number.unwrap_or_default(),
                item.index_number_end.unwrap_or_default()
            ),
            _ => format!("item:{}", item.id),
        });
    uuid_to_emby_guid(&Uuid::new_v5(&Uuid::NAMESPACE_URL, source.as_bytes()))
}

fn premiere_date_to_utc(value: Option<NaiveDate>) -> Option<DateTime<Utc>> {
    value.and_then(|date| {
        date.and_hms_opt(0, 0, 0)
            .map(|naive| DateTime::<Utc>::from_naive_utc_and_offset(naive, Utc))
    })
}

#[derive(Debug, FromRow)]
struct ItemPersonRow {
    id: Uuid,
    name: String,
    sort_name: Option<String>,
    overview: Option<String>,
    external_url: Option<String>,
    provider_ids: Value,
    role_type: String,
    role: Option<String>,
    primary_image_path: Option<String>,
    premiere_date: Option<DateTime<Utc>>,
    production_year: Option<i32>,
    favorite_count: i32,
    updated_at: DateTime<Utc>,
}

fn external_urls_from_provider_map(
    providers: &BTreeMap<String, String>,
    item_type: &str,
) -> Vec<ExternalUrlDto> {
    let mut urls = Vec::new();
    let is_series_family = matches!(item_type, "Series" | "Season" | "Episode");

    if let Some(imdb) = provider_value(providers, &["Imdb", "IMDb", "imdb"]) {
        urls.push(ExternalUrlDto {
            name: "IMDb".to_string(),
            url: format!("https://www.imdb.com/title/{imdb}"),
        });
    }

    if let Some(tmdb) = provider_value(providers, &["Tmdb", "TMDb", "tmdb"]) {
        let tmdb_path = if is_series_family { "tv" } else { "movie" };
        urls.push(ExternalUrlDto {
            name: "TheMovieDb".to_string(),
            url: format!("https://www.themoviedb.org/{tmdb_path}/{tmdb}"),
        });
        urls.push(ExternalUrlDto {
            name: "Trakt".to_string(),
            url: format!(
                "https://trakt.tv/search/tmdb/{tmdb}?id_type={}",
                if is_series_family { "show" } else { "movie" }
            ),
        });
    }

    if let Some(tvdb) = provider_value(providers, &["Tvdb", "TVDb", "tvdb"]) {
        urls.push(ExternalUrlDto {
            name: "TheTVDB".to_string(),
            url: format!("https://thetvdb.com/dereferrer/series/{tvdb}"),
        });
    }

    if let Some(trakt) = provider_value(providers, &["Trakt", "trakt"]) {
        urls.push(ExternalUrlDto {
            name: "Trakt".to_string(),
            url: format!("https://trakt.tv/search/trakt/{trakt}"),
        });
    }

    urls
}

fn genre_items_from_names(names: &[String]) -> Vec<NameLongIdDto> {
    name_long_id_items_from_names(names)
}

fn name_long_id_items_from_names(names: &[String]) -> Vec<NameLongIdDto> {
    names
        .iter()
        .filter(|name| !name.trim().is_empty())
        .map(|name| NameLongIdDto {
            name: name.clone(),
            id: stable_long_id_from_name(name),
        })
        .collect()
}

fn stable_long_id_from_name(name: &str) -> i64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let value = hasher.finish() & 0x7FFF_FFFF_FFFF_FFFF;
    i64::try_from(value).unwrap_or(i64::MAX)
}

async fn get_item_people(pool: &sqlx::PgPool, item_id: Uuid) -> Result<Vec<PersonDto>, AppError> {
    let rows = sqlx::query_as::<_, ItemPersonRow>(
        r#"
        SELECT
            p.id,
            p.name,
            p.sort_name,
            p.overview,
            p.external_url,
            p.provider_ids,
            pr.role_type,
            pr.role,
            p.primary_image_path,
            p.premiere_date,
            p.production_year,
            p.favorite_count,
            p.updated_at
        FROM person_roles pr
        INNER JOIN persons p ON p.id = pr.person_id
        WHERE pr.media_item_id = $1
        ORDER BY
            CASE pr.role_type
                WHEN 'Actor' THEN 0
                WHEN 'GuestStar' THEN 0
                WHEN 'Director' THEN 1
                WHEN 'Writer' THEN 2
                WHEN 'Producer' THEN 3
                ELSE 4
            END,
            pr.sort_order,
            p.name
        "#,
    )
    .bind(item_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let image_tags = row.primary_image_path.as_ref().map(|_| {
                let mut tags = std::collections::HashMap::new();
                tags.insert(
                    "Primary".to_string(),
                    row.updated_at.timestamp().to_string(),
                );
                tags
            });
            let provider_ids: Option<std::collections::HashMap<String, String>> =
                serde_json::from_value(row.provider_ids).ok();
            let external_url = row.external_url.or_else(|| {
                provider_ids.as_ref().and_then(|providers| {
                    providers
                        .get("Imdb")
                        .or_else(|| providers.get("IMDb"))
                        .or_else(|| providers.get("imdb"))
                        .map(|imdb| format!("https://www.imdb.com/name/{imdb}"))
                        .or_else(|| {
                            providers
                                .get("Tmdb")
                                .or_else(|| providers.get("TMDb"))
                                .or_else(|| providers.get("tmdb"))
                                .map(|tmdb| format!("https://www.themoviedb.org/person/{tmdb}"))
                        })
                })
            });

            PersonDto {
                name: row.name,
                id: uuid_to_emby_guid(&row.id),
                role: row.role,
                person_type: Some(row.role_type),
                primary_image_tag: row
                    .primary_image_path
                    .as_ref()
                    .map(|_| row.updated_at.timestamp().to_string()),
                sort_name: row.sort_name,
                overview: row.overview,
                external_url,
                premiere_date: row.premiere_date.map(|value| value.to_rfc3339()),
                end_date: None,
                production_year: row.production_year,
                production_locations: None,
                homepage_url: None,
                image_tags,
                provider_ids,
                favorite: Some(row.favorite_count > 0),
                backdrop_image_tag: None,
            }
        })
        .collect())
}

fn item_etag(item: &DbMediaItem) -> String {
    item.date_modified.timestamp().to_string()
}

fn item_size(item: &DbMediaItem, is_folder: bool) -> i64 {
    if is_folder {
        return 0;
    }

    if let Some(size) = item.size.filter(|&s| s > 0) {
        return size;
    }

    if naming::is_strm(Path::new(&item.path)) {
        return estimated_media_size(item).unwrap_or(0);
    }

    std::fs::metadata(&item.path)
        .ok()
        .and_then(|metadata| i64::try_from(metadata.len()).ok())
        .unwrap_or(0)
}

fn item_file_name(item: &DbMediaItem) -> Option<String> {
    Path::new(&item.path)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

fn estimated_media_size(item: &DbMediaItem) -> Option<i64> {
    let bitrate = item.bit_rate?;
    let runtime_ticks = item.runtime_ticks?;
    if bitrate <= 0 || runtime_ticks <= 0 {
        return None;
    }

    let seconds = runtime_ticks as f64 / 10_000_000_f64;
    Some(((bitrate as f64 * seconds) / 8_f64).round() as i64)
}

fn media_source_size(item: &DbMediaItem, is_remote: bool) -> Option<i64> {
    if let Some(size) = item.size.filter(|&s| s > 0) {
        return Some(size);
    }

    if is_remote {
        return estimated_media_size(item);
    }

    std::fs::metadata(&item.path)
        .ok()
        .and_then(|metadata| i64::try_from(metadata.len()).ok())
}

fn effective_container(item: &DbMediaItem) -> Option<String> {
    let local_path = Path::new(&item.path);
    let strm_target = naming::is_strm(local_path)
        .then(|| naming::read_strm_target(local_path))
        .flatten();
    Some(effective_container_from_target(
        item,
        strm_target.as_deref(),
    ))
}

fn effective_container_from_target(item: &DbMediaItem, strm_target: Option<&str>) -> String {
    let local_path = Path::new(&item.path);
    let raw = strm_target
        .and_then(naming::extension_from_url)
        .or_else(|| {
            item.container
                .clone()
                .filter(|container| !container.eq_ignore_ascii_case("strm"))
        })
        .or_else(|| {
            local_path
                .extension()
                .map(|ext| ext.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "mp4".to_string());
    first_container(&raw)
}

/// 把 CSV/管道/分号分隔的容器列表（如 `"mkv,mp4"`）规范化为单一容器；
/// 全空时回退 `mp4`。供 `effective_container_from_target` 与
/// `routes::items::build_direct_stream_url` 等共用，避免两边 fallback 不一致。
pub fn first_container(value: &str) -> String {
    let v = value
        .split([',', '|', ';'])
        .next()
        .unwrap_or("mp4")
        .trim()
        .trim_start_matches('.');
    if v.is_empty() {
        "mp4".to_string()
    } else {
        v.to_string()
    }
}

fn infer_timestamp(container: &str) -> Option<String> {
    match container.to_ascii_lowercase().as_str() {
        "ts" | "m2ts" | "mts" | "mpegts" | "mpeg" | "mpg" => Some("None".to_string()),
        _ => None,
    }
}

fn is_text_based_subtitle(codec: &str) -> bool {
    matches!(
        codec.to_ascii_lowercase().as_str(),
        "srt" | "subrip" | "vtt" | "webvtt" | "ass" | "ssa" | "smi" | "ttml" | "sub" | "lrc"
    )
}

fn media_source_name(item: &DbMediaItem, strm_target: Option<&str>) -> String {
    let local_stem = item_file_name(item)
        .and_then(|file_name| {
            Path::new(&file_name)
                .file_stem()
                .map(|stem| stem.to_string_lossy().to_string())
        })
        .map(|stem| decode_percent_encoded_component(stem.trim()));

    local_stem
        .as_deref()
        .and_then(|stem| {
            stem.rsplit_once(" - ")
                .map(|(_, quality)| quality.trim().to_string())
                .filter(|quality| !quality.is_empty())
        })
        .or(local_stem.filter(|stem| !stem.is_empty()))
        .or_else(|| strm_target.and_then(source_name_from_url))
        .unwrap_or_else(|| decode_percent_encoded_component(item.name.trim()))
}

fn source_name_from_url(value: &str) -> Option<String> {
    let url = url::Url::parse(value).ok()?;
    let file_name = Path::new(url.path()).file_stem()?.to_string_lossy();
    let decoded = decode_percent_encoded_component(file_name.trim());
    let name = decoded.trim();
    (!name.is_empty()).then(|| name.to_string())
}

fn decode_percent_encoded_component(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.contains('%') {
        return trimmed.to_string();
    }

    let bytes = trimmed.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = (bytes[index + 1] as char).to_digit(16);
            let lo = (bytes[index + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                decoded.push(((hi << 4) | lo) as u8);
                index += 3;
                continue;
            }
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&decoded).trim().to_string()
}

fn remote_trailers_from_urls(urls: &[String]) -> Vec<Value> {
    urls.iter()
        .filter(|url| !url.trim().is_empty())
        .map(|url| json!({ "Url": url }))
        .collect()
}

fn user_item_data_to_dto(data: DbUserItemData) -> UserItemDataDto {
    user_item_data_to_dto_with_runtime(data, None)
}

fn user_item_data_to_dto_with_runtime(
    data: DbUserItemData,
    runtime_ticks: Option<i64>,
) -> UserItemDataDto {
    let played_percentage = runtime_ticks
        .filter(|&rt| rt > 0 && data.playback_position_ticks > 0)
        .map(|rt| ((data.playback_position_ticks as f64 / rt as f64) * 100.0).min(100.0));
    UserItemDataDto {
        rating: None,
        played_percentage,
        unplayed_item_count: None,
        playback_position_ticks: data.playback_position_ticks,
        play_count: Some(data.play_count),
        is_favorite: data.is_favorite,
        likes: None,
        played: data.is_played,
        last_played_date: data.last_played_date,
        key: None,
        item_id: None,
        server_id: None,
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
        play_count: None,
        is_favorite: false,
        likes: None,
        played: false,
        last_played_date: None,
        key: None,
        item_id: None,
        server_id: None,
    }
}

fn user_item_data_to_dto_for_item(data: DbUserItemData, item_id: Uuid) -> UserItemDataDto {
    let mut dto = user_item_data_to_dto(data);
    let emby_id = uuid_to_emby_guid(&item_id);
    dto.key = Some(emby_id.clone());
    dto.item_id = Some(emby_id);
    dto
}

pub fn empty_user_data_for_item(item_id: Uuid) -> UserItemDataDto {
    let mut dto = empty_user_data();
    let emby_id = uuid_to_emby_guid(&item_id);
    dto.key = Some(emby_id.clone());
    dto.item_id = Some(emby_id);
    dto
}

pub async fn update_media_item_metadata(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    analysis: &crate::media_analyzer::MediaAnalysisResult,
) -> Result<(), crate::error::AppError> {
    let video_stream = analysis.streams.iter().find(|s| s.codec_type == "video");
    let audio_stream = analysis.streams.iter().find(|s| s.codec_type == "audio");
    let format = &analysis.format;

    let video_codec = video_stream.and_then(|s| s.codec_name.clone());
    let audio_codec = audio_stream.and_then(|s| s.codec_name.clone());
    let width = video_stream.and_then(|s| s.width);
    let height = video_stream.and_then(|s| s.height);
    let runtime_ticks = format
        .duration
        .as_deref()
        .and_then(|dur| dur.parse::<f64>().ok())
        .map(|seconds| (seconds * 10_000_000.0).round() as i64);
    let bit_rate = format
        .bit_rate
        .as_deref()
        .and_then(|br| br.parse::<i64>().ok());
    let file_size = format
        .size
        .as_deref()
        .and_then(|s| s.parse::<i64>().ok());

    sqlx::query(
        r#"
        UPDATE media_items
        SET video_codec = COALESCE($1, video_codec),
            audio_codec = COALESCE($2, audio_codec),
            width = COALESCE($3, width),
            height = COALESCE($4, height),
            runtime_ticks = COALESCE($5, runtime_ticks),
            bit_rate = COALESCE($6, bit_rate),
            size = COALESCE($7, size),
            date_modified = now()
        WHERE id = $8
        "#,
    )
    .bind(video_codec)
    .bind(audio_codec)
    .bind(width)
    .bind(height)
    .bind(runtime_ticks)
    .bind(bit_rate)
    .bind(file_size)
    .bind(item_id)
    .execute(pool)
    .await?;

    // 保存媒体流信息到media_streams表
    save_media_streams(pool, item_id, analysis).await?;

    Ok(())
}

pub async fn update_media_item_series_metadata(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    metadata: &ExternalSeriesMetadata,
) -> Result<(), AppError> {
    let provider_ids = serde_json::to_value(&metadata.provider_ids).unwrap_or_else(|_| json!({}));

    sqlx::query(
        r#"
        UPDATE media_items
        SET name = COALESCE($2, name),
            original_title = COALESCE($3, original_title),
            overview = COALESCE($4, overview),
            premiere_date = COALESCE($5, premiere_date),
            status = COALESCE($6, status),
            end_date = COALESCE($7, end_date),
            air_days = CASE
                WHEN cardinality($8::text[]) > 0 THEN $8
                ELSE air_days
            END,
            air_time = COALESCE($9, air_time),
            production_year = COALESCE($10, production_year),
            community_rating = COALESCE($11, community_rating),
            genres = CASE
                WHEN cardinality($12::text[]) > 0 THEN $12
                ELSE genres
            END,
            studios = CASE
                WHEN cardinality($13::text[]) > 0 THEN $13
                ELSE studios
            END,
            production_locations = CASE
                WHEN cardinality($14::text[]) > 0 THEN $14
                ELSE production_locations
            END,
            provider_ids = provider_ids || $15::jsonb,
            taglines = CASE
                WHEN $16::text IS NOT NULL AND length(btrim($16::text)) > 0 THEN ARRAY[$16::text]
                ELSE taglines
            END,
            tags = CASE
                WHEN cardinality($17::text[]) > 0 THEN $17
                ELSE tags
            END,
            date_modified = now()
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(metadata.name.as_deref())
    .bind(metadata.original_title.as_deref())
    .bind(metadata.overview.as_deref())
    .bind(metadata.premiere_date)
    .bind(metadata.status.as_deref())
    .bind(metadata.end_date)
    .bind(&metadata.air_days)
    .bind(metadata.air_time.as_deref())
    .bind(metadata.production_year)
    .bind(metadata.community_rating)
    .bind(&metadata.genres)
    .bind(&metadata.studios)
    .bind(&metadata.production_locations)
    .bind(provider_ids)
    .bind(metadata.tagline.as_deref())
    .bind(&metadata.tags)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_media_item_movie_metadata(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    metadata: &ExternalMovieMetadata,
) -> Result<(), AppError> {
    let provider_ids = serde_json::to_value(&metadata.provider_ids).unwrap_or_else(|_| json!({}));
    let parental_value: Option<i32> = metadata
        .official_rating
        .as_deref()
        .and_then(official_rating_to_value);

    sqlx::query(
        r#"
        UPDATE media_items
        SET name = COALESCE($2, name),
            original_title = COALESCE($3, original_title),
            overview = COALESCE($4, overview),
            premiere_date = COALESCE($5, premiere_date),
            production_year = COALESCE($6, production_year),
            community_rating = COALESCE($7, community_rating),
            critic_rating = COALESCE($8, critic_rating),
            official_rating = COALESCE($9, official_rating),
            parental_rating_value = CASE WHEN $9 IS NOT NULL THEN $18::integer ELSE parental_rating_value END,
            runtime_ticks = COALESCE($10, runtime_ticks),
            genres = CASE
                WHEN cardinality($11::text[]) > 0 THEN $11
                ELSE genres
            END,
            studios = CASE
                WHEN cardinality($12::text[]) > 0 THEN $12
                ELSE studios
            END,
            production_locations = CASE
                WHEN cardinality($13::text[]) > 0 THEN $13
                ELSE production_locations
            END,
            provider_ids = provider_ids || $14::jsonb,
            image_primary_path = CASE
                WHEN image_primary_path IS NULL OR image_primary_path = '' THEN COALESCE($15, image_primary_path)
                ELSE image_primary_path
            END,
            backdrop_path = CASE
                WHEN backdrop_path IS NULL OR backdrop_path = '' THEN COALESCE($16, backdrop_path)
                ELSE backdrop_path
            END,
            remote_trailers = CASE
                WHEN cardinality($17::text[]) > 0 THEN $17
                ELSE remote_trailers
            END,
            taglines = CASE
                WHEN $19::text IS NOT NULL AND length(btrim($19::text)) > 0 THEN ARRAY[$19::text]
                ELSE taglines
            END,
            tags = CASE
                WHEN cardinality($20::text[]) > 0 THEN $20
                ELSE tags
            END,
            date_modified = now()
        WHERE id = $1
        "#,
    )
    .bind(item_id)
    .bind(metadata.name.as_deref())
    .bind(metadata.original_title.as_deref())
    .bind(metadata.overview.as_deref())
    .bind(metadata.premiere_date)
    .bind(metadata.production_year)
    .bind(metadata.community_rating)
    .bind(metadata.critic_rating)
    .bind(metadata.official_rating.as_deref())
    .bind(metadata.runtime_ticks)
    .bind(&metadata.genres)
    .bind(&metadata.studios)
    .bind(&metadata.production_locations)
    .bind(provider_ids)
    .bind(metadata.poster_image_url.as_deref())
    .bind(metadata.backdrop_image_url.as_deref())
    .bind(&metadata.remote_trailers)
    .bind(parental_value)
    .bind(metadata.tagline.as_deref())
    .bind(&metadata.tags)
    .execute(pool)
    .await?;

    Ok(())
}

/// 利用 `series_episode_catalog` 中的 TMDB 数据回写到 Season/Episode `media_items` 行。
/// 只更新 name、overview、premiere_date 字段（仅当远程数据非空时）。
pub async fn backfill_season_episode_metadata_from_catalog(
    pool: &sqlx::PgPool,
    series_id: Uuid,
) -> Result<(), AppError> {
    // Episode: 按 season_number + episode_number 匹配
    sqlx::query(
        r#"
        UPDATE media_items ep
        SET name       = COALESCE(NULLIF(cat.name, ''), ep.name),
            overview   = COALESCE(NULLIF(cat.overview, ''), ep.overview),
            premiere_date = COALESCE(cat.premiere_date, ep.premiere_date),
            date_modified = now()
        FROM series_episode_catalog cat
        WHERE cat.series_id = $1
          AND ep.item_type = 'Episode'
          AND ep.series_id = $1
          AND ep.parent_index_number = cat.season_number
          AND ep.index_number = cat.episode_number
        "#,
    )
    .bind(series_id)
    .execute(pool)
    .await?;

    // Season: 用 catalog 中按 season_number 分组的最早 premiere_date 更新
    sqlx::query(
        r#"
        UPDATE media_items season
        SET premiere_date = COALESCE(agg.first_air, season.premiere_date),
            date_modified = now()
        FROM (
            SELECT season_number, MIN(premiere_date) AS first_air
            FROM series_episode_catalog
            WHERE series_id = $1 AND premiere_date IS NOT NULL
            GROUP BY season_number
        ) agg
        WHERE season.item_type = 'Season'
          AND season.parent_id = $1
          AND season.index_number = agg.season_number
        "#,
    )
    .bind(series_id)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn get_media_streams(
    pool: &sqlx::PgPool,
    media_item_id: Uuid,
) -> Result<Vec<DbMediaStream>, AppError> {
    let streams = sqlx::query_as::<_, DbMediaStream>(
        r#"
        SELECT id, media_item_id, index, stream_type, codec, codec_tag, language, title,
               is_default, is_forced, is_external, is_hearing_impaired, profile, width, height,
               channels, sample_rate, bit_rate, bit_depth, channel_layout, aspect_ratio,
               average_frame_rate, real_frame_rate, is_interlaced, color_range, color_space,
               color_transfer, color_primaries, rotation, hdr10_plus_present_flag,
               dv_version_major, dv_version_minor, dv_profile, dv_level,
               dv_bl_signal_compatibility_id, comment, time_base, codec_time_base,
               attachment_size, extended_video_sub_type, extended_video_sub_type_description,
               extended_video_type, is_anamorphic, is_avc, is_external_url,
               is_text_subtitle_stream, level, pixel_format, ref_frames, stream_start_time_ticks,
               created_at, updated_at
        FROM media_streams
        WHERE media_item_id = $1
        ORDER BY index
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await?;

    Ok(streams)
}

/// PB53：列表接口的媒体流批量预取。`/Users/{id}/Items` 一页 50 条 Movie/Episode
/// 走原来的 `get_media_streams` 会触发 50 次往返，而客户端（Hills/Yamby、Infuse、
/// Emby Web）拿到列表 MediaSources 时通常需要其中的 Video/Audio 流来显示分辨率、
/// 编码、声道等。一条 SQL 通过 `ANY($1)` 把整页的 streams 取回来，按 item_id 分桶。
pub async fn get_media_streams_batch(
    pool: &sqlx::PgPool,
    media_item_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, Vec<DbMediaStream>>, AppError> {
    let mut grouped: std::collections::HashMap<Uuid, Vec<DbMediaStream>> =
        std::collections::HashMap::with_capacity(media_item_ids.len());
    if media_item_ids.is_empty() {
        return Ok(grouped);
    }
    let rows = sqlx::query_as::<_, DbMediaStream>(
        r#"
        SELECT id, media_item_id, index, stream_type, codec, codec_tag, language, title,
               is_default, is_forced, is_external, is_hearing_impaired, profile, width, height,
               channels, sample_rate, bit_rate, bit_depth, channel_layout, aspect_ratio,
               average_frame_rate, real_frame_rate, is_interlaced, color_range, color_space,
               color_transfer, color_primaries, rotation, hdr10_plus_present_flag,
               dv_version_major, dv_version_minor, dv_profile, dv_level,
               dv_bl_signal_compatibility_id, comment, time_base, codec_time_base,
               attachment_size, extended_video_sub_type, extended_video_sub_type_description,
               extended_video_type, is_anamorphic, is_avc, is_external_url,
               is_text_subtitle_stream, level, pixel_format, ref_frames, stream_start_time_ticks,
               created_at, updated_at
        FROM media_streams
        WHERE media_item_id = ANY($1)
        ORDER BY media_item_id, index
        "#,
    )
    .bind(media_item_ids)
    .fetch_all(pool)
    .await?;
    for row in rows {
        grouped.entry(row.media_item_id).or_default().push(row);
    }
    Ok(grouped)
}

pub async fn get_media_chapters(
    pool: &sqlx::PgPool,
    media_item_id: Uuid,
) -> Result<Vec<DbMediaChapter>, AppError> {
    let chapters = sqlx::query_as::<_, DbMediaChapter>(
        r#"
        SELECT id, media_item_id, chapter_index, start_position_ticks, name, marker_type, image_path,
               created_at, updated_at
        FROM media_chapters
        WHERE media_item_id = $1
        ORDER BY chapter_index, start_position_ticks
        "#,
    )
    .bind(media_item_id)
    .fetch_all(pool)
    .await?;

    Ok(chapters)
}

pub async fn update_chapter_image_path(
    pool: &sqlx::PgPool,
    chapter_id: Uuid,
    image_path: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE media_chapters SET image_path = $1, updated_at = now() WHERE id = $2",
    )
    .bind(image_path)
    .bind(chapter_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 在指定库中查找同名 Series（用于 EnableAutomaticSeriesGrouping）
pub async fn find_series_by_name_in_library(
    pool: &sqlx::PgPool,
    library_id: Uuid,
    series_name: &str,
) -> Result<Option<DbMediaItem>, AppError> {
    Ok(sqlx::query_as::<_, DbMediaItem>(
        r#"
        SELECT *
        FROM media_items
        WHERE library_id = $1
          AND item_type = 'Series'
          AND LOWER(name) = LOWER($2)
        LIMIT 1
        "#,
    )
    .bind(library_id)
    .bind(series_name)
    .fetch_optional(pool)
    .await?)
}

/// PB53：判断条目是否远端 Emby/STRM 来源。
///
/// 注意 `naming::read_strm_target` 会做磁盘读，仅在 `is_strm()`（按文件后缀）
/// 命中时才执行；本地常规视频走快路径，零磁盘开销。
fn is_remote_media_item(item: &DbMediaItem) -> bool {
    let local_path = Path::new(&item.path);
    let normalized_path = item.path.replace('\\', "/");
    let legacy_virtual_remote = normalized_path
        .to_ascii_uppercase()
        .starts_with("REMOTE_EMBY/");
    let strm_target = naming::is_strm(local_path)
        .then(|| naming::read_strm_target(local_path))
        .flatten();
    let provider_remote = item
        .provider_ids
        .get("RemoteEmbySourceId")
        .and_then(serde_json::Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    strm_target.is_some() || legacy_virtual_remote || provider_remote
}

/// PB53：把 DB media_streams 行批量转换成 Emby `MediaStream` DTO。
///
/// 抽出来的目的是同时给：
/// - `get_media_source_with_streams`（PlaybackInfo 单条）
/// - `media_source_for_item_with_db_streams`（列表批量）
/// 共用，避免两个地方维护同一段 100+ 行字段映射。
pub fn convert_db_streams_to_dto(
    item: &DbMediaItem,
    db_streams: &[DbMediaStream],
    server_id: Uuid,
    is_remote: bool,
) -> Vec<MediaStreamDto> {
    let mut media_streams = Vec::with_capacity(db_streams.len());
    for stream in db_streams.iter() {
        let stream_type = match stream.stream_type.as_str() {
            "video" => "Video".to_string(),
            "audio" => "Audio".to_string(),
            "subtitle" => "Subtitle".to_string(),
            _ => stream.stream_type.clone(),
        };
        let is_subtitle_type = stream_type == "Subtitle";

        let display_language = display_language(stream.language.as_deref());
        let display_title =
            display_title_for_stream(stream, &stream_type, display_language.as_deref());

        // 根据流类型设置交付方法和字幕位置类型
        let (delivery_method, delivery_url, subtitle_location_type) = if stream_type == "Subtitle" {
            if stream.is_external {
                let item_emby_id = uuid_to_emby_guid(&item.id);
                let media_source_id = format!("mediasource_{item_emby_id}");
                let delivery_url = Some(format!(
                    "/Videos/{}/{}/Subtitles/{}/Stream.{}",
                    item_emby_id,
                    media_source_id,
                    stream.index,
                    stream.codec.as_deref().unwrap_or("sub")
                ));
                (
                    Some("External".to_string()),
                    delivery_url,
                    Some("External".to_string()),
                )
            } else {
                (
                    Some("Embed".to_string()),
                    None,
                    Some("InternalStream".to_string()),
                )
            }
        } else {
            (None, None, None)
        };

        media_streams.push(MediaStreamDto {
            index: stream.index,
            stream_type,
            codec: stream.codec.clone(),
            codec_tag: stream.codec_tag.clone(),
            language: stream.language.clone(),
            display_title,
            is_default: stream.is_default,
            is_forced: stream.is_forced,
            width: stream.width,
            height: stream.height,
            bit_rate: stream.bit_rate,
            channels: stream.channels,
            sample_rate: stream.sample_rate,
            is_external: stream.is_external,
            delivery_method,
            delivery_url,
            is_chunked_response: Some(false),
            supports_external_stream: if stream.is_external {
                true
            } else {
                stream.codec.as_deref().map_or(false, is_text_based_subtitle)
            },
            path: None,
            aspect_ratio: stream.aspect_ratio.clone(),
            attachment_size: stream.attachment_size,
            average_frame_rate: stream.average_frame_rate,
            bit_depth: stream.bit_depth,
            color_primaries: stream.color_primaries.clone(),
            color_space: stream.color_space.clone(),
            color_transfer: stream.color_transfer.clone(),
            display_language,
            extended_video_sub_type: stream.extended_video_sub_type.clone(),
            extended_video_sub_type_description: stream.extended_video_sub_type_description.clone(),
            extended_video_type: stream.extended_video_type.clone(),
            is_anamorphic: stream.is_anamorphic,
            is_avc: stream.is_avc,
            is_external_url: stream.is_external_url.clone(),
            is_hearing_impaired: Some(stream.is_hearing_impaired),
            is_interlaced: Some(stream.is_interlaced),
            is_text_subtitle_stream: stream.is_text_subtitle_stream.or_else(|| {
                if is_subtitle_type {
                    Some(stream.codec.as_deref().map_or(false, is_text_based_subtitle))
                } else {
                    None
                }
            }),
            level: stream.level,
            pixel_format: stream.pixel_format.clone(),
            profile: stream.profile.clone(),
            protocol: Some(if is_remote { "Http" } else { "File" }.to_string()),
            real_frame_rate: stream.real_frame_rate,
            ref_frames: stream.ref_frames,
            rotation: stream.rotation,
            stream_start_time_ticks: stream.stream_start_time_ticks,
            time_base: stream.time_base.clone(),
            title: stream.title.clone(),
            comment: stream.comment.clone(),
            video_range: stream.color_range.clone(),
            channel_layout: stream.channel_layout.clone(),
            item_id: Some(uuid_to_emby_guid(&item.id)),
            server_id: Some(uuid_to_emby_guid(&server_id)),
            mime_type: stream.codec.as_deref().and_then(mime_type_for_stream_codec),
            subtitle_location_type,
        });
    }
    media_streams
}

/// PB53：列表接口（`/Users/{id}/Items` 等）`Fields=MediaSources` 命中时使用的同步
/// 构造器。**调用方必须自己批量预取 streams**（见 `get_media_streams_batch`），
/// 这里不做任何 DB 查询、不扫描 sidecar 字幕、不读 chapters，避免 N+1。
///
/// 与 `get_media_source_with_streams` 的差异：
/// - 不附加 chapters（列表里通常不要 Chapters，单独再请求）。
/// - 不扫描 sidecar（每条 N×fs.read 太贵；播放前 PlaybackInfo 会补齐）。
/// - 永远输出一个 MediaSourceDto（即使 db_streams 为空，回退到 `media_source_for_item`）。
pub fn media_source_for_item_with_db_streams(
    item: &DbMediaItem,
    server_id: Uuid,
    db_streams: &[DbMediaStream],
) -> MediaSourceDto {
    let is_remote = is_remote_media_item(item);

    if db_streams.is_empty() {
        let mut dto = media_source_for_item(item);
        dto.server_id = Some(uuid_to_emby_guid(&server_id));
        return dto;
    }

    let media_streams = convert_db_streams_to_dto(item, db_streams, server_id, is_remote);

    let local_path = Path::new(&item.path);
    let strm_target = naming::is_strm(local_path)
        .then(|| naming::read_strm_target(local_path))
        .flatten();
    let container = effective_container_from_target(item, strm_target.as_deref());
    let size = media_source_size(item, is_remote);
    let item_emby_id = uuid_to_emby_guid(&item.id);
    let media_source_id = format!("mediasource_{item_emby_id}");
    let sanitized_path = if is_remote {
        format!(
            "/Videos/{}/stream.{}?Static=true&MediaSourceId={}",
            item_emby_id, container, media_source_id
        )
    } else {
        item.path.clone()
    };

    MediaSourceDto {
        chapters: Vec::new(),
        id: media_source_id.clone(),
        path: sanitized_path,
        protocol: if is_remote { "Http" } else { "File" }.to_string(),
        source_type: "Default".to_string(),
        container: container.clone(),
        name: media_source_name(item, strm_target.as_deref()),
        sort_name: None,
        is_remote,
        encoder_path: None,
        encoder_protocol: None,
        probe_path: None,
        probe_protocol: None,
        has_mixed_protocols: Some(false),
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: true,
        direct_stream_url: Some(format!(
            "/Videos/{}/stream.{}?Static=true&MediaSourceId={}&mediaSourceId={}",
            item_emby_id, container, media_source_id, media_source_id
        )),
        formats: vec![container.clone()],
        size,
        e_tag: Some(item.date_modified.timestamp().to_string()),
        bitrate: item.bit_rate,
        default_audio_stream_index: media_streams
            .iter()
            .find(|s| s.stream_type == "Audio" && s.is_default)
            .map(|s| s.index)
            .or_else(|| {
                media_streams
                    .iter()
                    .find(|s| s.stream_type == "Audio")
                    .map(|s| s.index)
            }),
        default_subtitle_stream_index: media_streams
            .iter()
            .find(|s| s.stream_type == "Subtitle" && s.is_default)
            .map(|s| s.index),
        run_time_ticks: item.runtime_ticks,
        container_start_time_ticks: None,
        is_infinite_stream: Some(false),
        requires_opening: Some(false),
        open_token: None,
        requires_closing: Some(false),
        live_stream_id: None,
        buffer_ms: None,
        requires_looping: Some(false),
        supports_probing: Some(true),
        video_type: Some("VideoFile".to_string()),
        iso_type: None,
        video_3d_format: None,
        timestamp: infer_timestamp(&container),
        ignore_dts: false,
        ignore_index: false,
        gen_pts_input: false,
        required_http_headers: if is_remote {
            BTreeMap::new()
        } else {
            BTreeMap::from([("Accept-Ranges".to_string(), "bytes".to_string())])
        },
        add_api_key_to_direct_stream_url: Some(false),
        transcoding_url: None,
        transcoding_sub_protocol: None,
        transcoding_container: None,
        analyze_duration_ms: None,
        read_at_native_framerate: Some(false),
        item_id: Some(item_emby_id),
        server_id: Some(uuid_to_emby_guid(&server_id)),
        media_streams,
        media_attachments: Vec::new(),
    }
}

pub async fn get_media_source_with_streams(
    pool: &sqlx::PgPool,
    item: &DbMediaItem,
    server_id: Uuid,
) -> Result<MediaSourceDto, AppError> {
    // 获取媒体流
    let db_streams = get_media_streams(pool, item.id).await?;
    let db_chapters = get_media_chapters(pool, item.id).await?;

    let local_path_early = Path::new(&item.path);
    // 兼容旧虚拟路径数据：未触发再同步前 DB 中可能仍有 `REMOTE_EMBY/...` 行
    let normalized_path_early = item.path.replace('\\', "/");
    let legacy_virtual_remote_early = normalized_path_early
        .to_ascii_uppercase()
        .starts_with("REMOTE_EMBY/");
    let strm_target_early = naming::is_strm(local_path_early)
        .then(|| naming::read_strm_target(local_path_early))
        .flatten();
    let provider_remote_early = item
        .provider_ids
        .get("RemoteEmbySourceId")
        .and_then(serde_json::Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let is_remote_early =
        strm_target_early.is_some() || legacy_virtual_remote_early || provider_remote_early;

    // 转换DbMediaStream为MediaStreamDto（PB53：抽出去的共用逻辑，PlaybackInfo 与列表共用）
    let mut media_streams =
        convert_db_streams_to_dto(item, &db_streams, server_id, is_remote_early);

    // 如果没有媒体流，则回退到旧的逻辑
    if media_streams.is_empty() {
        let mut dto = media_source_for_item(item);
        dto.server_id = Some(uuid_to_emby_guid(&server_id));
        dto.chapters = db_chapters.iter().map(chapter_to_value).collect();
        return Ok(dto);
    }

    // Append external sidecar subtitles with index starting after DB streams
    let max_db_index = media_streams.iter().map(|s| s.index).max().unwrap_or(0);
    let sidecar_start_index = max_db_index + 1;
    let item_emby_id_for_subs = uuid_to_emby_guid(&item.id);
    let media_source_id_for_subs = format!("mediasource_{item_emby_id_for_subs}");
    for (offset, subtitle) in naming::sidecar_subtitles(Path::new(&item.path)).into_iter().enumerate() {
        let idx = sidecar_start_index + offset as i32;
        media_streams.push(MediaStreamDto {
            index: idx,
            stream_type: "Subtitle".to_string(),
            codec: Some(subtitle.format.clone()),
            codec_tag: None,
            language: subtitle.language.clone(),
            display_title: Some(subtitle.title.clone()),
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
                item_emby_id_for_subs, media_source_id_for_subs, idx, subtitle.format
            )),
            is_chunked_response: Some(false),
            supports_external_stream: true,
            path: None,
            aspect_ratio: None,
            attachment_size: None,
            average_frame_rate: None,
            bit_depth: None,
            color_primaries: None,
            color_space: None,
            color_transfer: None,
            display_language: None,
            extended_video_sub_type: None,
            extended_video_sub_type_description: None,
            extended_video_type: None,
            is_anamorphic: None,
            is_avc: None,
            is_external_url: None,
            is_hearing_impaired: None,
            is_interlaced: None,
            is_text_subtitle_stream: Some(is_text_based_subtitle(&subtitle.format)),
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
            channel_layout: None,
            item_id: Some(item_emby_id_for_subs.clone()),
            server_id: Some(uuid_to_emby_guid(&server_id)),
            mime_type: Some(subtitle_mime_type(&subtitle.format)),
            subtitle_location_type: Some("External".to_string()),
        });
    }

    let container = effective_container_from_target(item, strm_target_early.as_deref());
    let is_remote = is_remote_early;
    let size = media_source_size(item, is_remote);

    let item_emby_id = uuid_to_emby_guid(&item.id);
    let media_source_id = format!("mediasource_{item_emby_id}");

    let sanitized_path = if is_remote {
        format!(
            "/Videos/{}/stream.{}?Static=true&MediaSourceId={}",
            item_emby_id, container, media_source_id
        )
    } else {
        item.path.clone()
    };

    Ok(MediaSourceDto {
        chapters: db_chapters.iter().map(chapter_to_value).collect(),
        id: media_source_id.clone(),
        path: sanitized_path,
        protocol: if is_remote { "Http" } else { "File" }.to_string(),
        source_type: "Default".to_string(),
        container: container.clone(),
        name: media_source_name(item, strm_target_early.as_deref()),
        sort_name: None,
        is_remote,
        encoder_path: None,
        encoder_protocol: None,
        probe_path: None,
        probe_protocol: None,
        has_mixed_protocols: Some(false),
        supports_direct_play: true,
        supports_direct_stream: true,
        supports_transcoding: true,
        direct_stream_url: Some(format!(
            "/Videos/{}/stream.{}?Static=true&MediaSourceId={}&mediaSourceId={}",
            item_emby_id, container, media_source_id, media_source_id
        )),
        formats: vec![container.clone()],
        size,
        e_tag: Some(item.date_modified.timestamp().to_string()),
        bitrate: item.bit_rate,
        default_audio_stream_index: media_streams
            .iter()
            .find(|s| s.stream_type == "Audio" && s.is_default)
            .map(|s| s.index)
            .or_else(|| {
                media_streams
                    .iter()
                    .find(|s| s.stream_type == "Audio")
                    .map(|s| s.index)
            }),
        default_subtitle_stream_index: media_streams
            .iter()
            .find(|s| s.stream_type == "Subtitle" && s.is_default)
            .map(|s| s.index),
        run_time_ticks: item.runtime_ticks,
        container_start_time_ticks: None,
        is_infinite_stream: Some(false),
        requires_opening: Some(false),
        open_token: None,
        requires_closing: Some(false),
        live_stream_id: None,
        buffer_ms: None,
        requires_looping: Some(false),
        supports_probing: Some(true),
        video_type: Some("VideoFile".to_string()),
        iso_type: None,
        video_3d_format: None,
        timestamp: infer_timestamp(&container),
        ignore_dts: false,
        ignore_index: false,
        gen_pts_input: false,
        required_http_headers: if is_remote {
            BTreeMap::new()
        } else {
            BTreeMap::from([("Accept-Ranges".to_string(), "bytes".to_string())])
        },
        add_api_key_to_direct_stream_url: Some(false),
        transcoding_url: None,
        transcoding_sub_protocol: None,
        transcoding_container: None,
        analyze_duration_ms: None,
        read_at_native_framerate: Some(false),
        item_id: Some(item_emby_id),
        server_id: Some(uuid_to_emby_guid(&server_id)),
        media_streams,
        media_attachments: Vec::new(),
    })
}

/// 将 ffprobe `codec_type` 或远端 Emby `MediaStream.Type`（经小写）规范为
/// `media_streams.stream_type` 上 CHECK 允许的 5 类值。
///
/// 旧逻辑把无法识别的类型统一写成 `"unknown"`，会触发
/// `media_streams_stream_type_check`（PG error: violates check constraint）。
/// 虚假/钓鱼 Emby、HTTP 302 后的畸形站点、或非标准 JSON 常返回 `EmbeddedImage` / `Unknown` /
/// 空串 / 带空格尾缀的 `Type` 字段。
fn normalize_stream_type_for_media_streams_db(raw: &str) -> Option<&'static str> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    match t.to_ascii_lowercase().as_str() {
        "video" => Some("video"),
        "audio" => Some("audio"),
        "subtitle" => Some("subtitle"),
        "data" => Some("data"),
        "attachment" => Some("attachment"),
        // Emby `MediaStreamType`：`EmbeddedImage` 等封面/缩略图轨
        "embeddedimage" | "stillimage" | "thumbnail" | "posterimage" | "coverart" => Some("data"),
        _ => None,
    }
}

pub async fn save_media_streams(
    pool: &sqlx::PgPool,
    media_item_id: Uuid,
    analysis: &crate::media_analyzer::MediaAnalysisResult,
) -> Result<(), crate::error::AppError> {
    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM media_streams WHERE media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM media_chapters WHERE media_item_id = $1")
        .bind(media_item_id)
        .execute(&mut *tx)
        .await?;

    for stream in &analysis.streams {
        let Some(stream_type) = normalize_stream_type_for_media_streams_db(stream.codec_type.as_str())
        else {
            tracing::warn!(
                media_item_id = %media_item_id,
                index = stream.index,
                raw_codec_type = %stream.codec_type,
                "跳过无法映射的 media_streams 轨道类型（CHECK 仅允许 video/audio/subtitle/data/attachment；可能为虚假或非标准 Emby）"
            );
            continue;
        };

        let bit_rate = stream
            .bit_rate
            .as_deref()
            .and_then(|br| br.parse::<i32>().ok());
        let sample_rate = stream
            .sample_rate
            .as_deref()
            .and_then(|sr| sr.parse::<i32>().ok());

        // 检查是否为默认轨道（从tags中获取）
        let is_default = stream.is_default;
        let is_forced = stream.is_forced;

        // 提取codec_tag和title
        let codec_tag = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("codec_tag_string"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let title = stream.title.clone();

        // 提取其他字段
        let profile = stream.profile.clone();
        let bit_depth = stream
            .tags
            .as_ref()
            .and_then(|tags| {
                tags.get("bits_per_raw_sample")
                    .or_else(|| tags.get("bits_per_sample"))
            })
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        let aspect_ratio = stream.aspect_ratio.clone();
        let average_frame_rate = stream.average_frame_rate;
        let real_frame_rate = stream.real_frame_rate;
        let is_interlaced = stream.is_interlaced;
        let color_range = stream.color_range.clone();
        let color_space = stream.color_space.clone();
        let color_transfer = stream.color_transfer.clone();
        let color_primaries = stream.color_primaries.clone();
        let rotation = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("rotation"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        let hdr10_plus_present_flag = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("hdr10_plus_present_flag"))
            .and_then(|v| v.as_bool());
        let dv_version_major = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("dv_version_major"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        let dv_version_minor = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("dv_version_minor"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        let dv_profile = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("dv_profile"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        let dv_level = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("dv_level"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        let dv_bl_signal_compatibility_id = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("dv_bl_signal_compatibility_id"))
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<i32>().ok());
        let comment = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("comment"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let time_base = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("time_base"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let codec_time_base = stream
            .tags
            .as_ref()
            .and_then(|tags| tags.get("codec_time_base"))
            .and_then(|v| v.as_str())
            .map(String::from);

        let attachment_size = stream.attachment_size;
        let extended_video_sub_type = stream.extended_video_sub_type.clone();
        let extended_video_sub_type_description =
            stream.extended_video_sub_type_description.clone();
        let extended_video_type = stream.extended_video_type.clone();
        let is_anamorphic = stream.is_anamorphic;
        let is_avc = stream.is_avc;
        let is_external_url = stream.is_external_url.clone();
        let is_text_subtitle_stream = stream.is_text_subtitle_stream;
        let level = stream.level;
        let pixel_format = stream.pixel_format.clone();
        let ref_frames = stream.ref_frames;
        let stream_start_time_ticks = stream.stream_start_time_ticks;

        sqlx::query(
            r#"
            INSERT INTO media_streams (
                id, media_item_id, index, stream_type, codec, codec_tag, language, title,
                is_default, is_forced, is_external, is_hearing_impaired, profile, width, height,
                channels, sample_rate, bit_rate, bit_depth, channel_layout, aspect_ratio,
                average_frame_rate, real_frame_rate, is_interlaced, color_range, color_space,
                color_transfer, color_primaries, rotation, hdr10_plus_present_flag,
                dv_version_major, dv_version_minor, dv_profile, dv_level,
                dv_bl_signal_compatibility_id, comment, time_base, codec_time_base,
                attachment_size, extended_video_sub_type, extended_video_sub_type_description,
                extended_video_type, is_anamorphic, is_avc, is_external_url,
                is_text_subtitle_stream, level, pixel_format, ref_frames, stream_start_time_ticks,
                created_at, updated_at
            ) VALUES (
                gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, false, $10, $11,
                $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23, $24, $25,
                $26, $27, $28, $29, $30, $31, $32, $33, $34, $35, $36, $37, $38, $39,
                $40, $41, $42, $43, $44, $45, $46, $47, $48, now(), now()
            )
            -- 同一物理媒体偶尔会被 ffprobe 报出相同 (index, stream_type)（容器异常/多 program 等）。
            -- 先到先得即可，后续重复项静默跳过，保证扫描流程整体不被单条流冲突打断。
            ON CONFLICT (media_item_id, index, stream_type) DO NOTHING
            "#,
        )
        .bind(media_item_id)
        .bind(stream.index) // 使用流的原始索引
        .bind(stream_type)
        .bind(stream.codec_name.clone())
        .bind(codec_tag)
        .bind(stream.language.clone())
        .bind(title)
        .bind(is_default)
        .bind(is_forced)
        .bind(stream.is_hearing_impaired)
        .bind(profile)
        .bind(stream.width)
        .bind(stream.height)
        .bind(stream.channels)
        .bind(sample_rate)
        .bind(bit_rate)
        .bind(bit_depth)
        .bind(stream.channel_layout.clone())
        .bind(aspect_ratio)
        .bind(average_frame_rate)
        .bind(real_frame_rate)
        .bind(is_interlaced)
        .bind(color_range)
        .bind(color_space)
        .bind(color_transfer)
        .bind(color_primaries)
        .bind(rotation)
        .bind(hdr10_plus_present_flag)
        .bind(dv_version_major)
        .bind(dv_version_minor)
        .bind(dv_profile)
        .bind(dv_level)
        .bind(dv_bl_signal_compatibility_id)
        .bind(comment)
        .bind(time_base)
        .bind(codec_time_base)
        .bind(attachment_size)
        .bind(extended_video_sub_type)
        .bind(extended_video_sub_type_description)
        .bind(extended_video_type)
        .bind(is_anamorphic)
        .bind(is_avc)
        .bind(is_external_url)
        .bind(is_text_subtitle_stream)
        .bind(level)
        .bind(pixel_format)
        .bind(ref_frames)
        .bind(stream_start_time_ticks)
        .execute(&mut *tx)
        .await?;
    }

    for chapter in &analysis.chapters {
        sqlx::query(
            r#"
            INSERT INTO media_chapters (
                id, media_item_id, chapter_index, start_position_ticks, name, marker_type, image_path,
                created_at, updated_at
            ) VALUES (
                gen_random_uuid(), $1, $2, $3, $4, $5, NULL, now(), now()
            )
            -- 迁移里已补齐 UNIQUE (media_item_id, chapter_index)，
            -- 再扫描时静默跳过重复章节，避免个别项目炸掉整个事务。
            ON CONFLICT (media_item_id, chapter_index) DO NOTHING
            "#,
        )
        .bind(media_item_id)
        .bind(chapter.chapter_index)
        .bind(chapter.start_position_ticks)
        .bind(chapter.name.clone())
        .bind(chapter.marker_type.clone())
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(())
}

fn display_language(language: Option<&str>) -> Option<String> {
    let code = language?.trim();
    if code.is_empty() {
        return None;
    }
    let normalized = code.to_ascii_lowercase();
    let display = match normalized.as_str() {
        "eng" | "en" => "English",
        "chi" | "zho" | "zh" | "zh-cn" | "zh-hans" => "Chinese",
        "zh-hant" | "cht" => "Chinese",
        "jpn" | "ja" => "Japanese",
        "kor" | "ko" => "Korean",
        "fre" | "fra" | "fr" => "French",
        "ger" | "deu" | "de" => "German",
        "spa" | "es" => "Spanish",
        "ita" | "it" => "Italian",
        "rus" | "ru" => "Russian",
        _ => code,
    };
    Some(display.to_string())
}

fn display_title_for_stream(
    stream: &DbMediaStream,
    stream_type: &str,
    display_language: Option<&str>,
) -> Option<String> {
    if stream_type == "Audio" {
        let codec = stream.codec.as_deref().unwrap_or("Unknown").to_uppercase();
        let lang =
            display_language.unwrap_or_else(|| stream.language.as_deref().unwrap_or("Unknown"));
        let channels = format_audio_channels(stream.channel_layout.as_deref(), stream.channels);
        let mut title = if let Some(channels) = channels {
            format!("{lang} {codec} {channels}")
        } else {
            format!("{lang} {codec}")
        };
        if stream.is_default {
            title.push_str(" (默认)");
        }
        return Some(title);
    }

    if stream_type == "Subtitle" {
        let lang =
            display_language.unwrap_or_else(|| stream.language.as_deref().unwrap_or("Unknown"));
        let codec = stream.codec.as_deref().unwrap_or("Unknown").to_uppercase();
        let mut title = format!("{lang} ({codec})");
        if stream.is_forced {
            title.push_str(" Forced");
        }
        if stream.is_default {
            title.push_str(" (默认)");
        }
        return Some(title);
    }

    if stream_type == "Video" {
        let codec = stream.codec.as_deref().unwrap_or("Unknown").to_uppercase();
        return match (stream.width, stream.height) {
            (Some(width), Some(height)) => Some(format!("{width}x{height} {codec}")),
            _ => Some(codec),
        };
    }

    None
}

fn format_audio_channels(channel_layout: Option<&str>, channels: Option<i32>) -> Option<String> {
    if let Some(layout) = channel_layout {
        let normalized = layout.trim();
        if !normalized.is_empty() {
            return Some(normalized.to_string());
        }
    }
    channels.map(|value| match value {
        1 => "mono".to_string(),
        2 => "stereo".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        other => other.to_string(),
    })
}

fn chapter_to_value(chapter: &DbMediaChapter) -> Value {
    json!({
        "StartPositionTicks": chapter.start_position_ticks,
        "Name": chapter.name.clone().unwrap_or_else(|| format!("第 {:02} 章", chapter.chapter_index + 1)),
        "ImageTag": chapter.image_path.as_ref().map(|_| chapter.updated_at.timestamp().to_string()),
        "MarkerType": chapter.marker_type.clone().unwrap_or_else(|| "Chapter".to_string()),
        "ChapterIndex": chapter.chapter_index
    })
}

fn mime_type_for_stream_codec(codec: &str) -> Option<String> {
    let normalized = codec.trim().to_ascii_lowercase();
    let mime = match normalized.as_str() {
        "h264" | "avc" => "video/avc",
        "hevc" | "h265" => "video/hevc",
        "mpeg4" => "video/mp4",
        "vp8" => "video/x-vnd.on2.vp8",
        "vp9" => "video/x-vnd.on2.vp9",
        "av1" => "video/av1",
        "aac" => "audio/aac",
        "ac3" => "audio/ac3",
        "eac3" => "audio/eac3",
        "dts" => "audio/vnd.dts",
        "truehd" => "audio/true-hd",
        "flac" => "audio/flac",
        "mp3" => "audio/mpeg",
        "opus" => "audio/opus",
        "vorbis" => "audio/vorbis",
        "srt" => "application/x-subrip",
        "ass" | "ssa" => "text/x-ssa",
        "vtt" | "webvtt" => "text/vtt",
        "subrip" => "application/x-subrip",
        "pgssub" => "application/pgs",
        "mjpeg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        _ => return None,
    };
    Some(mime.to_string())
}

pub async fn find_similar_items(
    pool: &sqlx::PgPool,
    target_item: &DbMediaItem,
    limit: i64,
    user_id: Option<Uuid>,
    server_id: Uuid,
    group_items_into_collections: bool,
) -> Result<Vec<BaseItemDto>, AppError> {
    let allowed_library_ids = if let Some(uid) = user_id {
        effective_library_filter_for_user(pool, uid).await?
    } else {
        None
    };
    if let Some(ref ids) = allowed_library_ids {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
    }

    let target_identity = item_identity_key(target_item, &provider_ids_for_item(target_item));

    // Emby algorithm: fetch a broad candidate set, then score in-app
    let candidate_limit: i64 = 500;
    let lib_filter = if allowed_library_ids.is_some() {
        "AND library_id = ANY($4)"
    } else {
        "AND ($4::uuid[] IS NULL OR TRUE)"
    };
    let sql = format!(
        r#"
        SELECT id, parent_id, name, original_title, sort_name, item_type,
               media_type, path, container, overview, production_year,
               official_rating, community_rating, critic_rating, runtime_ticks,
               premiere_date, status, end_date, air_days, air_time,
               series_name, season_name, index_number, index_number_end,
               parent_index_number, provider_ids, genres, studios, tags,
               production_locations, width, height, bit_rate, video_codec,
               audio_codec, image_primary_path, backdrop_path, logo_path,
               thumb_path, art_path, banner_path, disc_path, backdrop_paths,
               remote_trailers, date_created, date_modified, image_blur_hashes, series_id, taglines, locked_fields, lock_data, size,
               display_order
        FROM media_items
        WHERE id != $1 AND item_type = $2
        {lib_filter}
        ORDER BY random()
        LIMIT $3
        "#
    );
    let candidates: Vec<DbMediaItem> = sqlx::query_as::<_, DbMediaItem>(&sql)
        .bind(&target_item.id)
        .bind(&target_item.item_type)
        .bind(candidate_limit)
        .bind(allowed_library_ids.as_deref())
        .fetch_all(pool)
        .await?;

    if candidates.is_empty() {
        return Ok(Vec::new());
    }

    // Fetch people for target item
    let target_people: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT p.name, pr.role_type FROM person_roles pr JOIN persons p ON p.id = pr.person_id WHERE pr.media_item_id = $1"
    )
    .bind(target_item.id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Batch-fetch people for all candidate items
    let candidate_ids: Vec<Uuid> = candidates.iter().map(|c| c.id).collect();
    let all_people: Vec<(Uuid, String, String)> = sqlx::query_as::<_, (Uuid, String, String)>(
        "SELECT pr.media_item_id, p.name, pr.role_type FROM person_roles pr JOIN persons p ON p.id = pr.person_id WHERE pr.media_item_id = ANY($1)"
    )
    .bind(&candidate_ids)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Index people by item_id
    let mut people_by_item: std::collections::HashMap<Uuid, Vec<(String, String)>> = std::collections::HashMap::new();
    for (item_id, name, role_type) in all_people {
        people_by_item.entry(item_id).or_default().push((name, role_type));
    }

    // Build target sets for O(1) lookup
    let target_genres: std::collections::HashSet<&str> = target_item.genres.iter().map(|s| s.as_str()).collect();
    let target_tags: std::collections::HashSet<&str> = target_item.tags.iter().map(|s| s.as_str()).collect();
    let target_studios: std::collections::HashSet<&str> = target_item.studios.iter().map(|s| s.as_str()).collect();
    let target_rating = target_item.official_rating.as_deref().unwrap_or("");
    let target_people_set: std::collections::HashSet<&str> = target_people.iter().map(|(name, _)| name.as_str()).collect();

    // Score each candidate using Emby's formula
    let mut scored: Vec<(i32, usize)> = candidates
        .iter()
        .enumerate()
        .filter_map(|(idx, candidate)| {
            if same_item_identity(target_identity.as_deref(), candidate) {
                return None;
            }
            let mut score: i32 = 0;

            // OfficialRating match: +10
            if !target_rating.is_empty() {
                if let Some(ref cr) = candidate.official_rating {
                    if cr.eq_ignore_ascii_case(target_rating) {
                        score += 10;
                    }
                }
            }

            // Genres overlap: +10 each
            for g in &candidate.genres {
                if target_genres.contains(g.as_str()) {
                    score += 10;
                }
            }

            // Tags overlap: +10 each
            for t in &candidate.tags {
                if target_tags.contains(t.as_str()) {
                    score += 10;
                }
            }

            // Studios overlap: +3 each
            for s in &candidate.studios {
                if target_studios.contains(s.as_str()) {
                    score += 3;
                }
            }

            // People overlap with role-based weighting
            if let Some(candidate_people) = people_by_item.get(&candidate.id) {
                for (name, role_type) in candidate_people {
                    if target_people_set.contains(name.as_str()) {
                        score += match role_type.as_str() {
                            "Director" => 5,
                            "Actor" => 3,
                            "Composer" => 3,
                            "GuestStar" => 3,
                            "Writer" => 2,
                            _ => 1,
                        };
                    }
                }
            }

            // Year proximity
            if let (Some(ty), Some(cy)) = (target_item.production_year, candidate.production_year) {
                let diff = (ty - cy).unsigned_abs();
                if diff < 10 { score += 2; }
                if diff < 5 { score += 2; }
            }

            // Emby threshold: only keep score > 2
            if score > 2 {
                Some((score, idx))
            } else {
                None
            }
        })
        .collect();

    // Sort by score descending
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    // Build result DTOs
    let row_ids: Vec<Uuid> = scored.iter().map(|(_, idx)| candidates[*idx].id).collect();
    let user_data_map = if let Some(uid) = user_id {
        get_user_item_data_batch(pool, uid, &row_ids).await?
    } else {
        std::collections::HashMap::new()
    };

    let mut result = Vec::new();
    let mut seen_identity_keys = BTreeSet::new();
    for (_, idx) in &scored {
        let item = &candidates[*idx];
        let identity_key = item_identity_key(item, &provider_ids_for_item(item))
            .unwrap_or_else(|| format!("item:{}", item.id));
        if group_items_into_collections && !seen_identity_keys.insert(identity_key) {
            continue;
        }
        let prefetched = Some(user_data_map.get(&item.id).cloned());
        result.push(media_item_to_dto_for_list(
            item,
            server_id,
            prefetched,
            DtoCountPrefetch::default(),
        ));
        if result.len() >= limit as usize {
            break;
        }
    }

    Ok(result)
}

fn same_item_identity(target_identity: Option<&str>, candidate: &DbMediaItem) -> bool {
    let Some(target_identity) = target_identity else {
        return false;
    };
    let candidate_identity = item_identity_key(candidate, &provider_ids_for_item(candidate));
    candidate_identity.as_deref() == Some(target_identity)
}
