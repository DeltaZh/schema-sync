//! DDL 投放语句策略：常规 / 高风险 / 不允许

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// 策略档位（越严越大）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DdlPolicyLevel {
    /// 常规放行（普通确认即可）
    Normal,
    /// 高风险（执行需二次确认）
    High,
    /// 不允许执行
    Forbidden,
}

impl DdlPolicyLevel {
    fn severity(self) -> u8 {
        match self {
            Self::Normal => 0,
            Self::High => 1,
            Self::Forbidden => 2,
        }
    }

    /// 取更严的一档
    pub fn merge(self, other: Self) -> Self {
        if self.severity() >= other.severity() {
            self
        } else {
            other
        }
    }
}

impl PartialOrd for DdlPolicyLevel {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DdlPolicyLevel {
    fn cmp(&self, other: &Self) -> Ordering {
        self.severity().cmp(&other.severity())
    }
}

/// 可配置的语句类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DdlStmtKind {
    CreateTable,
    AlterTableSafe,
    AlterTableDrop,
    CreateIndex,
    DropTable,
    DropIndex,
    InsertReplace,
    Update,
    Delete,
    Truncate,
    DropDatabase,
}

impl DdlStmtKind {
    pub const ALL: &'static [DdlStmtKind] = &[
        Self::CreateTable,
        Self::AlterTableSafe,
        Self::AlterTableDrop,
        Self::CreateIndex,
        Self::DropTable,
        Self::DropIndex,
        Self::InsertReplace,
        Self::Update,
        Self::Delete,
        Self::Truncate,
        Self::DropDatabase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CreateTable => "创建表 CREATE TABLE",
            Self::AlterTableSafe => "修改表（非删除类子句）",
            Self::AlterTableDrop => "修改表（删除列/索引等）",
            Self::CreateIndex => "创建索引 CREATE INDEX",
            Self::DropTable => "删除表 DROP TABLE",
            Self::DropIndex => "删除索引 DROP INDEX",
            Self::InsertReplace => "插入/替换 INSERT / REPLACE",
            Self::Update => "更新 UPDATE",
            Self::Delete => "删除数据 DELETE",
            Self::Truncate => "清空表 TRUNCATE",
            Self::DropDatabase => "删除库 DROP DATABASE",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::CreateTable => "含 IF NOT EXISTS",
            Self::AlterTableSafe => "除删除类外的任意 ALTER 子句（含 CONVERT、ENGINE、分区等）",
            Self::AlterTableDrop => "DROP COLUMN/INDEX/KEY/分区、DISCARD/IMPORT TABLESPACE 等",
            Self::CreateIndex => "含 CREATE UNIQUE INDEX",
            Self::DropTable => "整表删除",
            Self::DropIndex => "独立 DROP INDEX 语句",
            Self::InsertReplace => "含 ON DUPLICATE KEY UPDATE",
            Self::Update => "改数据",
            Self::Delete => "删数据行",
            Self::Truncate => "清空表数据",
            Self::DropDatabase => "含 DROP SCHEMA；误操作风险极高",
        }
    }
}

/// 各语句类型的策略；缺字段时用默认
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DdlPolicy {
    #[serde(default = "default_create_table")]
    pub create_table: DdlPolicyLevel,
    #[serde(default = "default_alter_safe")]
    pub alter_table_safe: DdlPolicyLevel,
    #[serde(default = "default_alter_drop")]
    pub alter_table_drop: DdlPolicyLevel,
    #[serde(default = "default_create_index")]
    pub create_index: DdlPolicyLevel,
    #[serde(default = "default_drop_table")]
    pub drop_table: DdlPolicyLevel,
    #[serde(default = "default_drop_index")]
    pub drop_index: DdlPolicyLevel,
    #[serde(default = "default_insert_replace")]
    pub insert_replace: DdlPolicyLevel,
    #[serde(default = "default_update")]
    pub update: DdlPolicyLevel,
    #[serde(default = "default_delete")]
    pub delete: DdlPolicyLevel,
    #[serde(default = "default_truncate")]
    pub truncate: DdlPolicyLevel,
    #[serde(default = "default_drop_database")]
    pub drop_database: DdlPolicyLevel,
}

fn default_create_table() -> DdlPolicyLevel {
    DdlPolicyLevel::Normal
}
fn default_alter_safe() -> DdlPolicyLevel {
    DdlPolicyLevel::Normal
}
fn default_alter_drop() -> DdlPolicyLevel {
    DdlPolicyLevel::High
}
fn default_create_index() -> DdlPolicyLevel {
    DdlPolicyLevel::Normal
}
fn default_drop_table() -> DdlPolicyLevel {
    DdlPolicyLevel::High
}
fn default_drop_index() -> DdlPolicyLevel {
    DdlPolicyLevel::High
}
fn default_insert_replace() -> DdlPolicyLevel {
    DdlPolicyLevel::High
}
fn default_update() -> DdlPolicyLevel {
    DdlPolicyLevel::High
}
fn default_delete() -> DdlPolicyLevel {
    DdlPolicyLevel::High
}
fn default_truncate() -> DdlPolicyLevel {
    DdlPolicyLevel::High
}
fn default_drop_database() -> DdlPolicyLevel {
    DdlPolicyLevel::Forbidden
}

