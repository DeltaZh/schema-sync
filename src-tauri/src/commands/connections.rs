//! 连接 CRUD / ping

use tauri::State;

use crate::models::ConnectionConfig;
use crate::mysql;

use super::state::AppState;
use super::util::{decrypt_password, find_connection, is_password_unchanged};

#[tauri::command]
pub fn list_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(state.store.public_connections(&config))
}

#[tauri::command]
pub fn upsert_connection(
    state: State<'_, AppState>,
    mut connection: ConnectionConfig,
) -> Result<ConnectionConfig, String> {
    if connection.id.trim().is_empty() {
        return Err("连接 id 不能为空".into());
    }
    let mut config = state.config.lock().map_err(|e| e.to_string())?;

    if let Some(existing) = config.connections.iter().find(|c| c.id == connection.id) {
        if is_password_unchanged(&connection.password) {
            connection.password = existing.password.clone();
        }
        if let Some(slot) = config.connections.iter_mut().find(|c| c.id == connection.id) {
            *slot = connection.clone();
        }
    } else {
        if is_password_unchanged(&connection.password) {
            connection.password.clear();
        }
        config.connections.push(connection.clone());
    }

    state.store.save(config.clone()).map_err(|e| e.to_string())?;
    // 回读落盘结果，保证内存中为密文，与磁盘一致
    *config = state.store.load().map_err(|e| e.to_string())?;
    let saved = find_connection(&config, &connection.id)?.clone();
    Ok(saved.public_view())
}

/// 设置连接树可见库白名单（空列表表示尚未选择，树不展示库）
#[tauri::command]
pub fn set_visible_databases(
    state: State<'_, AppState>,
    id: String,
    databases: Vec<String>,
) -> Result<ConnectionConfig, String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let conn = config
        .connections
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| format!("未知连接: {id}"))?;
    let mut seen = std::collections::BTreeSet::new();
    let mut cleaned = Vec::new();
    for name in databases {
        let name = name.trim().to_string();
        if name.is_empty() || !seen.insert(name.clone()) {
            continue;
        }
        cleaned.push(name);
    }
    cleaned.sort();
    conn.visible_databases = cleaned;
    state.store.save(config.clone()).map_err(|e| e.to_string())?;
    *config = state.store.load().map_err(|e| e.to_string())?;
    Ok(find_connection(&config, &id)?.public_view())
}

#[tauri::command]
pub fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    let before = config.connections.len();
    config.connections.retain(|c| c.id != id);
    if config.connections.len() == before {
        return Err(format!("未知连接: {id}"));
    }
    state.store.save(config.clone()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn ping_connection(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let (conn, password) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let conn = find_connection(&config, &id)?.clone();
        let password = decrypt_password(&state.store, &conn)?;
        (conn, password)
    };
    mysql::ping(&conn, &password)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::state::AppState;
    use crate::crypto::PasswordCrypto;
    use crate::models::AppConfig;

    fn sample(password: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: "c1".into(),
            name: "本地".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: password.into(),
            enabled: true,
            remark: String::new(),
            visible_databases: Vec::new(),
        }
    }

    #[test]
    fn public_list_never_returns_plaintext() {
        let (_dir, state) = AppState::open_temp();
        {
            let mut cfg = state.config.lock().unwrap();
            *cfg = AppConfig {
                connections: vec![sample("secret")],
                rules: vec![],
            };
            state.store.save(cfg.clone()).unwrap();
            *cfg = state.store.load().unwrap();
        }
        let public = state.store.public_connections(&state.config.lock().unwrap());
        assert_eq!(public[0].password, "********");
        assert!(!public[0].password.contains("secret"));
        let stored = &state.config.lock().unwrap().connections[0].password;
        assert!(PasswordCrypto::is_encrypted(stored) || stored == "secret");
    }
}
