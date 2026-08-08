//! 扫描结果缓存：执行只认 scan_id + item_id，SQL 不下发给客户端再回传

use std::collections::HashMap;

use crate::diff::DiffItem;

/// 内存扫描缓存（进程内；前端只传 id）
#[derive(Debug, Default)]
pub struct ScanCache {
    scans: HashMap<String, Vec<DiffItem>>,
}

impl ScanCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 写入一次扫描的 DiffItem 列表（覆盖同 scan_id）
    pub fn put(&mut self, scan_id: impl Into<String>, items: Vec<DiffItem>) {
        self.scans.insert(scan_id.into(), items);
    }

    /// 取某次扫描的全部条目
    pub fn get(&self, scan_id: &str) -> Option<&[DiffItem]> {
        self.scans.get(scan_id).map(|v| v.as_slice())
    }

    /// 按 id 查找单条（供执行器从缓存取 SQL）
    pub fn get_item(&self, scan_id: &str, item_id: &str) -> Option<&DiffItem> {
        self.get(scan_id)?
            .iter()
            .find(|i| i.id == item_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffKind, Risk};

    fn sample_item(id: &str, sql: &str) -> DiffItem {
        DiffItem {
            id: id.into(),
            kind: DiffKind::AddColumn,
            risk: Risk::Safe,
            connection_id: "c1".into(),
            database: "db1".into(),
            table: "t".into(),
            title: "add".into(),
            detail: "字段注释: x".into(),
            sql: sql.into(),
            selected_default: true,
        }
    }

    #[test]
    fn put_then_get_returns_items() {
        let mut cache = ScanCache::new();
        let items = vec![sample_item("a", "ALTER TABLE t ADD COLUMN x int")];
        cache.put("scan-1", items.clone());
        assert_eq!(cache.get("scan-1"), Some(items.as_slice()));
        assert!(cache.get("missing").is_none());
    }

    #[test]
    fn get_item_finds_by_id() {
        let mut cache = ScanCache::new();
        cache.put(
            "s",
            vec![
                sample_item("id-1", "SQL1"),
                sample_item("id-2", "SQL2"),
            ],
        );
        assert_eq!(cache.get_item("s", "id-2").unwrap().sql, "SQL2");
        assert!(cache.get_item("s", "nope").is_none());
    }
}
