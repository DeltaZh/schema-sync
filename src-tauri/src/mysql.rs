//! MySQL 连接、浏览与 Schema 抽取

use std::time::Duration;

use sqlx::mysql::{MySqlPoolOptions, MySqlRow};
use sqlx::{MySqlPool, Row};
use thiserror::Error;

use crate::models::ConnectionConfig;
use crate::schema::{
    columns_from_rows, indexes_from_stats_rows, tables_from_rows, ColumnRow, StatsRow, TableRow,
    TableSchema, TableSummary,
};

#[derive(Debug, Error)]
pub enum MysqlError {
    #[error("数据库错误: {0}")]
    Db(#[from] sqlx::Error),
}

/// 构建 MySQL 连接 URL（密码与标识符做百分号编码）
pub fn connection_url(cfg: &ConnectionConfig, password_plain: &str, database: Option<&str>) -> String {
    let user = urlencoding::encode(&cfg.user);
    let pass = urlencoding::encode(password_plain);
    let base = format!("mysql://{user}:{pass}@{}:{}/", cfg.host, cfg.port);
    match database {
        Some(db) if !db.is_empty() => format!("{base}{}", urlencoding::encode(db)),
        _ => base,
    }
}

fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

async fn open_pool(
    cfg: &ConnectionConfig,
    password_plain: &str,
    database: Option<&str>,
) -> Result<MySqlPool, MysqlError> {
    let url = connection_url(cfg, password_plain, database);
    let pool = MySqlPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await?;
    Ok(pool)
}

/// 测连通：能建立连接并执行 `SELECT 1`
pub async fn ping(conn_cfg: &ConnectionConfig, password_plain: &str) -> Result<(), MysqlError> {
    let pool = open_pool(conn_cfg, password_plain, None).await?;
    sqlx::query("SELECT 1").execute(&pool).await?;
    pool.close().await;
    Ok(())
}

/// 在指定库执行单条 SQL（供缓存 id 执行器调用）
pub async fn execute_sql(
    conn_cfg: &ConnectionConfig,
    password_plain: &str,
    database: &str,
    sql: &str,
) -> Result<(), MysqlError> {
    let pool = open_pool(conn_cfg, password_plain, Some(database)).await?;
    let result = sqlx::query(sql).execute(&pool).await;
    pool.close().await;
    result?;
    Ok(())
}

/// 列出全部库名（按名称排序）
pub async fn list_databases(
    conn_cfg: &ConnectionConfig,
    password_plain: &str,
) -> Result<Vec<String>, MysqlError> {
    let pool = open_pool(conn_cfg, password_plain, None).await?;
    let rows = sqlx::query(
        "SELECT SCHEMA_NAME AS name FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
    )
    .fetch_all(&pool)
    .await?;
    let names = rows
        .iter()
        .map(|r| r.get::<String, _>("name"))
        .collect();
    pool.close().await;
    Ok(names)
}

/// 列出库内基表（含表注释）
pub async fn list_tables(
    conn_cfg: &ConnectionConfig,
    password_plain: &str,
    database: &str,
) -> Result<Vec<TableSummary>, MysqlError> {
    let pool = open_pool(conn_cfg, password_plain, Some(database)).await?;
    let rows = sqlx::query(
        r#"
        SELECT TABLE_NAME AS table_name,
               IFNULL(TABLE_COMMENT, '') AS table_comment
        FROM information_schema.TABLES
        WHERE TABLE_SCHEMA = ?
          AND TABLE_TYPE = 'BASE TABLE'
        ORDER BY TABLE_NAME
        "#,
    )
    .bind(database)
    .fetch_all(&pool)
    .await?;

    let table_rows: Vec<TableRow> = rows.iter().map(map_table_row).collect();
    pool.close().await;
    Ok(tables_from_rows(&table_rows))
}

/// 抽取单表结构；表不存在返回 `None`
pub async fn fetch_table_schema(
    conn_cfg: &ConnectionConfig,
    password_plain: &str,
    database: &str,
    table: &str,
) -> Result<Option<TableSchema>, MysqlError> {
    let pool = open_pool(conn_cfg, password_plain, Some(database)).await?;

    let comment_row = sqlx::query(
        r#"
        SELECT IFNULL(TABLE_COMMENT, '') AS table_comment
        FROM information_schema.TABLES
        WHERE TABLE_SCHEMA = ?
          AND TABLE_NAME = ?
          AND TABLE_TYPE = 'BASE TABLE'
        LIMIT 1
        "#,
    )
    .bind(database)
    .bind(table)
    .fetch_optional(&pool)
    .await?;

    let Some(comment_row) = comment_row else {
        pool.close().await;
        return Ok(None);
    };
    let table_comment: String = comment_row.get("table_comment");

    let col_rows = sqlx::query(
        r#"
        SELECT COLUMN_NAME AS column_name,
               COLUMN_TYPE AS column_type,
               IS_NULLABLE AS is_nullable,
               COLUMN_DEFAULT AS column_default,
               IFNULL(COLUMN_COMMENT, '') AS column_comment,
               IFNULL(EXTRA, '') AS extra
        FROM information_schema.COLUMNS
        WHERE TABLE_SCHEMA = ?
          AND TABLE_NAME = ?
        ORDER BY ORDINAL_POSITION
        "#,
    )
    .bind(database)
    .bind(table)
    .fetch_all(&pool)
    .await?;
    let columns = columns_from_rows(&col_rows.iter().map(map_column_row).collect::<Vec<_>>());

    let stats_rows = sqlx::query(
        r#"
        SELECT INDEX_NAME AS index_name,
               COLUMN_NAME AS column_name,
               NON_UNIQUE AS non_unique,
               SEQ_IN_INDEX AS seq_in_index
        FROM information_schema.STATISTICS
        WHERE TABLE_SCHEMA = ?
          AND TABLE_NAME = ?
        ORDER BY INDEX_NAME, SEQ_IN_INDEX
        "#,
    )
    .bind(database)
    .bind(table)
    .fetch_all(&pool)
    .await?;
    let indexes = indexes_from_stats_rows(&stats_rows.iter().map(map_stats_row).collect::<Vec<_>>());

    let show_sql = format!(
        "SHOW CREATE TABLE {}.{}",
        quote_ident(database),
        quote_ident(table)
    );
    let create_row = sqlx::query(&show_sql).fetch_one(&pool).await?;
    // MySQL 返回列名为 "Create Table"
    let create_sql: String = create_row
        .try_get("Create Table")
        .or_else(|_| create_row.try_get::<String, _>(1))?;

    pool.close().await;
    Ok(Some(TableSchema {
        name: table.to_string(),
        comment: table_comment,
        columns,
        indexes,
        create_sql,
    }))
}

fn map_table_row(r: &MySqlRow) -> TableRow {
    TableRow {
        table_name: r.get("table_name"),
        table_comment: r.get("table_comment"),
    }
}

fn map_column_row(r: &MySqlRow) -> ColumnRow {
    ColumnRow {
        column_name: r.get("column_name"),
        column_type: r.get("column_type"),
        is_nullable: r.get("is_nullable"),
        column_default: r.get::<Option<String>, _>("column_default"),
        column_comment: r.get("column_comment"),
        extra: r.get("extra"),
    }
}

fn map_stats_row(r: &MySqlRow) -> StatsRow {
    StatsRow {
        index_name: r.get("index_name"),
        column_name: r.get("column_name"),
        non_unique: r.get::<i64, _>("non_unique"),
        seq_in_index: r.get::<u32, _>("seq_in_index"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_cfg() -> ConnectionConfig {
        ConnectionConfig {
            id: "c1".into(),
            name: "local".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: String::new(),
            enabled: true,
            remark: String::new(),
        }
    }

    #[test]
    fn connection_url_encodes_special_password() {
        let cfg = sample_cfg();
        let url = connection_url(&cfg, "p@ss/word?", Some("my db"));
        assert!(url.starts_with("mysql://root:p%40ss%2Fword%3F@127.0.0.1:3306/"));
        assert!(url.contains("my%20db"));
    }

    #[test]
    fn quote_ident_escapes_backticks() {
        assert_eq!(quote_ident("a`b"), "`a``b`");
    }

    /// 真连库冒烟；默认忽略，本地有 MySQL 时可 `cargo test -- --ignored`
    #[tokio::test]
    #[ignore = "需要真实 MySQL 实例"]
    async fn real_ping_smoke() {
        let cfg = ConnectionConfig {
            id: "ci".into(),
            name: "ci".into(),
            host: std::env::var("SCHEMA_SYNC_MYSQL_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
            port: std::env::var("SCHEMA_SYNC_MYSQL_PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3306),
            user: std::env::var("SCHEMA_SYNC_MYSQL_USER").unwrap_or_else(|_| "root".into()),
            password: String::new(),
            enabled: true,
            remark: String::new(),
        };
        let password = std::env::var("SCHEMA_SYNC_MYSQL_PASSWORD").unwrap_or_default();
        ping(&cfg, &password).await.expect("ping 应成功");
    }
}
