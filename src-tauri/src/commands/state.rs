//! 应用共享状态（配置 / 扫描缓存 / DDL 预览 / 历史）

use std::path::Path;
use std::sync::Mutex;

use crate::config::{ConfigError, ConfigStore};
use crate::history::HistoryStore;
use crate::models::AppConfig;
use crate::paths::history_file_path;
use crate::preview_cache::PreviewCache;
use crate::scan_cache::ScanCache;

/// Tauri 全局状态
pub struct AppState {
    pub store: ConfigStore,
    pub config: Mutex<AppConfig>,
    pub cache: Mutex<ScanCache>,
    pub ddl_previews: Mutex<PreviewCache>,
    pub history: Mutex<HistoryStore>,
}

impl AppState {
    /// 在指定数据目录打开配置与历史
    pub fn open(data_dir: &Path) -> Result<Self, ConfigError> {
        let store = ConfigStore::open(data_dir)?;
        let config = store.load()?;
        let history = HistoryStore::new(history_file_path(data_dir));
        Ok(Self {
            store,
            config: Mutex::new(config),
            cache: Mutex::new(ScanCache::new()),
            ddl_previews: Mutex::new(PreviewCache::new()),
            history: Mutex::new(history),
        })
    }

    /// 测试用：临时目录
    #[cfg(test)]
    pub fn open_temp() -> (tempfile::TempDir, Self) {
        let dir = tempfile::tempdir().unwrap();
        let state = Self::open(dir.path()).unwrap();
        (dir, state)
    }
}
