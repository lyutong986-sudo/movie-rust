use crate::state::AppState;
use axum::Router;

pub mod admin;
pub mod client_compat;
pub mod compat;
pub mod dashboard;
pub mod genres;
pub mod images;
pub mod integrations;
pub mod items;
pub mod livetv;
pub mod management;
pub mod metadata_routes;
pub mod persons;
pub mod sessions;
pub mod shows;
pub mod startup;
pub mod system;
pub mod users;
pub mod videos;
pub mod websocket;

pub fn router(state: AppState) -> Router {
    let api = api_router();
    let dashboard = dashboard::router(state.config.static_dir.clone());

    Router::new()
        .merge(dashboard)
        .merge(api.clone())
        .nest("/emby", api.clone())
        .nest("/mediabrowser", api)
        .with_state(state)
}

fn api_router() -> Router<AppState> {
    Router::new()
        .route("/embywebsocket", axum::routing::get(websocket::emby_websocket_handler))
        .merge(client_compat::router())
        .merge(system::router())
        .merge(startup::router())
        .merge(users::router())
        .merge(items::router())
        .merge(livetv::router())
        .merge(integrations::router())
        .merge(management::router())
        .merge(images::router())
        .merge(videos::router())
        .merge(shows::router())
        .merge(sessions::router())
        .merge(compat::router())
        .merge(admin::router())
        .merge(genres::router())
        .merge(persons::router())
        .merge(metadata_routes::router())
}

#[cfg(test)]
mod tests {
    #[test]
    fn api_router_builds_without_route_conflicts() {
        let _router = super::api_router();
    }
}
