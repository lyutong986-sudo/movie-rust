use crate::error::AppError;
use async_trait::async_trait;
use chrono::NaiveDate;
use std::collections::HashMap;

use super::models::{
    ExternalMovieMetadata, ExternalPerson, ExternalPersonSearchResult, ExternalSeriesMetadata,
};

/// 元数据提供者接口
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    /// 提供者名称（如 "tmdb"）
    fn name(&self) -> &str;

    /// 搜索人物
    async fn search_person(&self, name: &str) -> Result<Vec<ExternalPersonSearchResult>, AppError>;

    /// 获取人物详细信息
    async fn get_person_details(&self, provider_id: &str) -> Result<ExternalPerson, AppError>;

    /// 获取人物参演作品
    async fn get_person_credits(&self, provider_id: &str) -> Result<Vec<ExternalPersonCredit>, AppError>;

    async fn get_series_details(&self, provider_id: &str) -> Result<ExternalSeriesMetadata, AppError>;

    async fn get_movie_details(&self, provider_id: &str) -> Result<ExternalMovieMetadata, AppError>;

    /// 获取条目人物信息（电影/剧集）
    async fn get_item_people(
        &self,
        media_type: &str,
        provider_id: &str,
    ) -> Result<Vec<ExternalItemPerson>, AppError>;

    async fn get_series_episode_catalog(
        &self,
        provider_id: &str,
    ) -> Result<Vec<ExternalEpisodeCatalogItem>, AppError>;

    async fn get_remote_images(
        &self,
        _media_type: &str,
        _provider_id: &str,
    ) -> Result<Vec<ExternalRemoteImage>, AppError> {
        Ok(Vec::new())
    }

    async fn get_remote_images_for_child(
        &self,
        media_type: &str,
        series_provider_id: &str,
        season_number: Option<i32>,
        episode_number: Option<i32>,
    ) -> Result<Vec<ExternalRemoteImage>, AppError> {
        let _ = season_number;
        let _ = episode_number;
        self.get_remote_images(media_type, series_provider_id).await
    }
}

/// 外部人物作品信息
#[derive(Debug, Clone)]
pub struct ExternalPersonCredit {
    /// 媒体项ID（外部提供者ID）
    pub external_id: String,
    /// 媒体项标题
    pub title: String,
    /// 角色类型
    pub role_type: String,
    /// 具体角色名称
    pub role: Option<String>,
    /// 是否为特色角色
    pub is_featured: bool,
    /// 是否为主演
    pub is_leading_role: bool,
    /// 媒体类型（movie, tv, episode等）
    pub media_type: String,
    /// 发行年份
    pub year: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct ExternalItemPerson {
    pub external_id: String,
    pub provider: String,
    pub name: String,
    pub role_type: String,
    pub role: Option<String>,
    pub sort_order: i32,
    pub image_url: Option<String>,
    pub external_url: Option<String>,
    pub provider_ids: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct ExternalEpisodeCatalogItem {
    pub provider: String,
    pub external_series_id: String,
    pub external_season_id: Option<String>,
    pub external_episode_id: Option<String>,
    pub season_number: i32,
    pub episode_number: i32,
    pub episode_number_end: Option<i32>,
    pub name: String,
    pub overview: Option<String>,
    pub premiere_date: Option<NaiveDate>,
    pub image_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ExternalRemoteImage {
    pub provider_name: String,
    pub url: String,
    pub thumbnail_url: Option<String>,
    pub image_type: String,
    pub language: Option<String>,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub community_rating: Option<f64>,
    pub vote_count: Option<i32>,
}

/// 元数据提供者管理器
pub struct MetadataProviderManager {
    providers: HashMap<String, Box<dyn MetadataProvider>>,
}

impl MetadataProviderManager {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// 注册提供者
    pub fn register_provider(&mut self, provider: Box<dyn MetadataProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// 获取提供者
    pub fn get_provider(&self, name: &str) -> Option<&dyn MetadataProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    /// 获取所有提供者
    pub fn providers(&self) -> Vec<&dyn MetadataProvider> {
        self.providers.values().map(|p| p.as_ref()).collect()
    }
}

impl Default for MetadataProviderManager {
    fn default() -> Self {
        Self::new()
    }
}
