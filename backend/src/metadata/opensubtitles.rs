use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

const API_BASE: &str = "https://api.opensubtitles.com/api/v1";
const DEFAULT_API_KEY: &str = "gUCLWGoAg2PmyseoTM0INFFVPcDCeDlT";
const USER_AGENT: &str = "MovieRust v0.1.0";

#[derive(Debug)]
pub struct OpenSubtitlesProvider {
    client: Client,
    token: Option<String>,
    api_key: String,
}

#[derive(Debug, Clone)]
pub struct SubtitleSearchResult {
    pub id: String,
    pub name: String,
    pub language: String,
    pub format: String,
    pub provider_name: String,
    pub download_count: i64,
    pub is_hearing_impaired: bool,
    pub comment: String,
    pub file_id: i64,
    pub author: Option<String>,
    pub date_created: Option<String>,
    pub community_rating: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct LoginResponse {
    token: Option<String>,
    status: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    data: Option<Vec<SearchItem>>,
    total_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SearchItem {
    id: Option<String>,
    attributes: Option<SearchAttributes>,
}

#[derive(Debug, Deserialize)]
struct SearchAttributes {
    language: Option<String>,
    download_count: Option<i64>,
    hearing_impaired: Option<bool>,
    release: Option<String>,
    #[serde(default, deserialize_with = "deserialize_comments")]
    comments: Option<String>,
    files: Option<Vec<SearchFile>>,
    feature_details: Option<FeatureDetails>,
    uploader: Option<UploaderInfo>,
    upload_date: Option<String>,
    ratings: Option<f64>,
}

fn deserialize_comments<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de;
    struct CommentsVisitor;
    impl<'de> de::Visitor<'de> for CommentsVisitor {
        type Value = Option<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a string or array of comment objects")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            if v.is_empty() { Ok(None) } else { Ok(Some(v.to_string())) }
        }
        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            if v.is_empty() { Ok(None) } else { Ok(Some(v)) }
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut parts = Vec::new();
            while let Some(item) = seq.next_element::<serde_json::Value>()? {
                if let Some(v) = item.get("value").and_then(|v| v.as_str()) {
                    parts.push(v.to_string());
                }
            }
            if parts.is_empty() { Ok(None) } else { Ok(Some(parts.join("; "))) }
        }
    }
    deserializer.deserialize_any(CommentsVisitor)
}

