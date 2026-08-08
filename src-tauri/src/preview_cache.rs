//! DDL 投放预览缓存：execute 只认 preview_id，不接受客户端回传 SQL

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// 规则展开后的目标库
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleTarget {
    pub connection_id: String,
    pub database: String,
    /// 探测时填写；未探测则为 `None`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
}

/// 一次 DDL 预览的服务端快照
#[derive(Debug, Clone)]
pub struct DdlPreviewEntry {
    pub statements: Vec<String>,
    pub targets: Vec<RuleTarget>,
}

/// 内存预览缓存（进程内）
#[derive(Debug, Default)]
pub struct PreviewCache {
    entries: HashMap<String, DdlPreviewEntry>,
}

impl PreviewCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, preview_id: impl Into<String>, entry: DdlPreviewEntry) {
        self.entries.insert(preview_id.into(), entry);
    }

    pub fn get(&self, preview_id: &str) -> Option<&DdlPreviewEntry> {
        self.entries.get(preview_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_get_roundtrip() {
        let mut cache = PreviewCache::new();
        cache.put(
            "p1",
            DdlPreviewEntry {
                statements: vec!["ALTER TABLE t ADD COLUMN c int".into()],
                targets: vec![RuleTarget {
                    connection_id: "c1".into(),
                    database: "db1".into(),
                    exists: Some(true),
                }],
            },
        );
        assert_eq!(cache.get("p1").unwrap().statements.len(), 1);
        assert!(cache.get("missing").is_none());
    }
}
