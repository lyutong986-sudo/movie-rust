use std::{env, net::SocketAddr, path::PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: u32,
    pub host: String,
    pub port: u16,
    pub server_name: String,
    pub server_id: Uuid,
    pub static_dir: PathBuf,
    pub tmdb_api_key: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let server_name = env::var("APP_SERVER_NAME").unwrap_or_else(|_| "Movie Rust".to_string());
        let server_id = env::var("APP_SERVER_ID")
            .ok()
            .and_then(|value| Uuid::parse_str(&value).ok())
            .unwrap_or_else(|| Uuid::new_v5(&Uuid::NAMESPACE_DNS, server_name.as_bytes()));

        Ok(Self {
            database_url: env::var("DATABASE_URL")
                .unwrap_or_else(|_| "postgres://movie:movie@localhost:5432/movie_rust".to_string()),
            database_max_connections: env::var("DATABASE_MAX_CONNECTIONS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8),
            host: env::var("APP_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
            port: env::var("APP_PORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(8096),
            server_name,
            server_id,
            static_dir: env::var("APP_STATIC_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from("frontend/dist")),
            tmdb_api_key: env::var("TMDB_API_KEY").ok(),
        })
    }

    pub fn bind_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(format!("{}:{}", self.host, self.port).parse()?)
    }
}
