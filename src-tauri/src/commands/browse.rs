//! 库表浏览

use tauri::State;

use crate::mysql;
use crate::schema::{TableSchema, TableSummary};

use super::state::AppState;
use super::util::{decrypt_password, find_connection};

#[tauri::command]
pub async fn list_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    let (conn, password) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let conn = find_connection(&config, &connection_id)?.clone();
        let password = decrypt_password(&state.store, &conn)?;
        (conn, password)
    };
    mysql::list_databases(&conn, &password)
        .await
        .map_err(|e| e.to_string())
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
