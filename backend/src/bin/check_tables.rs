use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    
    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://movie:movie@localhost:5432/movie_rust".to_string());
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("连接 PostgreSQL 失败")?;
    
    // 检查 media_streams 表是否存在
    let table_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'media_streams')"
    )
    .fetch_one(&pool)
    .await?;
    
    println!("media_streams 表存在: {}", table_exists.0);
    
    // 检查 persons 表是否存在
    let persons_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'persons')"
    )
    .fetch_one(&pool)
    .await?;
    
    println!("persons 表存在: {}", persons_exists.0);
    
    // 检查 person_roles 表是否存在
    let person_roles_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = 'person_roles')"
    )
    .fetch_one(&pool)
    .await?;
    
    println!("person_roles 表存在: {}", person_roles_exists.0);
    
    // 检查 update_updated_at_column 函数是否存在
    let function_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_proc WHERE proname = 'update_updated_at_column')"
    )
    .fetch_one(&pool)
    .await?;
    
    println!("update_updated_at_column 函数存在: {}", function_exists.0);
    
    Ok(())
}