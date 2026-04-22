use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tower_http::services::{ServeDir, ServeFile};

use crate::{auth::AuthSession, error::AppError, repository};
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
    let dashboard_index = dashboard_dir.join("index.html");
    let dashboard_service =
        ServeDir::new(&dashboard_dir).not_found_service(ServeFile::new(dashboard_index));

    Router::new()
        .route("/", get(root_redirect))
        .route("/web/index.html", get(web_index_redirect))
        .route("/web/ConfigurationPages", get(configuration_pages))
        .route("/web/ConfigurationPage", get(configuration_page))
        .route("/favicon.ico", get(favicon_redirect))
        .route("/robots.txt", get(robots_redirect))
        .nest_service("/web", dashboard_service)
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

async fn web_index_redirect() -> Redirect {
    Redirect::temporary("/web/")
}

async fn favicon_redirect() -> Redirect {
    Redirect::temporary("/web/favicon.ico")
}

async fn robots_redirect() -> Redirect {
    Redirect::temporary("/web/robots.txt")
}

async fn configuration_pages(
    _session: AuthSession,
    State(state): State<AppState>,
) -> Result<Json<Vec<ConfigurationPageInfo>>, AppError> {
    let installed_plugins = repository::named_system_configuration(&state.pool, "installed_plugins")
        .await?
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();

    let mut pages = vec![ConfigurationPageInfo {
        name: "supporter-key".to_string(),
        enable_in_main_menu: true,
        menu_section: Some("Server".to_string()),
        menu_icon: Some("vpn_key".to_string()),
        display_name: "Supporter Key".to_string(),
        configuration_page_type: "PluginConfiguration".to_string(),
        plugin_id: "MBSupporter".to_string(),
    }];

    for plugin in installed_plugins {
        let plugin_id = plugin
            .get("Id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if plugin_id.is_empty() {
            continue;
        }

        let display_name = plugin
            .get("Name")
            .and_then(Value::as_str)
            .unwrap_or("Plugin")
            .to_string();

        pages.push(ConfigurationPageInfo {
            name: format!("plugin-{}-configuration", plugin_id.to_lowercase()),
            enable_in_main_menu: false,
            menu_section: Some("Server".to_string()),
            menu_icon: Some("extension".to_string()),
            display_name,
            configuration_page_type: "PluginConfiguration".to_string(),
            plugin_id,
        });
    }

    Ok(Json(pages))
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

    if missing_name {
        return (
            StatusCode::BAD_REQUEST,
            Html("<div style=\"padding:2em;\">Configuration page name is required.</div>".to_string()),
        );
    }

    let page_name = query.name.unwrap_or_default();
    let title = if page_name.eq_ignore_ascii_case("supporter-key") {
        "Supporter Key"
    } else {
        "Plugin Configuration"
    };

    let body = format!(
        "<div class=\"page\" style=\"padding:2em;max-width:720px;\">\
            <h1 style=\"margin:0 0 .5em;\">{title}</h1>\
            <p style=\"margin:0 0 1em;\">Configuration page <strong>{page_name}</strong> is available for compatibility routing.</p>\
            <p style=\"margin:0;\">Plugin-specific settings can be stored through <code>/Plugins/{{id}}/Configuration</code> and will persist in the server database.</p>\
        </div>"
    );

    (StatusCode::OK, Html(body))
}
