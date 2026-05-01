use crate::error::AppError;
use async_trait::async_trait;
use chrono::Datelike;
use moka::future::Cache;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::{
    models::{
        ExternalMovieMetadata, ExternalPerson, ExternalPersonSearchResult, ExternalSeriesMetadata,
    },
    provider::{
        ExternalEpisodeCatalogItem, ExternalItemPerson, ExternalMediaSearchResult,
        ExternalPersonCredit, ExternalRemoteImage, MetadataProvider,
    },
};

/// TMDB API配置
#[derive(Debug, Clone)]
pub struct TmdbConfig {
    /// TMDB API密钥
    pub api_key: String,
    /// 额外 API 密钥列表（用于轮询以绕过速率限制）
    pub api_keys: Vec<String>,
    /// 基础URL（默认为 https://api.themoviedb.org/3）
    pub base_url: String,
    /// 图片基础URL（默认为 https://image.tmdb.org/t/p）
    pub image_base_url: String,
    /// 语言（默认为 en-US）
    pub language: String,
}

impl Default for TmdbConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_keys: Vec::new(),
            base_url: "https://api.themoviedb.org/3".to_string(),
            image_base_url: "https://image.tmdb.org/t/p".to_string(),
            language: "en-US".to_string(),
        }
    }
}

/// TMDB元数据提供者
pub struct TmdbProvider {
    config: TmdbConfig,
    client: Client,
    key_counter: AtomicUsize,
    /// In-memory response cache (TTL=1h, max 10000 entries)
    json_cache: Arc<Cache<String, Arc<JsonValue>>>,
}

impl TmdbProvider {
    pub fn new_with_preferences(
        api_key: String,
        preferred_metadata_language: &str,
        metadata_country_code: &str,
    ) -> Self {
        let normalized_language = preferred_metadata_language.trim();
        let normalized_country = metadata_country_code.trim().to_ascii_uppercase();
        let language = if normalized_language.is_empty() {
            "en-US".to_string()
        } else if normalized_country.is_empty() {
            normalized_language.to_string()
        } else {
            format!("{normalized_language}-{normalized_country}")
        };

        Self::with_config(TmdbConfig {
            api_key,
            language,
            ..Default::default()
        })
    }

    pub fn new_with_multi_keys(
        api_key: String,
        extra_keys: Vec<String>,
        preferred_metadata_language: &str,
        metadata_country_code: &str,
    ) -> Self {
        let normalized_language = preferred_metadata_language.trim();
        let normalized_country = metadata_country_code.trim().to_ascii_uppercase();
        let language = if normalized_language.is_empty() {
            "en-US".to_string()
        } else if normalized_country.is_empty() {
            normalized_language.to_string()
        } else {
            format!("{normalized_language}-{normalized_country}")
        };

        Self::with_config(TmdbConfig {
            api_key,
            api_keys: extra_keys,
            language,
            ..Default::default()
        })
    }

