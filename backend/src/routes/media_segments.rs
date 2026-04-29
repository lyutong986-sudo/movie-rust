use axum::{
    extract::{Path, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::{self, AuthSession},
    error::AppError,
    models::emby_id_to_uuid,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/MediaSegments/{itemId}", get(get_segments))
        .route("/mediasegments/{itemId}", get(get_segments))
        .route("/MediaSegments/{itemId}", post(set_segments))
        .route("/mediasegments/{itemId}", post(set_segments))
        .route("/MediaSegments/{itemId}/{segmentId}", delete(delete_segment))
        .route("/mediasegments/{itemId}/{segmentId}", delete(delete_segment))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
#[serde(rename_all = "PascalCase")]
pub struct MediaSegmentDto {
    pub id: Uuid,
    pub item_id: Uuid,
    #[serde(rename = "Type")]
    pub segment_type: String,
    pub start_ticks: i64,
    pub end_ticks: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CreateSegmentRequest {
    #[serde(rename = "Type", alias = "type", alias = "SegmentType")]
    pub segment_type: String,
    pub start_ticks: i64,
    pub end_ticks: i64,
}

async fn get_segments(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    _session: AuthSession,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;

    let segments = sqlx::query_as::<_, MediaSegmentDto>(
        r#"SELECT id, item_id, segment_type, start_ticks, end_ticks
           FROM media_segments WHERE item_id = $1 ORDER BY start_ticks"#,
    )
    .bind(item_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(json!({ "Items": segments, "TotalRecordCount": segments.len() })))
}

async fn set_segments(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    session: AuthSession,
    Json(segments): Json<Vec<CreateSegmentRequest>>,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;

    sqlx::query("DELETE FROM media_segments WHERE item_id = $1")
        .bind(item_id)
        .execute(&state.pool)
        .await?;

    let mut result = Vec::new();
    for seg in segments {
        let id = Uuid::new_v4();
        sqlx::query(
            r#"INSERT INTO media_segments (id, item_id, segment_type, start_ticks, end_ticks)
               VALUES ($1, $2, $3, $4, $5)"#,
        )
        .bind(id)
        .bind(item_id)
        .bind(&seg.segment_type)
        .bind(seg.start_ticks)
        .bind(seg.end_ticks)
        .execute(&state.pool)
        .await?;
        result.push(MediaSegmentDto {
            id,
            item_id,
            segment_type: seg.segment_type,
            start_ticks: seg.start_ticks,
            end_ticks: seg.end_ticks,
        });
    }

    Ok(Json(json!({ "Items": result, "TotalRecordCount": result.len() })))
}

async fn delete_segment(
    State(state): State<AppState>,
    Path((item_id_str, segment_id_str)): Path<(String, String)>,
    session: AuthSession,
) -> Result<Json<Value>, AppError> {
    auth::require_admin(&session)?;
    let _item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let segment_id = emby_id_to_uuid(&segment_id_str)
        .map_err(|_| AppError::BadRequest("无效的分段ID格式".to_string()))?;

    sqlx::query("DELETE FROM media_segments WHERE id = $1")
        .bind(segment_id)
        .execute(&state.pool)
        .await?;

    Ok(Json(json!({ "Status": "deleted" })))
}

/// Detect black frames at the beginning and end of a video to guess intro/outro boundaries.
/// Uses FFmpeg blackdetect filter. Returns (intro_end_ticks, outro_start_ticks) if detected.
pub async fn detect_segments(
    ffmpeg_path: &str,
    video_path: &str,
    runtime_ticks: i64,
) -> Option<Vec<(String, i64, i64)>> {
    let duration_secs = runtime_ticks as f64 / 10_000_000.0;
    if duration_secs < 120.0 {
        return None;
    }

    let mut segments = Vec::new();

    // Scan first 5 minutes for intro
    let scan_start = tokio::process::Command::new(ffmpeg_path)
        .args([
            "-i", video_path,
            "-t", "300",
            "-vf", "blackdetect=d=0.5:pix_th=0.10",
            "-an", "-f", "null", "-",
        ])
        .output()
        .await
        .ok()?;

    let stderr = String::from_utf8_lossy(&scan_start.stderr);
    if let Some(intro_end) = parse_last_black_end(&stderr, 180.0) {
        if intro_end > 5.0 {
            segments.push((
                "Intro".to_string(),
                0i64,
                (intro_end * 10_000_000.0) as i64,
            ));
        }
    }

    // Scan last 5 minutes for outro
    let skip_to = (duration_secs - 300.0).max(0.0);
    let scan_end = tokio::process::Command::new(ffmpeg_path)
        .args([
            "-ss", &format!("{skip_to:.1}"),
            "-i", video_path,
            "-vf", "blackdetect=d=0.5:pix_th=0.10",
            "-an", "-f", "null", "-",
        ])
        .output()
        .await
        .ok()?;

    let stderr_end = String::from_utf8_lossy(&scan_end.stderr);
    if let Some(outro_start_relative) = parse_first_black_start(&stderr_end) {
        let outro_start_abs = skip_to + outro_start_relative;
        if (duration_secs - outro_start_abs) > 10.0 && (duration_secs - outro_start_abs) < 240.0 {
            segments.push((
                "Outro".to_string(),
                (outro_start_abs * 10_000_000.0) as i64,
                runtime_ticks,
            ));
        }
    }

    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}

fn parse_last_black_end(stderr: &str, max_time: f64) -> Option<f64> {
    let mut last_end = None;
    for line in stderr.lines() {
        if let Some(idx) = line.find("black_end:") {
            let rest = &line[idx + 10..];
            let end_str = rest.split_whitespace().next()?;
            if let Ok(val) = end_str.parse::<f64>() {
                if val <= max_time {
                    last_end = Some(val);
                }
            }
        }
    }
    last_end
}

fn parse_first_black_start(stderr: &str) -> Option<f64> {
    for line in stderr.lines() {
        if let Some(idx) = line.find("black_start:") {
            let rest = &line[idx + 12..];
            let start_str = rest.split_whitespace().next()?;
            if let Ok(val) = start_str.parse::<f64>() {
                return Some(val);
            }
        }
    }
    None
}
