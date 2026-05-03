//! Jellyfin/Kodi 风格的 NFO 写入器。
//!
//! 严格对照 `MediaBrowser.XbmcMetadata.Savers.{SeriesNfoSaver, SeasonNfoSaver,
//! EpisodeNfoSaver, MovieNfoSaver}` 中的标签集合：
//! - Series → `<series_dir>/tvshow.nfo`，根 `<tvshow>`
//! - Season → `<season_dir>/season.nfo`，根 `<season>`
//! - Episode → 与视频同名 `*.nfo`，根 `<episodedetails>`
//! - Movie → 与视频同名 `*.nfo` 或同目录 `movie.nfo`，根 `<movie>`
//!
//! 仅在 `LibraryOptions.save_local_metadata` 为真时调用。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sqlx::PgPool;
use tokio::fs;
use uuid::Uuid;

use crate::error::AppError;
use crate::models::DbMediaItem;

/// 一条人物记录（NFO 输出用）。
#[derive(Debug, Clone)]
pub struct NfoPerson {
    pub name: String,
    pub role_type: String,
    pub role: Option<String>,
}

async fn load_people(pool: &PgPool, item_id: Uuid) -> Result<Vec<NfoPerson>, AppError> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        r#"
        SELECT p.name, pr.role_type, pr.role
        FROM person_roles pr
        INNER JOIN persons p ON p.id = pr.person_id
        WHERE pr.media_item_id = $1
        ORDER BY
            CASE pr.role_type
                WHEN 'Actor'     THEN 0
                WHEN 'GuestStar' THEN 0
                WHEN 'Director'  THEN 1
                WHEN 'Writer'    THEN 2
                WHEN 'Producer'  THEN 3
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
        .map(|(name, role_type, role)| NfoPerson {
            name,
            role_type,
            role,
        })
        .collect())
}

/// 写入 Movie 的 NFO（与视频文件同名）。如果 path 是目录则写 `movie.nfo`。
pub async fn write_movie_nfo(pool: &PgPool, item: &DbMediaItem) -> Result<PathBuf, AppError> {
    let people = load_people(pool, item.id).await?;
    let path = movie_nfo_path(&item.path);
    let xml = build_movie_xml(item, &people);
    write_xml_file(&path, &xml).await?;
    Ok(path)
}

/// 写入 Series 的 NFO（`<series_dir>/tvshow.nfo`）。
pub async fn write_series_nfo(pool: &PgPool, item: &DbMediaItem) -> Result<PathBuf, AppError> {
    let people = load_people(pool, item.id).await?;
    let series_dir = PathBuf::from(&item.path);
    let path = series_dir.join("tvshow.nfo");
    let xml = build_series_xml(item, &people);
    write_xml_file(&path, &xml).await?;
    Ok(path)
}

/// 写入 Season 的 NFO（`<season_dir>/season.nfo`）。
pub async fn write_season_nfo(pool: &PgPool, item: &DbMediaItem) -> Result<PathBuf, AppError> {
    let people = load_people(pool, item.id).await?;
    let season_dir = PathBuf::from(&item.path);
    let path = season_dir.join("season.nfo");
    let xml = build_season_xml(item, &people);
    write_xml_file(&path, &xml).await?;
    Ok(path)
}

/// 写入 Episode 的 NFO（与视频同名 sidecar）。
pub async fn write_episode_nfo(pool: &PgPool, item: &DbMediaItem) -> Result<PathBuf, AppError> {
    let people = load_people(pool, item.id).await?;
    let video_path = PathBuf::from(&item.path);
    let path = video_path.with_extension("nfo");
    let xml = build_episode_xml(item, &people);
    write_xml_file(&path, &xml).await?;
    Ok(path)
}

fn movie_nfo_path(item_path: &str) -> PathBuf {
    let buf = PathBuf::from(item_path);
    if buf.is_dir() {
        buf.join("movie.nfo")
    } else {
        buf.with_extension("nfo")
    }
}

