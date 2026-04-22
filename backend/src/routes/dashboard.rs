use axum::{
    extract::Query,
    http::StatusCode,
    response::{IntoResponse, Redirect},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tower_http::services::ServeDir;

use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct ConfigurationPageQuery {
    #[serde(rename = "Name")]
    name: Option<String>,
}

#[derive(Debug, Serialize)]
struct ConfigurationPageInfo {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "EnableInMainMenu")]
    enable_in_main_menu: bool,
    #[serde(rename = "MenuSection")]
    menu_section: Option<String>,
    #[serde(rename = "MenuIcon")]
    menu_icon: Option<String>,
    #[serde(rename = "DisplayName")]
    display_name: String,
    #[serde(rename = "ConfigurationPageType")]
    configuration_page_type: String,
    #[serde(rename = "PluginId")]
    plugin_id: String,
}

pub fn router(static_dir: PathBuf) -> Router<AppState> {
    let dashboard_dir = dashboard_dir(&static_dir);

    Router::new()
        .route("/", get(root_redirect))
        .route("/web", get(web_redirect))
        .route("/web/index.html", get(web_index_redirect))
        .route("/web/ConfigurationPages", get(configuration_pages))
        .route("/web/ConfigurationPage", get(configuration_page))
        .route("/favicon.ico", get(favicon_redirect))
        .route("/robots.txt", get(robots_redirect))
        .nest_service("/web", ServeDir::new(dashboard_dir))
}

fn dashboard_dir(static_dir: &std::path::Path) -> PathBuf {
    let nested_dashboard_dir = static_dir.join("dashboard-ui");
    if nested_dashboard_dir.is_dir() {
        nested_dashboard_dir
    } else {
        static_dir.to_path_buf()
    }
}

async fn root_redirect() -> Redirect {
    Redirect::temporary("/web/index.html")
}

async fn web_redirect() -> Redirect {
    Redirect::temporary("/web/index.html")
}

async fn web_index_redirect() -> Redirect {
    Redirect::temporary("/web/")
}

async fn favicon_redirect() -> Redirect {
    Redirect::temporary("/web/favicon.ico")
}

async fn robots_redirect() -> Redirect {
    Redirect::temporary("/web/robots.txt")
}

async fn configuration_pages() -> Json<Vec<ConfigurationPageInfo>> {
    Json(Vec::new())
}

async fn configuration_page(
    Query(query): Query<ConfigurationPageQuery>,
) -> impl IntoResponse {
    let missing_name = query
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none();

    let message = if missing_name {
        "Configuration page name is required"
    } else {
        "Configuration page is not implemented yet"
    };

    (StatusCode::NOT_FOUND, message)
}
