use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::{require_admin, AuthSession, OptionalAuthSession},
    error::AppError,
    repository,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Features", get(server_features))
        .route("/features", get(server_features))
        .route("/ItemTypes", get(item_types))
        .route("/ItemTypes/", get(item_types))
        .route("/itemtypes", get(item_types))
        .route("/StreamLanguages", get(stream_languages))
        .route("/streamlanguages", get(stream_languages))
        .route("/ExtendedVideoTypes", get(extended_video_types))
        .route("/extendedvideotypes", get(extended_video_types))
        .route(
            "/Libraries/AvailableOptions",
            get(library_available_options),
        )
        .route(
            "/libraries/availableoptions",
            get(library_available_options),
        )
        .route("/Playback/BitrateTest", get(playback_bitrate_test))
        .route("/playback/bitratetest", get(playback_bitrate_test))
        .route(
            "/Encoding/CodecConfiguration/Defaults",
            get(encoding_codec_defaults),
        )
        .route(
            "/encoding/codecconfiguration/defaults",
            get(encoding_codec_defaults),
        )
        .route(
            "/Encoding/CodecInformation/Video",
            get(encoding_codec_info_video),
        )
        .route(
            "/encoding/codecinformation/video",
            get(encoding_codec_info_video),
        )
        .route("/Encoding/CodecParameters", get(encoding_codec_parameters))
        .route("/encoding/codecparameters", get(encoding_codec_parameters))
        .route("/Encoding/FfmpegOptions", get(encoding_ffmpeg_options))
        .route("/encoding/ffmpegoptions", get(encoding_ffmpeg_options))
        .route(
            "/Encoding/FullToneMapOptions",
            get(encoding_full_tonemap_options),
        )
        .route(
            "/encoding/fulltonemapoptions",
            get(encoding_full_tonemap_options),
        )
        .route(
            "/Encoding/PublicToneMapOptions",
            get(encoding_public_tonemap_options),
        )
        .route(
            "/encoding/publictonemapoptions",
            get(encoding_public_tonemap_options),
        )
        .route("/Encoding/SubtitleOptions", get(encoding_subtitle_options))
        .route("/encoding/subtitleoptions", get(encoding_subtitle_options))
        .route(
            "/Encoding/ToneMapOptions",
            get(encoding_public_tonemap_options),
        )
        .route(
            "/encoding/tonemapoptions",
            get(encoding_public_tonemap_options),
        )
        .route("/BackupRestore/BackupInfo", get(backup_info))
        .route("/backuprestore/backupinfo", get(backup_info))
        .route("/BackupRestore/Restore", post(backup_restore_trigger))
        .route("/backuprestore/restore", post(backup_restore_trigger))
        .route("/BackupRestore/RestoreData", post(backup_restore_data))
        .route("/backuprestore/restoredata", post(backup_restore_data))
}

