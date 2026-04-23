use anyhow::Context;
use sqlx::migrate;
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

    // 删除失败的迁移记录（版本5）
    let result = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = 5 AND success = false")
        .execute(&pool)
        .await?;

    println!("删除了 {} 条失败的迁移记录", result.rows_affected());

    // 检查函数是否存在，如果不存在则创建
    let function_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_proc WHERE proname = 'update_updated_at_column')",
    )
    .fetch_one(&pool)
    .await?;

    if !function_exists.0 {
        println!("创建 update_updated_at_column 函数...");
        sqlx::query(
            r#"
            CREATE OR REPLACE FUNCTION update_updated_at_column()
            RETURNS TRIGGER AS $$
            BEGIN
                NEW.updated_at = NOW();
                RETURN NEW;
            END;
            $$ language 'plpgsql'
            "#,
        )
        .execute(&pool)
        .await?;
        println!("函数创建成功");
    } else {
        println!("函数已存在");
    }

    println!("修复完成");

    // 运行迁移
    println!("运行数据库迁移...");
    migrate!("./migrations")
        .run(&pool)
        .await
        .context("执行数据库迁移失败")?;
    println!("数据库迁移成功完成");

    Ok(())
}