async fn write_xml_file(path: &Path, xml: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await.map_err(AppError::Io)?;
    }
    fs::write(path, xml).await.map_err(AppError::Io)?;
    Ok(())
}

fn build_movie_xml(item: &DbMediaItem, people: &[NfoPerson]) -> String {
    let mut out = String::new();
    write_xml_header(&mut out);
    out.push_str("<movie>\n");
    write_common_movie_series_tags(&mut out, item);
    write_people(&mut out, people);
    out.push_str("</movie>\n");
    out
}

fn build_series_xml(item: &DbMediaItem, people: &[NfoPerson]) -> String {
    let mut out = String::new();
    write_xml_header(&mut out);
    out.push_str("<tvshow>\n");
    write_common_movie_series_tags(&mut out, item);
    if let Some(status) = &item.status {
        write_element(&mut out, "status", status, 1);
    }
    write_people(&mut out, people);
    out.push_str("</tvshow>\n");
    out
}

fn build_season_xml(item: &DbMediaItem, _people: &[NfoPerson]) -> String {
    let mut out = String::new();
    write_xml_header(&mut out);
    out.push_str("<season>\n");
    write_element(&mut out, "title", &item.name, 1);
    if let Some(plot) = &item.overview {
        write_element(&mut out, "plot", plot, 1);
    }
    if let Some(year) = item.production_year {
        write_element(&mut out, "year", &year.to_string(), 1);
    }
    if let Some(season_no) = item.index_number {
        write_element(&mut out, "seasonnumber", &season_no.to_string(), 1);
    }
    write_provider_ids(&mut out, &item.provider_ids);
    out.push_str("</season>\n");
    out
}

fn build_episode_xml(item: &DbMediaItem, people: &[NfoPerson]) -> String {
    let mut out = String::new();
    write_xml_header(&mut out);
    out.push_str("<episodedetails>\n");
    write_element(&mut out, "title", &item.name, 1);
    if let Some(original) = &item.original_title {
        if !original.is_empty() {
            write_element(&mut out, "originaltitle", original, 1);
        }
    }
    if let Some(plot) = &item.overview {
        write_element(&mut out, "plot", plot, 1);
    }
    if let Some(rating) = item.community_rating {
        write_element(&mut out, "rating", &format_float(rating), 1);
    }
    if let Some(season_no) = item.parent_index_number {
        write_element(&mut out, "season", &season_no.to_string(), 1);
    }
    if let Some(ep_no) = item.index_number {
        write_element(&mut out, "episode", &ep_no.to_string(), 1);
    }
    if let Some(ep_end) = item.index_number_end {
        write_element(&mut out, "episodenumberend", &ep_end.to_string(), 1);
    }
    if let Some(series_name) = &item.series_name {
        write_element(&mut out, "showtitle", series_name, 1);
    }
    if let Some(date) = item.premiere_date {
        write_element(
            &mut out,
            "aired",
            &date.format("%Y-%m-%d").to_string(),
            1,
        );
    }
    if let Some(ticks) = item.runtime_ticks {
        let minutes = ticks / 600_000_000;
        if minutes > 0 {
            write_element(&mut out, "runtime", &minutes.to_string(), 1);
        }
    }
    write_provider_ids(&mut out, &item.provider_ids);
    write_people(&mut out, people);
    out.push_str("</episodedetails>\n");
    out
}

