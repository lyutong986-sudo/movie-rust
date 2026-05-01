use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct DbUser {
    pub id: Uuid,
    pub name: String,
    pub password_hash: String,
    pub is_admin: bool,
    pub is_hidden: bool,
    pub is_disabled: bool,
    pub policy: Value,
    pub configuration: Value,
    pub primary_image_path: Option<String>,
    pub backdrop_image_path: Option<String>,
    pub logo_image_path: Option<String>,
    pub date_modified: DateTime<Utc>,
    #[sqlx(default)]
    pub easy_password_hash: Option<String>,
    #[sqlx(default)]
    pub created_at: Option<DateTime<Utc>>,
    /// 兼容外部用户库的密码格式，目前仅 `emby_sha1`（Emby SQLite LocalUsersv2 裸 SHA1）。
    /// 为 None 表示当前用户走标准 Argon2 校验。
    #[sqlx(default)]
    pub legacy_password_format: Option<String>,
    /// 与 `legacy_password_format` 配套：导入时存入的旧版哈希原文（Emby 是 40 字符 hex）。
    #[sqlx(default)]
    pub legacy_password_hash: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbLibrary {
    pub id: Uuid,
    pub name: String,
    pub collection_type: String,
    pub path: String,
    pub library_options: Value,
    pub created_at: DateTime<Utc>,
    pub primary_image_path: Option<String>,
    pub primary_image_tag: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbRemoteEmbySource {
    pub id: Uuid,
    pub name: String,
    pub server_url: String,
    pub username: String,
    pub password: String,
    pub spoofed_user_agent: String,
    pub target_library_id: Uuid,
    pub display_mode: String,
    #[sqlx(default)]
    pub remote_view_ids: Vec<String>,
    #[sqlx(default)]
    pub remote_views: Value,
    pub enabled: bool,
    pub remote_user_id: Option<String>,
    pub access_token: Option<String>,
    pub source_secret: Uuid,
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last_sync_error: Option<String>,
    #[sqlx(default)]
    pub strm_output_path: Option<String>,
    #[sqlx(default)]
    pub sync_metadata: bool,
    #[sqlx(default)]
    pub sync_subtitles: bool,
    #[sqlx(default)]
    pub token_refresh_interval_secs: i32,
    #[sqlx(default)]
    pub last_token_refresh_at: Option<DateTime<Utc>>,
    #[sqlx(default)]
    pub view_library_map: Value,
    /// 流量模式：`"proxy"`（本地中转，默认）或 `"redirect"`（302 直链，客户端直连远端，节省带宽）
    #[sqlx(default)]
    pub proxy_mode: String,
    /// 自动增量同步间隔（分钟），0 = 关闭。后端 `remote_emby_auto_sync_loop` 每分钟检查，
    /// 当 `now() >= last_sync_at + interval` 时触发该源的增量同步。
    #[sqlx(default)]
    pub auto_sync_interval_minutes: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl DbRemoteEmbySource {
    /// 是否为直链重定向模式（302 redirect，客户端直连远端）
    pub fn is_redirect_mode(&self) -> bool {
        self.proxy_mode.trim().eq_ignore_ascii_case("redirect")
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct DbMediaItem {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub original_title: Option<String>,
    pub sort_name: String,
    pub item_type: String,
    pub media_type: String,
    pub path: String,
    pub container: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub runtime_ticks: Option<i64>,
    pub premiere_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub end_date: Option<NaiveDate>,
    pub air_days: Vec<String>,
    pub air_time: Option<String>,
    pub series_name: Option<String>,
    pub season_name: Option<String>,
    pub index_number: Option<i32>,
    pub index_number_end: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub provider_ids: Value,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub tags: Vec<String>,
    pub production_locations: Vec<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bit_rate: Option<i64>,
    #[sqlx(default)]
    pub size: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub image_primary_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub thumb_path: Option<String>,
    pub art_path: Option<String>,
    pub banner_path: Option<String>,
    pub disc_path: Option<String>,
    pub backdrop_paths: Vec<String>,
    pub remote_trailers: Vec<String>,
    pub date_created: DateTime<Utc>,
    pub date_modified: DateTime<Utc>,
    pub image_blur_hashes: Value,
    #[sqlx(default)]
    pub series_id: Option<Uuid>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbMediaStream {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub index: i32,
    pub stream_type: String,
    pub codec: Option<String>,
    pub codec_tag: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_external: bool,
    pub is_hearing_impaired: bool,
    pub profile: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub channels: Option<i32>,
    pub sample_rate: Option<i32>,
    pub bit_rate: Option<i32>,
    pub bit_depth: Option<i32>,
    pub channel_layout: Option<String>,
    pub aspect_ratio: Option<String>,
    pub average_frame_rate: Option<f64>,
    pub real_frame_rate: Option<f64>,
    pub is_interlaced: bool,
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub rotation: Option<i32>,
    pub hdr10_plus_present_flag: Option<bool>,
    pub dv_version_major: Option<i32>,
    pub dv_version_minor: Option<i32>,
    pub dv_profile: Option<i32>,
    pub dv_level: Option<i32>,
    pub dv_bl_signal_compatibility_id: Option<i32>,
    pub comment: Option<String>,
    pub time_base: Option<String>,
    pub codec_time_base: Option<String>,
    pub attachment_size: Option<i32>,
    pub extended_video_sub_type: Option<String>,
    pub extended_video_sub_type_description: Option<String>,
    pub extended_video_type: Option<String>,
    pub is_anamorphic: Option<bool>,
    pub is_avc: Option<bool>,
    pub is_external_url: Option<String>,
    pub is_text_subtitle_stream: Option<bool>,
    pub level: Option<i32>,
    pub pixel_format: Option<String>,
    pub ref_frames: Option<i32>,
    pub stream_start_time_ticks: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbMediaChapter {
    pub id: Uuid,
    pub media_item_id: Uuid,
    pub chapter_index: i32,
    pub start_position_ticks: i64,
    pub name: Option<String>,
    pub marker_type: Option<String>,
    pub image_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbPerson {
    pub id: Uuid,
    pub name: String,
    pub sort_name: Option<String>,
    pub overview: Option<String>,
    pub external_url: Option<String>,
    pub provider_ids: Value, // JSONB
    pub premiere_date: Option<DateTime<Utc>>,
    pub production_year: Option<i32>,
    pub primary_image_path: Option<String>,
    pub backdrop_image_path: Option<String>,
    pub logo_image_path: Option<String>,
    pub favorite_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub death_date: Option<DateTime<Utc>>,
    pub place_of_birth: Option<String>,
    pub homepage_url: Option<String>,
    pub metadata_synced_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct MediaItemRow {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub name: String,
    pub original_title: Option<String>,
    pub sort_name: String,
    pub item_type: String,
    pub media_type: String,
    pub path: String,
    pub container: Option<String>,
    pub overview: Option<String>,
    pub production_year: Option<i32>,
    pub official_rating: Option<String>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub runtime_ticks: Option<i64>,
    pub premiere_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub end_date: Option<NaiveDate>,
    pub air_days: Vec<String>,
    pub air_time: Option<String>,
    pub series_name: Option<String>,
    pub season_name: Option<String>,
    pub index_number: Option<i32>,
    pub index_number_end: Option<i32>,
    pub parent_index_number: Option<i32>,
    pub provider_ids: Value,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub tags: Vec<String>,
    pub production_locations: Vec<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bit_rate: Option<i64>,
    #[sqlx(default)]
    pub size: Option<i64>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub image_primary_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub thumb_path: Option<String>,
    pub art_path: Option<String>,
    pub banner_path: Option<String>,
    pub disc_path: Option<String>,
    pub backdrop_paths: Vec<String>,
    pub remote_trailers: Vec<String>,
    pub date_created: DateTime<Utc>,
    pub date_modified: DateTime<Utc>,
    pub image_blur_hashes: Value,
    #[sqlx(default)]
    pub series_id: Option<Uuid>,
    pub total_count: i64,
}

impl From<MediaItemRow> for DbMediaItem {
    fn from(value: MediaItemRow) -> Self {
        Self {
            id: value.id,
            parent_id: value.parent_id,
            name: value.name,
            original_title: value.original_title,
            sort_name: value.sort_name,
            item_type: value.item_type,
            media_type: value.media_type,
            path: value.path,
            container: value.container,
            overview: value.overview,
            production_year: value.production_year,
            official_rating: value.official_rating,
            community_rating: value.community_rating,
            critic_rating: value.critic_rating,
            runtime_ticks: value.runtime_ticks,
            premiere_date: value.premiere_date,
            status: value.status,
            end_date: value.end_date,
            air_days: value.air_days,
            air_time: value.air_time,
            series_name: value.series_name,
            season_name: value.season_name,
            index_number: value.index_number,
            index_number_end: value.index_number_end,
            parent_index_number: value.parent_index_number,
            provider_ids: value.provider_ids,
            genres: value.genres,
            studios: value.studios,
            tags: value.tags,
            production_locations: value.production_locations,
            width: value.width,
            height: value.height,
            bit_rate: value.bit_rate,
            size: value.size,
            video_codec: value.video_codec,
            audio_codec: value.audio_codec,
            image_primary_path: value.image_primary_path,
            backdrop_path: value.backdrop_path,
            logo_path: value.logo_path,
            thumb_path: value.thumb_path,
            art_path: value.art_path,
            banner_path: value.banner_path,
            disc_path: value.disc_path,
            backdrop_paths: value.backdrop_paths,
            remote_trailers: value.remote_trailers,
            date_created: value.date_created,
            date_modified: value.date_modified,
            image_blur_hashes: value.image_blur_hashes,
            series_id: value.series_id,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct DbUserItemData {
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    pub is_played: bool,
    pub last_played_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct AuthSessionRow {
    pub access_token: String,
    pub user_id: Uuid,
    pub user_name: String,
    pub is_admin: bool,
    pub session_type: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
    pub client: Option<String>,
    pub application_version: Option<String>,
    pub last_activity_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct QueryResult<T> {
    pub items: Vec<T>,
    pub total_record_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct ItemCountsDto {
    pub movie_count: i32,
    pub series_count: i32,
    pub episode_count: i32,
    pub game_count: i32,
    pub artist_count: i32,
    pub program_count: i32,
    pub game_system_count: i32,
    pub trailer_count: i32,
    pub song_count: i32,
    pub album_count: i32,
    pub music_video_count: i32,
    pub box_set_count: i32,
    pub book_count: i32,
    pub item_count: i32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ContentSectionDto {
    pub name: String,
    pub id: String,
    pub section_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub view_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub monitor: Vec<String>,
    pub card_size_offset: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_item: Option<BaseItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_info: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_feature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_interval: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserDto {
    pub name: String,
    pub server_id: String,
    pub id: String,
    pub has_password: bool,
    pub has_configured_password: bool,
    pub has_configured_easy_password: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_login_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_activity_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<DateTime<Utc>>,
    pub policy: UserPolicyDto,
    pub configuration: UserConfigurationDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PublicUserDto {
    pub name: String,
    pub server_id: String,
    pub id: String,
    pub has_password: bool,
    pub has_configured_password: bool,
    pub has_configured_easy_password: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateUserByNameRequest {
    pub name: String,
    #[serde(default)]
    pub copy_from_user_id: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub new_pw: Option<String>,
    #[serde(default)]
    pub new_password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase", default)]
pub struct UserPolicyDto {
    pub is_administrator: bool,
    pub is_hidden: bool,
    pub is_disabled: bool,
    pub enable_remote_access: bool,
    pub enable_media_playback: bool,
    pub enable_content_deletion: bool,
    pub enable_content_downloading: bool,
    pub enable_sync_transcoding: bool,
    pub enable_media_conversion: bool,
    pub enable_collection_management: bool,
    pub enable_subtitle_management: bool,
    pub enable_lyric_management: bool,
    pub enable_live_tv_access: bool,
    pub enable_live_tv_management: bool,
    pub enable_audio_playback_transcoding: bool,
    pub enable_video_playback_transcoding: bool,
    pub enable_playback_remuxing: bool,
    pub force_remote_source_transcoding: bool,
    pub enable_remote_control_of_other_users: bool,
    pub enable_shared_device_control: bool,
    pub enable_public_sharing: bool,
    pub enable_user_preference_access: bool,
    pub max_parental_rating: Option<i32>,
    pub max_parental_sub_rating: Option<i32>,
    #[serde(rename = "SimultaneousStreamLimit")]
    pub max_active_sessions: i32,
    pub invalid_login_attempt_count: i32,
    pub login_attempts_before_lockout: i32,
    pub remote_client_bitrate_limit: i32,
    pub blocked_tags: Vec<String>,
    pub allowed_tags: Vec<String>,
    pub block_unrated_items: Vec<String>,
    pub access_schedules: Vec<AccessScheduleDto>,
    #[serde(deserialize_with = "deserialize_uuid_list_lossy", serialize_with = "serialize_uuid_list_emby")]
    pub enabled_folders: Vec<Uuid>,
    pub enable_all_folders: bool,
    #[serde(deserialize_with = "deserialize_uuid_list_lossy", serialize_with = "serialize_uuid_list_emby")]
    pub enabled_channels: Vec<Uuid>,
    pub enable_all_channels: bool,
    pub enabled_devices: Vec<String>,
    pub enable_all_devices: bool,
    #[serde(deserialize_with = "deserialize_uuid_list_lossy", serialize_with = "serialize_uuid_list_emby")]
    pub blocked_media_folders: Vec<Uuid>,
    #[serde(deserialize_with = "deserialize_uuid_list_lossy", serialize_with = "serialize_uuid_list_emby")]
    pub blocked_channels: Vec<Uuid>,
    pub authentication_provider_id: String,
    pub password_reset_provider_id: String,
    pub sync_play_access: String,
    /// Sakura_embyboss 等老 emby 控制脚本用 `IsHiddenRemotely` 表示
    /// "登录页 + 远程会话也隐藏"。本项目把它视为 `is_hidden` 的镜像保留写入，
    /// 实际语义并入 `is_hidden`。
    #[serde(default)]
    pub is_hidden_remotely: bool,
    /// emby 4.x 的"允许设备相机上传"开关。本项目目前不实现 CameraUpload，
    /// 仅保留契约字段以避免 400。
    #[serde(default)]
    pub allow_camera_upload: bool,
    /// emby 老字段：允许下载字幕。本项目用 `enable_subtitle_management`，
    /// 这里保留契约字段。
    #[serde(default)]
    pub enable_subtitle_downloading: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct AccessScheduleDto {
    pub day_of_week: String,
    pub start_hour: f64,
    pub end_hour: f64,
}

impl Default for UserPolicyDto {
    fn default() -> Self {
        Self {
            is_administrator: false,
            is_hidden: false,
            is_disabled: false,
            enable_remote_access: true,
            enable_media_playback: true,
            enable_content_deletion: false,
            enable_content_downloading: true,
            enable_sync_transcoding: false,
            enable_media_conversion: false,
            enable_collection_management: false,
            enable_subtitle_management: false,
            enable_lyric_management: false,
            enable_live_tv_access: false,
            enable_live_tv_management: false,
            enable_audio_playback_transcoding: true,
            enable_video_playback_transcoding: true,
            enable_playback_remuxing: true,
            force_remote_source_transcoding: false,
            enable_remote_control_of_other_users: false,
            enable_shared_device_control: false,
            enable_public_sharing: true,
            enable_user_preference_access: true,
            max_parental_rating: None,
            max_parental_sub_rating: None,
            max_active_sessions: 0,
            invalid_login_attempt_count: 0,
            login_attempts_before_lockout: -1,
            remote_client_bitrate_limit: 0,
            blocked_tags: Vec::new(),
            allowed_tags: Vec::new(),
            block_unrated_items: Vec::new(),
            access_schedules: Vec::new(),
            enabled_folders: Vec::new(),
            enable_all_folders: true,
            enabled_channels: Vec::new(),
            enable_all_channels: true,
            enabled_devices: Vec::new(),
            enable_all_devices: true,
            blocked_media_folders: Vec::new(),
            blocked_channels: Vec::new(),
            authentication_provider_id: "Default".to_string(),
            password_reset_provider_id: "Default".to_string(),
            sync_play_access: "CreateAndJoinGroups".to_string(),
            is_hidden_remotely: false,
            allow_camera_upload: false,
            enable_subtitle_downloading: false,
        }
    }
}

/// 序列化 `Vec<Uuid>` 为 Emby GUID 格式字符串列表（大写含连字符），
/// 与 `uuid_to_emby_guid` / `VirtualFolders.Guid` 输出一致，
/// 使 Sakura_embyboss 等管理工具做字符串比较时不会因大小写不一致而失配。
fn serialize_uuid_list_emby<S>(uuids: &[Uuid], s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeSeq;
    let mut seq = s.serialize_seq(Some(uuids.len()))?;
    for u in uuids {
        seq.serialize_element(&uuid_to_emby_guid(u))?;
    }
    seq.end()
}

/// 把 `Vec<Value>`（JSON 数组）反序列化成 `Vec<Uuid>`，单条无法解析为 Uuid 时不报错而是丢弃。
///
/// 设计动机：
/// - Emby SDK 对 `EnabledFolders / BlockedMediaFolders` 等字段约定为 GUID 列表，但
///   类似 `Sakura_embyboss` 的第三方管理脚本会**直接把库名（中文/英文字符串）**塞进
///   `BlockedMediaFolders`，再交给 emby 服务端做名字→GUID 的本地映射。
/// - 严格 `Vec<Uuid>` 反序列化会让这类客户端 400，本项目允许"无法 parse 即丢弃"，
///   同时由 `apply_user_policy_update` 的库名映射层负责将字符串名翻译为 GUID 注入回来。
/// - 接受标准 8-4-4-4-12 UUID、emby 32-hex GUID 两种格式。
fn deserialize_uuid_list_lossy<'de, D>(d: D) -> Result<Vec<Uuid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: Vec<serde_json::Value> = serde::Deserialize::deserialize(d)?;
    Ok(raw
        .into_iter()
        .filter_map(|v| {
            let s = match v {
                serde_json::Value::String(s) => s,
                other => other.as_str().map(ToOwned::to_owned).unwrap_or_default(),
            };
            let s = s.trim();
            if s.is_empty() {
                return None;
            }
            Uuid::parse_str(s)
                .ok()
                .or_else(|| emby_id_to_uuid(s).ok())
        })
        .collect())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase", default)]
pub struct UserConfigurationDto {
    pub play_default_audio_track: bool,
    pub play_default_subtitle_track: bool,
    pub subtitle_mode: String,
    pub audio_language_preference: String,
    pub subtitle_language_preference: String,
    pub display_missing_episodes: bool,
    pub grouped_folders: Vec<String>,
    pub latest_items_excludes: Vec<String>,
    pub my_media_excludes: Vec<String>,
    pub ordered_views: Vec<String>,
    pub hide_played_in_latest: bool,
    pub remember_audio_selections: bool,
    pub remember_subtitle_selections: bool,
    pub enable_local_password: bool,
    #[serde(alias = "enableBackdrops")]
    pub enable_backdrops: bool,
    #[serde(alias = "enableThemeSongs")]
    pub enable_theme_songs: bool,
    #[serde(alias = "displayUnairedEpisodes")]
    pub display_unaired_episodes: bool,
    #[serde(alias = "enableCinemaMode")]
    pub enable_cinema_mode: bool,
    #[serde(alias = "enableNextEpisodeAutoPlay")]
    pub enable_next_episode_auto_play: bool,
    #[serde(alias = "maxStreamingBitrate")]
    pub max_streaming_bitrate: i64,
    #[serde(alias = "maxChromecastBitrate")]
    pub max_chromecast_bitrate: i64,
}

impl Default for UserConfigurationDto {
    fn default() -> Self {
        Self {
            play_default_audio_track: true,
            play_default_subtitle_track: false,
            subtitle_mode: "Default".to_string(),
            audio_language_preference: String::new(),
            subtitle_language_preference: String::new(),
            display_missing_episodes: false,
            grouped_folders: Vec::new(),
            latest_items_excludes: Vec::new(),
            my_media_excludes: Vec::new(),
            ordered_views: Vec::new(),
            hide_played_in_latest: false,
            remember_audio_selections: true,
            remember_subtitle_selections: true,
            enable_local_password: true,
            enable_backdrops: true,
            enable_theme_songs: true,
            display_unaired_episodes: false,
            enable_cinema_mode: false,
            enable_next_episode_auto_play: true,
            max_streaming_bitrate: 140_000_000,
            max_chromecast_bitrate: 0,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionInfoDto {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
    pub server_id: String,
    pub client: String,
    pub device_id: String,
    pub device_name: String,
    pub application_version: String,
    pub is_active: bool,
    pub last_activity_date: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_end_point: Option<String>,
    pub supports_remote_control: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub playable_media_types: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_commands: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now_playing_item: Option<BaseItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub now_viewing_item: Option<BaseItemDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play_state: Option<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub additional_users: Vec<Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub now_playing_queue: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_primary_image_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserPasswordRequest {
    #[serde(default)]
    pub current_pw: Option<String>,
    #[serde(default)]
    pub current_password: Option<String>,
    #[serde(default)]
    pub new_pw: Option<String>,
    #[serde(default)]
    pub new_password: Option<String>,
    #[serde(default)]
    pub reset_password: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticateByNameRequest {
    #[serde(alias = "Name", alias = "username", alias = "UserName")]
    pub username: Option<String>,
    #[serde(alias = "pw")]
    pub pw: Option<String>,
    #[serde(alias = "password")]
    pub password: Option<String>,
    #[serde(alias = "DeviceId", alias = "deviceId")]
    pub device_id: Option<String>,
    #[serde(alias = "Device")]
    pub device_name: Option<String>,
    #[serde(alias = "Client")]
    pub client: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AuthenticationResult {
    pub user: UserDto,
    pub session_info: SessionInfoDto,
    pub access_token: String,
    pub server_id: String,
}

/// `GET /Connect/Exchange` 对应 Emby ConnectAuthenticationExchangeResult
/// 参考 <https://dev.emby.media/reference/RestAPI/ConnectService/getConnectExchange.html>
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ConnectAuthenticationExchangeResult {
    pub local_user_id: String,
    pub access_token: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PublicSystemInfo {
    pub local_address: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub local_addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wan_address: Option<String>,
    pub server_name: String,
    pub version: String,
    pub product_name: String,
    pub operating_system: String,
    pub id: String,
    pub startup_wizard_completed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SystemInfo {
    pub local_address: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub local_addresses: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wan_address: Option<String>,
    pub server_name: String,
    pub version: String,
    pub product_name: String,
    pub operating_system: String,
    pub id: String,
    pub startup_wizard_completed: bool,
    pub can_self_restart: bool,
    pub encoder_location_type: String,
    pub has_pending_restart: bool,
    pub program_data_path: String,
    pub items_by_name_path: String,
    pub log_path: String,
    pub internal_metadata_path: String,
    pub transcoding_temp_path: String,
    pub cache_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LogFileDto {
    pub name: String,
    pub date_modified: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityLogEntryDto {
    pub id: String,
    pub name: String,
    #[serde(rename = "Type")]
    pub entry_type: String,
    pub short_overview: Option<String>,
    pub severity: String,
    pub date: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct EndpointInfo {
    pub is_local: bool,
    pub is_in_network: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase", default)]
pub struct BrandingConfiguration {
    #[serde(alias = "loginDisclaimer")]
    pub login_disclaimer: String,
    #[serde(alias = "customCss")]
    pub custom_css: String,
    #[serde(alias = "splashscreenEnabled")]
    pub splashscreen_enabled: bool,
}

impl Default for BrandingConfiguration {
    fn default() -> Self {
        Self {
            login_disclaimer: String::new(),
            custom_css: String::new(),
            splashscreen_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct PlaybackConfiguration {
    #[serde(alias = "minResumePct")]
    pub min_resume_pct: i32,
    #[serde(alias = "maxResumePct")]
    pub max_resume_pct: i32,
    #[serde(alias = "minResumeDurationSeconds")]
    pub min_resume_duration_seconds: i32,
    #[serde(alias = "minAudiobookResume")]
    pub min_audiobook_resume: i32,
    #[serde(alias = "maxAudiobookResume")]
    pub max_audiobook_resume: i32,
}

impl Default for PlaybackConfiguration {
    fn default() -> Self {
        Self {
            min_resume_pct: 5,
            max_resume_pct: 90,
            min_resume_duration_seconds: 300,
            min_audiobook_resume: 5,
            max_audiobook_resume: 95,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct NetworkConfiguration {
    #[serde(alias = "localAddress")]
    pub local_address: String,
    #[serde(alias = "httpServerPortNumber")]
    pub http_server_port_number: u16,
    #[serde(alias = "httpsPortNumber")]
    pub https_port_number: u16,
    #[serde(alias = "publicHttpPort")]
    pub public_http_port: u16,
    #[serde(alias = "publicHttpsPort")]
    pub public_https_port: u16,
    #[serde(alias = "certificatePath")]
    pub certificate_path: String,
    #[serde(alias = "enableHttps")]
    pub enable_https: bool,
    #[serde(alias = "externalDomain", alias = "externalDdns")]
    pub external_domain: String,
    #[serde(alias = "enableUPnP")]
    pub enable_upnp: bool,
}

impl Default for NetworkConfiguration {
    fn default() -> Self {
        Self {
            local_address: String::new(),
            http_server_port_number: 8096,
            https_port_number: 8920,
            public_http_port: 8096,
            public_https_port: 8920,
            certificate_path: String::new(),
            enable_https: false,
            external_domain: String::new(),
            enable_upnp: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct LibraryDisplayConfiguration {
    #[serde(alias = "displayFolderView")]
    pub display_folder_view: bool,
    #[serde(alias = "displaySpecialsWithinSeasons")]
    pub display_specials_within_seasons: bool,
    #[serde(alias = "groupMoviesIntoCollections")]
    pub group_movies_into_collections: bool,
    #[serde(alias = "displayCollectionsView")]
    pub display_collections_view: bool,
    #[serde(alias = "enableExternalContentInSuggestions")]
    pub enable_external_content_in_suggestions: bool,
    #[serde(alias = "dateAddedBehavior")]
    pub date_added_behavior: i32,
    #[serde(alias = "metadataPath")]
    pub metadata_path: String,
    #[serde(alias = "saveMetadataHidden")]
    pub save_metadata_hidden: bool,
    #[serde(alias = "seasonZeroDisplayName")]
    pub season_zero_display_name: String,
    #[serde(alias = "fanartApiKey")]
    pub fanart_api_key: String,
}

impl Default for LibraryDisplayConfiguration {
    fn default() -> Self {
        Self {
            display_folder_view: false,
            display_specials_within_seasons: true,
            group_movies_into_collections: true,
            display_collections_view: true,
            enable_external_content_in_suggestions: false,
            date_added_behavior: 0,
            metadata_path: String::new(),
            save_metadata_hidden: false,
            season_zero_display_name: "Specials".to_string(),
            fanart_api_key: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct SubtitleDownloadConfiguration {
    #[serde(alias = "downloadSubtitlesForMovies")]
    pub download_subtitles_for_movies: bool,
    #[serde(alias = "downloadSubtitlesForEpisodes")]
    pub download_subtitles_for_episodes: bool,
    #[serde(alias = "downloadLanguages")]
    pub download_languages: Vec<String>,
    #[serde(alias = "requirePerfectMatch")]
    pub require_perfect_match: bool,
    #[serde(alias = "skipIfAudioTrackPresent")]
    pub skip_if_audio_track_present: bool,
    #[serde(alias = "skipIfGraphicalSubsPresent")]
    pub skip_if_graphical_subs_present: bool,
    #[serde(alias = "openSubtitlesUsername")]
    pub open_subtitles_username: String,
    #[serde(alias = "openSubtitlesPassword")]
    pub open_subtitles_password: String,
    #[serde(default, alias = "openSubtitlesApiKey")]
    pub open_subtitles_api_key: String,
}

impl Default for SubtitleDownloadConfiguration {
    fn default() -> Self {
        Self {
            download_subtitles_for_movies: false,
            download_subtitles_for_episodes: false,
            download_languages: Vec::new(),
            require_perfect_match: true,
            skip_if_audio_track_present: false,
            skip_if_graphical_subs_present: true,
            open_subtitles_username: String::new(),
            open_subtitles_password: String::new(),
            open_subtitles_api_key: String::new(),
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct DbPlaylist {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub media_type: String,
    pub overview: Option<String>,
    pub image_primary_path: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbPlaylistItem {
    pub id: Uuid,
    pub playlist_id: Uuid,
    pub media_item_id: Uuid,
    pub playlist_item_id: String,
    pub sort_index: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct EncodingOptionsDto {
    #[serde(default, alias = "enableTranscoding")]
    pub enable_transcoding: bool,
    #[serde(default, alias = "enableThrottling")]
    pub enable_throttling: bool,
    #[serde(default, alias = "hardwareAccelerationType")]
    pub hardware_acceleration_type: String,
    #[serde(default, alias = "vaapiDevice")]
    pub vaapi_device: String,
    #[serde(default, alias = "encodingThreadCount")]
    pub encoding_thread_count: i32,
    #[serde(default, alias = "downMixAudioBoost")]
    pub down_mix_audio_boost: f32,
    #[serde(default, alias = "encoderAppPath")]
    pub encoder_app_path: String,
    #[serde(default, alias = "encoderLocationType")]
    pub encoder_location_type: String,
    #[serde(default, alias = "transcodingTempPath")]
    pub transcoding_temp_path: String,
    #[serde(default, alias = "h264Preset")]
    pub h264_preset: String,
    #[serde(default, alias = "h264Crf")]
    pub h264_crf: i32,
    #[serde(default, alias = "maxTranscodeSessions")]
    pub max_transcode_sessions: u32,
}

impl EncodingOptionsDto {
    pub fn from_config(config: &crate::config::Config) -> Self {
        Self {
            enable_transcoding: config.enable_transcoding,
            enable_throttling: true,
            hardware_acceleration_type: String::new(),
            vaapi_device: String::new(),
            encoding_thread_count: config.transcode_threads as i32,
            down_mix_audio_boost: 1.0,
            encoder_app_path: config.ffmpeg_path.clone(),
            encoder_location_type: if config.ffmpeg_path.eq_ignore_ascii_case("ffmpeg") {
                "System".to_string()
            } else {
                "Custom".to_string()
            },
            transcoding_temp_path: config.transcode_dir.to_string_lossy().to_string(),
            h264_preset: String::new(),
            h264_crf: 23,
            max_transcode_sessions: config.max_transcode_sessions,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartupConfiguration {
    pub server_name: String,
    pub ui_culture: String,
    pub metadata_country_code: String,
    pub preferred_metadata_language: String,
    #[serde(default = "default_library_scan_thread_count")]
    pub library_scan_thread_count: i32,
    #[serde(default = "default_strm_analysis_thread_count")]
    pub strm_analysis_thread_count: i32,
    #[serde(default = "default_tmdb_metadata_thread_count")]
    pub tmdb_metadata_thread_count: i32,
    #[serde(default)]
    pub tmdb_api_key: String,
    #[serde(default)]
    pub tmdb_api_keys: Vec<String>,
    #[serde(default)]
    pub fanart_api_keys: Vec<String>,
    #[serde(default)]
    pub subtitle_api_keys: Vec<String>,
    #[serde(default = "default_performance_tier")]
    pub performance_tier: String,
    #[serde(default = "default_db_max_connections")]
    pub db_max_connections: i32,
    #[serde(default = "default_image_download_threads")]
    pub image_download_threads: i32,
    #[serde(default = "default_background_task_threads")]
    pub background_task_threads: i32,
}

fn default_library_scan_thread_count() -> i32 {
    2
}

fn default_strm_analysis_thread_count() -> i32 {
    8
}

fn default_tmdb_metadata_thread_count() -> i32 {
    4
}

fn default_performance_tier() -> String {
    "medium".to_string()
}

fn default_db_max_connections() -> i32 {
    20
}

fn default_image_download_threads() -> i32 {
    8
}

fn default_background_task_threads() -> i32 {
    4
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartupUserRequest {
    pub name: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartupRemoteAccessRequest {
    pub enable_remote_access: bool,
    pub enable_automatic_port_mapping: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BaseItemDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_title: Option<String>,
    pub server_id: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(rename = "Etag", skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_modified: Option<DateTime<Utc>>,
    pub can_delete: bool,
    pub can_download: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub can_edit_items: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_resume: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_unique_key: Option<String>,
    pub supports_sync: bool,
    #[serde(rename = "Type")]
    pub item_type: String,
    pub is_folder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forced_sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_created: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premiere_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genres: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub genre_items: Vec<NameLongIdDto>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_ids: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub external_urls: Vec<ExternalUrlDto>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub production_locations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_rating: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub critic_rating: Option<f64>,
    pub taglines: Vec<String>,
    pub remote_trailers: Vec<Value>,
    pub people: Vec<PersonDto>,
    pub studios: Vec<NameLongIdDto>,
    pub tag_items: Vec<NameLongIdDto>,
    pub local_trailer_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_preferences_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recursive_item_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub movie_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub air_days: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_movie: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_series: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_live: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_news: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_kids: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_sports: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_premiere: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_new: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_repeat: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number_end: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index_number: Option<i32>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub image_tags: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_blur_hashes: Option<BTreeMap<String, BTreeMap<String, String>>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub backdrop_image_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_logo_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_logo_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_backdrop_item_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub parent_backdrop_image_tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thumb_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thumb_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_studio: Option<String>,
    pub user_data: UserItemDataDto,
    pub media_sources: Vec<MediaSourceDto>,
    pub media_streams: Vec<MediaStreamDto>,
    pub part_count: i32,
    pub chapters: Vec<Value>,
    pub locked_fields: Vec<String>,
    pub lock_data: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special_feature_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_aspect_ratio: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_fields: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ExternalUrlDto {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NameLongIdDto {
    pub name: String,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserItemDataDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub played_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unplayed_item_count: Option<i32>,
    pub playback_position_ticks: i64,
    pub play_count: i32,
    pub is_favorite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub likes: Option<bool>,
    pub played: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_played_date: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSourceDto {
    pub chapters: Vec<Value>,
    pub id: String,
    pub path: String,
    pub protocol: String,
    #[serde(rename = "Type")]
    pub source_type: String,
    pub container: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    pub is_remote: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoder_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_mixed_protocols: Option<bool>,
    pub supports_direct_play: bool,
    pub supports_direct_stream: bool,
    pub supports_transcoding: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_stream_url: Option<String>,
    pub formats: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub e_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_audio_stream_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_subtitle_stream_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_start_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_infinite_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_opening: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub open_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_closing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_stream_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buffer_ms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires_looping: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_probing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iso_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_3d_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub ignore_dts: bool,
    pub ignore_index: bool,
    pub gen_pts_input: bool,
    pub required_http_headers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub add_api_key_to_direct_stream_url: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_sub_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_container: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub analyze_duration_ms: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at_native_framerate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    pub media_streams: Vec<MediaStreamDto>,
    pub media_attachments: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaStreamDto {
    pub index: i32,
    #[serde(rename = "Type")]
    pub stream_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codec_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i32>,
    pub is_external: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_chunked_response: Option<bool>,
    pub supports_external_stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment_size: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub average_frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_primaries: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_transfer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended_video_sub_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended_video_sub_type_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extended_video_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_anamorphic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_avc: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_external_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_hearing_impaired: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_interlaced: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_text_subtitle_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pixel_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub real_frame_rate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_frames: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_start_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_range: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_layout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_location_type: Option<String>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoResponse {
    pub media_sources: Vec<MediaSourceDto>,
    pub play_session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_info: Option<TranscodingInfoDto>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct TranscodingInfoDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_protocol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    pub is_video_direct: bool,
    pub is_audio_direct: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_bitrate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_bitrate: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framerate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_position_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcoding_start_position_ticks: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_channels: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware_acceleration_type: Option<String>,
    pub transcode_reasons: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct VideoStreamQuery {
    #[serde(default, alias = "container")]
    pub container: Option<String>,
    #[serde(default, rename = "Static", alias = "static", deserialize_with = "deserialize_option_bool_lenient")]
    pub static_param: Option<bool>,
    #[serde(default, rename = "MediaSourceId", alias = "mediaSourceId")]
    pub media_source_id: Option<String>,
    #[serde(default, alias = "videoCodec")]
    pub video_codec: Option<String>,
    #[serde(default, alias = "audioCodec")]
    pub audio_codec: Option<String>,
    #[serde(default, alias = "audioStreamIndex")]
    pub audio_stream_index: Option<i32>,
    #[serde(default, alias = "subtitleStreamIndex")]
    pub subtitle_stream_index: Option<i32>,
    #[serde(
        default,
        alias = "VideoBitRate",
        alias = "videoBitrate",
        alias = "videoBitRate"
    )]
    pub video_bitrate: Option<i64>,
    #[serde(
        default,
        alias = "AudioBitRate",
        alias = "audioBitrate",
        alias = "audioBitRate"
    )]
    pub audio_bitrate: Option<i64>,
    #[serde(default, alias = "maxAudioChannels")]
    pub max_audio_channels: Option<i32>,
    #[serde(default, alias = "maxFramerate")]
    pub max_framerate: Option<f64>,
    #[serde(default, alias = "maxWidth")]
    pub max_width: Option<i32>,
    #[serde(default, alias = "maxHeight")]
    pub max_height: Option<i32>,
    #[serde(default, alias = "maxRefFrames")]
    pub max_ref_frames: Option<i32>,
    #[serde(default, alias = "maxVideoBitDepth")]
    pub max_video_bit_depth: Option<i32>,
    #[serde(default, alias = "maxAudioBitDepth")]
    pub max_audio_bit_depth: Option<i32>,
    #[serde(default, alias = "audioSampleRate")]
    pub audio_sample_rate: Option<i32>,
    #[serde(default, rename = "PlaySessionId", alias = "playSessionId")]
    pub play_session_id: Option<String>,
    #[serde(default, alias = "copyTimestamps", deserialize_with = "deserialize_option_bool_lenient")]
    pub copy_timestamps: Option<bool>,
    #[serde(default, alias = "startTimeTicks")]
    pub start_time_ticks: Option<i64>,
    #[serde(default)]
    pub width: Option<i32>,
    #[serde(default)]
    pub height: Option<i32>,
    #[serde(
        default,
        alias = "MaxVideoBitRate",
        alias = "maxVideoBitrate",
        alias = "maxVideoBitRate"
    )]
    pub max_video_bitrate: Option<i64>,
    #[serde(default, alias = "MaxStreamingBitrate", alias = "maxStreamingBitrate")]
    pub max_streaming_bitrate: Option<i64>,
    #[serde(default, alias = "subtitleMethod")]
    pub subtitle_method: Option<String>,
    #[serde(default, alias = "requireAvc", deserialize_with = "deserialize_option_bool_lenient")]
    pub require_avc: Option<bool>,
    #[serde(default, alias = "deInterlace", deserialize_with = "deserialize_option_bool_lenient")]
    pub de_interlace: Option<bool>,
    #[serde(default, alias = "requireNonAnamorphic", deserialize_with = "deserialize_option_bool_lenient")]
    pub require_non_anamorphic: Option<bool>,
    #[serde(default, alias = "transcodingMaxAudioChannels")]
    pub transcoding_max_audio_channels: Option<i32>,
    #[serde(default, alias = "cpuCoreLimit")]
    pub cpu_core_limit: Option<i32>,
    #[serde(default, alias = "liveStreamId")]
    pub live_stream_id: Option<String>,
    #[serde(default, alias = "enableMpegtsM2TsMode", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_mpegts_m2_ts_mode: Option<bool>,
    #[serde(default, alias = "videoStreamIndex")]
    pub video_stream_index: Option<i32>,
    #[serde(default, alias = "transcodingProtocol")]
    pub transcoding_protocol: Option<String>,
    #[serde(default, alias = "segmentContainer")]
    pub segment_container: Option<String>,
    #[serde(default, alias = "segmentLength")]
    pub segment_length: Option<i32>,
    #[serde(default, alias = "minSegments")]
    pub min_segments: Option<i32>,
    #[serde(default, alias = "breakOnNonKeyFrames", deserialize_with = "deserialize_option_bool_lenient")]
    pub break_on_non_key_frames: Option<bool>,
    #[serde(default, alias = "manifestSubtitles")]
    pub manifest_subtitles: Option<String>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsQuery {
    #[serde(default, alias = "UserId", alias = "userId", deserialize_with = "deserialize_optional_uuid")]
    pub user_id: Option<Uuid>,
    #[serde(default, alias = "SeriesId", alias = "seriesId", deserialize_with = "deserialize_optional_uuid")]
    pub series_id: Option<Uuid>,
    #[serde(
        default,
        alias = "ParentId",
        alias = "parentId",
        deserialize_with = "deserialize_optional_uuid"
    )]
    pub parent_id: Option<Uuid>,
    #[serde(default, alias = "IncludeItemTypes", alias = "includeItemTypes")]
    pub include_item_types: Option<String>,
    #[serde(default, alias = "ExcludeItemTypes", alias = "excludeItemTypes")]
    pub exclude_item_types: Option<String>,
    #[serde(default, alias = "MediaTypes", alias = "mediaTypes")]
    pub media_types: Option<String>,
    #[serde(default, alias = "VideoTypes", alias = "videoTypes")]
    pub video_types: Option<String>,
    #[serde(default, alias = "ImageTypes", alias = "imageTypes")]
    pub image_types: Option<String>,
    #[serde(
        default,
        alias = "Genres",
        alias = "genres",
        alias = "GenreIds",
        alias = "genreIds"
    )]
    pub genres: Option<String>,
    #[serde(default, alias = "OfficialRatings", alias = "officialRatings")]
    pub official_ratings: Option<String>,
    #[serde(default, alias = "Tags", alias = "tags")]
    pub tags: Option<String>,
    #[serde(default, alias = "ExcludeTags", alias = "excludeTags")]
    pub exclude_tags: Option<String>,
    #[serde(default, alias = "Years", alias = "years")]
    pub years: Option<String>,
    #[serde(default, alias = "PersonIds", alias = "personIds")]
    pub person_ids: Option<String>,
    #[serde(default, alias = "PersonTypes", alias = "personTypes")]
    pub person_types: Option<String>,
    #[serde(default, alias = "Artists", alias = "artists")]
    pub artists: Option<String>,
    #[serde(default, alias = "ArtistIds", alias = "artistIds")]
    pub artist_ids: Option<String>,
    #[serde(default, alias = "Albums", alias = "albums")]
    pub albums: Option<String>,
    #[serde(default, alias = "Studios", alias = "studios")]
    pub studios: Option<String>,
    #[serde(default, alias = "StudioIds", alias = "studioIds")]
    pub studio_ids: Option<String>,
    #[serde(default, alias = "Containers", alias = "containers")]
    pub containers: Option<String>,
    #[serde(default, alias = "AudioCodecs", alias = "audioCodecs")]
    pub audio_codecs: Option<String>,
    #[serde(default, alias = "VideoCodecs", alias = "videoCodecs")]
    pub video_codecs: Option<String>,
    #[serde(default, alias = "SubtitleCodecs", alias = "subtitleCodecs")]
    pub subtitle_codecs: Option<String>,
    #[serde(default, alias = "Ids", alias = "ids")]
    pub ids: Option<String>,
    #[serde(default, alias = "AnyProviderIdEquals", alias = "anyProviderIdEquals")]
    pub any_provider_id_equals: Option<String>,
    #[serde(default, alias = "Recursive", deserialize_with = "deserialize_option_bool_lenient")]
    pub recursive: Option<bool>,
    #[serde(default, alias = "SearchTerm", alias = "searchTerm")]
    pub search_term: Option<String>,
    #[serde(default, alias = "NameStartsWith", alias = "nameStartsWith")]
    pub name_starts_with: Option<String>,
    #[serde(
        default,
        alias = "NameStartsWithOrGreater",
        alias = "nameStartsWithOrGreater"
    )]
    pub name_starts_with_or_greater: Option<String>,
    #[serde(default, alias = "NameLessThan", alias = "nameLessThan")]
    pub name_less_than: Option<String>,
    #[serde(default, alias = "SortBy", alias = "sortBy")]
    pub sort_by: Option<String>,
    #[serde(default, alias = "SortOrder", alias = "sortOrder")]
    pub sort_order: Option<String>,
    #[serde(default, alias = "Filters")]
    pub filters: Option<String>,
    #[serde(default, alias = "Fields")]
    pub fields: Option<String>,
    #[serde(default, alias = "EnableImages", alias = "enableImages", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_images: Option<bool>,
    #[serde(default, alias = "ImageTypeLimit", alias = "imageTypeLimit")]
    pub image_type_limit: Option<i64>,
    #[serde(default, alias = "EnableImageTypes", alias = "enableImageTypes")]
    pub enable_image_types: Option<String>,
    #[serde(default, alias = "EnableUserData", alias = "enableUserData", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_user_data: Option<bool>,
    #[serde(default, alias = "StartIndex", alias = "startIndex")]
    pub start_index: Option<i64>,
    #[serde(default, alias = "Limit", alias = "limit")]
    pub limit: Option<i64>,
    #[serde(default, alias = "ListItemIds", alias = "listItemIds")]
    pub list_item_ids: Option<String>,
    #[serde(default, alias = "IsPlayed", alias = "isPlayed", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_played: Option<bool>,
    #[serde(default, alias = "IsFavorite", alias = "isFavorite", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_favorite: Option<bool>,
    #[serde(default, alias = "IsMovie", alias = "isMovie", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_movie: Option<bool>,
    #[serde(default, alias = "IsSeries", alias = "isSeries", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_series: Option<bool>,
    #[serde(default, alias = "IsFolder", alias = "isFolder", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_folder: Option<bool>,
    #[serde(default, alias = "IsHD", alias = "isHD", alias = "isHd", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_hd: Option<bool>,
    #[serde(default, alias = "Is3D", alias = "is3D", alias = "is3d", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_3d: Option<bool>,
    #[serde(default, alias = "IsLocked", alias = "isLocked", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_locked: Option<bool>,
    #[serde(default, alias = "IsPlaceHolder", alias = "isPlaceHolder", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_place_holder: Option<bool>,
    #[serde(default, alias = "HasOverview", alias = "hasOverview", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_overview: Option<bool>,
    #[serde(default, alias = "HasSubtitles", alias = "hasSubtitles", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_subtitles: Option<bool>,
    #[serde(default, alias = "HasTrailer", alias = "hasTrailer", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_trailer: Option<bool>,
    #[serde(default, alias = "HasThemeSong", alias = "hasThemeSong", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_theme_song: Option<bool>,
    #[serde(default, alias = "HasThemeVideo", alias = "hasThemeVideo", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_theme_video: Option<bool>,
    #[serde(default, alias = "HasSpecialFeature", alias = "hasSpecialFeature", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_special_feature: Option<bool>,
    #[serde(default, alias = "HasTmdbId", alias = "hasTmdbId", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_tmdb_id: Option<bool>,
    #[serde(default, alias = "HasImdbId", alias = "hasImdbId", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_imdb_id: Option<bool>,
    #[serde(default, alias = "SeriesStatus", alias = "seriesStatus")]
    pub series_status: Option<String>,
    #[serde(default, alias = "MinCommunityRating", alias = "minCommunityRating")]
    pub min_community_rating: Option<f64>,
    #[serde(default, alias = "MinCriticRating", alias = "minCriticRating")]
    pub min_critic_rating: Option<f64>,
    #[serde(default, alias = "MinPremiereDate", alias = "minPremiereDate")]
    pub min_premiere_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxPremiereDate", alias = "maxPremiereDate")]
    pub max_premiere_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MinStartDate", alias = "minStartDate")]
    pub min_start_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxStartDate", alias = "maxStartDate")]
    pub max_start_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MinEndDate", alias = "minEndDate")]
    pub min_end_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxEndDate", alias = "maxEndDate")]
    pub max_end_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MinDateLastSaved", alias = "minDateLastSaved")]
    pub min_date_last_saved: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxDateLastSaved", alias = "maxDateLastSaved")]
    pub max_date_last_saved: Option<DateTime<Utc>>,
    #[serde(
        default,
        alias = "MinDateLastSavedForUser",
        alias = "minDateLastSavedForUser"
    )]
    pub min_date_last_saved_for_user: Option<DateTime<Utc>>,
    #[serde(
        default,
        alias = "MaxDateLastSavedForUser",
        alias = "maxDateLastSavedForUser"
    )]
    pub max_date_last_saved_for_user: Option<DateTime<Utc>>,
    #[serde(default, alias = "AiredDuringSeason", alias = "airedDuringSeason")]
    pub aired_during_season: Option<i32>,
    #[serde(default, alias = "ProjectToMedia", alias = "projectToMedia", deserialize_with = "deserialize_option_bool_lenient")]
    pub project_to_media: Option<bool>,
    #[serde(
        default,
        alias = "GroupItemsIntoCollections",
        alias = "groupItemsIntoCollections",
        deserialize_with = "deserialize_option_bool_lenient"
    )]
    pub group_items_into_collections: Option<bool>,
    #[serde(
        default,
        alias = "CollapseBoxSetItems",
        alias = "collapseBoxSetItems",
        deserialize_with = "deserialize_option_bool_lenient"
    )]
    pub collapse_box_set_items: Option<bool>,
    #[serde(
        default,
        alias = "EnableTotalRecordCount",
        alias = "enableTotalRecordCount",
        deserialize_with = "deserialize_option_bool_lenient"
    )]
    pub enable_total_record_count: Option<bool>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserItemDataQuery {
    #[serde(default, alias = "userId")]
    pub user_id: Option<Uuid>,
    #[serde(default, alias = "datePlayed")]
    pub date_played: Option<DateTime<Utc>>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateUserItemDataRequest {
    #[serde(default)]
    pub playback_position_ticks: Option<i64>,
    #[serde(default)]
    pub play_count: Option<i32>,
    #[serde(default)]
    pub is_favorite: Option<bool>,
    #[serde(default)]
    pub likes: Option<bool>,
    #[serde(default)]
    pub played: Option<bool>,
    #[serde(default)]
    pub last_played_date: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub played_percentage: Option<f64>,
    #[serde(default)]
    pub unplayed_item_count: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackReport {
    #[serde(default)]
    pub item_id: Option<Uuid>,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub play_session_id: Option<String>,
    #[serde(default, rename = "MediaSourceId")]
    pub media_source_id: Option<String>,
    #[serde(default)]
    pub position_ticks: Option<i64>,
    #[serde(default)]
    pub is_paused: Option<bool>,
    #[serde(default)]
    pub played_to_completion: Option<bool>,
    #[serde(default)]
    pub can_seek: Option<bool>,
    #[serde(default)]
    pub event_name: Option<String>,
    #[serde(default)]
    pub audio_stream_index: Option<i32>,
    #[serde(default)]
    pub subtitle_stream_index: Option<i32>,
    #[serde(default)]
    pub play_method: Option<String>,
    #[serde(default)]
    pub volume_level: Option<i32>,
    #[serde(default)]
    pub repeat_mode: Option<String>,
    #[serde(default)]
    pub playback_rate: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LegacyPlaybackQuery {
    #[serde(default)]
    pub position_ticks: Option<i64>,
    #[serde(default)]
    pub play_session_id: Option<String>,
    #[serde(default, rename = "MediaSourceId")]
    pub media_source_id: Option<String>,
    #[serde(default)]
    pub is_paused: Option<bool>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateLibraryRequest {
    pub name: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default = "default_collection_type")]
    pub collection_type: String,
    #[serde(default)]
    pub library_options: LibraryOptionsDto,
}

fn default_collection_type() -> String {
    "movies".to_string()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaPathInfoDto {
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryOptionsDto {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub enable_photos: bool,
    #[serde(default = "default_true")]
    pub enable_internet_providers: bool,
    #[serde(default)]
    pub download_images_in_advance: bool,
    #[serde(default)]
    pub enable_realtime_monitor: bool,
    #[serde(default)]
    pub exclude_from_search: bool,
    #[serde(default = "default_true")]
    pub ignore_hidden_files: bool,
    #[serde(default)]
    pub enable_chapter_image_extraction: bool,
    #[serde(default)]
    pub extract_chapter_images_during_library_scan: bool,
    #[serde(default)]
    pub save_local_metadata: bool,
    #[serde(default)]
    pub save_metadata_hidden: bool,
    #[serde(default)]
    pub merge_top_level_folders: bool,
    #[serde(default)]
    pub placeholder_metadata_refresh_interval_days: i32,
    #[serde(default)]
    pub import_missing_episodes: bool,
    #[serde(default = "default_true")]
    pub enable_automatic_series_grouping: bool,
    #[serde(default)]
    pub enable_embedded_titles: bool,
    #[serde(default)]
    pub enable_embedded_episode_infos: bool,
    #[serde(default = "default_true")]
    pub enable_multi_version_by_files: bool,
    #[serde(default)]
    pub enable_multi_version_by_metadata: bool,
    #[serde(default = "default_true")]
    pub enable_multi_part_items: bool,
    #[serde(default)]
    pub automatic_refresh_interval_days: i32,
    #[serde(default)]
    pub preferred_metadata_language: Option<String>,
    #[serde(default)]
    pub preferred_image_language: Option<String>,
    #[serde(default)]
    pub metadata_country_code: Option<String>,
    #[serde(default = "default_specials_name")]
    pub season_zero_display_name: String,
    #[serde(default)]
    pub metadata_savers: Vec<String>,
    #[serde(default)]
    pub import_collections: bool,
    #[serde(default = "default_min_collection_items")]
    pub min_collection_items: i32,
    #[serde(default)]
    pub disabled_local_metadata_readers: Vec<String>,
    #[serde(default)]
    pub local_metadata_reader_order: Vec<String>,
    #[serde(default)]
    pub path_infos: Vec<MediaPathInfoDto>,
}

impl Default for LibraryOptionsDto {
    fn default() -> Self {
        Self {
            enabled: true,
            enable_photos: true,
            enable_internet_providers: true,
            download_images_in_advance: false,
            enable_realtime_monitor: false,
            exclude_from_search: false,
            ignore_hidden_files: true,
            enable_chapter_image_extraction: false,
            extract_chapter_images_during_library_scan: false,
            save_local_metadata: false,
            save_metadata_hidden: false,
            merge_top_level_folders: false,
            placeholder_metadata_refresh_interval_days: 0,
            import_missing_episodes: false,
            enable_automatic_series_grouping: true,
            enable_embedded_titles: false,
            enable_embedded_episode_infos: false,
            enable_multi_version_by_files: true,
            enable_multi_version_by_metadata: false,
            enable_multi_part_items: true,
            automatic_refresh_interval_days: 0,
            preferred_metadata_language: Some("zh".to_string()),
            preferred_image_language: Some("zh".to_string()),
            metadata_country_code: Some("CN".to_string()),
            season_zero_display_name: default_specials_name(),
            metadata_savers: vec!["Nfo".to_string()],
            import_collections: true,
            min_collection_items: default_min_collection_items(),
            disabled_local_metadata_readers: Vec::new(),
            local_metadata_reader_order: vec!["Nfo".to_string()],
            path_infos: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolderInfoDto {
    pub name: String,
    pub collection_type: String,
    pub item_id: String,
    /// Emby 服务端老版本 (≤4.7) 在 `VirtualFolders` 列表里同时返回 `ItemId` 与
    /// `Guid`，第三方管理工具（如 `Sakura_embyboss`）只读 `Guid`。本项目把它视为
    /// `ItemId` 的别名一同输出，保证两类客户端都能取到库标识。
    pub guid: String,
    pub locations: Vec<String>,
    pub library_options: LibraryOptionsDto,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibrarySubFolderDto {
    pub name: String,
    pub id: String,
    pub path: String,
    pub is_user_access_configurable: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct LibraryMediaFolderDto {
    pub name: String,
    pub id: String,
    pub guid: String,
    pub sub_folders: Vec<LibrarySubFolderDto>,
    pub is_user_access_configurable: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AddVirtualFolderDto {
    #[serde(default)]
    pub library_options: Option<LibraryOptionsDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateLibraryOptionsDto {
    pub id: Uuid,
    pub library_options: LibraryOptionsDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MediaPathDto {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub path_info: Option<MediaPathInfoDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct UpdateMediaPathRequestDto {
    pub name: String,
    pub path_info: MediaPathInfoDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct VirtualFolderQuery {
    #[serde(default, alias = "Name", alias = "name")]
    pub name: Option<String>,
    #[serde(default, alias = "NewName", alias = "newName")]
    pub new_name: Option<String>,
    #[serde(default, alias = "CollectionType", alias = "collectionType")]
    pub collection_type: Option<String>,
    #[serde(default, alias = "Paths", alias = "paths")]
    pub paths: Option<String>,
    #[serde(default, alias = "Path", alias = "path")]
    pub path: Option<String>,
    #[serde(default, alias = "RefreshLibrary", alias = "refreshLibrary", deserialize_with = "deserialize_option_bool_lenient")]
    pub refresh_library: Option<bool>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_specials_name() -> String {
    "Specials".to_string()
}

fn default_min_collection_items() -> i32 {
    2
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScanSummary {
    pub libraries: i64,
    pub scanned_files: i64,
    pub imported_items: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ImageInfoDto {
    pub image_type: String,
    pub image_index: Option<i32>,
    pub image_tag: String,
    pub path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ActivityLogQuery {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub user_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct GenreDto {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tags: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PersonDto {
    pub name: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(rename = "Type", skip_serializing_if = "Option::is_none")]
    pub person_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_image_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premiere_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "ProductionLocations")]
    pub production_locations: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tags: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_ids: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backdrop_image_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SeasonsQuery {
    #[serde(default, alias = "userId")]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub series_id: Option<Uuid>,
    #[serde(default)]
    pub fields: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_bool_lenient")]
    pub is_special_season: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_option_bool_lenient")]
    pub is_missing: Option<bool>,
    #[serde(default)]
    pub adjacent_to: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_images: Option<bool>,
    #[serde(default)]
    pub image_type_limit: Option<i64>,
    #[serde(default)]
    pub enable_image_types: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_user_data: Option<bool>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct DirectPlayProfile {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub audio_codec: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct TranscodingProfile {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub container: Option<String>,
    #[serde(default)]
    pub protocol: Option<String>,
    #[serde(default)]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub audio_codec: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EpisodesQuery {
    #[serde(default, alias = "userId")]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub season: Option<i32>,
    #[serde(default)]
    pub season_id: Option<Uuid>,
    #[serde(default, deserialize_with = "deserialize_option_bool_lenient")]
    pub is_missing: Option<bool>,
    #[serde(default)]
    pub adjacent_to: Option<String>,
    #[serde(default)]
    pub start_item_id: Option<String>,
    #[serde(default)]
    pub fields: Option<String>,
    #[serde(default)]
    pub start_index: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_images: Option<bool>,
    #[serde(default)]
    pub image_type_limit: Option<i64>,
    #[serde(default)]
    pub enable_image_types: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_user_data: Option<bool>,
    #[serde(default)]
    pub sort_by: Option<String>,
    #[serde(default)]
    pub sort_order: Option<String>,
    #[serde(default, alias = "MediaTypes", alias = "mediaTypes")]
    pub media_types: Option<String>,
    #[serde(default, alias = "VideoTypes", alias = "videoTypes")]
    pub video_types: Option<String>,
    #[serde(default, alias = "ImageTypes", alias = "imageTypes")]
    pub image_types: Option<String>,
    #[serde(
        default,
        alias = "Genres",
        alias = "genres",
        alias = "GenreIds",
        alias = "genreIds"
    )]
    pub genres: Option<String>,
    #[serde(default, alias = "OfficialRatings", alias = "officialRatings")]
    pub official_ratings: Option<String>,
    #[serde(default, alias = "IsPlayed", alias = "isPlayed", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_played: Option<bool>,
    #[serde(default, alias = "IsFavorite", alias = "isFavorite", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_favorite: Option<bool>,
    #[serde(default, alias = "Years", alias = "years")]
    pub years: Option<String>,
    #[serde(default, alias = "Tags", alias = "tags")]
    pub tags: Option<String>,
    #[serde(default, alias = "AudioCodecs", alias = "audioCodecs")]
    pub audio_codecs: Option<String>,
    #[serde(default, alias = "VideoCodecs", alias = "videoCodecs")]
    pub video_codecs: Option<String>,
    #[serde(default, alias = "SubtitleCodecs", alias = "subtitleCodecs")]
    pub subtitle_codecs: Option<String>,
    #[serde(default, alias = "Containers", alias = "containers")]
    pub containers: Option<String>,
    #[serde(default, alias = "SearchTerm", alias = "searchTerm")]
    pub search_term: Option<String>,
    #[serde(default, alias = "IsHD", alias = "isHD", alias = "isHd", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_hd: Option<bool>,
    #[serde(default, alias = "HasSubtitles", alias = "hasSubtitles", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_subtitles: Option<bool>,
    #[serde(default, alias = "HasTrailer", alias = "hasTrailer", deserialize_with = "deserialize_option_bool_lenient")]
    pub has_trailer: Option<bool>,
    #[serde(default, alias = "MinPremiereDate", alias = "minPremiereDate")]
    pub min_premiere_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxPremiereDate", alias = "maxPremiereDate")]
    pub max_premiere_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MinStartDate", alias = "minStartDate")]
    pub min_start_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxStartDate", alias = "maxStartDate")]
    pub max_start_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MinEndDate", alias = "minEndDate")]
    pub min_end_date: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxEndDate", alias = "maxEndDate")]
    pub max_end_date: Option<DateTime<Utc>>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}

/// 将 UUID 转换为 Emby API 使用的 GUID 格式字符串
pub fn uuid_to_emby_guid(uuid: &Uuid) -> String {
    uuid.to_string().to_uppercase()
}
/// 将 Emby API 返回的 ID 解析为本地 UUID
/// 支持的格式:
/// 1. 标准 UUID
/// 2. Emby GUID 格式 (无连字符)
/// 3. mediasource_{GUID} 格式
pub fn emby_id_to_uuid(id_str: &str) -> Result<Uuid, uuid::Error> {
    // 检查并去除可能的 mediasource_ 前缀
    let id_to_parse = if id_str.starts_with("mediasource_") {
        &id_str[12..] // 去除 "mediasource_" 前缀
    } else {
        id_str
    };

    Uuid::parse_str(id_to_parse)
}
pub(crate) fn deserialize_optional_uuid<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    // 自定义反序列化器: 空字符串视为 None
    let opt_str: Option<String> = Option::deserialize(deserializer)?;

    match opt_str {
        Some(s) if s.trim().is_empty() => Ok(None), // 空字符串视为 None
        Some(s) => {
            let normalized = s.trim();
            if normalized.eq_ignore_ascii_case("root") {
                return Ok(Some(Uuid::nil()));
            }
            // 尝试解析为标准 UUID
            Uuid::parse_str(normalized)
                .map(Some)
                .map_err(serde::de::Error::custom)
        }
        None => Ok(None),
    }
}

/// 宽松反序列化 `Option<bool>`：接受原生 JSON bool、字符串 "true"/"false"/"1"/"0"、
/// 数值 1/0、空字符串(→None)、null(→None)。
/// Emby 客户端在查询参数中发送字符串，在 JSON body 中发送原生 bool。
pub(crate) fn deserialize_option_bool_lenient<'de, D>(
    deserializer: D,
) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(deserializer)?;
    match val {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::Bool(b)) => Ok(Some(b)),
        Some(serde_json::Value::Number(n)) => {
            if let Some(i) = n.as_i64() {
                Ok(Some(i != 0))
            } else {
                Ok(Some(true))
            }
        }
        Some(serde_json::Value::String(s)) => {
            if s.trim().is_empty() {
                return Ok(None);
            }
            match s.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Ok(Some(true)),
                "false" | "0" => Ok(Some(false)),
                other => Err(serde::de::Error::custom(format!(
                    "无法将 '{other}' 解析为布尔值"
                ))),
            }
        }
        Some(other) => Err(serde::de::Error::custom(format!(
            "无法将 {other} 解析为布尔值"
        ))),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetSimilarItems {
    #[serde(skip)]
    pub id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_uuid")]
    pub user_id: Option<Uuid>,
    #[serde(default, alias = "Limit")]
    pub limit: Option<i64>,
    #[serde(default, alias = "StartIndex", alias = "startIndex")]
    pub start_index: Option<i64>,
    #[serde(
        default,
        alias = "GroupItemsIntoCollections",
        alias = "groupItemsIntoCollections",
        deserialize_with = "deserialize_option_bool_lenient"
    )]
    pub group_items_into_collections: Option<bool>,
    #[serde(default, alias = "Fields")]
    pub fields: Option<String>,
    #[serde(default, alias = "EnableImages", alias = "enableImages", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_images: Option<bool>,
    #[serde(default, alias = "EnableUserData", alias = "enableUserData", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_user_data: Option<bool>,
    #[serde(default, alias = "ImageTypeLimit", alias = "imageTypeLimit")]
    pub image_type_limit: Option<i64>,
    #[serde(default, alias = "EnableImageTypes", alias = "enableImageTypes")]
    pub enable_image_types: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PlaybackInfoDto {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default, alias = "userId")]
    pub user_id: Option<Uuid>,
    #[serde(default, alias = "maxStreamingBitrate")]
    pub max_streaming_bitrate: Option<i64>,
    #[serde(default, alias = "startTimeTicks")]
    pub start_time_ticks: Option<i64>,
    #[serde(default, alias = "audioStreamIndex")]
    pub audio_stream_index: Option<i32>,
    #[serde(default, alias = "subtitleStreamIndex")]
    pub subtitle_stream_index: Option<i32>,
    #[serde(default, alias = "maxAudioChannels")]
    pub max_audio_channels: Option<i32>,
    #[serde(default, alias = "mediaSourceId")]
    pub media_source_id: Option<String>,
    #[serde(default, alias = "liveStreamId")]
    pub live_stream_id: Option<String>,
    #[serde(default, alias = "deviceProfile")]
    pub device_profile: Option<DeviceProfile>,
    #[serde(default, alias = "enableDirectPlay", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_direct_play: Option<bool>,
    #[serde(default, alias = "enableDirectStream", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_direct_stream: Option<bool>,
    #[serde(default, alias = "enableTranscoding", deserialize_with = "deserialize_option_bool_lenient")]
    pub enable_transcoding: Option<bool>,
    #[serde(default, alias = "allowInterlacedVideoStreamCopy", deserialize_with = "deserialize_option_bool_lenient")]
    pub allow_interlaced_video_stream_copy: Option<bool>,
    #[serde(default, alias = "allowVideoStreamCopy", deserialize_with = "deserialize_option_bool_lenient")]
    pub allow_video_stream_copy: Option<bool>,
    #[serde(default, alias = "allowAudioStreamCopy", deserialize_with = "deserialize_option_bool_lenient")]
    pub allow_audio_stream_copy: Option<bool>,
    #[serde(default, alias = "isPlayback", deserialize_with = "deserialize_option_bool_lenient")]
    pub is_playback: Option<bool>,
    #[serde(default, alias = "autoOpenLiveStream", deserialize_with = "deserialize_option_bool_lenient")]
    pub auto_open_live_stream: Option<bool>,
    #[serde(default, alias = "currentPlaySessionId")]
    pub current_play_session_id: Option<String>,
    #[serde(default, alias = "alwaysBurnInSubtitleWhenTranscoding")]
    pub always_burn_in_subtitle_when_transcoding: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceProfile {
    // 客户端提交的设备能力描述
    pub max_streaming_bitrate: Option<i64>,
    #[serde(default)]
    pub max_static_bitrate: Option<i64>,
    #[serde(default)]
    pub direct_play_protocols: Vec<String>,
    #[serde(default)]
    pub direct_play_profiles: Vec<DirectPlayProfile>,
    #[serde(default)]
    pub transcoding_profiles: Vec<TranscodingProfile>,
    #[serde(default)]
    pub container_profiles: Vec<Value>,
    #[serde(default)]
    pub codec_profiles: Vec<Value>,
    #[serde(default)]
    pub response_profiles: Vec<Value>,
    #[serde(default)]
    pub subtitle_profiles: Vec<Value>,
}
