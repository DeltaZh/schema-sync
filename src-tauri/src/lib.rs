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
pub mod scan_cache;
pub mod schema;
pub mod sql_gen;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
