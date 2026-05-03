use bytes::Bytes;
use moka::future::Cache;
use reqwest::Client;
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio::sync::broadcast;
use dashmap::DashMap;

use crate::error::AppError;

pub static SHARED: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .pool_max_idle_per_host(4)
        // 长任务（远端 Emby 全量同步可达 1500s+）容易踩到「池里的 keep-alive
        // 连接被中间反代/NAT 静默掐断」：表象就是下一次复用时 body 读到一半抛
        // `error decoding response body`。`tcp_keepalive` 强制周期性发探测，
        // `pool_idle_timeout` 限制空闲连接存活时间，两者一起把死连接挡在池外。
        // 30s/60s 是相对保守的折中：既能避开常见的 NAT/CDN 60–90s 空闲回收，
        // 又不会频繁建连影响短突发请求。
        .tcp_keepalive(std::time::Duration::from_secs(30))
        .pool_idle_timeout(std::time::Duration::from_secs(60))
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("failed to build shared HTTP client")
});

/// Short-lived byte cache (10s TTL) to prevent duplicate downloads of the same URL
static IMAGE_CACHE: LazyLock<Cache<String, Bytes>> = LazyLock::new(|| {
    Cache::builder()
        .max_capacity(500)
        .time_to_live(Duration::from_secs(10))
        .build()
});

/// In-flight dedup: if the same URL is being downloaded, waiters share the result
static INFLIGHT: LazyLock<DashMap<String, Arc<broadcast::Sender<Result<Bytes, String>>>>> =
    LazyLock::new(DashMap::new);

/// RAII guard that removes the INFLIGHT entry on drop, preventing leaks on panic/cancel
struct InflightGuard {
    key: String,
}

impl Drop for InflightGuard {
    fn drop(&mut self) {
        INFLIGHT.remove(&self.key);
    }
}

/// Download image bytes with URL deduplication.
/// If the same URL is currently being fetched, callers wait for the shared result.
/// Successfully fetched bytes are cached for 10s.
pub async fn download_image_bytes(url: &str) -> Result<Bytes, AppError> {
    if let Some(cached) = IMAGE_CACHE.get(url).await {
        return Ok(cached);
    }

    // Check if there's an in-flight request
    if let Some(sender_ref) = INFLIGHT.get(url) {
        let mut rx = sender_ref.value().subscribe();
        drop(sender_ref);
        return match rx.recv().await {
            Ok(Ok(bytes)) => Ok(bytes),
            Ok(Err(e)) => Err(AppError::Internal(e)),
            Err(_) => Err(AppError::Internal("image download channel closed".into())),
        };
    }

    let (tx, _) = broadcast::channel::<Result<Bytes, String>>(4);
    let tx = Arc::new(tx);
    INFLIGHT.insert(url.to_string(), tx.clone());

    let _guard = InflightGuard { key: url.to_string() };

    let result = async {
        let response = SHARED
            .get(url)
            .send()
            .await
            .map_err(|e| format!("下载远程图片失败: {e}"))?;
        if !response.status().is_success() {
            let upstream_status = response.status();
            tracing::debug!(url = %url, upstream_status = %upstream_status, "上游图片返回非 2xx 状态");
            return Err("远程图片不存在".to_string());
        }
        response
            .bytes()
            .await
            .map_err(|e| format!("读取远程图片失败: {e}"))
    }
    .await;

    match result {
        Ok(bytes) => {
            IMAGE_CACHE.insert(url.to_string(), bytes.clone()).await;
            let _ = tx.send(Ok(bytes.clone()));
            Ok(bytes)
        }
        Err(e) => {
            let _ = tx.send(Err(e.clone()));
            Err(AppError::Internal(e))
        }
    }
}
