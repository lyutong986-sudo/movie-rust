use crate::error::AppError;
use async_trait::async_trait;
use chrono::Datelike;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    models::{ExternalPerson, ExternalPersonSearchResult, ExternalSeriesMetadata},
    provider::{ExternalPersonCredit, MetadataProvider},
};

/// TMDB API配置
#[derive(Debug, Clone)]
pub struct TmdbConfig {
    /// TMDB API密钥
    pub api_key: String,
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
}

impl TmdbProvider {
    /// 创建新的TMDB提供者
    pub fn new(api_key: String) -> Self {
        Self::with_config(TmdbConfig {
            api_key,
            ..Default::default()
        })
    }

    /// 使用配置创建TMDB提供者
    pub fn with_config(config: TmdbConfig) -> Self {
        Self {
            config,
            client: Client::new(),
        }
    }

    /// 构建API URL
    fn build_url(&self, endpoint: &str) -> String {
        format!("{}{}", self.config.base_url, endpoint)
    }

    /// 添加API密钥参数
    fn add_api_key(&self, params: &mut HashMap<String, String>) {
        params.insert("api_key".to_string(), self.config.api_key.clone());
        params.insert("language".to_string(), self.config.language.clone());
    }

    /// 搜索人物
    async fn search_person_internal(&self, name: &str) -> Result<TmdbPersonSearchResponse, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert("query".to_string(), name.to_string());
        params.insert("page".to_string(), "1".to_string());

        let response = self
            .client
            .get(self.build_url("/search/person"))
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        let search_result: TmdbPersonSearchResponse = response.json().await?;
        Ok(search_result)
    }

    /// 获取人物详细信息
    async fn get_person_details_internal(&self, person_id: &str) -> Result<TmdbPersonDetails, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);

        let url = self.build_url(&format!("/person/{}", person_id));
        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        let person_details: TmdbPersonDetails = response.json().await?;
        Ok(person_details)
    }

    /// 获取人物作品
    async fn get_person_credits_internal(&self, person_id: &str) -> Result<TmdbPersonCredits, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);

        let url = self.build_url(&format!("/person/{}/combined_credits", person_id));
        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        let credits: TmdbPersonCredits = response.json().await?;
        Ok(credits)
    }

    async fn get_tv_details_internal(&self, tv_id: &str) -> Result<TmdbTvDetails, AppError> {
        let mut params = HashMap::new();
        self.add_api_key(&mut params);
        params.insert("append_to_response".to_string(), "external_ids".to_string());

        let url = self.build_url(&format!("/tv/{}", tv_id));
        let response = self
            .client
            .get(&url)
            .query(&params)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json().await?)
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
                image_url: person.profile_path.map(|path| format!("{}/original{}", self.config.image_base_url, path)),
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
        let person_details = self.get_person_details_internal(provider_id).await?;
        
        let mut provider_ids = HashMap::new();
        provider_ids.insert("tmdb".to_string(), person_details.id.to_string());
        
        if let Some(imdb_id) = &person_details.imdb_id {
            provider_ids.insert("imdb".to_string(), imdb_id.clone());
        }

        let birth_date = person_details.birthday.clone().and_then(|date| {
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()
        });

        let death_date = person_details.deathday.clone().and_then(|date| {
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()
        });

        let metadata = serde_json::to_value(&person_details).unwrap_or_default();
        let external_person = ExternalPerson {
            external_id: person_details.id.to_string(),
            provider: "tmdb".to_string(),
            name: person_details.name,
            sort_name: None,
            overview: Some(person_details.biography),
            external_url: Some(format!("https://www.themoviedb.org/person/{}", person_details.id)),
            birth_date,
            death_date,
            place_of_birth: person_details.place_of_birth,
            image_url: person_details.profile_path.map(|path| format!("{}/original{}", self.config.image_base_url, path)),
            homepage_url: person_details.homepage,
            popularity: Some(person_details.popularity),
            adult: Some(person_details.adult),
            provider_ids,
            metadata,
        };

        Ok(external_person)
    }

    async fn get_person_credits(&self, provider_id: &str) -> Result<Vec<ExternalPersonCredit>, AppError> {
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

            let year = cast.release_date.or(cast.first_air_date)
                .and_then(|date| date.split('-').next().map(str::to_string))
                .and_then(|year_str| year_str.parse::<i32>().ok());

            results.push(ExternalPersonCredit {
                external_id: cast.id.to_string(),
                title: cast.title.or(cast.name).unwrap_or_default(),
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
            }.to_string();

            let media_type = if crew.media_type == "tv" {
                "tv".to_string()
            } else {
                "movie".to_string()
            };

            let year = crew.release_date.or(crew.first_air_date)
                .and_then(|date| date.split('-').next().map(str::to_string))
                .and_then(|year_str| year_str.parse::<i32>().ok());

            results.push(ExternalPersonCredit {
                external_id: crew.id.to_string(),
                title: crew.title.or(crew.name).unwrap_or_default(),
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

    async fn get_series_details(&self, provider_id: &str) -> Result<ExternalSeriesMetadata, AppError> {
        let details = self.get_tv_details_internal(provider_id).await?;
        let first_air_date = parse_tmdb_date(details.first_air_date.as_deref());
        let last_air_date = parse_tmdb_date(details.last_air_date.as_deref());
        let next_air_date = details
            .next_episode_to_air
            .as_ref()
            .and_then(|episode| parse_tmdb_date(episode.air_date.as_deref()));
        let inferred_air_day = next_air_date.or(last_air_date).map(|date| weekday_name(date.weekday()));

        let mut provider_ids = HashMap::new();
        provider_ids.insert("Tmdb".to_string(), details.id.to_string());
        if let Some(external_ids) = &details.external_ids {
            if let Some(value) = external_ids.imdb_id.as_ref().filter(|value| !value.trim().is_empty()) {
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
            status: details.status.and_then(|value| normalize_tmdb_series_status(&value)),
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
                .chain(details.production_companies.into_iter().map(|company| company.name))
                .filter(|name| !name.trim().is_empty())
                .collect(),
            production_locations: details.origin_country,
            provider_ids,
            homepage_url: details.homepage,
            metadata,
        })
    }
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

// TMDB API响应结构

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
    title: Option<String>,
    name: Option<String>,
    character: Option<String>,
    order: Option<i32>,
    media_type: String,
    release_date: Option<String>,
    first_air_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbCrewCredit {
    id: i32,
    title: Option<String>,
    name: Option<String>,
    job: String,
    department: String,
    media_type: String,
    release_date: Option<String>,
    first_air_date: Option<String>,
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
struct TmdbEpisodeStub {
    air_date: Option<String>,
}
