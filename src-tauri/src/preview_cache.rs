//! DDL 投放预览缓存：execute 只认 preview_id，不接受客户端回传 SQL

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::models::ConnectionConfig;

/// 规则展开后的目标库
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleTarget {
    pub connection_id: String,
    pub database: String,
    /// 探测时填写；未探测则为 `None`
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exists: Option<bool>,
}

/// 预览时冻结的连接端点（execute 只用快照，避免同 id 配置事后被改）
#[derive(Debug, Clone)]
pub struct FrozenConnection {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 预览时捕获的 password 字段（密文或明文），仅存内存、随 token 生命周期
    pub password: String,
}

impl FrozenConnection {
    pub fn from_config(conn: &ConnectionConfig) -> Self {
        Self {
            host: conn.host.clone(),
            port: conn.port,
            user: conn.user.clone(),
            password: conn.password.clone(),
        }
    }

    /// 还原为可交给 mysql 模块的连接配置
    pub fn to_connection_config(&self, id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.to_string(),
            name: String::new(),
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            password: self.password.clone(),
            enabled: true,
            remark: String::new(),
        }
    }
}

/// 一次 DDL 预览的服务端快照
#[derive(Debug, Clone)]
pub struct DdlPreviewEntry {
    pub statements: Vec<String>,
    pub targets: Vec<RuleTarget>,
    /// connection_id → 预览时端点
    pub connections: HashMap<String, FrozenConnection>,
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
        let mut connections = HashMap::new();
        connections.insert(
            "c1".into(),
            FrozenConnection {
                host: "127.0.0.1".into(),
                port: 3306,
                user: "root".into(),
                password: "enc:v1:x".into(),
            },
        );
        cache.put(
            "p1",
            DdlPreviewEntry {
                statements: vec!["ALTER TABLE t ADD COLUMN c int".into()],
                targets: vec![RuleTarget {
                    connection_id: "c1".into(),
                    database: "db1".into(),
                    exists: Some(true),
                }],
                connections,
            },
        );
        let entry = cache.get("p1").unwrap();
        assert_eq!(entry.statements.len(), 1);
        assert_eq!(entry.connections["c1"].host, "127.0.0.1");
        assert!(cache.get("missing").is_none());
    }

    #[test]
    fn frozen_connection_roundtrip_endpoint() {
        let cfg = ConnectionConfig {
            id: "c1".into(),
            name: "local".into(),
            host: "db.example".into(),
            port: 3307,
            user: "sync".into(),
            password: "enc:v1:abc".into(),
            enabled: true,
            remark: String::new(),
        };
        let frozen = FrozenConnection::from_config(&cfg);
        let restored = frozen.to_connection_config("c1");
        assert_eq!(restored.host, "db.example");
        assert_eq!(restored.port, 3307);
        assert_eq!(restored.user, "sync");
        assert_eq!(restored.password, "enc:v1:abc");
    }
}
