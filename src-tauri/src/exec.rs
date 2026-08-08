//! 按扫描缓存 id 安全执行 DDL（禁止接受客户端 SQL）

use std::future::Future;

use serde::{Deserialize, Serialize};

use crate::diff::{DiffItem, DiffKind};
use crate::scan_cache::ScanCache;

/// 单条执行结果（含面向用户的展示字段）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecResult {
    pub diff_id: String,
    pub ok: bool,
    pub error: Option<String>,
    #[serde(default)]
    pub connection_id: String,
    /// 连接显示名；旧历史记录可能为空
    #[serde(default)]
    pub connection_name: String,
    #[serde(default)]
    pub database: String,
    /// 人类可读说明（如「第 1 条语句」或差异标题）
    #[serde(default)]
    pub summary: String,
    /// 语句摘要（截断）
    #[serde(default)]
    pub sql_preview: String,
}

/// 截取语句首行摘要，便于结果列表展示
pub fn sql_preview(sql: &str) -> String {
    let one_line = sql
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .chars()
        .take(120)
        .collect::<String>();
    if sql.chars().count() > one_line.chars().count() || sql.contains('\n') {
        if one_line.chars().count() >= 120 {
            format!("{one_line}…")
        } else {
            format!("{one_line} …")
        }
    } else {
        one_line
    }
}

pub fn exec_result(
    diff_id: impl Into<String>,
    ok: bool,
    error: Option<String>,
    connection_id: impl Into<String>,
    connection_name: impl Into<String>,
    database: impl Into<String>,
    summary: impl Into<String>,
    sql: &str,
) -> ExecResult {
    ExecResult {
        diff_id: diff_id.into(),
        ok,
        error,
        connection_id: connection_id.into(),
        connection_name: connection_name.into(),
        database: database.into(),
        summary: summary.into(),
        sql_preview: sql_preview(sql),
    }
}

fn result_from_item(item: &DiffItem, ok: bool, error: Option<String>) -> ExecResult {
    exec_result(
        item.id.clone(),
        ok,
        error,
        item.connection_id.clone(),
        String::new(),
        item.database.clone(),
        item.title.clone(),
        &item.sql,
    )
}

fn kind_order(kind: DiffKind) -> u8 {
    match kind {
        DiffKind::CreateTable => 0,
        DiffKind::AddColumn => 1,
        DiffKind::ModifyColumn => 2,
        DiffKind::AlterTableComment => 3,
        DiffKind::DropColumn => 4,
        DiffKind::DropIndex => 5,
        DiffKind::AddIndex => 6,
    }
}

