//! MySQL 连接、浏览与 Schema 抽取

use std::time::Duration;

use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions, MySqlRow, MySqlSslMode};
use sqlx::{MySqlPool, Row};
use thiserror::Error;

use crate::models::ConnectionConfig;
use crate::schema::{
    columns_from_rows, indexes_from_stats_rows, tables_from_rows, ColumnRow, StatsRow, TableRow,
    TableSchema, TableSummary,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Debug, Error)]
pub enum MysqlError {
    #[error("数据库错误: {0}")]
    Db(#[from] sqlx::Error),
    #[error("{0}")]
    Message(String),
}

/// 规范化主机：localhost → 127.0.0.1，避免 macOS 上 IPv6 优先导致久挂
pub fn normalize_host(host: &str) -> &str {
    if host.eq_ignore_ascii_case("localhost") {
        "127.0.0.1"
    } else {
        host
    }
}

/// 构建 MySQL 连接 URL（密码与标识符做百分号编码；测试/日志用）
pub fn connection_url(cfg: &ConnectionConfig, password_plain: &str, database: Option<&str>) -> String {
    let user = urlencoding::encode(&cfg.user);
    let pass = urlencoding::encode(password_plain);
    let host = normalize_host(&cfg.host);
    let base = format!("mysql://{user}:{pass}@{host}:{}/", cfg.port);
    match database {
        Some(db) if !db.is_empty() => format!("{base}{}", urlencoding::encode(db)),
        _ => base,
    }
}

fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

/// information_schema 在部分 MySQL/字符集下会以 VARBINARY 返回标识符，不能直接 get::<String>
fn mysql_text(row: &MySqlRow, col: &str) -> Result<String, MysqlError> {
    if let Ok(s) = row.try_get::<String, _>(col) {
        return Ok(s);
    }
    if let Ok(bytes) = row.try_get::<Vec<u8>, _>(col) {
        return Ok(String::from_utf8_lossy(&bytes).into_owned());
    }
    Err(MysqlError::Message(format!(
        "无法解码列 `{col}`（既非 VARCHAR 也非 VARBINARY）"
    )))
}

fn mysql_text_opt(row: &MySqlRow, col: &str) -> Result<Option<String>, MysqlError> {
    match row.try_get::<Option<Vec<u8>>, _>(col) {
        Ok(Some(bytes)) => Ok(Some(String::from_utf8_lossy(&bytes).into_owned())),
        Ok(None) => Ok(None),
        Err(_) => match row.try_get::<Option<String>, _>(col) {
            Ok(v) => Ok(v),
            Err(e) => Err(MysqlError::Db(e)),
        },
    }
}

fn connect_options(
    cfg: &ConnectionConfig,
    password_plain: &str,
    database: Option<&str>,
) -> MySqlConnectOptions {
    let mut opts = MySqlConnectOptions::new()
        .host(normalize_host(&cfg.host))
        .port(cfg.port)
        .username(&cfg.user)
        .password(password_plain)
        // 内网运维默认不走 SSL；Preferred 在部分环境会长时间卡住
        .ssl_mode(MySqlSslMode::Disabled)
        .charset("utf8mb4");
    if let Some(db) = database {
        if !db.is_empty() {
            opts = opts.database(db);
        }
    }
    opts
}

async fn open_pool(
    cfg: &ConnectionConfig,
    password_plain: &str,
    database: Option<&str>,
) -> Result<MySqlPool, MysqlError> {
    let opts = connect_options(cfg, password_plain, database);
    let connect = MySqlPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(CONNECT_TIMEOUT)
        .connect_with(opts);
    match tokio::time::timeout(CONNECT_TIMEOUT, connect).await {
        Ok(Ok(pool)) => Ok(pool),
        Ok(Err(e)) => Err(MysqlError::Db(e)),
        Err(_) => Err(MysqlError::Message(format!(
            "连接 {}:{} 超时（{}s）。请确认主机/端口可达，或先点「测连通」",
            normalize_host(&cfg.host),
            cfg.port,
            CONNECT_TIMEOUT.as_secs()
        ))),
    }
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

pub fn is_system_schema(name: &str) -> bool {
    matches!(
        name,
        "information_schema" | "mysql" | "performance_schema" | "sys"
    )
}

/// 列出业务库名（按名称排序，排除系统库）
pub async fn list_databases(
    conn_cfg: &ConnectionConfig,
    password_plain: &str,
) -> Result<Vec<String>, MysqlError> {
    let pool = open_pool(conn_cfg, password_plain, None).await?;
    // CAST 为 CHAR，并配合 mysql_text 兼容 VARBINARY 解码
    let rows = sqlx::query(
        "SELECT CAST(SCHEMA_NAME AS CHAR CHARACTER SET utf8mb4) AS name \
         FROM information_schema.SCHEMATA ORDER BY SCHEMA_NAME",
    )
    .fetch_all(&pool)
    .await?;
    let mut names = Vec::with_capacity(rows.len());
    for r in &rows {
        let n = mysql_text(r, "name")?;
        if !is_system_schema(&n) {
            names.push(n);
        }
    }
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
        SELECT CAST(TABLE_NAME AS CHAR CHARACTER SET utf8mb4) AS table_name,
               CAST(IFNULL(TABLE_COMMENT, '') AS CHAR CHARACTER SET utf8mb4) AS table_comment
        FROM information_schema.TABLES
        WHERE TABLE_SCHEMA = ?
          AND TABLE_TYPE = 'BASE TABLE'
        ORDER BY TABLE_NAME
        "#,
    )
    .bind(database)
    .fetch_all(&pool)
    .await?;

    let mut table_rows = Vec::with_capacity(rows.len());
    for r in &rows {
        table_rows.push(map_table_row(r)?);
    }
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
        SELECT CAST(IFNULL(TABLE_COMMENT, '') AS CHAR CHARACTER SET utf8mb4) AS table_comment
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
    let table_comment = mysql_text(&comment_row, "table_comment")?;

    let col_rows = sqlx::query(
        r#"
        SELECT CAST(COLUMN_NAME AS CHAR CHARACTER SET utf8mb4) AS column_name,
               CAST(COLUMN_TYPE AS CHAR CHARACTER SET utf8mb4) AS column_type,
               CAST(IS_NULLABLE AS CHAR CHARACTER SET utf8mb4) AS is_nullable,
               CAST(COLUMN_DEFAULT AS CHAR CHARACTER SET utf8mb4) AS column_default,
               CAST(IFNULL(COLUMN_COMMENT, '') AS CHAR CHARACTER SET utf8mb4) AS column_comment,
               CAST(IFNULL(EXTRA, '') AS CHAR CHARACTER SET utf8mb4) AS extra
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
    let mut mapped_cols = Vec::with_capacity(col_rows.len());
    for r in &col_rows {
        mapped_cols.push(map_column_row(r)?);
    }
    let columns = columns_from_rows(&mapped_cols);

    let stats_rows = sqlx::query(
        r#"
        SELECT CAST(INDEX_NAME AS CHAR CHARACTER SET utf8mb4) AS index_name,
               CAST(COLUMN_NAME AS CHAR CHARACTER SET utf8mb4) AS column_name,
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
    let mut mapped_stats = Vec::with_capacity(stats_rows.len());
    for r in &stats_rows {
        mapped_stats.push(map_stats_row(r)?);
    }
    let indexes = indexes_from_stats_rows(&mapped_stats);

    let show_sql = format!(
        "SHOW CREATE TABLE {}.{}",
        quote_ident(database),
        quote_ident(table)
    );
    let create_row = sqlx::query(&show_sql).fetch_one(&pool).await?;
    // MySQL 返回列名为 "Create Table"；兼容 String / VARBINARY
    let create_sql = match create_row.try_get::<String, _>("Create Table") {
        Ok(s) => s,
        Err(_) => match create_row.try_get::<Vec<u8>, _>("Create Table") {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(_) => match create_row.try_get::<String, _>(1) {
                Ok(s) => s,
                Err(_) => {
                    let b: Vec<u8> = create_row.try_get(1)?;
                    String::from_utf8_lossy(&b).into_owned()
                }
            },
        },
    };

    pool.close().await;
    Ok(Some(TableSchema {
        name: table.to_string(),
        comment: table_comment,
        columns,
        indexes,
        create_sql,
    }))
}

fn map_table_row(r: &MySqlRow) -> Result<TableRow, MysqlError> {
    Ok(TableRow {
        table_name: mysql_text(r, "table_name")?,
        table_comment: mysql_text(r, "table_comment")?,
    })
}

fn map_column_row(r: &MySqlRow) -> Result<ColumnRow, MysqlError> {
    Ok(ColumnRow {
        column_name: mysql_text(r, "column_name")?,
        column_type: mysql_text(r, "column_type")?,
        is_nullable: mysql_text(r, "is_nullable")?,
        column_default: mysql_text_opt(r, "column_default")?,
        column_comment: mysql_text(r, "column_comment")?,
        extra: mysql_text(r, "extra")?,
    })
}

fn map_stats_row(r: &MySqlRow) -> Result<StatsRow, MysqlError> {
    Ok(StatsRow {
        index_name: mysql_text(r, "index_name")?,
        column_name: mysql_text(r, "column_name")?,
        non_unique: r.try_get::<i64, _>("non_unique").map_err(MysqlError::Db)?,
        seq_in_index: r
            .try_get::<u32, _>("seq_in_index")
            .or_else(|_| r.try_get::<i64, _>("seq_in_index").map(|v| v as u32))
            .map_err(MysqlError::Db)?,
    })
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
            visible_databases: Vec::new(),
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

    #[test]
    fn system_schemas_are_filtered() {
        assert!(is_system_schema("mysql"));
        assert!(is_system_schema("information_schema"));
        assert!(!is_system_schema("order_2025_demo"));
    }

    #[test]
    fn normalize_localhost_to_loopback() {
        assert_eq!(normalize_host("localhost"), "127.0.0.1");
        assert_eq!(normalize_host("LOCALHOST"), "127.0.0.1");
        assert_eq!(normalize_host("10.0.0.1"), "10.0.0.1");
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
            visible_databases: Vec::new(),
        };
        let password = std::env::var("SCHEMA_SYNC_MYSQL_PASSWORD").unwrap_or_default();
        ping(&cfg, &password).await.expect("ping 应成功");
    }
}
