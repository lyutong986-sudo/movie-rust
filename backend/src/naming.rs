use chrono::{Datelike, NaiveDate};
use regex::Regex;
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use url::Url;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "3g2", "3gp", "asf", "avi", "divx", "dv", "dvr-ms", "f4v", "flv", "ifo", "iso", "m2t", "m2ts",
    "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "mts", "ogm", "ogv", "rm", "rmvb", "strm", "tp",
    "ts", "vob", "webm", "wmv",
];

pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

pub const SUBTITLE_EXTENSIONS: &[&str] = &[
    "ass", "mks", "sami", "smi", "srt", "ssa", "sub", "sup", "vtt",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedMediaName {
    pub title: String,
    pub production_year: Option<i32>,
    pub series_name: Option<String>,
    pub season_number: Option<i32>,
    pub episode_number: Option<i32>,
    pub ending_episode_number: Option<i32>,
    pub premiere_date: Option<NaiveDate>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleFile {
    pub path: PathBuf,
    pub language: Option<String>,
    pub format: String,
    pub title: String,
}

pub fn parse_media_path(path: &Path) -> ParsedMediaName {
    let raw_name = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("Untitled");
    let parent_name = path
        .parent()
        .and_then(Path::file_name)
        .and_then(OsStr::to_str)
        .map(clean_title);

    let quality = infer_quality(raw_name);
    let video_codec = infer_video_codec(raw_name);
    let audio_codec = infer_audio_codec(raw_name);

    if let Some(episode) = parse_numbered_episode(raw_name, parent_name.as_deref()) {
        return ParsedMediaName {
            title: episode.title,
            production_year: extract_year(raw_name),
            series_name: episode.series_name,
            season_number: Some(episode.season_number),
            episode_number: Some(episode.episode_number),
            ending_episode_number: episode.ending_episode_number,
            premiere_date: None,
            width: quality.map(|value| value.0),
            height: quality.map(|value| value.1),
            video_codec,
            audio_codec,
        };
    }

    if let Some(episode) = parse_dated_episode(raw_name, parent_name.as_deref()) {
        return ParsedMediaName {
            title: episode.title,
            production_year: Some(episode.date.year()),
            series_name: episode.series_name,
            season_number: Some(episode.date.year()),
            episode_number: Some(episode.date.ordinal() as i32),
            ending_episode_number: None,
            premiere_date: Some(episode.date),
            width: quality.map(|value| value.0),
            height: quality.map(|value| value.1),
            video_codec,
            audio_codec,
        };
    }

    ParsedMediaName {
        title: clean_title(raw_name),
        production_year: extract_year(raw_name),
        series_name: None,
        season_number: None,
        episode_number: None,
        ending_episode_number: None,
        premiere_date: None,
        width: quality.map(|value| value.0),
        height: quality.map(|value| value.1),
        video_codec,
        audio_codec,
    }
}

pub fn clean_display_name(raw: &str) -> String {
    clean_title(raw)
}

pub fn is_video(path: &Path) -> bool {
    extension_matches(path, VIDEO_EXTENSIONS)
}

pub fn is_strm(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("strm"))
}

pub fn is_subtitle(path: &Path) -> bool {
    extension_matches(path, SUBTITLE_EXTENSIONS)
}

pub fn read_strm_target(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    strm_target_from_text(&content)
}

pub fn strm_target_from_text(content: &str) -> Option<String> {
    let valid_protocols = [
        "http://", "https://", "rtsp://", "rtp://", "rtmp://", "mms://",
    ];

    content
        .lines()
        .map(str::trim)
        .find(|line| {
            !line.is_empty()
                && !line.starts_with('#')
                && valid_protocols.iter().any(|proto| line.starts_with(proto))
        })
        .map(ToOwned::to_owned)
}

pub fn extension_from_url(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    let path = Path::new(url.path());
    path.extension()
        .and_then(OsStr::to_str)
        .map(str::to_lowercase)
        .filter(|extension| !extension.is_empty())
}

pub fn find_sidecar_image(video: &Path) -> Option<PathBuf> {
    let parent = video.parent()?;
    let stem = video.file_stem()?.to_string_lossy();

    let mut candidates = Vec::new();
    for extension in IMAGE_EXTENSIONS {
        candidates.push(parent.join(format!("{stem}.{extension}")));
        candidates.push(parent.join(format!("{stem}-poster.{extension}")));
        candidates.push(parent.join(format!("{stem}.poster.{extension}")));
        candidates.push(parent.join(format!("{stem}-thumb.{extension}")));
        candidates.push(parent.join(format!("{stem}.thumb.{extension}")));
    }

    candidates.extend(folder_image_candidates(parent));
    candidates.into_iter().find(|candidate| candidate.exists())
}

pub fn find_folder_image(folder: &Path) -> Option<PathBuf> {
    folder_image_candidates(folder)
        .into_iter()
        .find(|candidate| candidate.exists())
}

pub fn find_backdrop_image(folder: &Path) -> Option<PathBuf> {
    folder_backdrop_candidates(folder)
        .into_iter()
        .find(|candidate| candidate.exists())
}

pub fn sidecar_subtitles(video: &Path) -> Vec<SubtitleFile> {
    let Some(parent) = video.parent() else {
        return Vec::new();
    };
    let Some(stem) = video.file_stem().and_then(OsStr::to_str) else {
        return Vec::new();
    };

    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };

    let stem_lower = stem.to_lowercase();
    let mut files = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_subtitle(path))
        .filter_map(|path| {
            let file_stem = path.file_stem()?.to_string_lossy().to_string();
            let file_stem_lower = file_stem.to_lowercase();

            if file_stem_lower != stem_lower
                && !file_stem_lower
                    .strip_prefix(&stem_lower)
                    .is_some_and(|suffix| suffix.starts_with('.') || suffix.starts_with('-'))
            {
                return None;
            }

            let format = path.extension()?.to_string_lossy().to_lowercase();
            let language = subtitle_language(stem, &file_stem);
            let title = language
                .as_deref()
                .map(|value| format!("{value} {format}"))
                .unwrap_or_else(|| format!("External {format}"));

            Some(SubtitleFile {
                path,
                language,
                format,
                title,
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

fn extension_matches(path: &Path, candidates: &[&str]) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|extension| {
            candidates
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(extension))
        })
        .unwrap_or(false)
}

fn folder_image_candidates(folder: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for name in [
        "poster", "folder", "cover", "series", "tvshow", "movie", "season",
    ] {
        for extension in IMAGE_EXTENSIONS {
            candidates.push(folder.join(format!("{name}.{extension}")));
        }
    }
    candidates
}

fn folder_backdrop_candidates(folder: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for name in ["backdrop", "fanart", "background", "landscape"] {
        for extension in IMAGE_EXTENSIONS {
            candidates.push(folder.join(format!("{name}.{extension}")));
        }
    }
    candidates
}

struct NumberedEpisode {
    title: String,
    series_name: Option<String>,
    season_number: i32,
    episode_number: i32,
    ending_episode_number: Option<i32>,
}

fn parse_numbered_episode(raw_name: &str, parent_name: Option<&str>) -> Option<NumberedEpisode> {
    let patterns = [sxe_regex(), nx_regex()];

    for regex in patterns {
        let Some(captures) = regex.captures(raw_name) else {
            continue;
        };
        let matched = captures.get(0)?;
        let season_number = captures.name("season")?.as_str().parse().ok()?;
        let episode_number = captures.name("episode")?.as_str().parse().ok()?;
        let ending_episode_number = captures
            .name("ending")
            .and_then(|value| value.as_str().parse().ok());

        let series_name = captures
            .name("series")
            .map(|value| clean_title(value.as_str()))
            .filter(|value| !value.is_empty())
            .or_else(|| parent_name.map(ToOwned::to_owned));

        let suffix = raw_name
            .get(matched.end()..)
            .unwrap_or_default()
            .trim_matches(|value: char| value.is_whitespace() || matches!(value, '.' | '_' | '-'));
        let suffix = strip_continuation_episode_tokens(suffix);
        let title = if suffix.is_empty() {
            format!("Episode {episode_number}")
        } else {
            clean_title(&suffix)
        };

        return Some(NumberedEpisode {
            title,
            series_name,
            season_number,
            episode_number,
            ending_episode_number,
        });
    }

    None
}

struct DatedEpisode {
    title: String,
    series_name: Option<String>,
    date: NaiveDate,
}

fn parse_dated_episode(raw_name: &str, parent_name: Option<&str>) -> Option<DatedEpisode> {
    let captures = date_episode_regex().captures(raw_name)?;
    let year = captures.name("year")?.as_str().parse().ok()?;
    let month = captures.name("month")?.as_str().parse().ok()?;
    let day = captures.name("day")?.as_str().parse().ok()?;
    let date = NaiveDate::from_ymd_opt(year, month, day)?;
    let matched = captures.get(0)?;

    let series_name = captures
        .name("series")
        .map(|value| clean_title(value.as_str()))
        .filter(|value| !value.is_empty())
        .or_else(|| parent_name.map(ToOwned::to_owned));

    let suffix = raw_name
        .get(matched.end()..)
        .unwrap_or_default()
        .trim_matches(|value: char| value.is_whitespace() || matches!(value, '.' | '_' | '-'));
    let title = if suffix.is_empty() {
        date.to_string()
    } else {
        clean_title(suffix)
    };

    Some(DatedEpisode {
        title,
        series_name,
        date,
    })
}

fn clean_title(raw: &str) -> String {
    let without_brackets = bracket_regex().replace_all(raw, " ");
    let without_year = remove_release_year(&without_brackets);
    let without_tokens = clean_token_regex().replace_all(&without_year, " ");
    without_tokens
        .replace(['.', '_'], " ")
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn extract_year(raw: &str) -> Option<i32> {
    year_regex()
        .captures_iter(raw)
        .last()
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse().ok())
}

fn remove_release_year(raw: &str) -> String {
    let mut value = raw.to_string();
    let Some(year_match) = year_regex().find_iter(raw).last() else {
        return value;
    };

    value.replace_range(year_match.start()..year_match.end(), " ");
    value
}

fn infer_quality(raw: &str) -> Option<(i32, i32)> {
    let lower = raw.to_lowercase();
    if lower.contains("4320p") || lower.contains("8k") {
        Some((7680, 4320))
    } else if lower.contains("2160p") || lower.contains("4k") || lower.contains("uhd") {
        Some((3840, 2160))
    } else if lower.contains("1080p") {
        Some((1920, 1080))
    } else if lower.contains("720p") {
        Some((1280, 720))
    } else if lower.contains("576p") {
        Some((1024, 576))
    } else if lower.contains("480p") {
        Some((854, 480))
    } else {
        None
    }
}

fn infer_video_codec(raw: &str) -> Option<String> {
    let lower = raw.to_lowercase();
    [
        (["x265", "h265", "hevc"].as_slice(), "hevc"),
        (["x264", "h264", "avc"].as_slice(), "h264"),
        (["av1"].as_slice(), "av1"),
        (["vp9"].as_slice(), "vp9"),
        (["mpeg2"].as_slice(), "mpeg2video"),
    ]
    .into_iter()
    .find_map(|(tokens, codec)| {
        tokens
            .iter()
            .any(|token| lower.contains(token))
            .then(|| codec.to_string())
    })
}

fn infer_audio_codec(raw: &str) -> Option<String> {
    let lower = raw.to_lowercase();
    [
        (["truehd"].as_slice(), "truehd"),
        (["eac3", "e-ac3", "ddp"].as_slice(), "eac3"),
        (["ac3", "dd5"].as_slice(), "ac3"),
        (["dts-hd", "dtshd", "dts"].as_slice(), "dts"),
        (["aac"].as_slice(), "aac"),
        (["flac"].as_slice(), "flac"),
        (["mp3"].as_slice(), "mp3"),
    ]
    .into_iter()
    .find_map(|(tokens, codec)| {
        tokens
            .iter()
            .any(|token| lower.contains(token))
            .then(|| codec.to_string())
    })
}

fn subtitle_language(video_stem: &str, subtitle_stem: &str) -> Option<String> {
    let suffix = subtitle_stem.strip_prefix(video_stem)?;
    let normalized = suffix
        .trim_start_matches(['.', '-', '_'])
        .split(['.', '-', '_'])
        .next()
        .unwrap_or_default()
        .trim();

    if (2..=8).contains(&normalized.len())
        && normalized.chars().all(|value| value.is_ascii_alphabetic())
    {
        Some(normalized.to_lowercase())
    } else {
        None
    }
}

fn sxe_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)
            ^
            (?P<series>.*?)
            [ ._\-\[]*
            s(?P<season>\d{1,4})
            [ ._\-]*
            e(?P<episode>\d{1,3})
            (?:
                [ ._\-]*
                (?:e|x|-|to)?
                [ ._\-]*
                (?P<ending>\d{1,3})
            )?
            ",
        )
        .expect("valid SxE regex")
    })
}

