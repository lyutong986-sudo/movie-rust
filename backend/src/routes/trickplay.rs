use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, StatusCode},
    response::Response,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use uuid::Uuid;

use crate::{
    auth::AuthSession,
    error::AppError,
    models::emby_id_to_uuid,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/Items/{itemId}/Trickplay", get(get_trickplay_info))
        .route("/items/{itemId}/trickplay", get(get_trickplay_info))
        .route(
            "/Videos/{itemId}/Trickplay/{width}/{tileFile}",
            get(get_trickplay_tile),
        )
        .route(
            "/videos/{itemId}/trickplay/{width}/{tileFile}",
            get(get_trickplay_tile),
        )
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct TrickplayResolutionInfo {
    width: i32,
    height: i32,
    tile_width: i32,
    tile_height: i32,
    tile_count: i32,
    interval: i32,
    bandwidth: i32,
}

async fn get_trickplay_info(
    State(state): State<AppState>,
    Path(item_id_str): Path<String>,
    _session: AuthSession,
) -> Result<Json<Value>, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;

    let rows = sqlx::query_as::<_, (i32, i32, i32, i32, i32, i32, i32)>(
        r#"SELECT width, height, tile_width, tile_height, thumb_count, interval_ms, bandwidth
           FROM trickplay_info WHERE item_id = $1 ORDER BY width"#,
    )
    .bind(item_id)
    .fetch_all(&state.pool)
    .await?;

    let mut resolutions: BTreeMap<String, TrickplayResolutionInfo> = BTreeMap::new();
    for (width, height, tw, th, count, interval, bw) in rows {
        resolutions.insert(
            width.to_string(),
            TrickplayResolutionInfo {
                width,
                height,
                tile_width: tw,
                tile_height: th,
                tile_count: count,
                interval,
                bandwidth: bw,
            },
        );
    }

    Ok(Json(json!({ "Resolutions": resolutions })))
}

async fn get_trickplay_tile(
    State(state): State<AppState>,
    Path((item_id_str, width, tile_file)): Path<(String, i32, String)>,
) -> Result<Response, AppError> {
    let item_id = emby_id_to_uuid(&item_id_str)
        .map_err(|_| AppError::BadRequest(format!("无效的项目ID格式: {item_id_str}")))?;
    let tile_index: i32 = tile_file
        .trim_end_matches(".jpg")
        .parse()
        .map_err(|_| AppError::BadRequest("无效的瓦片索引".to_string()))?;

    let row = sqlx::query_as::<_, (Vec<u8>,)>(
        "SELECT data FROM trickplay_tiles WHERE item_id = $1 AND width = $2 AND tile_index = $3",
    )
    .bind(item_id)
    .bind(width)
    .bind(tile_index)
    .fetch_optional(&state.pool)
    .await?;

    match row {
        Some((data,)) => Ok(Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "public, max-age=604800")
            .body(Body::from(data))
            .unwrap()),
        None => Err(AppError::NotFound("Trickplay 瓦片不存在".to_string())),
    }
}

pub async fn generate_trickplay(
    pool: &sqlx::PgPool,
    item_id: Uuid,
    video_path: &str,
    ffmpeg_path: &str,
) -> Result<(), AppError> {
    let interval_sec = 10;
    let thumb_width = 320;
    let tile_cols = 10;
    let tile_rows = 10;

    let temp_dir = tempfile::tempdir()
        .map_err(|e| AppError::Internal(format!("创建临时目录失败: {e}")))?;

    let output = tokio::process::Command::new(ffmpeg_path)
        .args([
            "-i",
            video_path,
            "-vf",
            &format!("fps=1/{interval_sec},scale={thumb_width}:-1"),
            "-q:v",
            "8",
            &format!("{}/thumb_%06d.jpg", temp_dir.path().display()),
        ])
        .output()
        .await
        .map_err(|e| AppError::Internal(format!("执行 ffmpeg 失败: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(format!(
            "ffmpeg 生成缩略图失败: {stderr}"
        )));
    }

    let mut thumb_files: Vec<std::path::PathBuf> = std::fs::read_dir(temp_dir.path())
        .map_err(AppError::Io)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map_or(false, |ext| ext == "jpg"))
        .collect();
    thumb_files.sort();

    if thumb_files.is_empty() {
        return Ok(());
    }

    let first_img = image::open(&thumb_files[0])
        .map_err(|e| AppError::Internal(format!("读取缩略图失败: {e}")))?;
    let single_w = first_img.width();
    let single_h = first_img.height();

    let thumbs_per_tile = tile_cols * tile_rows;
    let tile_count = (thumb_files.len() as i32 + thumbs_per_tile - 1) / thumbs_per_tile;

    sqlx::query("DELETE FROM trickplay_info WHERE item_id = $1 AND width = $2")
        .bind(item_id)
        .bind(thumb_width)
        .execute(pool)
        .await?;

    sqlx::query(
        r#"INSERT INTO trickplay_info (item_id, width, height, tile_width, tile_height, thumb_count, interval_ms)
           VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
    )
    .bind(item_id)
    .bind(thumb_width)
    .bind(single_h as i32)
    .bind(tile_cols)
    .bind(tile_rows)
    .bind(thumb_files.len() as i32)
    .bind(interval_sec * 1000)
    .execute(pool)
    .await?;

    for tile_idx in 0..tile_count {
        let start = (tile_idx * thumbs_per_tile) as usize;
        let end = std::cmp::min(start + thumbs_per_tile as usize, thumb_files.len());
        let chunk = &thumb_files[start..end];

        let tile_w = single_w * tile_cols as u32;
        let tile_h = single_h * tile_rows as u32;
        let mut tile_img = image::RgbImage::new(tile_w, tile_h);

        for (i, thumb_path) in chunk.iter().enumerate() {
            let row = i as u32 / tile_cols as u32;
            let col = i as u32 % tile_cols as u32;
            if let Ok(thumb) = image::open(thumb_path) {
                let thumb_rgb = thumb.to_rgb8();
                image::imageops::overlay(
                    &mut tile_img,
                    &thumb_rgb,
                    (col * single_w) as i64,
                    (row * single_h) as i64,
                );
            }
        }

        let mut jpeg_data = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut jpeg_data);
        image::DynamicImage::ImageRgb8(tile_img)
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|e| AppError::Internal(format!("编码瓦片图失败: {e}")))?;

        sqlx::query(
            "INSERT INTO trickplay_tiles (item_id, width, tile_index, data) VALUES ($1, $2, $3, $4)
             ON CONFLICT (item_id, width, tile_index) DO UPDATE SET data = EXCLUDED.data",
        )
        .bind(item_id)
        .bind(thumb_width)
        .bind(tile_idx)
        .bind(&jpeg_data)
        .execute(pool)
        .await?;
    }

    tracing::info!(
        "Trickplay 生成完成: item_id={}, 共 {} 帧, {} 瓦片",
        item_id,
        thumb_files.len(),
        tile_count
    );
    Ok(())
}