    /// 使用配置创建TMDB提供者
    pub fn with_config(config: TmdbConfig) -> Self {
        let json_cache = Arc::new(
            Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(3600))
                .build(),
        );
        Self {
            config,
            client: crate::http_client::SHARED.clone(),
            key_counter: AtomicUsize::new(0),
            json_cache,
        }
    }

    /// 轮询获取下一个可用的 API Key
    fn next_api_key(&self) -> &str {
        if self.config.api_keys.is_empty() {
            return &self.config.api_key;
        }
        let total = 1 + self.config.api_keys.len();
        let idx = self.key_counter.fetch_add(1, Ordering::Relaxed) % total;
        if idx == 0 {
            &self.config.api_key
        } else {
            &self.config.api_keys[idx - 1]
        }
    }

    /// 构建API URL
    fn build_url(&self, endpoint: &str) -> String {
        format!("{}{}", self.config.base_url, endpoint)
    }

    fn is_bearer_token_key(key: &str) -> bool {
        key.len() > 40
    }

    fn is_bearer_token(&self) -> bool {
        Self::is_bearer_token_key(&self.config.api_key)
    }

    fn add_api_key(&self, params: &mut HashMap<String, String>) {
        let key = self.next_api_key();
        if !Self::is_bearer_token_key(key) {
            params.insert("api_key".to_string(), key.to_string());
        }
        params.insert("language".to_string(), self.config.language.clone());
    }

    fn auth_get(&self, url: impl reqwest::IntoUrl) -> reqwest::RequestBuilder {
        let key = self.next_api_key();
        let r = self.client.get(url);
        if Self::is_bearer_token_key(key) {
            r.bearer_auth(key)
        } else {
            r
        }
    }

    /// Cached GET: check moka cache first, on miss perform HTTP GET and store 2xx responses.
    ///
    /// PB35-4 (P2-1)：对网络/5xx/429 增加指数退避重试（最多 3 次：300ms/600ms/1200ms）。
    /// 之前任何瞬时网络抖动都会让 do_refresh_item_metadata 直接失败，重新触发整个
    /// 详情页的按需刷新链路；现在通过本地重试可以把绝大多数瞬时错误吸收掉。
    async fn cached_get<T: serde::de::DeserializeOwned>(
        &self,
        cache_key: &str,
        url: &str,
        params: &HashMap<String, String>,
    ) -> Result<T, AppError> {
        if let Some(cached) = self.json_cache.get(cache_key).await {
            return serde_json::from_value((*cached).clone())
                .map_err(|e| AppError::Internal(format!("cache deser: {e}")));
        }

        const MAX_RETRIES: u32 = 3;
        const BASE_DELAY_MS: u64 = 300;
        let mut last_err: Option<AppError> = None;
        for attempt in 0..=MAX_RETRIES {
            match self.auth_get(url).query(params).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let json_val: JsonValue = match response.json().await {
                            Ok(value) => value,
                            Err(error) => {
                                last_err = Some(error.into());
                                break;
                            }
                        };
                        let arc_val = Arc::new(json_val);
                        self.json_cache
                            .insert(cache_key.to_string(), arc_val.clone())
                            .await;
                        return serde_json::from_value((*arc_val).clone())
                            .map_err(|e| AppError::Internal(format!("tmdb deser: {e}")));
                    }
                    if (status.is_server_error()
                        || status == reqwest::StatusCode::TOO_MANY_REQUESTS)
                        && attempt < MAX_RETRIES
                    {
                        let delay = BASE_DELAY_MS << attempt;
                        tracing::warn!(
                            url,
                            status = %status,
                            attempt = attempt + 1,
                            delay_ms = delay,
                            "PB35-4 (P2-1)：TMDB GET 5xx/429，退避后重试"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        continue;
                    }
                    return Err(AppError::Internal(format!(
                        "TMDB GET 失败：HTTP {} for {}",
                        status, url
                    )));
                }
                Err(error) => {
                    if (error.is_timeout() || error.is_connect() || error.is_request())
                        && attempt < MAX_RETRIES
                    {
                        let delay = BASE_DELAY_MS << attempt;
                        tracing::warn!(
                            url,
                            attempt = attempt + 1,
                            delay_ms = delay,
                            error = %error,
                            "PB35-4 (P2-1)：TMDB GET 网络错误，退避后重试"
                        );
                        last_err = Some(error.into());
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        continue;
                    }
                    return Err(error.into());
                }
            }
        }
        Err(last_err.unwrap_or_else(|| AppError::Internal("TMDB GET 失败：重试耗尽".to_string())))
    }

    /// 搜索人物
    async fn search_person_internal(
        &self,
        name: &str,
    ) -> Result<TmdbPersonSearchResponse, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert("query".to_string(), name.to_string());
        params.insert("page".to_string(), "1".to_string());

        let response = self
            .auth_get(self.build_url("/search/person"))
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        let search_result: TmdbPersonSearchResponse = response.json().await?;
        Ok(search_result)
    }

    /// 获取人物详细信息
    async fn get_person_details_internal(
        &self,
        person_id: &str,
    ) -> Result<TmdbPersonDetails, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);

        let url = self.build_url(&format!("/person/{}", person_id));
        let cache_key = format!("person:{person_id}");
        self.cached_get(&cache_key, &url, &params).await
    }

    /// 取人物详情时，若用户首选语言（如 zh-CN）下 biography 为空字符串，
    /// 再用 en-US 请求一次把 biography / place_of_birth 拼回去。
    /// Jellyfin 上游 TmdbPersonProvider 有等价行为：本地化字段缺失时会回退 default。
    async fn get_person_details_with_fallback(
        &self,
        person_id: &str,
    ) -> Result<TmdbPersonDetails, AppError> {
        let mut details = self.get_person_details_internal(person_id).await?;
        let biography_empty = details.biography.trim().is_empty();
        let place_empty = details
            .place_of_birth
            .as_deref()
            .map(|s| s.trim().is_empty())
            .unwrap_or(true);
        if !biography_empty && !place_empty {
            return Ok(details);
        }
        if self.config.language.eq_ignore_ascii_case("en-US") {
            return Ok(details);
        }

        let mut params = HashMap::new();
        if !self.is_bearer_token() {
            // PB11：fallback 也走 next_api_key 多 Key 轮询，避免主路径轮询、回退路径
            // 固定 self.config.api_key 引发的 429 体感不一致。
            params.insert("api_key".to_string(), self.next_api_key().to_string());
        }
        params.insert("language".to_string(), "en-US".to_string());
        let url = self.build_url(&format!("/person/{}", person_id));
        let response = match self.auth_get(&url).query(&params).send().await {
            Ok(r) => r,
            Err(err) => {
                tracing::debug!(person_id, ?err, "fallback en-US 人物详情请求失败");
                return Ok(details);
            }
        };
        let response = match response.error_for_status() {
            Ok(r) => r,
            Err(err) => {
                tracing::debug!(person_id, ?err, "fallback en-US 人物详情非 2xx");
                return Ok(details);
            }
        };
        let en_details: TmdbPersonDetails = match response.json().await {
            Ok(d) => d,
            Err(err) => {
                tracing::debug!(person_id, ?err, "fallback en-US 人物详情 JSON 解析失败");
                return Ok(details);
            }
        };

        if biography_empty && !en_details.biography.trim().is_empty() {
            details.biography = en_details.biography;
        }
        if place_empty {
            if let Some(p) = en_details.place_of_birth {
                if !p.trim().is_empty() {
                    details.place_of_birth = Some(p);
                }
            }
        }
        if details.homepage.is_none() {
            details.homepage = en_details.homepage;
        }
        Ok(details)
    }

    /// 获取人物作品
    async fn get_person_credits_internal(
        &self,
        person_id: &str,
    ) -> Result<TmdbPersonCredits, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);

        let url = self.build_url(&format!("/person/{}/combined_credits", person_id));
        let response = self
            .auth_get(&url)
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        let credits: TmdbPersonCredits = response.json().await?;
        Ok(credits)
    }

    async fn get_movie_details_internal(
        &self,
        movie_id: &str,
    ) -> Result<TmdbMovieDetails, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert(
            "append_to_response".to_string(),
            "external_ids,release_dates,videos,keywords".to_string(),
        );

        let url = self.build_url(&format!("/movie/{movie_id}"));
        let cache_key = format!("movie:{movie_id}");
        self.cached_get(&cache_key, &url, &params).await
    }

    async fn get_tv_details_internal(&self, tv_id: &str) -> Result<TmdbTvDetails, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert(
            "append_to_response".to_string(),
            "external_ids,keywords".to_string(),
        );

        let url = self.build_url(&format!("/tv/{}", tv_id));
        let cache_key = format!("tv:{tv_id}");
        self.cached_get(&cache_key, &url, &params).await
    }

    async fn get_movie_images_internal(
        &self,
        movie_id: &str,
    ) -> Result<TmdbImageCollection, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert(
            "include_image_language".to_string(),
            format!("{},null,en", language_code(&self.config.language)),
        );

        let url = self.build_url(&format!("/movie/{movie_id}/images"));
        let cache_key = format!("movie_img:{movie_id}");
        self.cached_get(&cache_key, &url, &params).await
    }

    async fn get_tv_images_internal(&self, tv_id: &str) -> Result<TmdbImageCollection, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert(
            "include_image_language".to_string(),
            format!("{},null,en", language_code(&self.config.language)),
        );

        let url = self.build_url(&format!("/tv/{tv_id}/images"));
        let cache_key = format!("tv_img:{tv_id}");
        self.cached_get(&cache_key, &url, &params).await
    }

    async fn get_tv_season_details_internal(
        &self,
        tv_id: &str,
        season_number: i32,
    ) -> Result<TmdbSeasonDetails, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);

        let url = self.build_url(&format!("/tv/{tv_id}/season/{season_number}"));
        let cache_key = format!("tv_season:{tv_id}:{season_number}");
        self.cached_get(&cache_key, &url, &params).await
    }

    async fn get_movie_credits_internal(
        &self,
        movie_id: &str,
    ) -> Result<TmdbItemCredits, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);

        let url = self.build_url(&format!("/movie/{movie_id}/credits"));
        let cache_key = format!("movie_credits:{movie_id}");
        self.cached_get(&cache_key, &url, &params).await
    }

    async fn get_tv_credits_internal(&self, tv_id: &str) -> Result<TmdbItemCredits, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);

        let url = self.build_url(&format!("/tv/{tv_id}/credits"));
        let cache_key = format!("tv_credits:{tv_id}");
        self.cached_get(&cache_key, &url, &params).await
    }

    async fn search_movie_internal(
        &self,
        name: &str,
        year: Option<i32>,
    ) -> Result<TmdbMovieSearchResponse, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert("query".to_string(), name.to_string());
        params.insert("page".to_string(), "1".to_string());
        params.insert("include_adult".to_string(), "false".to_string());
        if let Some(year) = year {
            params.insert("year".to_string(), year.to_string());
        }

        let response = self
            .auth_get(self.build_url("/search/movie"))
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json().await?)
    }

    async fn search_tv_internal(
        &self,
        name: &str,
        year: Option<i32>,
    ) -> Result<TmdbTvSearchResponse, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert("query".to_string(), name.to_string());
        params.insert("page".to_string(), "1".to_string());
        params.insert("include_adult".to_string(), "false".to_string());
        if let Some(year) = year {
            params.insert("first_air_date_year".to_string(), year.to_string());
        }

        let response = self
            .auth_get(self.build_url("/search/tv"))
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json().await?)
    }

    fn build_movie_search_result(&self, item: TmdbMovieSearchResult) -> ExternalMediaSearchResult {
        let premiere_date = parse_tmdb_date(item.release_date.as_deref());
        let production_year = premiere_date.map(|date| date.year());
        let image_url = item
            .poster_path
            .as_ref()
            .map(|path| format!("{}/original{}", self.config.image_base_url, path));
        let mut provider_ids = HashMap::new();
        provider_ids.insert("Tmdb".to_string(), item.id.to_string());

        let popularity = item.popularity;
        ExternalMediaSearchResult {
            provider: "tmdb".to_string(),
            external_id: item.id.to_string(),
            name: item
                .title
                .clone()
                .or_else(|| item.original_title.clone())
                .unwrap_or_else(|| format!("TMDb movie {}", item.id)),
            original_name: item.original_title,
            overview: item.overview.filter(|value| !value.trim().is_empty()),
            premiere_date,
            production_year,
            image_url,
            provider_ids,
            popularity,
        }
    }

    fn build_tv_search_result(&self, item: TmdbTvSearchResult) -> ExternalMediaSearchResult {
        let premiere_date = parse_tmdb_date(item.first_air_date.as_deref());
        let production_year = premiere_date.map(|date| date.year());
        let image_url = item
            .poster_path
            .as_ref()
            .map(|path| format!("{}/original{}", self.config.image_base_url, path));
        let mut provider_ids = HashMap::new();
        provider_ids.insert("Tmdb".to_string(), item.id.to_string());

        let popularity = item.popularity;
        ExternalMediaSearchResult {
            provider: "tmdb".to_string(),
            external_id: item.id.to_string(),
            name: item
                .name
                .clone()
                .or_else(|| item.original_name.clone())
                .unwrap_or_else(|| format!("TMDb series {}", item.id)),
            original_name: item.original_name,
            overview: item.overview.filter(|value| !value.trim().is_empty()),
            premiere_date,
            production_year,
            image_url,
            provider_ids,
            popularity,
        }
    }
}

