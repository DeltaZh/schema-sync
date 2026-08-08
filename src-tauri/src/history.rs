//! 本机执行历史：JSON 文件追加与最近列表

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::diff::DiffItem;
use crate::exec::ExecResult;

#[derive(Debug, Error)]
pub enum HistoryError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 错误: {0}")]
    Json(#[from] serde_json::Error),
}

/// 一条执行历史
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub id: String,
    /// Unix 毫秒
    pub ts: u64,
    pub scan_id: String,
    pub stop_on_error: bool,
    pub results: Vec<ExecResult>,
    pub item_snapshots: Vec<DiffItem>,
}

/// JSONL 历史存储（每行一条记录，追加写入）
#[derive(Debug, Clone)]
pub struct HistoryStore {
    path: PathBuf,
}

impl HistoryStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 追加一条记录（原子性：先确保父目录存在再 append）
    pub fn append(&self, record: &HistoryRecord) -> Result<(), HistoryError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let line = serde_json::to_string(record)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{line}")?;
        Ok(())
    }

    /// 最近 `limit` 条（文件末尾优先；limit=0 返回空）
    pub fn list_recent(&self, limit: usize) -> Result<Vec<HistoryRecord>, HistoryError> {
        if limit == 0 || !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut all = Vec::new();
        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            all.push(serde_json::from_str::<HistoryRecord>(trimmed)?);
        }
        let start = all.len().saturating_sub(limit);
        Ok(all.split_off(start))
    }
}

/// 生成历史 id 与时间戳
pub fn new_record_meta() -> (String, u64) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let nonce: u32 = rand::thread_rng().gen();
    (format!("{ts}-{nonce:08x}"), ts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffKind, Risk};

    fn snap(id: &str) -> DiffItem {
        DiffItem {
            id: id.into(),
            kind: DiffKind::AddColumn,
            risk: Risk::Safe,
            connection_id: "c1".into(),
            database: "db1".into(),
            table: "t".into(),
            title: "t".into(),
            detail: "".into(),
            sql: "ALTER TABLE t ADD COLUMN x int".into(),
            selected_default: true,
        }
    }

    #[test]
    fn append_then_list_recent() {
        let dir = tempfile::tempdir().unwrap();
        let store = HistoryStore::new(dir.path().join("history.jsonl"));
        let (id1, ts1) = new_record_meta();
        let r1 = HistoryRecord {
            id: id1,
            ts: ts1,
            scan_id: "s1".into(),
            stop_on_error: true,
            results: vec![ExecResult {
                diff_id: "a".into(),
                ok: true,
                error: None,
            }],
            item_snapshots: vec![snap("a")],
        };
        store.append(&r1).unwrap();

        let (id2, ts2) = new_record_meta();
        let r2 = HistoryRecord {
            id: id2.clone(),
            ts: ts2.saturating_add(1),
            scan_id: "s2".into(),
            stop_on_error: false,
            results: vec![],
            item_snapshots: vec![],
        };
        store.append(&r2).unwrap();

        let recent = store.list_recent(1).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].id, id2);

        let all = store.list_recent(50).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].scan_id, "s1");
        assert_eq!(all[1].scan_id, "s2");
    }
}
