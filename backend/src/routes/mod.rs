use crate::state::AppState;
use axum::Router;

pub mod admin;
pub mod images;
pub mod items;
pub mod sessions;
pub mod startup;
pub mod system;
pub mod users;
pub mod videos;

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
        .merge(system::router())
        .merge(startup::router())
        .merge(users::router())
        .merge(items::router())
        .merge(images::router())
        .merge(videos::router())
        .merge(sessions::router())
        .merge(admin::router())
}
