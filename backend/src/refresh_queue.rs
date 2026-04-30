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
