//! 应用数据目录解析

use std::path::PathBuf;

/// 环境变量：测试/开发期覆盖数据目录
pub const DATA_DIR_ENV: &str = "SCHEMA_SYNC_DATA";

/// 解析本机应用数据目录。
///
/// 优先读取 `SCHEMA_SYNC_DATA`；未设置时：
/// 1. 使用调用方传入的 Tauri `app.path().app_data_dir()`（生产 setup）
/// 2. 再回落 `dirs::data_dir()/schema-sync`
pub fn app_data_dir() -> PathBuf {
    resolve_data_dir(None)
}

/// 与 [`app_data_dir`] 相同，但可注入 Tauri Application Support 路径作为次优先候选。
pub fn resolve_data_dir(tauri_app_data: Option<PathBuf>) -> PathBuf {
    if let Ok(override_dir) = std::env::var(DATA_DIR_ENV) {
        let path = PathBuf::from(override_dir);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    if let Some(path) = tauri_app_data {
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    default_app_data_dir()
}

/// 平台默认数据目录（不依赖 Tauri Runtime，便于单测与库内使用）
pub fn default_app_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("schema-sync")
}

/// 配置文件路径
pub fn config_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("config.json")
}

/// 密钥文件路径
pub fn key_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join(".schema-sync.key")
}

/// 执行历史文件路径（JSONL）
pub fn history_file_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("history.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_sync_data_env_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        std::env::set_var(DATA_DIR_ENV, &path);
        assert_eq!(app_data_dir(), path);
        std::env::remove_var(DATA_DIR_ENV);
    }

    #[test]
    fn resolve_prefers_env_over_tauri_path() {
        let dir = tempfile::tempdir().unwrap();
        let env_path = dir.path().join("env");
        let tauri_path = dir.path().join("tauri");
        std::env::set_var(DATA_DIR_ENV, &env_path);
        assert_eq!(resolve_data_dir(Some(tauri_path)), env_path);
        std::env::remove_var(DATA_DIR_ENV);
    }

    #[test]
    fn resolve_uses_tauri_path_when_no_env() {
        std::env::remove_var(DATA_DIR_ENV);
        let tauri_path = PathBuf::from("/tmp/schema-sync-tauri-test");
        assert_eq!(resolve_data_dir(Some(tauri_path.clone())), tauri_path);
    }
}