/// 从缓存按 `scan_id` + `item_ids` 取 SQL 并执行；**不接受**外部传入的 SQL。
///
/// `execute_one` 由调用方注入（生产侧连 MySQL；单测可 mock）。
/// 未知 scan/item id → 对应结果 `ok=false`；连接/执行失败同样记入结果。
pub async fn execute_by_ids<F, Fut>(
    cache: &ScanCache,
    scan_id: &str,
    item_ids: &[String],
    stop_on_error: bool,
    mut execute_one: F,
) -> Vec<ExecResult>
where
    F: FnMut(&DiffItem) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let Some(all) = cache.get(scan_id) else {
        return item_ids
            .iter()
            .map(|id| {
                exec_result(
                    id.clone(),
                    false,
                    Some(format!("未知扫描: {scan_id}")),
                    "",
                    "",
                    "",
                    "未知扫描",
                    "",
                )
            })
            .collect();
    };

    let by_id: std::collections::HashMap<&str, &DiffItem> =
        all.iter().map(|i| (i.id.as_str(), i)).collect();

    let mut resolved: Vec<&DiffItem> = Vec::with_capacity(item_ids.len());
    let mut unknown: Vec<ExecResult> = Vec::new();

    for id in item_ids {
        match by_id.get(id.as_str()) {
            Some(item) => resolved.push(item),
            None => unknown.push(exec_result(
                id.clone(),
                false,
                Some(format!("未知差异项: {id}")),
                "",
                "",
                "",
                "未知差异项",
                "",
            )),
        }
    }

    // 未知 id 一律拒绝；若已有未知且要求遇错停止，不再执行已知项
    if !unknown.is_empty() && stop_on_error {
        let mut out = unknown;
        for item in resolved {
            out.push(result_from_item(
                item,
                false,
                Some("已因未知 id 停止，未执行".into()),
            ));
        }
        return out;
    }

    // 按 (connection_id, database) 分组，组内按 DDL 安全顺序
    let mut groups: std::collections::BTreeMap<(String, String), Vec<&DiffItem>> =
        std::collections::BTreeMap::new();
    for item in &resolved {
        groups
            .entry((item.connection_id.clone(), item.database.clone()))
            .or_default()
            .push(item);
    }
    for items in groups.values_mut() {
        items.sort_by_key(|i| (kind_order(i.kind), i.id.as_str()));
    }

    let mut results: Vec<ExecResult> = unknown;
    let mut stopped = false;

    for items in groups.values() {
        for item in items {
            if stopped {
                results.push(result_from_item(
                    item,
                    false,
                    Some("已因前序错误停止，未执行".into()),
                ));
                continue;
            }
            match execute_one(item).await {
                Ok(()) => {
                    results.push(result_from_item(item, true, None));
                }
                Err(e) => {
                    results.push(result_from_item(item, false, Some(e)));
                    if stop_on_error {
                        stopped = true;
                    }
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffKind, Risk};
    use std::sync::{Arc, Mutex};

    fn item(id: &str, kind: DiffKind, sql: &str) -> DiffItem {
        DiffItem {
            id: id.into(),
            kind,
            risk: Risk::Safe,
            connection_id: "c1".into(),
            database: "db1".into(),
            table: "t".into(),
            object_name: id.into(),
            title: id.into(),
            detail: "".into(),
            baseline_view: String::new(),
            target_view: String::new(),
            sql: sql.into(),
            selected_default: true,
        }
    }

    #[tokio::test]
    async fn rejects_unknown_item_id() {
        let mut cache = ScanCache::new();
        cache.put("scan-1", vec![item("known", DiffKind::AddColumn, "SQL_A")]);

        let called = Arc::new(Mutex::new(Vec::<String>::new()));
        let called2 = called.clone();
        let results = execute_by_ids(
            &cache,
            "scan-1",
            &["known".into(), "ghost".into()],
            false,
            move |it| {
                let c = called2.clone();
                let id = it.id.clone();
                let sql = it.sql.clone();
                async move {
                    c.lock().unwrap().push(format!("{id}:{sql}"));
                    Ok(())
                }
            },
        )
        .await;

        let ghost = results.iter().find(|r| r.diff_id == "ghost").unwrap();
        assert!(!ghost.ok);
        assert!(ghost.error.as_deref().unwrap().contains("未知差异项"));

        let known = results.iter().find(|r| r.diff_id == "known").unwrap();
        assert!(known.ok);

        let calls = called.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].starts_with("known:SQL_A"));
        // 执行器只用缓存 SQL，从未接受客户端 SQL
        assert!(!calls.iter().any(|s| s.contains("CLIENT")));
    }

    #[tokio::test]
    async fn unknown_scan_id_rejects_all() {
        let cache = ScanCache::new();
        let results = execute_by_ids(
            &cache,
            "no-such",
            &["a".into()],
            true,
            |_it| async { Ok(()) },
        )
        .await;
        assert_eq!(results.len(), 1);
        assert!(!results[0].ok);
        assert!(results[0].error.as_deref().unwrap().contains("未知扫描"));
    }

    #[tokio::test]
    async fn connection_failure_recorded_in_result() {
        let mut cache = ScanCache::new();
        cache.put(
            "s",
            vec![item("x", DiffKind::AddColumn, "ALTER TABLE t ADD COLUMN c int")],
        );

        let results = execute_by_ids(&cache, "s", &["x".into()], false, |_it| async {
            Err("连接失败: timeout".into())
        })
        .await;

        assert_eq!(results.len(), 1);
        assert!(!results[0].ok);
        assert_eq!(results[0].diff_id, "x");
        assert!(results[0]
            .error
            .as_deref()
            .unwrap()
            .contains("连接失败"));
    }

    #[tokio::test]
    async fn stop_on_error_skips_remaining() {
        let mut cache = ScanCache::new();
        cache.put(
            "s",
            vec![
                item("a", DiffKind::AddColumn, "SQL_A"),
                item("b", DiffKind::AddColumn, "SQL_B"),
            ],
        );

        let called = Arc::new(Mutex::new(Vec::<String>::new()));
        let called2 = called.clone();
        let results = execute_by_ids(
            &cache,
            "s",
            &["a".into(), "b".into()],
            true,
            move |it| {
                let c = called2.clone();
                let id = it.id.clone();
                async move {
                    c.lock().unwrap().push(id.clone());
                    if id == "a" {
                        Err("boom".into())
                    } else {
                        Ok(())
                    }
                }
            },
        )
        .await;

        assert_eq!(called.lock().unwrap().as_slice(), &["a".to_string()]);
        let b = results.iter().find(|r| r.diff_id == "b").unwrap();
        assert!(!b.ok);
        assert!(b.error.as_deref().unwrap().contains("停止"));
    }

    #[tokio::test]
    async fn uses_only_cached_sql_not_client_payload() {
        let mut cache = ScanCache::new();
        cache.put(
            "s",
            vec![item("id1", DiffKind::AddColumn, "CACHED_SAFE_SQL")],
        );

        let seen_sql = Arc::new(Mutex::new(String::new()));
        let seen2 = seen_sql.clone();
        let _ = execute_by_ids(&cache, "s", &["id1".into()], false, move |it| {
            let s = seen2.clone();
            let sql = it.sql.clone();
            async move {
                *s.lock().unwrap() = sql;
                Ok(())
            }
        })
        .await;

        assert_eq!(*seen_sql.lock().unwrap(), "CACHED_SAFE_SQL");
    }
}
