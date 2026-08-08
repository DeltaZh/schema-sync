//! Schema 模型与 information_schema 行解析（纯函数，便于假数据单测）

use serde::{Deserialize, Serialize};

/// 库内表摘要（浏览用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSummary {
    pub name: String,
    pub comment: String,
}

/// 列定义
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub comment: String,
    pub extra: String,
}

/// 索引定义
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexDef {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}

/// 完整表结构（对比 / 展示用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSchema {
    pub name: String,
    pub comment: String,
    pub columns: Vec<ColumnDef>,
    pub indexes: Vec<IndexDef>,
    pub create_sql: String,
}

/// 仿 `information_schema.COLUMNS` 行
#[derive(Debug, Clone)]
pub struct ColumnRow {
    pub column_name: String,
    pub column_type: String,
    pub is_nullable: String,
    pub column_default: Option<String>,
    pub column_comment: String,
    pub extra: String,
}

/// 仿 `information_schema.STATISTICS` 行
#[derive(Debug, Clone)]
pub struct StatsRow {
    pub index_name: String,
    pub column_name: String,
    pub non_unique: i64,
    pub seq_in_index: u32,
}

/// 仿 `information_schema.TABLES` 行
#[derive(Debug, Clone)]
pub struct TableRow {
    pub table_name: String,
    pub table_comment: String,
}

/// 将 COLUMNS 行解析为 `ColumnDef` 列表（保留输入顺序）
pub fn columns_from_rows(rows: &[ColumnRow]) -> Vec<ColumnDef> {
    rows.iter()
        .map(|r| ColumnDef {
            name: r.column_name.clone(),
            col_type: r.column_type.clone(),
            nullable: r.is_nullable.eq_ignore_ascii_case("YES"),
            default: r.column_default.clone(),
            comment: r.column_comment.clone(),
            extra: r.extra.clone(),
        })
        .collect()
}

/// 将 STATISTICS 行聚合为 `IndexDef`（按索引名合并列序）
pub fn indexes_from_stats_rows(rows: &[StatsRow]) -> Vec<IndexDef> {
    use std::collections::BTreeMap;

    // index_name -> (非唯一标记, 按 seq 排序的列)
    let mut grouped: BTreeMap<String, (i64, Vec<(u32, String)>)> = BTreeMap::new();
    for r in rows {
        let entry = grouped
            .entry(r.index_name.clone())
            .or_insert((r.non_unique, Vec::new()));
        entry.0 = r.non_unique;
        entry.1.push((r.seq_in_index, r.column_name.clone()));
    }

    grouped
        .into_iter()
        .map(|(name, (non_unique, mut cols))| {
            cols.sort_by_key(|(seq, _)| *seq);
            let primary = name.eq_ignore_ascii_case("PRIMARY");
            IndexDef {
                name,
                columns: cols.into_iter().map(|(_, c)| c).collect(),
                unique: non_unique == 0,
                primary,
            }
        })
        .collect()
}

/// 将 TABLES 行解析为 `TableSummary` 列表
pub fn tables_from_rows(rows: &[TableRow]) -> Vec<TableSummary> {
    rows.iter()
        .map(|r| TableSummary {
            name: r.table_name.clone(),
            comment: r.table_comment.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_include_comment_and_nullable() {
        let rows = vec![
            ColumnRow {
                column_name: "id".into(),
                column_type: "int".into(),
                is_nullable: "NO".into(),
                column_default: None,
                column_comment: "主键".into(),
                extra: "auto_increment".into(),
            },
            ColumnRow {
                column_name: "name".into(),
                column_type: "varchar(64)".into(),
                is_nullable: "YES".into(),
                column_default: Some("".into()),
                column_comment: "名称".into(),
                extra: "".into(),
            },
        ];
        let cols = columns_from_rows(&rows);
        assert_eq!(cols.len(), 2);
        assert_eq!(cols[0].name, "id");
        assert_eq!(cols[0].comment, "主键");
        assert!(!cols[0].nullable);
        assert_eq!(cols[0].extra, "auto_increment");
        assert_eq!(cols[1].name, "name");
        assert_eq!(cols[1].comment, "名称");
        assert!(cols[1].nullable);
        assert_eq!(cols[1].default.as_deref(), Some(""));
    }

    #[test]
    fn indexes_group_columns_and_detect_primary() {
        let rows = vec![
            StatsRow {
                index_name: "PRIMARY".into(),
                column_name: "id".into(),
                non_unique: 0,
                seq_in_index: 1,
            },
            StatsRow {
                index_name: "uk_name".into(),
                column_name: "name".into(),
                non_unique: 0,
                seq_in_index: 1,
            },
            StatsRow {
                index_name: "idx_a_b".into(),
                column_name: "a".into(),
                non_unique: 1,
                seq_in_index: 1,
            },
            StatsRow {
                index_name: "idx_a_b".into(),
                column_name: "b".into(),
                non_unique: 1,
                seq_in_index: 2,
            },
        ];
        let indexes = indexes_from_stats_rows(&rows);
        assert_eq!(indexes.len(), 3);

        let primary = indexes.iter().find(|i| i.primary).expect("primary");
        assert_eq!(primary.name, "PRIMARY");
        assert_eq!(primary.columns, vec!["id"]);
        assert!(primary.unique);

        let uk = indexes.iter().find(|i| i.name == "uk_name").expect("uk");
        assert!(uk.unique);
        assert!(!uk.primary);
        assert_eq!(uk.columns, vec!["name"]);

        let idx = indexes.iter().find(|i| i.name == "idx_a_b").expect("idx");
        assert!(!idx.unique);
        assert_eq!(idx.columns, vec!["a", "b"]);
    }

    #[test]
    fn tables_include_comment() {
        let rows = vec![
            TableRow {
                table_name: "users".into(),
                table_comment: "用户表".into(),
            },
            TableRow {
                table_name: "orders".into(),
                table_comment: "".into(),
            },
        ];
        let tables = tables_from_rows(&rows);
        assert_eq!(
            tables,
            vec![
                TableSummary {
                    name: "users".into(),
                    comment: "用户表".into(),
                },
                TableSummary {
                    name: "orders".into(),
                    comment: "".into(),
                },
            ]
        );
    }
}