fn nx_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)
            ^
            (?P<series>.*?)
            [ ._\-\[]*
            (?P<season>\d{1,4})x(?P<episode>\d{1,3})
            (?:
                [ ._\-]*
                (?:x|-|to)?
                [ ._\-]*
                (?P<ending>\d{1,3})
            )?
            ",
        )
        .expect("valid Nx regex")
    })
}

fn continuation_episode_token_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?ix)^(?:[ ._\-]*(?:s\d{1,4})?[ ._\-]*(?:e|x|-|to)?[ ._\-]*\d{1,3})+")
            .expect("valid continuation episode token regex")
    })
}

fn strip_continuation_episode_tokens(value: &str) -> String {
    let stripped = continuation_episode_token_regex().replace(value, "");
    stripped
        .into_owned()
        .trim_matches(|ch: char| ch.is_whitespace() || matches!(ch, '.' | '_' | '-'))
        .to_string()
}

fn date_episode_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)^(?P<series>.*?)[ ._\-\[]*(?P<year>19\d{2}|20\d{2})[ ._\-](?P<month>\d{1,2})[ ._\-](?P<day>\d{1,2})",
        )
        .expect("valid date episode regex")
    })
}

fn bracket_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[\[\(\{][^\]\)\}]{1,80}[\]\)\}]").expect("valid bracket regex")
    })
}

