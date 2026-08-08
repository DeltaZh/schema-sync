//! 应用共享状态（配置 / 扫描缓存 / DDL 预览 / 历史 / 扫描取消）

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
    /// job_id → 取消标志
    pub scan_jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
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
            scan_jobs: Mutex::new(HashMap::new()),
        })
    }

    /// 测试用：临时目录
    #[cfg(test)]
    pub fn open_temp() -> (tempfile::TempDir, Self) {
        let dir = tempfile::tempdir().unwrap();
        let state = Self::open(dir.path()).unwrap();
        (dir, state)
    }

    pub fn begin_scan_job(&self, job_id: &str) -> Result<Arc<AtomicBool>, String> {
        let flag = Arc::new(AtomicBool::new(false));
        self.scan_jobs
            .lock()
            .map_err(|e| e.to_string())?
            .insert(job_id.to_string(), Arc::clone(&flag));
        Ok(flag)
    }

    pub fn request_cancel_scan(&self, job_id: &str) -> Result<bool, String> {
        let jobs = self.scan_jobs.lock().map_err(|e| e.to_string())?;
        if let Some(flag) = jobs.get(job_id) {
            flag.store(true, Ordering::SeqCst);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub fn end_scan_job(&self, job_id: &str) {
        if let Ok(mut jobs) = self.scan_jobs.lock() {
            jobs.remove(job_id);
        }
    }
}