#[derive(Debug, Deserialize)]
struct SearchFile {
    file_id: Option<i64>,
    file_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UploaderInfo {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FeatureDetails {
    year: Option<i32>,
    title: Option<String>,
    feature_type: Option<String>,
    imdb_id: Option<i64>,
}

impl OpenSubtitlesProvider {
    pub fn new(api_key: &str) -> Self {
        let key = if api_key.is_empty() {
            DEFAULT_API_KEY.to_string()
        } else {
            api_key.to_string()
        };
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .unwrap_or_default();
        Self {
            client,
            token: None,
            api_key: key,
        }
    }

    pub async fn login(&mut self, username: &str, password: &str) -> Result<(), String> {
        let body = serde_json::json!({
            "username": username,
            "password": password,
        });

        let resp = self
            .client
            .post(format!("{API_BASE}/login"))
            .header("Api-Key", &self.api_key)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenSubtitles login request failed: {e}"))?;

        let status = resp.status();
        let raw = resp
            .text()
            .await
            .map_err(|e| format!("OpenSubtitles login read failed: {e}"))?;

        if !status.is_success() {
            tracing::warn!(?status, body = %raw, "OpenSubtitles login HTTP 非 2xx");
            return Err(format!(
                "OpenSubtitles login HTTP {}: {}",
                status,
                raw.chars().take(300).collect::<String>()
            ));
        }

        let login: LoginResponse = serde_json::from_str(&raw).map_err(|e| {
            tracing::warn!(error = %e, body = %raw, "OpenSubtitles login JSON 解析失败");
            format!(
                "OpenSubtitles login parse failed: {} body={}",
                e,
                raw.chars().take(200).collect::<String>()
            )
        })?;

        if let Some(token) = login.token {
            self.token = Some(token);
            Ok(())
        } else {
            tracing::warn!(body = %raw, status = ?login.status, "OpenSubtitles 登录响应缺少 token");
            Err(format!(
                "OpenSubtitles login failed: status={:?} body={}",
                login.status,
                raw.chars().take(300).collect::<String>()
            ))
        }
    }

    pub async fn search_subtitles(
        &self,
        query: &str,
        language: &str,
        imdb_id: Option<&str>,
        year: Option<i32>,
    ) -> Result<Vec<SubtitleSearchResult>, String> {
        let mut params: Vec<(&str, String)> = Vec::new();
        params.push(("query", query.to_string()));

        let lang_code = normalize_language_code(language);
        if !lang_code.is_empty() {
            params.push(("languages", lang_code));
        }

        if let Some(imdb) = imdb_id {
            let normalized = imdb.trim_start_matches("tt");
            if let Ok(id) = normalized.parse::<i64>() {
                params.push(("imdb_id", id.to_string()));
            }
        }

        if let Some(y) = year {
            params.push(("year", y.to_string()));
        }

        let mut request = self
            .client
            .get(format!("{API_BASE}/subtitles"))
            .header("Api-Key", &self.api_key)
            .header("User-Agent", USER_AGENT)
            .header("Content-Type", "application/json");

        if let Some(token) = &self.token {
            request = request.header("Authorization", format!("Bearer {token}"));
        }

        let resp = request
            .query(&params)
            .send()
            .await
            .map_err(|e| format!("OpenSubtitles search failed: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!(
                "OpenSubtitles search HTTP {status}: {body}"
            ));
        }

        let search: SearchResponse = resp
            .json()
            .await
            .map_err(|e| format!("OpenSubtitles search parse failed: {e}"))?;

        let items = search.data.unwrap_or_default();
        let mut results = Vec::with_capacity(items.len());

        for item in items {
            let attrs = match item.attributes {
                Some(a) => a,
                None => continue,
            };

            let file = attrs.files.as_ref().and_then(|f| f.first());
            let file_name = file
                .and_then(|f| f.file_name.clone())
                .unwrap_or_default();
            let file_id = file.and_then(|f| f.file_id).unwrap_or(0);

            let name = if !file_name.is_empty() {
                file_name
            } else {
                attrs.release.clone().unwrap_or_else(|| "Unknown".to_string())
            };

            let format = if name.ends_with(".srt") {
                "srt"
            } else if name.ends_with(".ass") || name.ends_with(".ssa") {
                "ass"
            } else if name.ends_with(".sub") {
                "sub"
            } else {
                "srt"
            }
            .to_string();

            results.push(SubtitleSearchResult {
                id: item.id.unwrap_or_default(),
                name,
                language: attrs.language.unwrap_or_else(|| language.to_string()),
                format,
                provider_name: "OpenSubtitles".to_string(),
                download_count: attrs.download_count.unwrap_or(0),
                is_hearing_impaired: attrs.hearing_impaired.unwrap_or(false),
                comment: attrs.comments.clone().unwrap_or_default(),
                file_id,
                author: attrs.uploader.as_ref().and_then(|u| u.name.clone()),
                date_created: attrs.upload_date.clone(),
                community_rating: attrs.ratings,
            });
        }

        results.sort_by(|a, b| b.download_count.cmp(&a.download_count));

        Ok(results)
    }

    pub async fn download_subtitle(
        &self,
        file_id: i64,
        sub_format: &str,
    ) -> Result<SubtitleDownloadResult, String> {
        let token = self
            .token
            .as_ref()
            .ok_or_else(|| "需要先登录 OpenSubtitles 才能下载字幕".to_string())?;

        let body = serde_json::json!({
            "file_id": file_id,
            "sub_format": sub_format,
        });

        let resp = self
            .client
            .post(format!("{API_BASE}/download"))
            .header("Api-Key", &self.api_key)
            .header("User-Agent", USER_AGENT)
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("OpenSubtitles download request failed: {e}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OpenSubtitles download HTTP {status}: {body}"));
        }

        let dl_info: DownloadResponse = resp
            .json()
            .await
            .map_err(|e| format!("OpenSubtitles download parse failed: {e}"))?;

        let link = dl_info
            .link
            .ok_or_else(|| "OpenSubtitles 未返回下载链接".to_string())?;

        let content_resp = self
            .client
            .get(&link)
            .header("Api-Key", &self.api_key)
            .header("User-Agent", USER_AGENT)
            .send()
            .await
            .map_err(|e| format!("下载字幕文件失败: {e}"))?;

        let content = content_resp
            .text()
            .await
            .map_err(|e| format!("读取字幕内容失败: {e}"))?;

        Ok(SubtitleDownloadResult {
            content,
            format: sub_format.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct SubtitleDownloadResult {
    pub content: String,
    pub format: String,
}

#[derive(Debug, Deserialize)]
struct DownloadResponse {
    link: Option<String>,
    remaining: Option<i32>,
}

fn normalize_language_code(lang: &str) -> String {
    let lang_lower = lang.to_ascii_lowercase();
    let mut map = HashMap::new();
    map.insert("chi", "zh-cn");
    map.insert("zho", "zh-cn");
    map.insert("zh", "zh-cn");
    map.insert("zh-cn", "zh-cn");
    map.insert("zh-tw", "zh-tw");
    map.insert("eng", "en");
    map.insert("en", "en");
    map.insert("jpn", "ja");
    map.insert("ja", "ja");
    map.insert("kor", "ko");
    map.insert("ko", "ko");
    map.insert("fre", "fr");
    map.insert("fra", "fr");
    map.insert("fr", "fr");
    map.insert("ger", "de");
    map.insert("deu", "de");
    map.insert("de", "de");
    map.insert("spa", "es");
    map.insert("es", "es");
    map.insert("por", "pt-pt");
    map.insert("pt", "pt-pt");
    map.insert("rus", "ru");
    map.insert("ru", "ru");
    map.insert("ita", "it");
    map.insert("it", "it");

    map.get(lang_lower.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| lang_lower)
}
