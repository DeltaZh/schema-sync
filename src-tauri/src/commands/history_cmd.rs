//! 执行历史列表

use tauri::State;

use crate::history::HistoryRecord;

use super::state::AppState;

#[tauri::command]
pub fn list_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<HistoryRecord>, String> {
    let limit = limit.unwrap_or(50);
    state
        .history
        .lock()
        .map_err(|e| e.to_string())?
        .list_recent(limit)
        .map_err(|e| e.to_string())
}
