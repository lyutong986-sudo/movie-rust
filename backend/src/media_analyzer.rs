use serde::{Deserialize, Serialize};
use std::{ffi::OsStr, path::Path};
use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Error)]
pub enum MediaAnalyzerError {
    #[error("ffprobe 执行失败: {0}")]
    FfprobeError(String),
    #[error("ffprobe 输出解析失败: {0}")]
    ParseError(String),
    #[error("文件不存在: {0}")]
    FileNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaStreamInfo {
    pub index: i32,
    pub codec_type: String,
    pub codec_name: Option<String>,
    pub codec_long_name: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub bit_rate: Option<String>,
    pub channels: Option<i32>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<String>,
    pub language: Option<String>,
    pub title: Option<String>,
    pub profile: Option<String>,
    pub average_frame_rate: Option<f32>,
    pub real_frame_rate: Option<f32>,
    pub aspect_ratio: Option<String>,
    pub is_default: bool,
    pub is_forced: bool,
    pub is_hearing_impaired: bool,
    pub is_interlaced: bool,
    pub color_range: Option<String>,
    pub color_space: Option<String>,
    pub color_transfer: Option<String>,
    pub color_primaries: Option<String>,
    pub level: Option<i32>,
    pub pixel_format: Option<String>,
    pub ref_frames: Option<i32>,
    pub stream_start_time_ticks: Option<i64>,
    pub attachment_size: Option<i32>,
    pub extended_video_sub_type: Option<String>,
    pub extended_video_sub_type_description: Option<String>,
    pub extended_video_type: Option<String>,
    pub is_anamorphic: Option<bool>,
    pub is_avc: Option<bool>,
    pub is_external_url: Option<String>,
    pub is_text_subtitle_stream: Option<bool>,
    pub tags: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormatInfo {
    pub filename: String,
    pub format_name: Option<String>,
    pub format_long_name: Option<String>,
    pub duration: Option<String>,
    pub size: Option<String>,
    pub bit_rate: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaChapterInfo {
    pub chapter_index: i32,
    pub start_position_ticks: i64,
    pub name: Option<String>,
    pub marker_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaAnalysisResult {
    pub streams: Vec<MediaStreamInfo>,
    pub chapters: Vec<MediaChapterInfo>,
    pub format: MediaFormatInfo,
}

pub async fn analyze_media_file(path: &Path) -> Result<MediaAnalysisResult, MediaAnalyzerError> {
    if !path.exists() {
        return Err(MediaAnalyzerError::FileNotFound(
            path.to_string_lossy().to_string(),
        ));
    }

    let probe_result = run_ffprobe(path.as_os_str()).await?;
    parse_probe_result(&probe_result)
}

/// 分析远程媒体URL
pub async fn analyze_remote_media(url: &str) -> Result<MediaAnalysisResult, MediaAnalyzerError> {
    let probe_result = run_ffprobe(url).await?;
    parse_probe_result(&probe_result)
}

async fn run_ffprobe(target: impl AsRef<OsStr>) -> Result<serde_json::Value, MediaAnalyzerError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
        ])
        .arg(target)
        .output()
        .await
        .map_err(|e| MediaAnalyzerError::FfprobeError(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MediaAnalyzerError::FfprobeError(format!(
            "ffprobe 失败: {}",
            stderr
        )));
    }

    let json_output = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(&json_output).map_err(|e| MediaAnalyzerError::ParseError(e.to_string()))
}

fn parse_probe_result(
    probe_result: &serde_json::Value,
) -> Result<MediaAnalysisResult, MediaAnalyzerError> {
    let streams = probe_result
        .get("streams")
        .and_then(|v| v.as_array())
        .ok_or_else(|| MediaAnalyzerError::ParseError("缺少 streams 字段".to_string()))?;

    let format = probe_result
        .get("format")
        .ok_or_else(|| MediaAnalyzerError::ParseError("缺少 format 字段".to_string()))?;

    let stream_infos: Vec<MediaStreamInfo> = streams
        .iter()
        .filter_map(|stream| {
            let index = stream.get("index")?.as_i64()? as i32;
            let codec_type = stream.get("codec_type")?.as_str()?.to_string();
            let codec_name = stream
                .get("codec_name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let codec_long_name = stream
                .get("codec_long_name")
                .and_then(|v| v.as_str())
                .map(String::from);
            let width = stream
                .get("width")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            let height = stream
                .get("height")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            let bit_rate = stream
                .get("bit_rate")
                .and_then(|v| v.as_str())
                .map(String::from);
            let channels = stream
                .get("channels")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            let channel_layout = stream
                .get("channel_layout")
                .and_then(|v| v.as_str())
                .map(String::from);
            let sample_rate = stream
                .get("sample_rate")
                .and_then(|v| v.as_str())
                .map(String::from);
            let language = stream
                .get("tags")
                .and_then(|tags| tags.get("language"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let title = stream
                .get("tags")
                .and_then(|tags| tags.get("title"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let tags = stream.get("tags").cloned();
            let disposition = stream.get("disposition");

            let level = stream
                .get("level")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            let profile = stream
                .get("profile")
                .and_then(|v| v.as_str())
                .map(String::from);
            let pixel_format = stream
                .get("pix_fmt")
                .and_then(|v| v.as_str())
                .map(String::from);
            let ref_frames = stream
                .get("refs")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            let average_frame_rate =
                parse_ffprobe_rational(stream.get("avg_frame_rate").and_then(|v| v.as_str()));
            let real_frame_rate =
                parse_ffprobe_rational(stream.get("r_frame_rate").and_then(|v| v.as_str()));
            let aspect_ratio = stream
                .get("display_aspect_ratio")
                .and_then(|v| v.as_str())
                .map(String::from);
            let is_default = disposition
                .and_then(|value| value.get("default"))
                .and_then(|v| v.as_i64())
                .map(|v| v != 0)
                .unwrap_or(false);
            let is_forced = disposition
                .and_then(|value| value.get("forced"))
                .and_then(|v| v.as_i64())
                .map(|v| v != 0)
                .unwrap_or(false);
            let is_hearing_impaired = disposition
                .and_then(|value| value.get("hearing_impaired"))
                .and_then(|v| v.as_i64())
                .map(|v| v != 0)
                .unwrap_or(false);
            let is_interlaced = stream
                .get("field_order")
                .and_then(|v| v.as_str())
                .map(|value| value != "progressive" && value != "unknown")
                .unwrap_or(false);
            let stream_start_time_ticks = stream
                .get("start_time")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .map(|seconds| (seconds * 10_000_000.0) as i64);
            let color_range = stream
                .get("color_range")
                .and_then(|v| v.as_str())
                .map(String::from);
            let color_space = stream
                .get("color_space")
                .and_then(|v| v.as_str())
                .map(String::from);
            let color_transfer = stream
                .get("color_transfer")
                .and_then(|v| v.as_str())
                .map(String::from);
            let color_primaries = stream
                .get("color_primaries")
                .and_then(|v| v.as_str())
                .map(String::from);
            let attachment_size = stream
                .get("tags")
                .and_then(|tags| tags.get("attachment_size"))
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            let extended_video_sub_type = stream
                .get("tags")
                .and_then(|tags| tags.get("extended_video_sub_type"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let extended_video_sub_type_description = stream
                .get("tags")
                .and_then(|tags| tags.get("extended_video_sub_type_description"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let extended_video_type = stream
                .get("tags")
                .and_then(|tags| tags.get("extended_video_type"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let is_anamorphic = stream
                .get("tags")
                .and_then(|tags| tags.get("is_anamorphic"))
                .and_then(|v| v.as_bool());
            let is_avc = stream
                .get("tags")
                .and_then(|tags| tags.get("is_avc"))
                .and_then(|v| v.as_bool());
            let is_external_url = stream
                .get("tags")
                .and_then(|tags| tags.get("is_external_url"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let is_text_subtitle_stream = stream
                .get("tags")
                .and_then(|tags| tags.get("is_text_subtitle_stream"))
                .and_then(|v| v.as_bool());

            Some(MediaStreamInfo {
                index,
                codec_type,
                codec_name,
                codec_long_name,
                width,
                height,
                bit_rate,
                channels,
                channel_layout,
                sample_rate,
                language,
                title,
                profile,
                average_frame_rate,
                real_frame_rate,
                aspect_ratio,
                is_default,
                is_forced,
                is_hearing_impaired,
                is_interlaced,
                color_range,
                color_space,
                color_transfer,
                color_primaries,
                level,
                pixel_format,
                ref_frames,
                stream_start_time_ticks,
                attachment_size,
                extended_video_sub_type,
                extended_video_sub_type_description,
                extended_video_type,
                is_anamorphic,
                is_avc,
                is_external_url,
                is_text_subtitle_stream,
                tags,
            })
        })
        .collect();

    let chapter_infos: Vec<MediaChapterInfo> = probe_result
        .get("chapters")
        .and_then(|v| v.as_array())
        .map(|chapters| {
            chapters
                .iter()
                .enumerate()
                .filter_map(|(position, chapter)| {
                    let start_position_ticks = chapter
                        .get("start_time")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                        .map(|seconds| (seconds * 10_000_000.0).round() as i64)
                        .or_else(|| chapter.get("start").and_then(|v| v.as_i64()))
                        .unwrap_or(0);
                    let tags = chapter.get("tags");
                    let name = tags
                        .and_then(|value| value.get("title"))
                        .and_then(|v| v.as_str())
                        .map(String::from)
                        .or_else(|| Some(format!("第 {:02} 章", position + 1)));

                    Some(MediaChapterInfo {
                        chapter_index: chapter
                            .get("id")
                            .and_then(|v| v.as_i64())
                            .map(|v| v as i32)
                            .unwrap_or(position as i32),
                        start_position_ticks,
                        name,
                        marker_type: Some("Chapter".to_string()),
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let format_info = MediaFormatInfo {
        filename: format
            .get("filename")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_default(),
        format_name: format
            .get("format_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        format_long_name: format
            .get("format_long_name")
            .and_then(|v| v.as_str())
            .map(String::from),
        duration: format
            .get("duration")
            .and_then(|v| v.as_str())
            .map(String::from),
        size: format
            .get("size")
            .and_then(|v| v.as_str())
            .map(String::from),
        bit_rate: format
            .get("bit_rate")
            .and_then(|v| v.as_str())
            .map(String::from),
    };

    Ok(MediaAnalysisResult {
        streams: stream_infos,
        chapters: chapter_infos,
        format: format_info,
    })
}

fn parse_ffprobe_rational(value: Option<&str>) -> Option<f32> {
    let value = value?;
    if value.trim().is_empty() || value == "0/0" {
        return None;
    }
    if let Some((num, den)) = value.split_once('/') {
        let num = num.parse::<f32>().ok()?;
        let den = den.parse::<f32>().ok()?;
        if den == 0.0 {
            return None;
        }
        return Some(num / den);
    }
    value.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_analyze_media_file() {
        let test_file = PathBuf::from("test.mp4");
        if !test_file.exists() {
            println!("测试文件不存在，跳过测试");
            return;
        }
        let result = analyze_media_file(&test_file).await;
        assert!(result.is_ok());
        let result = result.unwrap();
        assert!(!result.streams.is_empty());
        println!("分析结果: {:?}", result);
    }
}
