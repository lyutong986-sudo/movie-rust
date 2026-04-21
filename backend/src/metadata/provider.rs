use crate::error::AppError;
use async_trait::async_trait;
use std::collections::HashMap;

use super::models::{ExternalPerson, ExternalPersonSearchResult, ExternalSeriesMetadata};

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
