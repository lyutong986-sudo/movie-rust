use std::{collections::HashSet, path::PathBuf, sync::Arc};

use crate::{
    error::AppError,
    metadata::{
        models::ExternalPersonSearchResult,
        provider::{ExternalItemPerson, MetadataProviderManager},
    },
    repository,
};
use chrono::{DateTime, Datelike, Utc};
use reqwest::header;
use serde_json::to_value;
use sqlx::PgPool;
use tokio::fs;
use uuid::Uuid;

pub struct PersonService {
    pool: PgPool,
    metadata_manager: Arc<MetadataProviderManager>,
}

/// 人物 refresh 完成后的报告，便于上层拼接错误日志或测试断言。
#[derive(Debug, Clone, Default)]
pub struct PersonRefreshReport {
    pub person_id: Uuid,
    pub overview_filled: bool,
    pub primary_image_saved: bool,
    pub provider: Option<String>,
    pub external_id: Option<String>,
}

impl PersonService {
    pub fn new(pool: PgPool, metadata_manager: Arc<MetadataProviderManager>) -> Self {
        Self {
            pool,
            metadata_manager,
        }
    }

    pub async fn fetch_person_from_external(
        &self,
        provider_name: &str,
        external_id: &str,
    ) -> Result<Uuid, AppError> {
        let provider = self
            .metadata_manager
            .get_provider(provider_name)
            .ok_or_else(|| AppError::BadRequest(format!("Provider '{provider_name}' not found")))?;

        let external_person = provider.get_person_details(external_id).await?;
        let db_person = external_person.to_db_person();

        let existing_person =
            repository::get_person_by_external_id(&self.pool, provider_name, external_id).await?;

        let person_id = if let Some(existing) = existing_person {
            repository::update_person(&self.pool, existing.id, &db_person).await?;
            existing.id
        } else {
            repository::create_person(&self.pool, &db_person).await?
        };

        let credits = provider.get_person_credits(external_id).await?;
        let mut seen_links = HashSet::new();
        for (index, credit) in credits.iter().enumerate() {
            let items =
                repository::find_items_for_external_person_credit(&self.pool, credit).await?;
            for item in items {
                if !seen_links.insert(item.id) {
                    continue;
                }
                repository::upsert_person_role(
                    &self.pool,
                    person_id,
                    item.id,
                    &credit.role_type,
                    credit.role.as_deref(),
                    index as i32,
                )
                .await?;
            }
        }

        Ok(person_id)
    }

    pub async fn search_external_person(
        &self,
        provider_name: &str,
        name: &str,
    ) -> Result<Vec<ExternalPersonSearchResult>, AppError> {
        let provider = self
            .metadata_manager
            .get_provider(provider_name)
            .ok_or_else(|| AppError::BadRequest(format!("Provider '{provider_name}' not found")))?;

        provider.search_person(name).await
    }

    pub(crate) async fn upsert_item_person(
        &self,
        item_id: Uuid,
        person: ExternalItemPerson,
    ) -> Result<(), AppError> {
        let provider_ids = to_value(&person.provider_ids).unwrap_or_default();
        let person_id = repository::upsert_person_reference(
            &self.pool,
            &person.name,
            provider_ids,
            person.image_url.as_deref(),
            person.external_url.as_deref(),
        )
        .await?;

        repository::upsert_person_role(
            &self.pool,
            person_id,
            item_id,
            &person.role_type,
            person.role.as_deref(),
            person.sort_order,
        )
        .await
    }

    /// 用 TMDB 详情把 `persons` 表中的简介 / 出生日期 / 出生地 / 主页 / 头像填齐。
    ///
    /// - `force_image=false`：只在 primary_image_path 为空、或仍是远程 URL 时尝试落盘
    /// - `force_image=true`：覆盖已有本地图片（用于"重新刷新"场景）
    ///
    /// 缺 TMDB id 或 provider 未注册时返回 `Ok(None)`，方便上层批量调用时静默跳过。
    pub async fn refresh_person_from_tmdb(
        &self,
        person_id: Uuid,
        static_dir: &std::path::Path,
        force_image: bool,
    ) -> Result<Option<PersonRefreshReport>, AppError> {
        let Some(person_dto) = repository::get_person_by_uuid(&self.pool, person_id).await? else {
            return Ok(None);
        };
        let provider_ids_map = person_dto.provider_ids.clone().unwrap_or_default();
        let tmdb_id = provider_ids_map
            .get("Tmdb")
            .or_else(|| provider_ids_map.get("TMDb"))
            .or_else(|| provider_ids_map.get("tmdb"))
            .cloned();
        let Some(tmdb_id) = tmdb_id else {
            return Ok(None);
        };

        let Some(provider) = self.metadata_manager.get_provider("tmdb") else {
            return Ok(None);
        };

        let external_person = match provider.get_person_details(&tmdb_id).await {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(person_id = %person_id, tmdb_id = %tmdb_id, ?err, "拉取 TMDB 人物详情失败");
                return Ok(None);
            }
        };

