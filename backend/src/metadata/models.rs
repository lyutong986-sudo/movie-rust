use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalSeriesMetadata {
    pub external_id: String,
    pub provider: String,
    pub name: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub premiere_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub end_date: Option<NaiveDate>,
    pub air_days: Vec<String>,
    pub air_time: Option<String>,
    pub production_year: Option<i32>,
    pub community_rating: Option<f64>,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub production_locations: Vec<String>,
    pub provider_ids: HashMap<String, String>,
    pub homepage_url: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalMovieMetadata {
    pub external_id: String,
    pub provider: String,
    pub name: Option<String>,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub premiere_date: Option<NaiveDate>,
    pub production_year: Option<i32>,
    pub community_rating: Option<f64>,
    pub critic_rating: Option<f64>,
    pub official_rating: Option<String>,
    pub runtime_ticks: Option<i64>,
    pub genres: Vec<String>,
    pub studios: Vec<String>,
    pub production_locations: Vec<String>,
    pub provider_ids: HashMap<String, String>,
    pub poster_image_url: Option<String>,
    pub backdrop_image_url: Option<String>,
    pub remote_trailers: Vec<String>,
    pub metadata: Value,
}

/// 外部人物搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalPersonSearchResult {
    /// 外部提供者ID
    pub external_id: String,
    /// 提供者名称（如 "tmdb"）
    pub provider: String,
    /// 人物名称
    pub name: String,
    /// 排序名称
    pub sort_name: Option<String>,
    /// 简介
    pub overview: Option<String>,
    /// 外部URL
    pub external_url: Option<String>,
    /// 图片路径
    pub image_url: Option<String>,
    /// 已知作品
    pub known_for: Vec<String>,
    /// 受欢迎程度分数
    pub popularity: Option<f64>,
    /// 是否为成人内容演员
    pub adult: Option<bool>,
}

/// 外部人物详细信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExternalPerson {
    /// 外部提供者ID
    pub external_id: String,
    /// 提供者名称（如 "tmdb"）
    pub provider: String,
    /// 人物名称
    pub name: String,
    /// 排序名称
    pub sort_name: Option<String>,
    /// 简介
    pub overview: Option<String>,
    /// 外部URL
    pub external_url: Option<String>,
    /// 出生日期
    pub birth_date: Option<NaiveDate>,
    /// 死亡日期
    pub death_date: Option<NaiveDate>,
    /// 出生地
    pub place_of_birth: Option<String>,
    /// 图片路径
    pub image_url: Option<String>,
    /// 主页URL
    pub homepage_url: Option<String>,
    /// 受欢迎程度分数
    pub popularity: Option<f64>,
    /// 是否为成人内容演员
    pub adult: Option<bool>,
    /// 外部提供者ID映射
    pub provider_ids: HashMap<String, String>,
    /// 元数据
    pub metadata: Value,
}

impl ExternalPerson {
    /// 转换为数据库Person模型
    pub fn to_db_person(&self) -> crate::models::DbPerson {
        crate::models::DbPerson {
            id: Uuid::new_v4(),
            name: self.name.clone(),
            sort_name: self.sort_name.clone(),
            overview: self.overview.clone(),
            external_url: self.external_url.clone(),
            provider_ids: serde_json::to_value(&self.provider_ids).unwrap_or_default(),
            premiere_date: self.birth_date.map(|d| DateTime::<Utc>::from_utc(d.and_hms_opt(0, 0, 0).unwrap_or_default(), Utc)),
            production_year: self.birth_date.map(|d| d.year()),
            primary_image_path: self.image_url.clone(),
            backdrop_image_path: None,
            logo_image_path: None,
            favorite_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }
}
