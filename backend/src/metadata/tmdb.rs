use crate::error::AppError;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{
    models::{ExternalPerson, ExternalPersonSearchResult},
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

        let birth_date = person_details.birthday.and_then(|date| {
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()
        });

        let death_date = person_details.deathday.and_then(|date| {
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok()
        });

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
            metadata: serde_json::to_value(&person_details).unwrap_or_default(),
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
                .and_then(|date| date.split('-').next())
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
                .and_then(|date| date.split('-').next())
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
}

// TMDB API响应结构

#[derive(Debug, Deserialize, Serialize)]
struct TmdbPersonSearchResponse {
    page: i32,
    results: Vec<TmdbPersonSearchResult>,
    total_pages: i32,
    total_results: i32,
}

#[derive(Debug, Deserialize)]
struct TmdbPersonSearchResult {
    id: i32,
    name: String,
    popularity: f64,
    profile_path: Option<String>,
    adult: bool,
    known_for: Option<Vec<TmdbKnownFor>>,
}

#[derive(Debug, Deserialize)]
struct TmdbKnownFor {
    title: Option<String>,
    name: Option<String>,
    media_type: String,
}

#[derive(Debug, Deserialize)]
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