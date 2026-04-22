use anyhow::{Context, Result};
use sqlx::{migrate::Migrator, postgres::PgPoolOptions, FromRow};
use std::{collections::HashMap, env, path::Path};

#[derive(Debug, FromRow)]
struct AppliedMigrationRow {
    version: i64,
    success: bool,
    checksum: Vec<u8>,
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://movie:movie@localhost:5432/movie_rust".to_string());
    let dry_run = env::var("MIGRATION_FIX_DRY_RUN")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .context("连接 PostgreSQL 失败")?;

    let migrator = Migrator::new(Path::new("./migrations"))
        .await
        .context("加载本地迁移文件失败")?;

    let local_migrations: HashMap<i64, Vec<u8>> = migrator
        .iter()
        .filter(|migration| !migration.migration_type.is_down_migration())
        .map(|migration| (migration.version, migration.checksum.clone().into_owned()))
        .collect();

    println!(
        "已加载 {} 个本地迁移，开始检查 _sqlx_migrations ...",
        local_migrations.len()
    );

    let applied_rows = sqlx::query_as::<_, AppliedMigrationRow>(
        r#"
        SELECT version, success, checksum
        FROM _sqlx_migrations
        ORDER BY version
        "#,
    )
    .fetch_all(&pool)
    .await
    .context("读取 _sqlx_migrations 失败")?;

    let mut deleted_dirty = 0_u64;
    let mut repaired_checksums = 0_u64;
    let mut missing_local_versions = Vec::new();

    for row in applied_rows {
        if !row.success {
            println!("发现 dirty 迁移记录：version={}，准备删除后重跑。", row.version);
            if !dry_run {
                let result = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
                    .bind(row.version)
                    .execute(&pool)
                    .await
                    .with_context(|| format!("删除 dirty 迁移记录失败：version={}", row.version))?;
                deleted_dirty += result.rows_affected();
            }
            continue;
        }

        match local_migrations.get(&row.version) {
            Some(local_checksum) if row.checksum != *local_checksum => {
                println!("发现 checksum 不匹配：version={}，准备修正为当前本地迁移内容。", row.version);
                if !dry_run {
                    sqlx::query(
                        r#"
                        UPDATE _sqlx_migrations
                        SET checksum = $2
                        WHERE version = $1
                        "#,
                    )
                    .bind(row.version)
                    .bind(local_checksum)
                    .execute(&pool)
                    .await
                    .with_context(|| format!("修正迁移 checksum 失败：version={}", row.version))?;
                    repaired_checksums += 1;
                }
            }
            Some(_) => {}
            None => {
                missing_local_versions.push(row.version);
            }
        }
    }

    ensure_update_updated_at_function(&pool, dry_run).await?;

    if !missing_local_versions.is_empty() {
        println!(
            "警告：数据库中存在本地已不存在的迁移版本：{}",
            missing_local_versions
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        );
        println!("如果这些版本是历史遗留文件，请先确认是否需要保留，再决定是否手工清理 _sqlx_migrations。");
    }

    if dry_run {
        println!(
            "dry-run 完成：将删除 {} 条 dirty 记录，修正 {} 条 checksum。",
            deleted_dirty, repaired_checksums
        );
        return Ok(());
    }

    println!(
        "修复完成：删除 {} 条 dirty 记录，修正 {} 条 checksum。",
        deleted_dirty, repaired_checksums
    );
    println!("开始执行正式迁移...");

    migrator
        .run(&pool)
        .await
        .context("执行数据库迁移失败")?;

    println!("数据库迁移完成。");
    Ok(())
}

async fn ensure_update_updated_at_function(
    pool: &sqlx::PgPool,
    dry_run: bool,
) -> Result<()> {
    let function_exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM pg_proc WHERE proname = 'update_updated_at_column')",
    )
    .fetch_one(pool)
    .await
    .context("检查 update_updated_at_column 函数失败")?;

    if function_exists.0 {
        println!("update_updated_at_column 函数已存在。");
        return Ok(());
    }

    println!("缺少 update_updated_at_column 函数，准备补齐。");

    if !dry_run {
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
        .execute(pool)
        .await
        .context("创建 update_updated_at_column 函数失败")?;
    }

    Ok(())
}
