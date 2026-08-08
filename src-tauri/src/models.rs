//! 配置领域模型

use serde::{Deserialize, Serialize};

/// 命名规则中可排序的库名部件
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    Tenant,
    Year,
    Shard,
}

/// 数据库连接配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    /// 落盘应为 `enc:v1:` 密文；对外展示时掩码
    pub password: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub remark: String,
}

fn default_true() -> bool {
    true
}

impl ConnectionConfig {
    /// 返回给 UI 的连接视图：密码掩码，永不回显明文
    pub fn public_view(&self) -> Self {
        let mut v = self.clone();
        if !v.password.is_empty() {
            v.password = "********".into();
        }
        v
    }
}

/// 可组合命名规则
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct NamingRule {
    pub id: String,
    pub logical_name: String,
    #[serde(default)]
    pub parts_order: Vec<PartKind>,
    #[serde(default)]
    pub tenants: Vec<String>,
    #[serde(default)]
    pub years: Vec<String>,
    #[serde(default)]
    pub shards: Vec<String>,
    #[serde(default)]
    pub connection_ids: Vec<String>,
}

/// 应用完整配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub connections: Vec<ConnectionConfig>,
    #[serde(default)]
    pub rules: Vec<NamingRule>,
}
