use moka::future::Cache;
use serde_json::Value;
use std::sync::LazyLock;
use std::time::Duration;

use crate::error::AppError;
use crate::models::ItemCountsDto;

/// Short TTL cache for expensive aggregate queries (30s)
static ITEM_COUNTS_CACHE: LazyLock<Cache<(), ItemCountsDto>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(30))
        .build()
});

static YEARS_CACHE: LazyLock<Cache<(), Vec<i32>>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(1)
        .time_to_live(Duration::from_secs(60))
        .build()
});

static ARRAY_VALUES_CACHE: LazyLock<Cache<String, Vec<String>>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(10)
        .time_to_live(Duration::from_secs(60))
        .build()
});

pub async fn cached_item_counts(pool: &sqlx::PgPool) -> Result<ItemCountsDto, AppError> {
    if let Some(cached) = ITEM_COUNTS_CACHE.get(&()).await {
        return Ok(cached);
    }
    let result = super::repository::item_counts_uncached(pool).await?;
    ITEM_COUNTS_CACHE.insert((), result.clone()).await;
    Ok(result)
}

pub async fn cached_aggregate_years(pool: &sqlx::PgPool) -> Result<Vec<i32>, AppError> {
    if let Some(cached) = YEARS_CACHE.get(&()).await {
        return Ok(cached);
    }
    let result = super::repository::aggregate_years_uncached(pool).await?;
    YEARS_CACHE.insert((), result.clone()).await;
    Ok(result)
}

pub async fn cached_aggregate_array_values(
    pool: &sqlx::PgPool,
    field: &str,
) -> Result<Vec<String>, AppError> {
    let key = field.to_string();
    if let Some(cached) = ARRAY_VALUES_CACHE.get(&key).await {
        return Ok(cached);
    }
    let result = super::repository::aggregate_array_values_uncached(pool, field).await?;
    ARRAY_VALUES_CACHE.insert(key, result.clone()).await;
    Ok(result)
}

/// Cached system_settings lookup (10s TTL) — avoids per-item DB hit for startup_configuration
static SYSTEM_SETTING_CACHE: LazyLock<Cache<String, Option<Value>>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(20)
        .time_to_live(Duration::from_secs(10))
        .build()
});

pub async fn cached_system_setting(
    pool: &sqlx::PgPool,
    key: &str,
) -> Result<Option<Value>, AppError> {
    let cache_key = key.to_string();
    if let Some(cached) = SYSTEM_SETTING_CACHE.get(&cache_key).await {
        return Ok(cached);
    }
    let result = super::repository::get_system_setting(pool, key).await?;
    SYSTEM_SETTING_CACHE
        .insert(cache_key, result.clone())
        .await;
    Ok(result)
}

/// Invalidate all aggregate caches (call after scan/import completes)
pub async fn invalidate_all() {
    ITEM_COUNTS_CACHE.invalidate_all();
    YEARS_CACHE.invalidate_all();
    ARRAY_VALUES_CACHE.invalidate_all();
    SYSTEM_SETTING_CACHE.invalidate_all();
}