fn write_common_movie_series_tags(out: &mut String, item: &DbMediaItem) {
    write_element(out, "title", &item.name, 1);
    if let Some(original) = &item.original_title {
        if !original.is_empty() {
            write_element(out, "originaltitle", original, 1);
        }
    }
    if !item.sort_name.is_empty() && item.sort_name != item.name.to_lowercase() {
        write_element(out, "sorttitle", &item.sort_name, 1);
    }
    if let Some(plot) = &item.overview {
        write_element(out, "plot", plot, 1);
    }
    if let Some(year) = item.production_year {
        write_element(out, "year", &year.to_string(), 1);
    }
    if let Some(date) = item.premiere_date {
        write_element(out, "premiered", &date.format("%Y-%m-%d").to_string(), 1);
    }
    if let Some(date) = item.end_date {
        write_element(out, "enddate", &date.format("%Y-%m-%d").to_string(), 1);
    }
    if let Some(rating) = item.community_rating {
        write_element(out, "rating", &format_float(rating), 1);
    }
    if let Some(rating) = item.critic_rating {
        write_element(out, "criticrating", &format_float(rating), 1);
    }
    if let Some(mpaa) = &item.official_rating {
        write_element(out, "mpaa", mpaa, 1);
    }
    if let Some(ticks) = item.runtime_ticks {
        let minutes = ticks / 600_000_000;
        if minutes > 0 {
            write_element(out, "runtime", &minutes.to_string(), 1);
        }
    }
    for genre in &item.genres {
        write_element(out, "genre", genre, 1);
    }
    for studio in &item.studios {
        write_element(out, "studio", studio, 1);
    }
    for tag in &item.tags {
        write_element(out, "tag", tag, 1);
    }
    write_provider_ids(out, &item.provider_ids);
    let _ = writeln!(
        out,
        "  <dateadded>{}</dateadded>",
        item.date_created.format("%Y-%m-%d %H:%M:%S")
    );
}

fn write_provider_ids(out: &mut String, value: &Value) {
    let Some(map) = value.as_object() else {
        return;
    };
    for (key, val) in map {
        let id = match val {
            Value::String(s) => s.trim().to_string(),
            Value::Number(n) => n.to_string(),
            _ => continue,
        };
        if id.is_empty() {
            continue;
        }
        match key.to_ascii_lowercase().as_str() {
            "imdb" => {
                write_element(out, "imdb_id", &id, 1);
                write_unique_id(out, "imdb", &id);
            }
            "tmdb" => {
                write_element(out, "tmdbid", &id, 1);
                write_unique_id(out, "tmdb", &id);
            }
            "tvdb" => {
                write_element(out, "tvdbid", &id, 1);
                write_unique_id(out, "tvdb", &id);
            }
            other => {
                write_unique_id(out, other, &id);
            }
        }
    }
}

fn write_unique_id(out: &mut String, scheme: &str, value: &str) {
    let _ = writeln!(
        out,
        "  <uniqueid type=\"{}\">{}</uniqueid>",
        xml_escape(scheme),
        xml_escape(value)
    );
}

fn write_people(out: &mut String, people: &[NfoPerson]) {
    for p in people {
        match p.role_type.as_str() {
            "Director" => write_element(out, "director", &p.name, 1),
            "Writer" => write_element(out, "credits", &p.name, 1),
            "Producer" => write_element(out, "producer", &p.name, 1),
            _ => {
                out.push_str("  <actor>\n");
                let _ = writeln!(out, "    <name>{}</name>", xml_escape(&p.name));
                if let Some(role) = &p.role {
                    if !role.is_empty() {
                        let _ = writeln!(out, "    <role>{}</role>", xml_escape(role));
                    }
                }
                out.push_str("  </actor>\n");
            }
        }
    }
}

fn write_xml_header(out: &mut String) {
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
}

fn write_element(out: &mut String, tag: &str, value: &str, indent: usize) {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return;
    }
    let pad = "  ".repeat(indent);
    let _ = writeln!(out, "{pad}<{tag}>{}</{tag}>", xml_escape(trimmed));
}

fn format_float(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            ch if (ch as u32) < 0x20 && ch != '\n' && ch != '\r' && ch != '\t' => {
                // 跳过 XML 1.0 不允许的控制字符
            }
            ch => out.push(ch),
        }
    }
    out
}
