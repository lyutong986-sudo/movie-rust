use std::{env, net::SocketAddr};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: u32,
    pub host: String,
    pub port: u16,
    pub server_name: String,
    pub server_id: Uuid,
    pub default_admin: String,
    pub default_password: String,
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
            default_admin: env::var("APP_DEFAULT_ADMIN").unwrap_or_else(|_| "admin".to_string()),
            default_password: env::var("APP_DEFAULT_PASSWORD")
                .unwrap_or_else(|_| "admin123".to_string()),
        })
    }

    pub fn bind_addr(&self) -> anyhow::Result<SocketAddr> {
        Ok(format!("{}:{}", self.host, self.port).parse()?)
    }
}
