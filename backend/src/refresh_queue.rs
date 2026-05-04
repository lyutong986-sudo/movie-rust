use dashmap::DashSet;
use std::sync::LazyLock;
use uuid::Uuid;

/// Tracks item IDs currently being refreshed to prevent duplicate spawns.
static REFRESHING: LazyLock<DashSet<Uuid>> = LazyLock::new(DashSet::new);

/// Try to mark an item as "currently refreshing". Returns `true` if successfully
/// marked (i.e., was not already refreshing). Returns `false` if already in-flight.
pub fn try_begin_refresh(item_id: Uuid) -> bool {
    REFRESHING.insert(item_id)
}

/// Mark an item refresh as completed, allowing future refreshes.
pub fn end_refresh(item_id: Uuid) {
    REFRESHING.remove(&item_id);
}

/// Check if an item is currently being refreshed.
pub fn is_refreshing(item_id: Uuid) -> bool {
    REFRESHING.contains(&item_id)
}

/// Tracks (item_id, image_type, backdrop_index) currently being persisted to local
/// disk to avoid concurrent re-download of the same remote-Emby image.
///
/// `backdrop_index` 用 i32（`-1` 代表 None，0 = 主壁纸，>0 = 第 N 张）。
static IMAGE_PERSISTING: LazyLock<DashSet<(Uuid, String, i32)>> = LazyLock::new(DashSet::new);

/// 标记一张图片正在被异步持久化到本地。已在持久化中的同一图片直接跳过，避免重复下载。
pub fn try_begin_image_persist(item_id: Uuid, image_type: &str, backdrop_index: Option<i32>) -> bool {
    let key = (
        item_id,
        image_type.to_ascii_lowercase(),
        backdrop_index.unwrap_or(-1),
    );
    IMAGE_PERSISTING.insert(key)
}

/// 持久化任务结束，无论成功失败都要释放，否则同一图永远拒绝重试。
pub fn end_image_persist(item_id: Uuid, image_type: &str, backdrop_index: Option<i32>) {
    let key = (
        item_id,
        image_type.to_ascii_lowercase(),
        backdrop_index.unwrap_or(-1),
    );
    IMAGE_PERSISTING.remove(&key);
}
