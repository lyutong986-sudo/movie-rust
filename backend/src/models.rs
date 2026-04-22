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
    pub primary_image_path: Option<String>,
    pub backdrop_image_path: Option<String>,
    pub logo_image_path: Option<String>,
    pub date_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbLibrary {
    pub id: Uuid,
    pub name: String,
    pub collection_type: String,
    pub path: String,
    pub library_options: Value,
    pub created_at: DateTime<Utc>,
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
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub image_primary_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub thumb_path: Option<String>,
    pub remote_trailers: Vec<String>,
    pub date_created: DateTime<Utc>,
    pub date_modified: DateTime<Utc>,
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
pub struct DbSeriesEpisodeCatalog {
    pub id: Uuid,
    pub series_id: Uuid,
    pub provider: String,
    pub external_series_id: String,
    pub external_season_id: Option<String>,
    pub external_episode_id: Option<String>,
    pub season_number: i32,
    pub episode_number: i32,
    pub episode_number_end: Option<i32>,
    pub name: String,
    pub overview: Option<String>,
    pub premiere_date: Option<NaiveDate>,
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
    pub provider_ids: Value,  // JSONB
    pub premiere_date: Option<DateTime<Utc>>,
    pub production_year: Option<i32>,
    pub primary_image_path: Option<String>,
    pub backdrop_image_path: Option<String>,
    pub logo_image_path: Option<String>,
    pub favorite_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DbPersonRole {
    pub id: Uuid,
    pub person_id: Uuid,
    pub media_item_id: Uuid,
    pub role_type: String,
    pub role: Option<String>,
    pub sort_order: i32,
    pub is_featured: bool,
    pub is_leading_role: bool,
    pub is_recurring: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub image_primary_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub logo_path: Option<String>,
    pub thumb_path: Option<String>,
    pub remote_trailers: Vec<String>,
    pub date_created: DateTime<Utc>,
    pub date_modified: DateTime<Utc>,
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
            video_codec: value.video_codec,
            audio_codec: value.audio_codec,
            image_primary_path: value.image_primary_path,
            backdrop_path: value.backdrop_path,
            logo_path: value.logo_path,
            thumb_path: value.thumb_path,
            remote_trailers: value.remote_trailers,
            date_created: value.date_created,
            date_modified: value.date_modified,
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

#[derive(Debug, Serialize, Default)]
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
    pub policy: UserPolicyDto,
    pub configuration: UserConfigurationDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateUserByNameRequest {
    pub name: String,
    #[serde(default)]
    pub copy_from_user_id: Option<String>,
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
    pub enable_remote_control_of_other_users: bool,
    pub enable_shared_device_control: bool,
    pub enable_public_sharing: bool,
    pub enable_user_preference_access: bool,
    pub max_parental_rating: Option<i32>,
    pub max_parental_sub_rating: Option<i32>,
    pub max_active_sessions: i32,
    pub login_attempts_before_lockout: i32,
    pub remote_client_bitrate_limit: i32,
    pub blocked_tags: Vec<String>,
    pub allowed_tags: Vec<String>,
    pub enabled_folders: Vec<Uuid>,
    pub enable_all_folders: bool,
    pub enabled_channels: Vec<Uuid>,
    pub enable_all_channels: bool,
    pub enabled_devices: Vec<String>,
    pub enable_all_devices: bool,
    pub blocked_media_folders: Vec<Uuid>,
    pub blocked_channels: Vec<Uuid>,
    pub authentication_provider_id: String,
    pub password_reset_provider_id: String,
    pub sync_play_access: String,
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
            enable_remote_control_of_other_users: false,
            enable_shared_device_control: false,
            enable_public_sharing: true,
            enable_user_preference_access: true,
            max_parental_rating: None,
            max_parental_sub_rating: None,
            max_active_sessions: 0,
            login_attempts_before_lockout: -1,
            remote_client_bitrate_limit: 0,
            blocked_tags: Vec::new(),
            allowed_tags: Vec::new(),
            enabled_folders: Vec::new(),
            enable_all_folders: true,
            enabled_channels: Vec::new(),
            enable_all_channels: true,
            enabled_devices: Vec::new(),
            enable_all_devices: true,
            blocked_media_folders: Vec::new(),
            blocked_channels: Vec::new(),
            authentication_provider_id: "".to_string(),
            password_reset_provider_id: "".to_string(),
            sync_play_access: "CreateAndJoinGroups".to_string(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct UserConfigurationDto {
    pub play_default_audio_track: bool,
    pub subtitle_mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SessionInfoDto {
    pub id: String,
    pub user_id: String,
    pub user_name: String,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct PublicSystemInfo {
    pub local_address: String,
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
    pub server_name: String,
    pub version: String,
    pub product_name: String,
    pub operating_system: String,
    pub id: String,
    pub startup_wizard_completed: bool,
    pub can_self_restart: bool,
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct BrandingConfiguration {
    pub login_disclaimer: String,
    pub custom_css: String,
    pub splashscreen_enabled: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct StartupConfiguration {
    pub server_name: String,
    pub ui_culture: String,
    pub metadata_country_code: String,
    pub preferred_metadata_language: String,
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

#[derive(Debug, Serialize)]
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
    pub bitrate: Option<i32>,
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
pub struct NameIdDto {
    pub name: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct NameLongIdDto {
    pub name: String,
    pub id: i64,
}

#[derive(Debug, Serialize)]
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
    pub bitrate: Option<i32>,
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
    pub video_3d_format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
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
    pub container: Option<String>,
    pub is_video_direct: bool,
    pub is_audio_direct: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framerate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardware_acceleration_type: Option<String>,
    pub transcode_reasons: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct VideoStreamQuery {
    #[serde(default, alias = "container")]
    pub container: Option<String>,
    #[serde(default, rename = "Static", alias = "static")]
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
    #[serde(default, alias = "VideoBitRate", alias = "videoBitrate", alias = "videoBitRate")]
    pub video_bitrate: Option<i64>,
    #[serde(default, alias = "AudioBitRate", alias = "audioBitrate", alias = "audioBitRate")]
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
    #[serde(default, alias = "copyTimestamps")]
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
    #[serde(default, alias = "requireAvc")]
    pub require_avc: Option<bool>,
    #[serde(default, alias = "deInterlace")]
    pub de_interlace: Option<bool>,
    #[serde(default, alias = "requireNonAnamorphic")]
    pub require_non_anamorphic: Option<bool>,
    #[serde(default, alias = "transcodingMaxAudioChannels")]
    pub transcoding_max_audio_channels: Option<i32>,
    #[serde(default, alias = "cpuCoreLimit")]
    pub cpu_core_limit: Option<i32>,
    #[serde(default, alias = "liveStreamId")]
    pub live_stream_id: Option<String>,
    #[serde(default, alias = "enableMpegtsM2TsMode")]
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
    #[serde(default, alias = "breakOnNonKeyFrames")]
    pub break_on_non_key_frames: Option<bool>,
    #[serde(default, alias = "manifestSubtitles")]
    pub manifest_subtitles: Option<String>,
    #[serde(default, rename = "api_key", alias = "ApiKey", alias = "apiKey")]
    pub _api_key: Option<String>,
}



#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ItemsQuery {
    #[serde(default, alias = "UserId", alias = "userId")]
    pub user_id: Option<Uuid>,
    #[serde(default, alias = "SeriesId", alias = "seriesId")]
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
    #[serde(default, alias = "Genres", alias = "genres", alias = "GenreIds", alias = "genreIds")]
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
    #[serde(default, alias = "Recursive")]
    pub recursive: Option<bool>,
    #[serde(default, alias = "SearchTerm", alias = "searchTerm")]
    pub search_term: Option<String>,
    #[serde(default, alias = "NameStartsWith", alias = "nameStartsWith")]
    pub name_starts_with: Option<String>,
    #[serde(default, alias = "NameStartsWithOrGreater", alias = "nameStartsWithOrGreater")]
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
    #[serde(default, alias = "StartIndex", alias = "startIndex")]
    pub start_index: Option<i64>,
    #[serde(default, alias = "Limit", alias = "limit")]
    pub limit: Option<i64>,
    #[serde(default, alias = "ListItemIds", alias = "listItemIds")]
    pub list_item_ids: Option<String>,
    #[serde(default, alias = "IsPlayed", alias = "isPlayed")]
    pub is_played: Option<bool>,
    #[serde(default, alias = "IsFavorite", alias = "isFavorite")]
    pub is_favorite: Option<bool>,
    #[serde(default, alias = "IsMovie", alias = "isMovie")]
    pub is_movie: Option<bool>,
    #[serde(default, alias = "IsSeries", alias = "isSeries")]
    pub is_series: Option<bool>,
    #[serde(default, alias = "IsFolder", alias = "isFolder")]
    pub is_folder: Option<bool>,
    #[serde(default, alias = "IsHD", alias = "isHD", alias = "isHd")]
    pub is_hd: Option<bool>,
    #[serde(default, alias = "Is3D", alias = "is3D", alias = "is3d")]
    pub is_3d: Option<bool>,
    #[serde(default, alias = "IsLocked", alias = "isLocked")]
    pub is_locked: Option<bool>,
    #[serde(default, alias = "IsPlaceHolder", alias = "isPlaceHolder")]
    pub is_place_holder: Option<bool>,
    #[serde(default, alias = "HasOverview", alias = "hasOverview")]
    pub has_overview: Option<bool>,
    #[serde(default, alias = "HasSubtitles", alias = "hasSubtitles")]
    pub has_subtitles: Option<bool>,
    #[serde(default, alias = "HasTrailer", alias = "hasTrailer")]
    pub has_trailer: Option<bool>,
    #[serde(default, alias = "HasThemeSong", alias = "hasThemeSong")]
    pub has_theme_song: Option<bool>,
    #[serde(default, alias = "HasThemeVideo", alias = "hasThemeVideo")]
    pub has_theme_video: Option<bool>,
    #[serde(default, alias = "HasSpecialFeature", alias = "hasSpecialFeature")]
    pub has_special_feature: Option<bool>,
    #[serde(default, alias = "HasTmdbId", alias = "hasTmdbId")]
    pub has_tmdb_id: Option<bool>,
    #[serde(default, alias = "HasImdbId", alias = "hasImdbId")]
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
    #[serde(default, alias = "MinDateLastSavedForUser", alias = "minDateLastSavedForUser")]
    pub min_date_last_saved_for_user: Option<DateTime<Utc>>,
    #[serde(default, alias = "MaxDateLastSavedForUser", alias = "maxDateLastSavedForUser")]
    pub max_date_last_saved_for_user: Option<DateTime<Utc>>,
    #[serde(default, alias = "AiredDuringSeason", alias = "airedDuringSeason")]
    pub aired_during_season: Option<i32>,
    #[serde(default, alias = "ProjectToMedia", alias = "projectToMedia")]
    pub project_to_media: Option<bool>,
    #[serde(
        default,
        alias = "GroupItemsIntoCollections",
        alias = "groupItemsIntoCollections"
    )]
    pub group_items_into_collections: Option<bool>,
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
    #[serde(default)]
    pub _media_source_id: Option<String>,
    #[serde(default)]
    pub position_ticks: Option<i64>,
    #[serde(default)]
    pub is_paused: Option<bool>,
    #[serde(default)]
    pub played_to_completion: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct LegacyPlaybackQuery {
    #[serde(default)]
    pub position_ticks: Option<i64>,
    #[serde(default)]
    pub play_session_id: Option<String>,
    #[serde(default)]
    pub _media_source_id: Option<String>,
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
    #[serde(default)]
    pub enable_realtime_monitor: bool,
    #[serde(default)]
    pub enable_chapter_image_extraction: bool,
    #[serde(default)]
    pub extract_chapter_images_during_library_scan: bool,
    #[serde(default)]
    pub save_local_metadata: bool,
    #[serde(default = "default_true")]
    pub enable_automatic_series_grouping: bool,
    #[serde(default)]
    pub enable_embedded_titles: bool,
    #[serde(default)]
    pub enable_embedded_episode_infos: bool,
    #[serde(default)]
    pub automatic_refresh_interval_days: i32,
    #[serde(default)]
    pub preferred_metadata_language: Option<String>,
    #[serde(default)]
    pub metadata_country_code: Option<String>,
    #[serde(default = "default_specials_name")]
    pub season_zero_display_name: String,
    #[serde(default)]
    pub metadata_savers: Vec<String>,
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
            enable_realtime_monitor: false,
            enable_chapter_image_extraction: false,
            extract_chapter_images_during_library_scan: false,
            save_local_metadata: false,
            enable_automatic_series_grouping: true,
            enable_embedded_titles: false,
            enable_embedded_episode_infos: false,
            automatic_refresh_interval_days: 0,
            preferred_metadata_language: Some("zh".to_string()),
            metadata_country_code: Some("CN".to_string()),
            season_zero_display_name: default_specials_name(),
            metadata_savers: vec!["Nfo".to_string()],
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
    #[serde(default, alias = "RefreshLibrary", alias = "refreshLibrary")]
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

#[derive(Debug, Serialize)]
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

#[derive(Debug, Serialize)]
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
    pub production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_tags: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_ids: Option<std::collections::HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite: Option<bool>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SeasonDto {
    pub name: String,
    pub server_id: String,
    pub id: String,
    #[serde(rename = "Type")]
    pub item_type: String,
    pub is_folder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premiere_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i32>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub image_tags: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_primary_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserItemDataDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct EpisodeDto {
    pub name: String,
    pub server_id: String,
    pub id: String,
    #[serde(rename = "Type")]
    pub item_type: String,
    pub is_folder: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_number_end: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_index_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub season_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premiere_date: Option<NaiveDate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub production_year: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_time_ticks: Option<i64>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub image_tags: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_primary_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_data: Option<UserItemDataDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_sources: Option<Vec<MediaSourceDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_streams: Option<Vec<MediaStreamDto>>,
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
    #[serde(default)]
    pub is_special_season: Option<bool>,
    #[serde(default)]
    pub is_missing: Option<bool>,
    #[serde(default)]
    pub adjacent_to: Option<String>,
    #[serde(default)]
    pub enable_images: Option<bool>,
    #[serde(default)]
    pub image_type_limit: Option<i64>,
    #[serde(default)]
    pub enable_image_types: Option<String>,
    #[serde(default)]
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
    #[serde(default)]
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
    #[serde(default)]
    pub enable_images: Option<bool>,
    #[serde(default)]
    pub image_type_limit: Option<i64>,
    #[serde(default)]
    pub enable_image_types: Option<String>,
    #[serde(default)]
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
    #[serde(default, alias = "Genres", alias = "genres", alias = "GenreIds", alias = "genreIds")]
    pub genres: Option<String>,
    #[serde(default, alias = "OfficialRatings", alias = "officialRatings")]
    pub official_ratings: Option<String>,
    #[serde(default, alias = "IsPlayed", alias = "isPlayed")]
    pub is_played: Option<bool>,
    #[serde(default, alias = "IsFavorite", alias = "isFavorite")]
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
    #[serde(default, alias = "IsHD", alias = "isHD", alias = "isHd")]
    pub is_hd: Option<bool>,
    #[serde(default, alias = "HasSubtitles", alias = "hasSubtitles")]
    pub has_subtitles: Option<bool>,
    #[serde(default, alias = "HasTrailer", alias = "hasTrailer")]
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

/// 将UUID转换为Emby API兼容的大写GUID格式
pub fn uuid_to_emby_guid(uuid: &Uuid) -> String {
    uuid.to_string().to_uppercase()
}

/// 将Option<Uuid>转换为Option<Emby GUID字符串>
pub fn optional_uuid_to_emby_guid(uuid: Option<Uuid>) -> Option<String> {
    uuid.map(|u| uuid_to_emby_guid(&u))
}

/// 将Emby API中的ID字符串转换为UUID
/// 支持格式:
/// 1. 纯UUID (如: "12345678-1234-1234-1234-123456789012")
/// 2. Emby GUID格式 (大写UUID)
/// 3. mediasource_{GUID} 格式 (如: "mediasource_12345678-1234-1234-1234-123456789012")
pub fn emby_id_to_uuid(id_str: &str) -> Result<Uuid, uuid::Error> {
    // 检查是否是 mediasource_ 前缀
    let id_to_parse = if id_str.starts_with("mediasource_") {
        &id_str[12..] // 移除 "mediasource_" 前缀
    } else {
        id_str
    };
    
    Uuid::parse_str(id_to_parse)
}

/// 尝试将Emby API中的ID字符串转换为UUID，如果失败返回None
pub fn try_emby_id_to_uuid(id_str: &str) -> Option<Uuid> {
    emby_id_to_uuid(id_str).ok()
}

fn deserialize_optional_uuid<'de, D>(deserializer: D) -> Result<Option<Uuid>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;
    
    // 先尝试作为字符串反序列化
    let opt_str: Option<String> = Option::deserialize(deserializer)?;
    
    match opt_str {
        Some(s) if s.trim().is_empty() => Ok(None), // 空字符串视为None
        Some(s) => {
            let normalized = s.trim();
            if normalized.eq_ignore_ascii_case("root") {
                return Ok(Some(Uuid::nil()));
            }
            // 尝试解析UUID
            Uuid::parse_str(normalized).map(Some).map_err(serde::de::Error::custom)
        }
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GetSimilarItems {
    #[serde(skip)]
    pub id: Option<String>, // 从路径参数提取，不由查询字符串解析
    #[serde(default, alias = "UserId", alias = "userId")]
    pub user_id: Option<Uuid>,
    #[serde(default, alias = "Limit")]
    pub limit: Option<i64>,
    #[serde(default, alias = "Fields")]
    pub fields: Option<String>,
    #[serde(default, alias = "EnableImages", alias = "enableImages")]
    pub enable_images: Option<bool>,
    #[serde(default, alias = "EnableUserData", alias = "enableUserData")]
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
    #[serde(default, alias = "enableDirectPlay")]
    pub enable_direct_play: Option<bool>,
    #[serde(default, alias = "enableDirectStream")]
    pub enable_direct_stream: Option<bool>,
    #[serde(default, alias = "enableTranscoding")]
    pub enable_transcoding: Option<bool>,
    #[serde(default, alias = "allowInterlacedVideoStreamCopy")]
    pub allow_interlaced_video_stream_copy: Option<bool>,
    #[serde(default, alias = "allowVideoStreamCopy")]
    pub allow_video_stream_copy: Option<bool>,
    #[serde(default, alias = "allowAudioStreamCopy")]
    pub allow_audio_stream_copy: Option<bool>,
    #[serde(default, alias = "isPlayback")]
    pub is_playback: Option<bool>,
    #[serde(default, alias = "autoOpenLiveStream")]
    pub auto_open_live_stream: Option<bool>,
    #[serde(default, alias = "currentPlaySessionId")]
    pub current_play_session_id: Option<String>,
    #[serde(default, alias = "alwaysBurnInSubtitleWhenTranscoding")]
    pub always_burn_in_subtitle_when_transcoding: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct DeviceProfile {
    // 简化设备配置，后续可扩展
    #[serde(default)]
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
    pub container_profiles: Vec<String>,
    #[serde(default)]
    pub codec_profiles: Vec<String>,
    #[serde(default)]
    pub response_profiles: Vec<String>,
    #[serde(default)]
    pub subtitle_profiles: Vec<String>,
}