#[async_trait]
impl MetadataProvider for TmdbProvider {
    fn name(&self) -> &str {
        "tmdb"
    }

    async fn search_person(&self, name: &str) -> Result<Vec<ExternalPersonSearchResult>, AppError> {
        let search_result = self.search_person_internal(name).await?;

        let results = search_result
            .results
            .into_iter()
            .map(|person| ExternalPersonSearchResult {
                external_id: person.id.to_string(),
                provider: "tmdb".to_string(),
                name: person.name,
                sort_name: None,
                overview: None, // TMDB搜索不提供简介
                external_url: Some(format!("https://www.themoviedb.org/person/{}", person.id)),
                image_url: person
                    .profile_path
                    .map(|path| format!("{}/original{}", self.config.image_base_url, path)),
                known_for: person
                    .known_for
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|item| item.title.or(item.name))
                    .collect(),
                popularity: Some(person.popularity),
                adult: Some(person.adult),
            })
            .collect();

        Ok(results)
    }

    async fn get_person_details(&self, provider_id: &str) -> Result<ExternalPerson, AppError> {
        let person_details = self.get_person_details_with_fallback(provider_id).await?;

        let mut provider_ids = HashMap::new();
        provider_ids.insert("Tmdb".to_string(), person_details.id.to_string());

        if let Some(imdb_id) = &person_details.imdb_id {
            provider_ids.insert("Imdb".to_string(), imdb_id.clone());
        }

        let birth_date = person_details
            .birthday
            .clone()
            .and_then(|date| chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok());

        let death_date = person_details
            .deathday
            .clone()
            .and_then(|date| chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok());

        let metadata = serde_json::to_value(&person_details).unwrap_or_default();
        let external_person = ExternalPerson {
            external_id: person_details.id.to_string(),
            provider: "tmdb".to_string(),
            name: person_details.name,
            sort_name: None,
            overview: Some(person_details.biography),
            external_url: Some(format!(
                "https://www.themoviedb.org/person/{}",
                person_details.id
            )),
            birth_date,
            death_date,
            place_of_birth: person_details.place_of_birth,
            image_url: person_details
                .profile_path
                .map(|path| format!("{}/original{}", self.config.image_base_url, path)),
            homepage_url: person_details.homepage,
            popularity: Some(person_details.popularity),
            adult: Some(person_details.adult),
            provider_ids,
            metadata,
        };

        Ok(external_person)
    }

    async fn get_person_credits(
        &self,
        provider_id: &str,
    ) -> Result<Vec<ExternalPersonCredit>, AppError> {
        let credits = self.get_person_credits_internal(provider_id).await?;

        let mut results = Vec::new();

        // 处理电影作品
        for cast in credits.cast {
            let role_type = "Actor".to_string();
            let is_featured = cast.order.map(|o| o <= 10).unwrap_or(false);
            let is_leading_role = cast.order.map(|o| o <= 5).unwrap_or(false);

            let media_type = if cast.media_type == "tv" {
                "tv".to_string()
            } else {
                "movie".to_string()
            };

            let year = cast
                .release_date
                .or(cast.first_air_date)
                .and_then(|date| date.split('-').next().map(str::to_string))
                .and_then(|year_str| year_str.parse::<i32>().ok());

            results.push(ExternalPersonCredit {
                external_id: cast.id.to_string(),
                title: cast
                    .title
                    .or(cast.name)
                    .unwrap_or_else(|| format!("TMDb {}", cast.id)),
                role_type: role_type.clone(),
                role: cast.character,
                is_featured,
                is_leading_role,
                media_type,
                year,
            });
        }

        // 处理剧组作品（导演、编剧等）
        for crew in credits.crew {
            let role_type = match crew.job.as_str() {
                "Director" => "Director",
                "Writer" => "Writer",
                "Producer" => "Producer",
                "Original Music Composer" => "Composer",
                "Director of Photography" => "Cinematographer",
                "Editor" => "Editor",
                _ => "Other",
            }
            .to_string();

            let media_type = if crew.media_type == "tv" {
                "tv".to_string()
            } else {
                "movie".to_string()
            };

            let year = crew
                .release_date
                .or(crew.first_air_date)
                .and_then(|date| date.split('-').next().map(str::to_string))
                .and_then(|year_str| year_str.parse::<i32>().ok());

            results.push(ExternalPersonCredit {
                external_id: crew.id.to_string(),
                title: crew
                    .title
                    .or(crew.name)
                    .unwrap_or_else(|| format!("TMDb {}", crew.id)),
                role_type,
                role: Some(crew.job),
                is_featured: false,
                is_leading_role: false,
                media_type,
                year,
            });
        }

        Ok(results)
    }

    async fn get_series_details(
        &self,
        provider_id: &str,
    ) -> Result<ExternalSeriesMetadata, AppError> {
        let details = self.get_tv_details_internal(provider_id).await?;
        let first_air_date = parse_tmdb_date(details.first_air_date.as_deref());
        let last_air_date = parse_tmdb_date(details.last_air_date.as_deref());
        let next_air_date = details
            .next_episode_to_air
            .as_ref()
            .and_then(|episode| parse_tmdb_date(episode.air_date.as_deref()));
        let inferred_air_day = next_air_date
            .or(last_air_date)
            .map(|date| weekday_name(date.weekday()));

        let mut provider_ids = HashMap::new();
        provider_ids.insert("Tmdb".to_string(), details.id.to_string());
        if let Some(external_ids) = &details.external_ids {
            if let Some(value) = external_ids
                .imdb_id
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                provider_ids.insert("Imdb".to_string(), value.clone());
            }
            if let Some(value) = external_ids.tvdb_id {
                provider_ids.insert("Tvdb".to_string(), value.to_string());
            }
        }

        let metadata = serde_json::to_value(&details).unwrap_or_default();
        Ok(ExternalSeriesMetadata {
            external_id: details.id.to_string(),
            provider: "tmdb".to_string(),
            name: details.name,
            original_title: details.original_name,
            overview: details.overview,
            premiere_date: first_air_date,
            status: details
                .status
                .and_then(|value| normalize_tmdb_series_status(&value)),
            end_date: last_air_date,
            air_days: inferred_air_day.into_iter().collect(),
            air_time: None,
            production_year: first_air_date.map(|date| date.year()),
            community_rating: details.vote_average,
            genres: details.genres.into_iter().map(|genre| genre.name).collect(),
            studios: details
                .networks
                .into_iter()
                .map(|network| network.name)
                .chain(
                    details
                        .production_companies
                        .into_iter()
                        .map(|company| company.name),
                )
                .filter(|name| !name.trim().is_empty())
                .collect(),
            production_locations: details.origin_country,
            provider_ids,
            homepage_url: details.homepage,
            tagline: details.tagline.filter(|s| !s.trim().is_empty()),
            tags: details
                .keywords
                .as_ref()
                .map(|k| {
                    k.results
                        .iter()
                        .map(|kw| kw.name.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            metadata,
        })
    }

    async fn get_movie_details(
        &self,
        provider_id: &str,
    ) -> Result<ExternalMovieMetadata, AppError> {
        let details = self.get_movie_details_internal(provider_id).await?;
        let premiere_date = parse_tmdb_date(details.release_date.as_deref());
        let runtime_ticks = details
            .runtime
            .map(|minutes| i64::from(minutes) * 60 * 10_000_000);
        let official_rating = tmdb_movie_certification(&details.release_dates);

        let mut provider_ids = HashMap::new();
        provider_ids.insert("Tmdb".to_string(), details.id.to_string());
        if let Some(external_ids) = &details.external_ids {
            if let Some(value) = external_ids
                .imdb_id
                .as_ref()
                .filter(|value| !value.trim().is_empty())
            {
                provider_ids.insert("Imdb".to_string(), value.clone());
            }
        }

        let remote_trailers = details
            .videos
            .as_ref()
            .map(tmdb_trailer_urls)
            .unwrap_or_default();
        let metadata = serde_json::to_value(&details).unwrap_or_default();

        let collection_info = details.belongs_to_collection.as_ref().map(|c| {
            use crate::metadata::models::MovieCollectionInfo;
            MovieCollectionInfo {
                tmdb_collection_id: c.id,
                name: c.name.clone().unwrap_or_else(|| format!("Collection {}", c.id)),
                poster_url: c.poster_path.as_ref().map(|p| format!("{}/original{}", self.config.image_base_url, p)),
                backdrop_url: c.backdrop_path.as_ref().map(|p| format!("{}/original{}", self.config.image_base_url, p)),
            }
        });

        Ok(ExternalMovieMetadata {
            external_id: details.id.to_string(),
            provider: "tmdb".to_string(),
            name: details.title,
            original_title: details.original_title,
            overview: details.overview,
            premiere_date,
            production_year: premiere_date.map(|date| date.year()),
            community_rating: details.vote_average,
            critic_rating: None,
            official_rating,
            runtime_ticks,
            genres: details.genres.into_iter().map(|genre| genre.name).collect(),
            studios: details
                .production_companies
                .into_iter()
                .map(|company| company.name)
                .filter(|name| !name.trim().is_empty())
                .collect(),
            production_locations: details
                .production_countries
                .into_iter()
                .map(|country| country.name)
                .filter(|name| !name.trim().is_empty())
                .collect(),
            provider_ids,
            poster_image_url: details
                .poster_path
                .map(|path| format!("{}/original{}", self.config.image_base_url, path)),
            backdrop_image_url: details
                .backdrop_path
                .map(|path| format!("{}/original{}", self.config.image_base_url, path)),
            remote_trailers,
            tagline: details.tagline.filter(|s| !s.trim().is_empty()),
            tags: details
                .keywords
                .as_ref()
                .map(|k| {
                    k.keywords
                        .iter()
                        .map(|kw| kw.name.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            metadata,
            collection_info,
        })
    }

    async fn get_item_people(
        &self,
        media_type: &str,
        provider_id: &str,
    ) -> Result<Vec<ExternalItemPerson>, AppError> {
        let credits =
            if media_type.eq_ignore_ascii_case("tv") || media_type.eq_ignore_ascii_case("series") {
                self.get_tv_credits_internal(provider_id).await?
            } else {
                self.get_movie_credits_internal(provider_id).await?
            };

        let mut people = Vec::new();

        for cast in credits.cast {
            let mut provider_ids = HashMap::new();
            provider_ids.insert("Tmdb".to_string(), cast.id.to_string());

            people.push(ExternalItemPerson {
                external_id: cast.id.to_string(),
                provider: "tmdb".to_string(),
                name: cast.name,
                role_type: "Actor".to_string(),
                role: cast.character,
                sort_order: cast.order.unwrap_or(0),
                image_url: cast
                    .profile_path
                    .map(|path| format!("{}/original{}", self.config.image_base_url, path)),
                external_url: Some(format!("https://www.themoviedb.org/person/{}", cast.id)),
                provider_ids,
                overview: None,
                birth_date: None,
                death_date: None,
                place_of_birth: None,
                homepage_url: None,
            });
        }

        for (index, crew) in credits.crew.into_iter().enumerate() {
            let Some(role_type) = (match crew.job.as_str() {
                "Director" => Some("Director"),
                "Writer" | "Screenplay" | "Story" => Some("Writer"),
                "Producer" | "Executive Producer" => Some("Producer"),
                _ => None,
            }) else {
                continue;
            };

            let mut provider_ids = HashMap::new();
            provider_ids.insert("Tmdb".to_string(), crew.id.to_string());

            people.push(ExternalItemPerson {
                external_id: crew.id.to_string(),
                provider: "tmdb".to_string(),
                name: crew.name,
                role_type: role_type.to_string(),
                role: Some(crew.job),
                sort_order: 1000 + index as i32,
                image_url: crew
                    .profile_path
                    .map(|path| format!("{}/original{}", self.config.image_base_url, path)),
                external_url: Some(format!("https://www.themoviedb.org/person/{}", crew.id)),
                provider_ids,
                overview: None,
                birth_date: None,
                death_date: None,
                place_of_birth: None,
                homepage_url: None,
            });
        }

        Ok(people)
    }

    async fn get_series_episode_catalog(
        &self,
        provider_id: &str,
    ) -> Result<Vec<ExternalEpisodeCatalogItem>, AppError> {
        let details = self.get_tv_details_internal(provider_id).await?;
        let mut items = Vec::new();

        for season in details.seasons {
            if season.season_number < 0 {
                continue;
            }

            let season_details = self
                .get_tv_season_details_internal(provider_id, season.season_number)
                .await?;
            for episode in season_details.episodes {
                items.push(ExternalEpisodeCatalogItem {
                    provider: "tmdb".to_string(),
                    external_series_id: provider_id.to_string(),
                    external_season_id: season.id.map(|value| value.to_string()),
                    external_episode_id: Some(episode.id.to_string()),
                    season_number: episode.season_number.unwrap_or(season.season_number),
                    episode_number: episode.episode_number,
                    episode_number_end: None,
                    name: episode.name,
                    overview: episode.overview.filter(|value| !value.trim().is_empty()),
                    premiere_date: parse_tmdb_date(episode.air_date.as_deref()),
                    image_path: episode
                        .still_path
                        .map(|path| format!("{}/original{}", self.config.image_base_url, path)),
                });
            }
        }

        Ok(items)
    }

    async fn get_remote_images(
        &self,
        media_type: &str,
        provider_id: &str,
    ) -> Result<Vec<ExternalRemoteImage>, AppError> {
        let mut images = Vec::new();
        if media_type.eq_ignore_ascii_case("season") || media_type.eq_ignore_ascii_case("episode") {
            return Ok(images);
        }

        if media_type.eq_ignore_ascii_case("series") || media_type.eq_ignore_ascii_case("tv") {
            let details = self.get_tv_details_internal(provider_id).await?;
            push_tmdb_remote_image(
                &mut images,
                details.poster_path,
                "Primary",
                &self.config.image_base_url,
            );
            push_tmdb_remote_image(
                &mut images,
                details.backdrop_path,
                "Backdrop",
                &self.config.image_base_url,
            );
            if let Ok(collection) = self.get_tv_images_internal(provider_id).await {
                push_tmdb_image_collection(&mut images, collection, &self.config.image_base_url);
            }
        } else {
            let details = self.get_movie_details_internal(provider_id).await?;
            push_tmdb_remote_image(
                &mut images,
                details.poster_path,
                "Primary",
                &self.config.image_base_url,
            );
            push_tmdb_remote_image(
                &mut images,
                details.backdrop_path,
                "Backdrop",
                &self.config.image_base_url,
            );
            if let Ok(collection) = self.get_movie_images_internal(provider_id).await {
                push_tmdb_image_collection(&mut images, collection, &self.config.image_base_url);
            }
        }

        Ok(images)
    }

    async fn get_remote_images_for_child(
        &self,
        media_type: &str,
        series_provider_id: &str,
        season_number: Option<i32>,
        episode_number: Option<i32>,
    ) -> Result<Vec<ExternalRemoteImage>, AppError> {
        if media_type.eq_ignore_ascii_case("season") {
            let Some(season_number) = season_number else {
                return Ok(Vec::new());
            };
            let details = self
                .get_tv_season_details_internal(series_provider_id, season_number)
                .await?;
            let mut images = Vec::new();
            push_tmdb_remote_image(
                &mut images,
                details.poster_path,
                "Primary",
                &self.config.image_base_url,
            );
            return Ok(images);
        }

        if media_type.eq_ignore_ascii_case("episode") {
            let Some(season_number) = season_number else {
                return Ok(Vec::new());
            };
            let Some(episode_number) = episode_number else {
                return Ok(Vec::new());
            };
            let details = self
                .get_tv_season_details_internal(series_provider_id, season_number)
                .await?;
            let mut images = Vec::new();
            if let Some(episode) = details
                .episodes
                .into_iter()
                .find(|episode| episode.episode_number == episode_number)
            {
                push_tmdb_remote_image(
                    &mut images,
                    episode.still_path.clone(),
                    "Primary",
                    &self.config.image_base_url,
                );
                push_tmdb_remote_image(
                    &mut images,
                    episode.still_path,
                    "Thumb",
                    &self.config.image_base_url,
                );
            }
            return Ok(images);
        }

        self.get_remote_images(media_type, series_provider_id).await
    }

    async fn search_movie(
        &self,
        name: &str,
        year: Option<i32>,
    ) -> Result<Vec<ExternalMediaSearchResult>, AppError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let response = self.search_movie_internal(trimmed, year).await?;
        Ok(response
            .results
            .into_iter()
            .map(|item| self.build_movie_search_result(item))
            .collect())
    }

    async fn search_series(
        &self,
        name: &str,
        year: Option<i32>,
    ) -> Result<Vec<ExternalMediaSearchResult>, AppError> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let response = self.search_tv_internal(trimmed, year).await?;
        Ok(response
            .results
            .into_iter()
            .map(|item| self.build_tv_search_result(item))
            .collect())
    }
}

fn push_tmdb_image_collection(
    images: &mut Vec<ExternalRemoteImage>,
    collection: TmdbImageCollection,
    image_base_url: &str,
) {
    for image in collection.posters {
        push_tmdb_remote_image_info(images, image, "Primary", image_base_url);
    }
    for image in collection.backdrops {
        push_tmdb_remote_image_info(images, image, "Backdrop", image_base_url);
    }
    for image in collection.logos {
        push_tmdb_remote_image_info(images, image, "Logo", image_base_url);
    }
}

fn push_tmdb_remote_image_info(
    images: &mut Vec<ExternalRemoteImage>,
    image: TmdbImageInfo,
    image_type: &str,
    image_base_url: &str,
) {
    let Some(path) = image.file_path.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    images.push(ExternalRemoteImage {
        provider_name: "TheMovieDb".to_string(),
        url: format!("{image_base_url}/original{path}"),
        thumbnail_url: Some(format!("{image_base_url}/w500{path}")),
        image_type: image_type.to_string(),
        language: image.iso_639_1,
        width: image.width,
        height: image.height,
        community_rating: image.vote_average,
        vote_count: image.vote_count,
    });
}

fn push_tmdb_remote_image(
    images: &mut Vec<ExternalRemoteImage>,
    path: Option<String>,
    image_type: &str,
    image_base_url: &str,
) {
    let Some(path) = path.filter(|value| !value.trim().is_empty()) else {
        return;
    };
    images.push(ExternalRemoteImage {
        provider_name: "TheMovieDb".to_string(),
        url: format!("{image_base_url}/original{path}"),
        thumbnail_url: Some(format!("{image_base_url}/w500{path}")),
        image_type: image_type.to_string(),
        language: None,
        width: None,
        height: None,
        community_rating: None,
        vote_count: None,
    });
}

fn language_code(value: &str) -> &str {
    value
        .split(['-', '_'])
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("en")
}

fn parse_tmdb_date(value: Option<&str>) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(value?, "%Y-%m-%d").ok()
}

fn normalize_tmdb_series_status(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let status = match value.to_ascii_lowercase().as_str() {
        "ended" | "canceled" | "cancelled" => "Ended",
        "returning series" | "in production" | "planned" | "pilot" => "Continuing",
        _ => value,
    };
    Some(status.to_string())
}

fn weekday_name(value: chrono::Weekday) -> String {
    match value {
        chrono::Weekday::Mon => "Monday",
        chrono::Weekday::Tue => "Tuesday",
        chrono::Weekday::Wed => "Wednesday",
        chrono::Weekday::Thu => "Thursday",
        chrono::Weekday::Fri => "Friday",
        chrono::Weekday::Sat => "Saturday",
        chrono::Weekday::Sun => "Sunday",
    }
    .to_string()
}

fn tmdb_movie_certification(release_dates: &Option<TmdbMovieReleaseDates>) -> Option<String> {
    let release_dates = release_dates.as_ref()?;
    release_dates
        .results
        .iter()
        .find(|entry| entry.iso_3166_1.eq_ignore_ascii_case("US"))
        .or_else(|| release_dates.results.first())
        .and_then(|entry| {
            entry.release_dates.iter().find_map(|date| {
                let certification = date.certification.trim();
                (!certification.is_empty()).then(|| certification.to_string())
            })
        })
}

fn tmdb_trailer_urls(videos: &TmdbVideoCollection) -> Vec<String> {
    videos
        .results
        .iter()
        .filter(|video| video.site.eq_ignore_ascii_case("YouTube"))
        .filter(|video| video.kind.eq_ignore_ascii_case("Trailer"))
        .map(|video| format!("https://www.youtube.com/watch?v={}", video.key))
        .collect()
}

// TMDB API响应结构

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TmdbMovieSearchResponse {
    #[serde(default)]
    pub(crate) page: i32,
    #[serde(default)]
    pub(crate) results: Vec<TmdbMovieSearchResult>,
    #[serde(default)]
    pub(crate) total_pages: i32,
    #[serde(default)]
    pub(crate) total_results: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TmdbMovieSearchResult {
    pub(crate) id: i32,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) original_title: Option<String>,
    #[serde(default)]
    pub(crate) overview: Option<String>,
    #[serde(default)]
    pub(crate) release_date: Option<String>,
    #[serde(default)]
    pub(crate) poster_path: Option<String>,
    #[serde(default)]
    pub(crate) vote_average: Option<f64>,
    #[serde(default)]
    pub(crate) popularity: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TmdbTvSearchResponse {
    #[serde(default)]
    pub(crate) page: i32,
    #[serde(default)]
    pub(crate) results: Vec<TmdbTvSearchResult>,
    #[serde(default)]
    pub(crate) total_pages: i32,
    #[serde(default)]
    pub(crate) total_results: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct TmdbTvSearchResult {
    pub(crate) id: i32,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) original_name: Option<String>,
    #[serde(default)]
    pub(crate) overview: Option<String>,
    #[serde(default)]
    pub(crate) first_air_date: Option<String>,
    #[serde(default)]
    pub(crate) poster_path: Option<String>,
    #[serde(default)]
    pub(crate) vote_average: Option<f64>,
    #[serde(default)]
    pub(crate) popularity: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbPersonSearchResponse {
    page: i32,
    results: Vec<TmdbPersonSearchResult>,
    total_pages: i32,
    total_results: i32,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbPersonSearchResult {
    id: i32,
    name: String,
    popularity: f64,
    profile_path: Option<String>,
    adult: bool,
    known_for: Option<Vec<TmdbKnownFor>>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbKnownFor {
    title: Option<String>,
    name: Option<String>,
    media_type: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbPersonDetails {
    id: i32,
    name: String,
    biography: String,
    birthday: Option<String>,
    deathday: Option<String>,
    place_of_birth: Option<String>,
    profile_path: Option<String>,
    homepage: Option<String>,
    popularity: f64,
    adult: bool,
    imdb_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbPersonCredits {
    id: i32,
    cast: Vec<TmdbCastCredit>,
    crew: Vec<TmdbCrewCredit>,
}

#[derive(Debug, Deserialize)]
struct TmdbCastCredit {
    id: i32,
    name: Option<String>,
    title: Option<String>,
    character: Option<String>,
    order: Option<i32>,
    media_type: String,
    release_date: Option<String>,
    first_air_date: Option<String>,
    profile_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbCrewCredit {
    id: i32,
    name: Option<String>,
    title: Option<String>,
    job: String,
    department: String,
    media_type: String,
    release_date: Option<String>,
    first_air_date: Option<String>,
    profile_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbItemCredits {
    cast: Vec<TmdbItemCastPerson>,
    crew: Vec<TmdbItemCrewPerson>,
}

#[derive(Debug, Deserialize)]
struct TmdbItemCastPerson {
    id: i32,
    name: String,
    character: Option<String>,
    order: Option<i32>,
    profile_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbItemCrewPerson {
    id: i32,
    name: String,
    job: String,
    profile_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbTvDetails {
    id: i32,
    name: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    first_air_date: Option<String>,
    last_air_date: Option<String>,
    status: Option<String>,
    homepage: Option<String>,
    vote_average: Option<f64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    /// PB35-5 (P3-3)：剧集 tagline（"标语"）。
    #[serde(default)]
    tagline: Option<String>,
    #[serde(default)]
    genres: Vec<TmdbNamedItem>,
    #[serde(default)]
    networks: Vec<TmdbNamedItem>,
    #[serde(default)]
    production_companies: Vec<TmdbNamedItem>,
    #[serde(default)]
    origin_country: Vec<String>,
    external_ids: Option<TmdbExternalIds>,
    next_episode_to_air: Option<TmdbEpisodeStub>,
    last_episode_to_air: Option<TmdbEpisodeStub>,
    #[serde(default)]
    seasons: Vec<TmdbSeasonStub>,
    /// PB35-5 (P3-3)：TMDB TV /keywords append 返回 `{"results": [{"id":..,"name":..}]}`。
    #[serde(default)]
    keywords: Option<TmdbTvKeywords>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbMovieDetails {
    id: i32,
    title: Option<String>,
    original_title: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    runtime: Option<i32>,
    vote_average: Option<f64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    /// PB35-5 (P3-3)：电影 tagline（"标语"）。
    #[serde(default)]
    tagline: Option<String>,
    #[serde(default)]
    genres: Vec<TmdbNamedItem>,
    #[serde(default)]
    production_companies: Vec<TmdbNamedItem>,
    #[serde(default)]
    production_countries: Vec<TmdbNamedItem>,
    external_ids: Option<TmdbMovieExternalIds>,
    release_dates: Option<TmdbMovieReleaseDates>,
    videos: Option<TmdbVideoCollection>,
    belongs_to_collection: Option<TmdbCollectionRef>,
    /// PB35-5 (P3-3)：TMDB Movie /keywords append 返回 `{"keywords": [{"id":..,"name":..}]}`。
    #[serde(default)]
    keywords: Option<TmdbMovieKeywords>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct TmdbTvKeywords {
    #[serde(default)]
    results: Vec<TmdbNamedItem>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct TmdbMovieKeywords {
    #[serde(default)]
    keywords: Vec<TmdbNamedItem>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbCollectionRef {
    id: i32,
    name: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbNamedItem {
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbExternalIds {
    imdb_id: Option<String>,
    tvdb_id: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbMovieExternalIds {
    imdb_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbMovieReleaseDates {
    #[serde(default)]
    results: Vec<TmdbMovieReleaseDateCountry>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbMovieReleaseDateCountry {
    iso_3166_1: String,
    #[serde(default)]
    release_dates: Vec<TmdbMovieReleaseDate>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbMovieReleaseDate {
    certification: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbVideoCollection {
    #[serde(default)]
    results: Vec<TmdbVideoResult>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbVideoResult {
    key: String,
    site: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct TmdbImageCollection {
    #[serde(default)]
    backdrops: Vec<TmdbImageInfo>,
    #[serde(default)]
    posters: Vec<TmdbImageInfo>,
    #[serde(default)]
    logos: Vec<TmdbImageInfo>,
}

#[derive(Debug, Deserialize)]
struct TmdbImageInfo {
    file_path: Option<String>,
    width: Option<i32>,
    height: Option<i32>,
    iso_639_1: Option<String>,
    vote_average: Option<f64>,
    vote_count: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbEpisodeStub {
    air_date: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TmdbSeasonStub {
    id: Option<i32>,
    season_number: i32,
}

#[derive(Debug, Deserialize)]
struct TmdbSeasonDetails {
    id: Option<i32>,
    season_number: i32,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    poster_path: Option<String>,
    #[serde(default)]
    episodes: Vec<TmdbSeasonEpisode>,
}

#[derive(Debug, Deserialize)]
struct TmdbSeasonEpisode {
    id: i32,
    name: String,
    overview: Option<String>,
    air_date: Option<String>,
    episode_number: i32,
    season_number: Option<i32>,
    still_path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_provider() -> TmdbProvider {
        TmdbProvider::with_config(TmdbConfig {
            api_key: "test".to_string(),
            ..Default::default()
        })
    }

    #[test]
    fn build_movie_search_result_populates_tmdb_id_year_and_poster_url() {
        let provider = fixture_provider();
        let hit = provider.build_movie_search_result(TmdbMovieSearchResult {
            id: 603,
            title: Some("The Matrix".to_string()),
            original_title: Some("The Matrix".to_string()),
            overview: Some("A hacker learns the truth...".to_string()),
            release_date: Some("1999-03-31".to_string()),
            poster_path: Some("/p.jpg".to_string()),
            vote_average: Some(8.2),
            popularity: Some(99.0),
        });
        assert_eq!(hit.provider, "tmdb");
        assert_eq!(hit.external_id, "603");
        assert_eq!(hit.production_year, Some(1999));
        assert_eq!(
            hit.image_url.as_deref(),
            Some("https://image.tmdb.org/t/p/original/p.jpg")
        );
        assert_eq!(
            hit.provider_ids.get("Tmdb").map(String::as_str),
            Some("603")
        );
    }

    #[test]
    fn build_tv_search_result_handles_missing_release_dates_gracefully() {
        let provider = fixture_provider();
        let hit = provider.build_tv_search_result(TmdbTvSearchResult {
            id: 1399,
            name: Some("Game of Thrones".to_string()),
            original_name: None,
            overview: None,
            first_air_date: None,
            poster_path: None,
            vote_average: None,
            popularity: None,
        });
        assert_eq!(hit.name, "Game of Thrones");
        assert!(hit.production_year.is_none());
        assert!(hit.image_url.is_none());
        assert!(hit.overview.is_none());
    }

    #[test]
    fn tmdb_search_response_tolerates_missing_optional_fields() {
        let raw = serde_json::json!({ "results": [] });
        let parsed: TmdbMovieSearchResponse =
            serde_json::from_value(raw).expect("搜索响应容错解析");
        assert!(parsed.results.is_empty());
    }
}