        let provider_ids_value = to_value(&external_person.provider_ids).ok();
        let premiere_date: Option<DateTime<Utc>> = external_person
            .birth_date
            .and_then(|d| d.and_hms_opt(0, 0, 0).map(|n| n.and_utc()));
        let death_date: Option<DateTime<Utc>> = external_person
            .death_date
            .and_then(|d| d.and_hms_opt(0, 0, 0).map(|n| n.and_utc()));
        let production_year = external_person.birth_date.map(|d| d.year());

        // 仅在 biography 实际有内容时才下发 SQL，避免空字符串把库里已有简介覆盖掉。
        let overview_for_sql: Option<&str> = external_person
            .overview
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let place_of_birth_for_sql: Option<&str> = external_person
            .place_of_birth
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let homepage_for_sql: Option<&str> = external_person
            .homepage_url
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());

        repository::patch_person_metadata(
            &self.pool,
            person_id,
            overview_for_sql,
            external_person.external_url.as_deref(),
            provider_ids_value.as_ref(),
            premiere_date,
            death_date,
            production_year,
            place_of_birth_for_sql,
            homepage_for_sql,
            external_person.sort_name.as_deref(),
        )
        .await?;

        let mut report = PersonRefreshReport {
            person_id,
            overview_filled: external_person
                .overview
                .as_ref()
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            provider: Some("tmdb".to_string()),
            external_id: Some(tmdb_id.clone()),
            ..Default::default()
        };

        if let Some(image_url) = external_person.image_url.as_deref() {
            match download_person_primary_image(
                &self.pool,
                person_id,
                image_url,
                static_dir,
                force_image,
            )
            .await
            {
                Ok(saved) => report.primary_image_saved = saved,
                Err(err) => {
                    tracing::warn!(person_id = %person_id, ?err, "下载人物头像失败");
                }
            }
        }

        Ok(Some(report))
    }

    /// 为某个媒体条目下的所有人物批量做一次 TMDB 详情补全。
    ///
    /// `top_n` 限制每条 item 最多 refresh 多少人，避免 TMDB 速率失控；超过部分按 cast 顺序丢弃。
    /// 任何单个 person 的失败都吞掉记一行 warn，不影响其他 person。
    pub async fn refresh_persons_for_item(
        &self,
        item_id: Uuid,
        static_dir: &std::path::Path,
        top_n: usize,
        force_image: bool,
    ) -> Result<Vec<PersonRefreshReport>, AppError> {
        let person_ids = repository::list_item_person_ids(&self.pool, item_id, top_n).await?;
        let mut reports = Vec::new();
        for pid in person_ids {
            match self
                .refresh_person_from_tmdb(pid, static_dir, force_image)
                .await
            {
                Ok(Some(report)) => reports.push(report),
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(person_id = %pid, ?err, "刷新人物时出错");
                }
            }
        }
        Ok(reports)
    }
}

/// 把 TMDB 提供的 image_url 下载到 `<static_dir>/person-images/<uuid>-primary.<ext>`，
/// 写完后回填 `persons.primary_image_path`。
///
/// `force=false` 时若已有本地图片则跳过；`force=true` 始终覆盖。
async fn download_person_primary_image(
    pool: &PgPool,
    person_id: Uuid,
    image_url: &str,
    static_dir: &std::path::Path,
    force: bool,
) -> Result<bool, AppError> {
    if image_url.trim().is_empty() {
        return Ok(false);
    }

    let existing =
        repository::get_person_image_path(pool, &person_id.to_string(), "primary").await?;
    if !force {
        if let Some(existing_path) = existing.as_deref() {
            if !existing_path.is_empty()
                && !existing_path.starts_with("http://")
                && !existing_path.starts_with("https://")
                && std::path::Path::new(existing_path).exists()
            {
                return Ok(false);
            }
        }
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| AppError::Internal(format!("构造 HTTP 客户端失败: {e}")))?;
    let response = client.get(image_url).send().await.map_err(|e| {
        AppError::Internal(format!("下载人物头像失败: {e}"))
    })?;
    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "下载人物头像非 2xx: {}",
            response.status()
        )));
    }

    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/jpeg")
        .to_string();
    let extension = match content_type.to_ascii_lowercase().as_str() {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    };
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AppError::Internal(format!("读取人物头像失败: {e}")))?;

    let dir: PathBuf = static_dir.join("person-images");
    fs::create_dir_all(&dir).await.map_err(AppError::Io)?;
    let filename = format!("{}-primary.{}", person_id, extension);
    let local_path = dir.join(filename);
    fs::write(&local_path, &bytes).await.map_err(AppError::Io)?;

    let local_path_text = local_path.to_string_lossy().to_string();
    repository::update_person_image_path(pool, person_id, "primary", Some(&local_path_text))
        .await?;

    Ok(true)
}