fn year_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\b(19\d{2}|20\d{2})\b").expect("valid year regex"))
}

fn clean_token_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(
            r"(?ix)\b(3d|4k|8k|hdr10?|hdr|uhd|dv|dolby[ ._-]?vision|bluray|blu[ ._-]?ray|brrip|bdrip|web[ ._-]?dl|webrip|hdtv|dvdrip|dvd|remux|proper|repack|x264|h264|avc|x265|h265|hevc|av1|vp9|aac|ac3|eac3|ddp?|dts(?:[ ._-]?hd)?|truehd|atmos|flac|mp3|480p|576p|720p|1080p|2160p|4320p|10bit|8bit)\b",
        )
        .expect("valid clean token regex")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_movie_title_year_and_quality() {
        let parsed = parse_media_path(Path::new("Inception.2010.2160p.UHD.x265.mkv"));
        assert_eq!(parsed.title, "Inception");
        assert_eq!(parsed.production_year, Some(2010));
        assert_eq!(parsed.height, Some(2160));
        assert_eq!(parsed.video_codec.as_deref(), Some("hevc"));
    }

    #[test]
    fn parses_sxe_episode() {
        let parsed = parse_media_path(Path::new("Show.Name.S02E03.The.Test.1080p.mkv"));
        assert_eq!(parsed.series_name.as_deref(), Some("Show Name"));
        assert_eq!(parsed.season_number, Some(2));
        assert_eq!(parsed.episode_number, Some(3));
        assert_eq!(parsed.title, "The Test");
    }

    #[test]
    fn parses_multi_episode_sxe_with_dash() {
        let parsed = parse_media_path(Path::new("Show.Name.S01E02-E03.The.Test.mkv"));
        assert_eq!(parsed.series_name.as_deref(), Some("Show Name"));
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(2));
        assert_eq!(parsed.ending_episode_number, Some(3));
    }

    #[test]
    fn parses_multi_episode_nx_with_x() {
        let parsed = parse_media_path(Path::new("Show Name 01x02x03 episode name.mkv"));
        assert_eq!(parsed.series_name.as_deref(), Some("Show Name"));
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(2));
        assert_eq!(parsed.ending_episode_number, Some(3));
    }

    #[test]
    fn parses_dated_style_syear_episode() {
        let parsed = parse_media_path(Path::new("Running Man S2017E368.mkv"));
        assert_eq!(parsed.series_name.as_deref(), Some("Running Man"));
        assert_eq!(parsed.season_number, Some(2017));
        assert_eq!(parsed.episode_number, Some(368));
    }

    #[test]
    fn strips_extra_multi_episode_tokens_from_title() {
        let parsed = parse_media_path(Path::new("Elementary - 02x03x04x15 - Ep Name.mp4"));
        assert_eq!(parsed.series_name.as_deref(), Some("Elementary"));
        assert_eq!(parsed.season_number, Some(2));
        assert_eq!(parsed.episode_number, Some(3));
        assert_eq!(parsed.title, "Ep Name");
    }

    #[test]
    fn parses_multi_episode_sxe_compact() {
        let parsed = parse_media_path(Path::new("Show.Name.S01E02E03.mkv"));
        assert_eq!(parsed.series_name.as_deref(), Some("Show Name"));
        assert_eq!(parsed.season_number, Some(1));
        assert_eq!(parsed.episode_number, Some(2));
        assert_eq!(parsed.ending_episode_number, Some(3));
    }

    #[test]
    fn strips_dash_episode_suffix_tokens_from_title() {
        let parsed = parse_media_path(Path::new("Elementary - 02x03-E15 - Ep Name.mp4"));
        assert_eq!(parsed.series_name.as_deref(), Some("Elementary"));
        assert_eq!(parsed.season_number, Some(2));
        assert_eq!(parsed.episode_number, Some(3));
        assert_eq!(parsed.title, "Ep Name");
    }
}
