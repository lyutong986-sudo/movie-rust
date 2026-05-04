use crate::state::AppState;
use axum::Router;

pub mod admin;
pub mod collections;
pub mod compat;
pub mod connect;
pub mod devices;
pub mod genres;
pub mod images;
pub mod items;
pub mod live_streams;
pub mod media_segments;
pub mod metadata_routes;
pub mod misc;
pub mod persons;
pub mod playlists;
pub mod remote_emby;
pub mod scheduled_tasks;
pub mod sessions;
pub mod shows;
pub mod startup;
pub mod system;
pub mod translation;
pub mod trickplay;
pub mod usage_stats;
pub mod users;
pub mod videos;
pub mod webhooks;
pub mod websocket;

pub fn router(state: AppState) -> Router {
    let api = api_router();

    Router::new()
        .merge(api.clone())
        .nest("/emby", api.clone())
        .nest("/mediabrowser", api)
        .with_state(state)
}

fn api_router() -> Router<AppState> {
    Router::new()
        .route(
            "/embywebsocket",
            axum::routing::get(websocket::emby_websocket_handler),
        )
        .route(
            "/socket",
            axum::routing::get(websocket::emby_websocket_handler),
        )
        // PB20：补充 `/websocket`、`/Socket` 别名。Emby SDK 在不同客户端上对端点大小写
        // 与拼写有差异（Web/原生/移动），全部指到同一个 handler，避免 404 升级失败。
        .route(
            "/websocket",
            axum::routing::get(websocket::emby_websocket_handler),
        )
        .route(
            "/Socket",
            axum::routing::get(websocket::emby_websocket_handler),
        )
        .merge(system::router())
        .merge(startup::router())
        .merge(users::router())
        .merge(connect::router())
        .merge(items::router())
        .merge(images::router())
        .merge(videos::router())
        .merge(shows::router())
        .merge(sessions::router())
        .merge(compat::router())
        .merge(admin::router())
        .merge(genres::router())
        .merge(persons::router())
        .merge(metadata_routes::router())
        .merge(misc::router())
        .merge(devices::router())
        .merge(scheduled_tasks::router())
        .merge(collections::router())
        .merge(live_streams::router())
        .merge(playlists::router())
        .merge(trickplay::router())
        .merge(media_segments::router())
        .merge(remote_emby::router())
        .merge(translation::router())
        .merge(webhooks::router())
        .merge(usage_stats::router())
}

#[cfg(test)]
mod tests {
    #[test]
    fn api_router_builds_without_route_conflicts() {
        let _router = super::api_router();
    }
}
