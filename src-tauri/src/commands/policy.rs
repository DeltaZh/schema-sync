//! DDL 投放策略：读取 / 保存

use tauri::State;

use crate::ddl_policy::{DdlPolicy, DdlPolicyRow};

use super::state::AppState;

#[tauri::command]
pub fn get_ddl_policy(state: State<'_, AppState>) -> Result<Vec<DdlPolicyRow>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.ddl_policy.to_rows())
}

#[tauri::command]
pub fn save_ddl_policy(
    state: State<'_, AppState>,
    rows: Vec<DdlPolicyRow>,
) -> Result<Vec<DdlPolicyRow>, String> {
    let policy = DdlPolicy::from_rows(&rows);
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.ddl_policy = policy;
    state.store.save(config.clone()).map_err(|e| e.to_string())?;
    Ok(config.ddl_policy.to_rows())
}

#[tauri::command]
pub fn reset_ddl_policy(state: State<'_, AppState>) -> Result<Vec<DdlPolicyRow>, String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.ddl_policy = DdlPolicy::default();
    state.store.save(config.clone()).map_err(|e| e.to_string())?;
    Ok(config.ddl_policy.to_rows())
}
