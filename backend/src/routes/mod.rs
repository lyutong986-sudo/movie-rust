use crate::state::AppState;
use axum::Router;

pub mod admin;
pub mod images;
pub mod items;
pub mod sessions;
pub mod system;
pub mod users;
pub mod videos;

pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(system::router())
        .merge(users::router())
        .merge(items::router())
        .merge(images::router())
        .merge(videos::router())
        .merge(sessions::router())
        .merge(admin::router())
        .with_state(state)
}
