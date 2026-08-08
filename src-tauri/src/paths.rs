//! 应用数据目录解析

use std::path::PathBuf;

/// 环境变量：测试/开发期覆盖数据目录
pub const DATA_DIR_ENV: &str = "SCHEMA_SYNC_DATA";

/// 解析本机应用数据目录。
///
/// 优先读取 `SCHEMA_SYNC_DATA`；未设置时回落到平台默认目录
///（macOS: `~/Library/Application Support/schema-sync`）。
/// 正式运行时可传入 Tauri `app.path().app_data_dir()` 的结果作为 fallback。
pub fn app_data_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var(DATA_DIR_ENV) {
        let path = PathBuf::from(override_dir);
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
}
