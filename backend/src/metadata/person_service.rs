use crate::{
    error::AppError,
    metadata::{
        models::{ExternalPerson, ExternalPersonSearchResult},
        provider::{MetadataProvider, MetadataProviderManager},
    },
    repository,
};
use sqlx::PgPool;
use uuid::Uuid;

/// 人物数据服务
pub struct PersonService {
    pool: PgPool,
    metadata_manager: MetadataProviderManager,
}

impl PersonService {
    /// 创建新的人物数据服务
    pub fn new(pool: PgPool, metadata_manager: MetadataProviderManager) -> Self {
        Self {
            pool,
            metadata_manager,
        }
    }

    /// 从外部元数据源提取人物数据
    pub async fn fetch_person_from_external(
        &self,
        provider_name: &str,
        external_id: &str,
    ) -> Result<Uuid, AppError> {
        let provider = self
            .metadata_manager
            .get_provider(provider_name)
            .ok_or_else(|| AppError::BadRequest(format!("Provider '{}' not found", provider_name)))?;

        // 从外部源获取人物详细信息
        let external_person = provider.get_person_details(external_id).await?;

        // 将外部人物转换为数据库模型
        let db_person = external_person.to_db_person();

        // 检查人物是否已存在（通过外部ID）
        let existing_person = repository::get_person_by_external_id(
            &self.pool,
            provider_name,
            external_id,
        ).await?;

        let person_id = if let Some(existing) = existing_person {
            // 更新现有人物
            repository::update_person(&self.pool, existing.id, &db_person).await?;
            existing.id
        } else {
            // 创建新人物
            repository::create_person(&self.pool, &db_person).await?
        };

        // 获取人物作品并关联到项目
        // TODO: 实现作品关联

        Ok(person_id)
    }

    /// 搜索外部人物
    pub async fn search_external_person(
        &self,
        provider_name: &str,
        name: &str,
    ) -> Result<Vec<ExternalPersonSearchResult>, AppError> {
        let provider = self
            .metadata_manager
            .get_provider(provider_name)
            .ok_or_else(|| AppError::BadRequest(format!("Provider '{}' not found", provider_name)))?;

        provider.search_person(name).await
    }

    /// 批量提取人物数据（用于电影/电视剧）
    pub async fn fetch_persons_for_item(
        &self,
        item_id: Uuid,
        provider_name: &str,
        external_item_id: &str,
    ) -> Result<(), AppError> {
        // TODO: 根据外部项目ID获取人物列表并关联
        Ok(())
    }
}

