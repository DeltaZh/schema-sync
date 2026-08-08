//! 库表浏览

use std::time::Duration;

use tauri::State;

use crate::mysql;
use crate::schema::{TableSchema, TableSummary};

use super::state::AppState;
use super::util::{decrypt_password, find_connection};

const LIST_DB_TIMEOUT: Duration = Duration::from_secs(12);

async fn list_databases_timed(
    conn: &crate::models::ConnectionConfig,
    password: &str,
) -> Result<Vec<String>, String> {
    match tokio::time::timeout(LIST_DB_TIMEOUT, mysql::list_databases(conn, password)).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err(format!(
            "拉取库列表超时（{}s）。请检查主机/端口/账号，或先点「测连通」",
            LIST_DB_TIMEOUT.as_secs()
        )),
    }
}

#[tauri::command]
pub async fn list_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    if connection_id.trim().is_empty() {
        return Err("connectionId 为空，无法拉取库列表".into());
    }
    let (conn, password, visible) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let conn = find_connection(&config, &connection_id)?.clone();
        let password = decrypt_password(&state.store, &conn)?;
        let visible = conn.visible_databases.clone();
        (conn, password, visible)
    };
    // 未配置可见库：不拉全库，避免树被系统库淹没；前端应引导选择
    if visible.is_empty() {
        return Ok(Vec::new());
    }
    let all = list_databases_timed(&conn, &password).await?;
    Ok(filter_visible_databases(&all, &visible))
}

/// 列出服务器上全部业务库（供「选择可见库」对话框使用，不受白名单限制）
#[tauri::command]
pub async fn list_all_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    if connection_id.trim().is_empty() {
        return Err("connectionId 为空，无法拉取库列表".into());
    }
    let (conn, password) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let conn = find_connection(&config, &connection_id)?.clone();
        let password = decrypt_password(&state.store, &conn)?;
        (conn, password)
    };
    eprintln!(
        "[schema-sync] list_all_databases conn={} host={}:{}",
        conn.id, conn.host, conn.port
    );
    list_databases_timed(&conn, &password).await
}

#[tauri::command]
pub async fn list_tables(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableSummary>, String> {
    let (conn, password) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let conn = find_connection(&config, &connection_id)?.clone();
        let password = decrypt_password(&state.store, &conn)?;
        (conn, password)
    };
    mysql::list_tables(&conn, &password, &database)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_table_structure(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
    table: String,
) -> Result<Option<TableSchema>, String> {
    let (conn, password) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let conn = find_connection(&config, &connection_id)?.clone();
        let password = decrypt_password(&state.store, &conn)?;
        (conn, password)
    };
    mysql::fetch_table_schema(&conn, &password, &database, &table)
        .await
        .map_err(|e| e.to_string())
}

/// 按可见库白名单过滤（白名单为空 → 空结果）
pub fn filter_visible_databases(all: &[String], visible: &[String]) -> Vec<String> {
    if visible.is_empty() {
        return Vec::new();
    }
    let allow: std::collections::HashSet<&str> =
        visible.iter().map(String::as_str).collect();
    all.iter()
        .filter(|n| allow.contains(n.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::filter_visible_databases;

    #[test]
    fn empty_visible_means_show_none() {
        let all = vec!["a".into(), "b".into()];
        assert!(filter_visible_databases(&all, &[]).is_empty());
    }

    #[test]
    fn filters_to_whitelist_preserving_server_order() {
        let all = vec!["z".into(), "a".into(), "m".into()];
        let visible = vec!["m".into(), "a".into(), "missing".into()];
        assert_eq!(
            filter_visible_databases(&all, &visible),
            vec!["a".to_string(), "m".to_string()]
        );
    }
}
