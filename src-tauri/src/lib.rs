pub mod commands;
pub mod config;
pub mod crypto;
pub mod ddl_guard;
pub mod diff;
pub mod exec;
pub mod history;
pub mod models;
pub mod mysql;
pub mod naming;
pub mod paths;
pub mod preview_cache;
pub mod scan_cache;
pub mod schema;
pub mod sql_gen;

use tauri::Manager;

use commands::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let tauri_dir = app.path().app_data_dir().ok();
            let data_dir = paths::resolve_data_dir(tauri_dir);
            let state = AppState::open(&data_dir).map_err(|e| {
                Box::<dyn std::error::Error>::from(e.to_string())
            })?;
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_connections,
            commands::upsert_connection,
            commands::delete_connection,
            commands::ping_connection,
            commands::set_visible_databases,
            commands::list_databases,
            commands::list_all_databases,
            commands::list_tables,
            commands::get_table_structure,
            commands::list_rules,
            commands::save_rules,
            commands::expand_rule_targets,
            commands::baseline_scan,
            commands::baseline_execute,
            commands::ddl_preview,
            commands::ddl_execute,
            commands::list_history,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