async fn server_features(_session: OptionalAuthSession) -> Json<Value> {
    // Emby 客户端用于判断服务器支持的功能位。
    // 我们按照实际实现关闭 LiveTV/DLNA/Plugins，只开启已实现的能力。
    Json(json!({
        "SupportsLibraryMonitor": true,
        "SupportsSync": false,
        "SupportsContentUploading": false,
        "SupportsHardwareEncoding": true,
        "SupportsTrickplay": true,
        "SupportsLiveTv": false,
        "SupportsDlna": false,
        "SupportsPlugins": false,
        "SupportsRemoteAccess": true,
        "SupportsWebSocket": true,
        "SupportsCollections": true,
        "SupportsPlaylists": true,
        "SupportsChannels": false,
        "SupportsNotifications": false,
        "SupportsCustomBranding": true,
        "SupportsTranscoding": true,
        "SupportsHlsTranscoding": true,
        "SupportsDirectPlay": true,
        "SupportsDirectStream": true,
        "SupportsScheduledTasks": true,
        "SupportsPublicAccess": true,
        "SupportsRemoteSearch": true,
        "SupportsMetadataFetching": true,
        "SupportsSubtitleFetching": true,
        "SupportsBackupRestore": true,
        "Version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn item_types() -> Json<Value> {
    // Emby 官方 BaseItemKind 枚举的常用子集（排除已声明不实现的域）。
    Json(json!([
        "AggregateFolder",
        "BoxSet",
        "CollectionFolder",
        "Episode",
        "Folder",
        "Genre",
        "Movie",
        "Person",
        "Season",
        "Series",
        "Studio",
        "Tag",
        "Trailer",
        "UserRootFolder",
        "UserView",
        "Video",
        "Year"
    ]))
}

async fn stream_languages() -> Json<Value> {
    // 常用音轨 / 字幕语种。ISO 639-2 三字代码 + 显示名。
    Json(json!([
        { "Name": "English", "Value": "eng" },
        { "Name": "Chinese", "Value": "chi" },
        { "Name": "Japanese", "Value": "jpn" },
        { "Name": "Korean", "Value": "kor" },
        { "Name": "French", "Value": "fre" },
        { "Name": "German", "Value": "ger" },
        { "Name": "Spanish", "Value": "spa" },
        { "Name": "Italian", "Value": "ita" },
        { "Name": "Russian", "Value": "rus" },
        { "Name": "Portuguese", "Value": "por" },
        { "Name": "Thai", "Value": "tha" },
        { "Name": "Vietnamese", "Value": "vie" },
        { "Name": "Arabic", "Value": "ara" },
        { "Name": "Hindi", "Value": "hin" },
        { "Name": "Indonesian", "Value": "ind" },
        { "Name": "Hebrew", "Value": "heb" },
        { "Name": "Turkish", "Value": "tur" },
        { "Name": "Polish", "Value": "pol" },
        { "Name": "Dutch", "Value": "dut" },
        { "Name": "Cantonese", "Value": "yue" }
    ]))
}

async fn extended_video_types() -> Json<Value> {
    Json(json!([
        { "Name": "3D", "Value": "3D" },
        { "Name": "HDR", "Value": "HDR" },
        { "Name": "DolbyVision", "Value": "DolbyVision" },
        { "Name": "HDR10", "Value": "HDR10" },
        { "Name": "HDR10Plus", "Value": "HDR10Plus" },
        { "Name": "HLG", "Value": "HLG" }
    ]))
}

async fn library_available_options(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    require_admin(&session)?;
    let encoding = repository::encoding_options(&state.pool, &state.config).await?;
    // Emby 客户端 "库设置" 页面读取的元数据/图片/字幕 provider 选项。
    // 结合当前后端已实现的 Provider。
    Ok(Json(json!({
        "MetadataReaders": [
            { "Name": "Nfo",     "DefaultEnabled": true,  "Order": 0 },
            { "Name": "Tmdb",    "DefaultEnabled": true,  "Order": 1 },
            { "Name": "ImdbId",  "DefaultEnabled": true,  "Order": 2 }
        ],
        "MetadataFetchers": [
            { "Name": "Tmdb",    "DefaultEnabled": true,  "Order": 0 },
            { "Name": "Imdb",    "DefaultEnabled": true,  "Order": 1 }
        ],
        "ImageFetchers": [
            { "Name": "Tmdb", "DefaultEnabled": true, "Order": 0 }
        ],
        "SubtitleFetchers": [
            { "Name": "Local", "DefaultEnabled": true, "Order": 0 }
        ],
        "TypeOptions": [
            {
                "Type": "Movie",
                "MetadataFetchers": ["Tmdb"],
                "ImageFetchers": ["Tmdb"],
                "ImageOptions": [
                    { "Type": "Primary",  "Limit": 1, "MinWidth": 400 },
                    { "Type": "Backdrop", "Limit": 4, "MinWidth": 1280 },
                    { "Type": "Logo",     "Limit": 1, "MinWidth": 400 }
                ]
            },
            {
                "Type": "Series",
                "MetadataFetchers": ["Tmdb"],
                "ImageFetchers": ["Tmdb"],
                "ImageOptions": [
                    { "Type": "Primary",  "Limit": 1, "MinWidth": 400 },
                    { "Type": "Backdrop", "Limit": 4, "MinWidth": 1280 },
                    { "Type": "Banner",   "Limit": 1, "MinWidth": 1000 }
                ]
            },
            {
                "Type": "Season",
                "MetadataFetchers": ["Tmdb"],
                "ImageFetchers": ["Tmdb"],
                "ImageOptions": [
                    { "Type": "Primary", "Limit": 1, "MinWidth": 400 }
                ]
            },
            {
                "Type": "Episode",
                "MetadataFetchers": ["Tmdb"],
                "ImageFetchers": ["Tmdb"],
                "ImageOptions": [
                    { "Type": "Primary", "Limit": 1, "MinWidth": 400 }
                ]
            }
        ],
        "SupportedImageTypes": ["Primary", "Backdrop", "Thumb", "Logo", "Banner", "Art", "Box", "Disc"],
        "DefaultMetadataLanguage": "zh-CN",
        "DefaultMetadataCountryCode": "CN",
        "MediaEncoderPath": encoding.encoder_app_path,
        "EncoderLocationType": encoding.encoder_location_type,
    })))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BitrateTestQuery {
    #[serde(default, alias = "size", alias = "Size")]
    size: Option<u64>,
}

async fn playback_bitrate_test(
    _session: OptionalAuthSession,
    Query(query): Query<BitrateTestQuery>,
) -> Result<impl IntoResponse, AppError> {
    // 客户端用来测带宽。生成随机字节以避开缓存/压缩。
    // 默认 1 MiB，限制最大 100 MiB 防止滥用。
    const DEFAULT_SIZE: u64 = 1024 * 1024;
    const MAX_SIZE: u64 = 100 * 1024 * 1024;
    let requested = query.size.unwrap_or(DEFAULT_SIZE).min(MAX_SIZE) as usize;

    // 用 UUID v4 批量填充，避免额外依赖；每 16 字节从一次 v4 拷贝。
    let mut buffer = Vec::with_capacity(requested);
    while buffer.len() < requested {
        let chunk = *uuid::Uuid::new_v4().as_bytes();
        let remaining = requested - buffer.len();
        buffer.extend_from_slice(&chunk[..remaining.min(chunk.len())]);
    }

    let headers = [
        (header::CONTENT_TYPE, "application/octet-stream"),
        (header::CACHE_CONTROL, "no-store"),
    ];
    Ok((StatusCode::OK, headers, buffer))
}

async fn encoding_codec_defaults() -> Json<Value> {
    Json(json!({
        "h264":  { "Preset": "veryfast", "Crf": 23, "Profile": "high", "Level": "4.1" },
        "hevc":  { "Preset": "veryfast", "Crf": 28, "Profile": "main",  "Level": "5.1" },
        "av1":   { "Preset": "6",        "Crf": 32, "Profile": "main",  "Level": "5.1" },
        "aac":   { "Bitrate": 192 },
        "mp3":   { "Bitrate": 192 },
        "ac3":   { "Bitrate": 384 },
        "eac3":  { "Bitrate": 448 },
        "opus":  { "Bitrate": 160 }
    }))
}

async fn encoding_codec_info_video() -> Json<Value> {
    Json(json!({
        "Decoders": ["h264", "hevc", "av1", "mpeg2video", "mpeg4", "vc1", "vp8", "vp9"],
        "Encoders": ["libx264", "libx265", "libsvtav1", "libvpx-vp9"],
        "HardwareAccelerations": [
            { "Name": "nvenc", "Codecs": ["h264", "hevc", "av1"] },
            { "Name": "qsv",   "Codecs": ["h264", "hevc", "av1"] },
            { "Name": "vaapi", "Codecs": ["h264", "hevc"] },
            { "Name": "videotoolbox", "Codecs": ["h264", "hevc"] }
        ]
    }))
}

async fn encoding_codec_parameters() -> Json<Value> {
    Json(json!([
        {
            "Codec": "h264",
            "Type": "Video",
            "Presets": ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"],
            "Profiles": ["baseline", "main", "high", "high10", "high422", "high444"],
            "Tunes": ["film", "animation", "grain", "stillimage", "fastdecode", "zerolatency"]
        },
        {
            "Codec": "hevc",
            "Type": "Video",
            "Presets": ["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow"],
            "Profiles": ["main", "main10", "main-still-picture"],
            "Tunes": ["psnr", "ssim", "grain", "fastdecode", "zerolatency"]
        },
        {
            "Codec": "av1",
            "Type": "Video",
            "Presets": ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13"],
            "Profiles": ["main", "high", "professional"],
            "Tunes": []
        }
    ]))
}

async fn encoding_ffmpeg_options(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    require_admin(&session)?;
    let encoding = repository::encoding_options(&state.pool, &state.config).await?;
    Ok(Json(json!({
        "EncoderPath": encoding.encoder_app_path,
        "EncoderLocationType": encoding.encoder_location_type,
        "ProbeSizeBytes": 5_000_000,
        "AnalyzeDurationMs": 5_000,
        "HardwareAccelerationType": encoding.hardware_acceleration_type,
        "EnableThrottling": encoding.enable_throttling,
        "EncoderAppPath": encoding.encoder_app_path,
        "VaapiDevice": encoding.vaapi_device,
        "EncodingThreadCount": encoding.encoding_thread_count,
        "DownMixAudioBoost": encoding.down_mix_audio_boost,
        "H264Preset": encoding.h264_preset,
        "H264Crf": encoding.h264_crf,
        "TranscodingTempPath": encoding.transcoding_temp_path,
        "MaxTranscodeSessions": encoding.max_transcode_sessions,
        "EnableTranscoding": encoding.enable_transcoding,
    })))
}

async fn encoding_full_tonemap_options() -> Json<Value> {
    Json(json!({
        "Algorithms": ["none", "clip", "linear", "gamma", "reinhard", "hable", "mobius"],
        "Ranges": ["auto", "tv", "pc"],
        "DefaultAlgorithm": "hable",
        "PeakLuminance": 100,
        "Desaturation": 0.75,
        "Threshold": 0.8,
        "Param": 0.5,
        "MaxBoost": 1.5,
        "MaxContrast": 1.5,
    }))
}

async fn encoding_public_tonemap_options() -> Json<Value> {
    Json(json!({
        "Algorithms": ["none", "clip", "linear", "reinhard", "hable", "mobius"],
        "DefaultAlgorithm": "hable",
    }))
}

async fn encoding_subtitle_options() -> Json<Value> {
    Json(json!({
        "BurnInSupportedFormats": ["srt", "ass", "ssa", "sub", "idx", "pgs", "dvbsub", "dvdsub"],
        "DefaultMethod": "Encode",
        "Encode": true,
        "EmbedSupportedContainers": ["mkv", "mp4", "ts", "m4v"],
        "ExtractionSupported": true,
        "FallbackLanguages": ["eng", "chi"],
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BackupRestoreRequest {
    #[serde(default, alias = "path", alias = "Path")]
    path: Option<String>,
    #[serde(default, alias = "mode", alias = "Mode")]
    mode: Option<String>,
}

async fn backup_info(
    session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Value>, AppError> {
    require_admin(&session)?;
    let startup = repository::startup_configuration(&state.pool, &state.config).await?;
    let last_backup_at = repository::get_setting_value(&state.pool, "backup:last_at").await?;
    let last_backup_status =
        repository::get_setting_value(&state.pool, "backup:last_status").await?;
    let last_backup_path = repository::get_setting_value(&state.pool, "backup:last_path").await?;
    Ok(Json(json!({
        "ServerName": startup.server_name,
        "ServerVersion": env!("CARGO_PKG_VERSION"),
        "LastBackupAt": last_backup_at,
        "LastBackupStatus": last_backup_status,
        "LastBackupPath": last_backup_path,
        "SupportedModes": ["Database", "Full"],
    })))
}

async fn backup_restore_trigger(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<BackupRestoreRequest>,
) -> Result<Json<Value>, AppError> {
    require_admin(&session)?;
    let mode = payload.mode.as_deref().unwrap_or("Database").to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let backup_dir = state.config.log_dir.parent().unwrap_or(&state.config.log_dir).join("backups");
    std::fs::create_dir_all(&backup_dir).map_err(AppError::Io)?;
    let filename = format!("movie_rust_backup_{}.sql", chrono::Utc::now().format("%Y%m%d_%H%M%S"));
    let backup_path = payload
        .path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| backup_dir.join(&filename));

    let db_url = state.config.database_url.clone();
    let pool = state.pool.clone();
    let backup_path_str = backup_path.to_string_lossy().to_string();
    let backup_path_display = backup_path_str.clone();

    repository::set_setting_value(&pool, "backup:last_at", json!(now)).await?;
    repository::set_setting_value(&pool, "backup:last_status", json!("running")).await?;
    repository::set_setting_value(&pool, "backup:last_path", json!(&backup_path_str)).await?;

    tokio::spawn(async move {
        let result = run_pg_dump(&db_url, &backup_path).await;
        let status = if result.is_ok() { "success" } else { "failed" };
        let _ = repository::set_setting_value(&pool, "backup:last_status", json!(status)).await;
        if let Err(e) = result {
            tracing::error!("pg_dump 失败: {e}");
        } else {
            tracing::info!("数据库备份完成: {backup_path_str}");
        }
    });

    Ok(Json(json!({
        "Status": "running",
        "Mode": mode,
        "RequestedAt": now,
        "BackupPath": backup_path_display,
    })))
}

async fn run_pg_dump(db_url: &str, output_path: &std::path::Path) -> Result<(), AppError> {
    let output = tokio::process::Command::new("pg_dump")
        .arg("--clean")
        .arg("--if-exists")
        .arg("--no-owner")
        .arg("--no-privileges")
        .arg("-f")
        .arg(output_path.as_os_str())
        .arg(db_url)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("无法执行 pg_dump: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!("pg_dump 退出码 {}: {stderr}", output.status)));
    }
    Ok(())
}

async fn run_pg_restore(db_url: &str, input_path: &std::path::Path) -> Result<(), AppError> {
    let output = tokio::process::Command::new("psql")
        .arg("-f")
        .arg(input_path.as_os_str())
        .arg(db_url)
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("无法执行 psql: {e}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!("psql 退出码 {}: {stderr}", output.status)));
    }
    Ok(())
}

async fn backup_restore_data(
    session: AuthSession,
    State(state): State<AppState>,
    Json(payload): Json<BackupRestoreRequest>,
) -> Result<Json<Value>, AppError> {
    require_admin(&session)?;
    let now = chrono::Utc::now().to_rfc3339();

    let restore_path = payload
        .path
        .as_deref()
        .filter(|p| !p.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("缺少 path 参数".to_string()))?;
    let restore_file = std::path::PathBuf::from(restore_path);
    if !restore_file.exists() {
        return Err(AppError::NotFound(format!("备份文件不存在: {restore_path}")));
    }

    let db_url = state.config.database_url.clone();
    let pool = state.pool.clone();
    let restore_path_owned = restore_path.to_string();

    repository::set_setting_value(&pool, "restore:last_at", json!(now)).await?;
    repository::set_setting_value(&pool, "restore:last_status", json!("running")).await?;
    repository::set_setting_value(&pool, "restore:last_path", json!(&restore_path_owned)).await?;

    tokio::spawn(async move {
        let result = run_pg_restore(&db_url, &std::path::PathBuf::from(&restore_path_owned)).await;
        let status = if result.is_ok() { "success" } else { "failed" };
        let _ = repository::set_setting_value(&pool, "restore:last_status", json!(status)).await;
        if let Err(e) = result {
            tracing::error!("数据库还原失败: {e}");
        } else {
            tracing::info!("数据库还原完成: {restore_path_owned}");
        }
    });

    Ok(Json(json!({
        "Status": "running",
        "Mode": payload.mode.unwrap_or_else(|| "Database".to_string()),
        "RequestedAt": now,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn misc_router_builds_without_conflicts() {
        let _ = router();
    }

    #[tokio::test]
    async fn server_features_declares_declared_capabilities() {
        let Json(value) = server_features(OptionalAuthSession(None)).await;
        assert_eq!(value["SupportsLiveTv"], json!(false));
        assert_eq!(value["SupportsDlna"], json!(false));
        assert_eq!(value["SupportsPlugins"], json!(false));
        assert_eq!(value["SupportsCollections"], json!(true));
        assert_eq!(value["SupportsBackupRestore"], json!(true));
    }

    #[tokio::test]
    async fn item_types_exclude_music_and_game_domains() {
        let Json(value) = item_types().await;
        let types: Vec<String> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert!(types.iter().any(|t| t == "Movie"));
        assert!(types.iter().any(|t| t == "Series"));
        assert!(types.iter().any(|t| t == "BoxSet"));
        assert!(!types.iter().any(|t| t == "MusicAlbum"));
        assert!(!types.iter().any(|t| t == "Game"));
    }

    #[tokio::test]
    async fn stream_languages_include_major_locales() {
        let Json(value) = stream_languages().await;
        let values: Vec<String> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["Value"].as_str().unwrap().to_string())
            .collect();
        for expected in ["eng", "chi", "jpn", "kor"] {
            assert!(values.iter().any(|v| v == expected), "missing {expected}");
        }
    }
}