impl Default for DdlPolicy {
    fn default() -> Self {
        Self {
            create_table: default_create_table(),
            alter_table_safe: default_alter_safe(),
            alter_table_drop: default_alter_drop(),
            create_index: default_create_index(),
            drop_table: default_drop_table(),
            drop_index: default_drop_index(),
            insert_replace: default_insert_replace(),
            update: default_update(),
            delete: default_delete(),
            truncate: default_truncate(),
            drop_database: default_drop_database(),
        }
    }
}

impl DdlPolicy {
    pub fn level_of(&self, kind: DdlStmtKind) -> DdlPolicyLevel {
        match kind {
            DdlStmtKind::CreateTable => self.create_table,
            DdlStmtKind::AlterTableSafe => self.alter_table_safe,
            DdlStmtKind::AlterTableDrop => self.alter_table_drop,
            DdlStmtKind::CreateIndex => self.create_index,
            DdlStmtKind::DropTable => self.drop_table,
            DdlStmtKind::DropIndex => self.drop_index,
            DdlStmtKind::InsertReplace => self.insert_replace,
            DdlStmtKind::Update => self.update,
            DdlStmtKind::Delete => self.delete,
            DdlStmtKind::Truncate => self.truncate,
            DdlStmtKind::DropDatabase => self.drop_database,
        }
    }

    pub fn set_level(&mut self, kind: DdlStmtKind, level: DdlPolicyLevel) {
        match kind {
            DdlStmtKind::CreateTable => self.create_table = level,
            DdlStmtKind::AlterTableSafe => self.alter_table_safe = level,
            DdlStmtKind::AlterTableDrop => self.alter_table_drop = level,
            DdlStmtKind::CreateIndex => self.create_index = level,
            DdlStmtKind::DropTable => self.drop_table = level,
            DdlStmtKind::DropIndex => self.drop_index = level,
            DdlStmtKind::InsertReplace => self.insert_replace = level,
            DdlStmtKind::Update => self.update = level,
            DdlStmtKind::Delete => self.delete = level,
            DdlStmtKind::Truncate => self.truncate = level,
            DdlStmtKind::DropDatabase => self.drop_database = level,
        }
    }

    /// 合并多种语句类型策略为一条语句的最终档位
    pub fn resolve(&self, kinds: &[DdlStmtKind]) -> DdlPolicyLevel {
        kinds
            .iter()
            .map(|k| self.level_of(*k))
            .fold(DdlPolicyLevel::Normal, DdlPolicyLevel::merge)
    }
}

/// 设置页展示用条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlPolicyRow {
    pub kind: DdlStmtKind,
    pub label: String,
    pub hint: String,
    pub level: DdlPolicyLevel,
}

impl DdlPolicy {
    pub fn to_rows(&self) -> Vec<DdlPolicyRow> {
        DdlStmtKind::ALL
            .iter()
            .map(|k| DdlPolicyRow {
                kind: *k,
                label: k.label().into(),
                hint: k.hint().into(),
                level: self.level_of(*k),
            })
            .collect()
    }

    pub fn from_rows(rows: &[DdlPolicyRow]) -> Self {
        let mut policy = Self::default();
        for row in rows {
            policy.set_level(row.kind, row.level);
        }
        policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_create_table_is_normal_drop_db_forbidden() {
        let p = DdlPolicy::default();
        assert_eq!(p.create_table, DdlPolicyLevel::Normal);
        assert_eq!(p.drop_database, DdlPolicyLevel::Forbidden);
        assert_eq!(p.insert_replace, DdlPolicyLevel::High);
    }

    #[test]
    fn resolve_takes_most_severe() {
        let p = DdlPolicy::default();
        let level = p.resolve(&[DdlStmtKind::AlterTableSafe, DdlStmtKind::AlterTableDrop]);
        assert_eq!(level, DdlPolicyLevel::High);
    }

    #[test]
    fn rows_roundtrip_preserves_custom() {
        let mut p = DdlPolicy::default();
        p.drop_database = DdlPolicyLevel::High;
        p.create_table = DdlPolicyLevel::Forbidden;
        let back = DdlPolicy::from_rows(&p.to_rows());
        assert_eq!(back, p);
    }
}
