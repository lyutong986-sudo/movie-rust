use std::{collections::HashSet, sync::Arc};

use crate::{
    error::AppError,
    metadata::{
        models::ExternalPersonSearchResult,
        provider::{ExternalItemPerson, MetadataProviderManager},
    },
    repository,
};
use serde_json::to_value;
use sqlx::PgPool;
use uuid::Uuid;

pub struct PersonService {
    pool: PgPool,
    metadata_manager: Arc<MetadataProviderManager>,
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

    pub async fn fetch_persons_for_item(
        &self,
        item_id: Uuid,
        provider_name: &str,
        external_item_id: &str,
        media_type: &str,
    ) -> Result<(), AppError> {
        let provider = self
            .metadata_manager
            .get_provider(provider_name)
            .ok_or_else(|| AppError::BadRequest(format!("Provider '{provider_name}' not found")))?;

        let people = provider
            .get_item_people(media_type, external_item_id)
            .await?;
        let tmdb_person_ids = if provider_name.eq_ignore_ascii_case("tmdb")
            || provider_name.eq_ignore_ascii_case("themoviedb")
        {
            people
                .iter()
                .filter_map(|person| {
                    person
                        .provider_ids
                        .get("Tmdb")
                        .or_else(|| person.provider_ids.get("TMDb"))
                        .or_else(|| person.provider_ids.get("tmdb"))
                        .cloned()
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        if !tmdb_person_ids.is_empty()
            || provider_name.eq_ignore_ascii_case("tmdb")
            || provider_name.eq_ignore_ascii_case("themoviedb")
        {
            repository::delete_tmdb_person_roles_except(&self.pool, item_id, &tmdb_person_ids)
                .await?;
        }
        for person in people {
            self.upsert_item_person(item_id, person).await?;
        }

        Ok(())
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
}
