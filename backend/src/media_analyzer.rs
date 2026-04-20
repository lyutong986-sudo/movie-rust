use serde::{Deserialize, Serialize};
use std::path::Path;
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
    #[error("未找到视频流")]
    NoVideoStream,
    #[error("未找到音频流")]
    NoAudioStream,
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
pub struct MediaAnalysisResult {
    pub streams: Vec<MediaStreamInfo>,
    pub format: MediaFormatInfo,
}

pub async fn analyze_media_file(path: &Path) -> Result<MediaAnalysisResult, MediaAnalyzerError> {
    if !path.exists() {
        return Err(MediaAnalyzerError::FileNotFound(
            path.to_string_lossy().to_string(),
        ));
    }

    let output = Command::new("ffprobe")
        .args(&[
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            path.to_str().unwrap(),
        ])
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
    let probe_result: serde_json::Value = serde_json::from_str(&json_output)
        .map_err(|e| MediaAnalyzerError::ParseError(e.to_string()))?;

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
            let codec_name = stream.get("codec_name").and_then(|v| v.as_str()).map(String::from);
            let codec_long_name = stream.get("codec_long_name").and_then(|v| v.as_str()).map(String::from);
            let width = stream.get("width").and_then(|v| v.as_i64()).map(|v| v as i32);
            let height = stream.get("height").and_then(|v| v.as_i64()).map(|v| v as i32);
            let bit_rate = stream.get("bit_rate").and_then(|v| v.as_str()).map(String::from);
            let channels = stream.get("channels").and_then(|v| v.as_i64()).map(|v| v as i32);
            let channel_layout = stream.get("channel_layout").and_then(|v| v.as_str()).map(String::from);
            let sample_rate = stream.get("sample_rate").and_then(|v| v.as_str()).map(String::from);
            let language = stream.get("tags")
                .and_then(|tags| tags.get("language"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let tags = stream.get("tags").cloned();

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
                tags,
            })
        })
        .collect();

    let format_info = MediaFormatInfo {
        filename: format.get("filename").and_then(|v| v.as_str()).map(String::from).unwrap_or_default(),
        format_name: format.get("format_name").and_then(|v| v.as_str()).map(String::from),
        format_long_name: format.get("format_long_name").and_then(|v| v.as_str()).map(String::from),
        duration: format.get("duration").and_then(|v| v.as_str()).map(String::from),
        size: format.get("size").and_then(|v| v.as_str()).map(String::from),
        bit_rate: format.get("bit_rate").and_then(|v| v.as_str()).map(String::from),
    };

    Ok(MediaAnalysisResult {
        streams: stream_infos,
        format: format_info,
    })
}

pub fn extract_primary_video_stream(result: &MediaAnalysisResult) -> Option<&MediaStreamInfo> {
    result.streams.iter().find(|s| s.codec_type == "video")
}

pub fn extract_audio_streams(result: &MediaAnalysisResult) -> Vec<&MediaStreamInfo> {
    result.streams.iter().filter(|s| s.codec_type == "audio").collect()
}

pub fn extract_subtitle_streams(result: &MediaAnalysisResult) -> Vec<&MediaStreamInfo> {
    result.streams.iter().filter(|s| s.codec_type == "subtitle").collect()
}

/// 分析远程媒体URL
pub async fn analyze_remote_media(url: &str) -> Result<MediaAnalysisResult, MediaAnalyzerError> {
    let output = Command::new("ffprobe")
        .args(&[
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            url,
        ])
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
    let probe_result: serde_json::Value = serde_json::from_str(&json_output)
        .map_err(|e| MediaAnalyzerError::ParseError(e.to_string()))?;

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
            let codec_name = stream.get("codec_name").and_then(|v| v.as_str()).map(String::from);
            let codec_long_name = stream.get("codec_long_name").and_then(|v| v.as_str()).map(String::from);
            let width = stream.get("width").and_then(|v| v.as_i64()).map(|v| v as i32);
            let height = stream.get("height").and_then(|v| v.as_i64()).map(|v| v as i32);
            let bit_rate = stream.get("bit_rate").and_then(|v| v.as_str()).map(String::from);
            let channels = stream.get("channels").and_then(|v| v.as_i64()).map(|v| v as i32);
            let channel_layout = stream.get("channel_layout").and_then(|v| v.as_str()).map(String::from);
            let sample_rate = stream.get("sample_rate").and_then(|v| v.as_str()).map(String::from);
            let language = stream.get("tags")
                .and_then(|tags| tags.get("language"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let tags = stream.get("tags").cloned();

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
                tags,
            })
        })
        .collect();

    let format_info = MediaFormatInfo {
        filename: format.get("filename").and_then(|v| v.as_str()).map(String::from).unwrap_or_default(),
        format_name: format.get("format_name").and_then(|v| v.as_str()).map(String::from),
        format_long_name: format.get("format_long_name").and_then(|v| v.as_str()).map(String::from),
        duration: format.get("duration").and_then(|v| v.as_str()).map(String::from),
        size: format.get("size").and_then(|v| v.as_str()).map(String::from),
        bit_rate: format.get("bit_rate").and_then(|v| v.as_str()).map(String::from),
    };

    Ok(MediaAnalysisResult {
        streams: stream_infos,
        format: format_info,
    })
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