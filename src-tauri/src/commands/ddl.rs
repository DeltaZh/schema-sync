//! 模式 2：DDL 投放 preview / execute

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ddl_guard;
use crate::exec::ExecResult;
use crate::history::{new_record_meta, HistoryRecord};
use crate::mysql;
use crate::preview_cache::{DdlPreviewEntry, RuleTarget};

use super::rules::expand_targets_offline;
use super::state::AppState;
use super::util::{decrypt_password, find_connection, new_id};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlPreviewRequest {
    pub sql: String,
    pub rule_id: String,
    #[serde(default)]
    pub exclude: Vec<RuleTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlPreviewResponse {
    pub preview_id: String,
    pub statements: Vec<String>,
    pub targets: Vec<RuleTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DdlExecuteRequest {
    pub preview_id: String,
    #[serde(default = "default_true")]
    pub stop_on_error: bool,
}

fn default_true() -> bool {
    true
}

/// 校验 SQL + 展开目标并写入预览缓存（供 command / 单测）
pub fn ddl_preview_core(
    state: &AppState,
    req: &DdlPreviewRequest,
) -> Result<DdlPreviewResponse, String> {
    let statements = ddl_guard::validate_structure_ddl(&req.sql)?;
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let rule = config
        .rules
        .iter()
        .find(|r| r.id == req.rule_id)
        .cloned()
        .ok_or_else(|| format!("未知规则: {}", req.rule_id))?;
    drop(config);

    let targets = expand_targets_offline(&rule, &req.exclude);
    let preview_id = new_id("preview");
    let entry = DdlPreviewEntry {
        statements: statements.clone(),
        targets: targets.clone(),
    };
    state
        .ddl_previews
        .lock()
        .map_err(|e| e.to_string())?
        .put(preview_id.clone(), entry);

    Ok(DdlPreviewResponse {
        preview_id,
        statements,
        targets,
    })
}

#[tauri::command]
pub fn ddl_preview(
    state: State<'_, AppState>,
    req: DdlPreviewRequest,
) -> Result<DdlPreviewResponse, String> {
    ddl_preview_core(&state, &req)
}

#[tauri::command]
pub async fn ddl_execute(
    state: State<'_, AppState>,
    req: DdlExecuteRequest,
) -> Result<Vec<ExecResult>, String> {
    let entry = {
        let cache = state.ddl_previews.lock().map_err(|e| e.to_string())?;
        cache
            .get(&req.preview_id)
            .cloned()
            .ok_or_else(|| format!("未知预览: {}", req.preview_id))?
    };

    let config = state.config.lock().map_err(|e| e.to_string())?.clone();
    let mut results = Vec::new();
    let mut stopped = false;

    for target in &entry.targets {
        for (idx, sql) in entry.statements.iter().enumerate() {
            let diff_id = format!(
                "{}|{}|stmt{}",
                target.connection_id, target.database, idx
            );
            if stopped {
                results.push(ExecResult {
                    diff_id,
                    ok: false,
                    error: Some("已因前序错误停止，未执行".into()),
                });
                continue;
            }

            let conn = match find_connection(&config, &target.connection_id) {
                Ok(c) => c.clone(),
                Err(e) => {
                    results.push(ExecResult {
                        diff_id,
                        ok: false,
                        error: Some(e),
                    });
                    if req.stop_on_error {
                        stopped = true;
                    }
                    continue;
                }
            };
            let password = match decrypt_password(&state.store, &conn) {
                Ok(p) => p,
                Err(e) => {
                    results.push(ExecResult {
                        diff_id,
                        ok: false,
                        error: Some(e),
                    });
                    if req.stop_on_error {
                        stopped = true;
                    }
                    continue;
                }
            };

            match mysql::execute_sql(&conn, &password, &target.database, sql).await {
                Ok(()) => results.push(ExecResult {
                    diff_id,
                    ok: true,
                    error: None,
                }),
                Err(e) => {
                    results.push(ExecResult {
                        diff_id,
                        ok: false,
                        error: Some(e.to_string()),
                    });
                    if req.stop_on_error {
                        stopped = true;
                    }
                }
            }
        }
    }

    let (id, ts) = new_record_meta();
    let record = HistoryRecord {
        id,
        ts,
        scan_id: req.preview_id.clone(),
        stop_on_error: req.stop_on_error,
        results: results.clone(),
        item_snapshots: vec![],
    };
    state
        .history
        .lock()
        .map_err(|e| e.to_string())?
        .append(&record)
        .map_err(|e| e.to_string())?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::state::AppState;
    use crate::models::{NamingRule, PartKind};

    fn seed_rule(state: &AppState) {
        let mut cfg = state.config.lock().unwrap();
        cfg.rules = vec![NamingRule {
            id: "r1".into(),
            logical_name: "order".into(),
            parts_order: vec![PartKind::Tenant],
            tenants: vec!["lemi".into()],
            years: vec![],
            shards: vec![],
            connection_ids: vec!["c1".into()],
        }];
        state.store.save(cfg.clone()).unwrap();
    }

    #[test]
    fn ddl_preview_rejects_dangerous_sql() {
        let (_dir, state) = AppState::open_temp();
        seed_rule(&state);
        let err = ddl_preview_core(
            &state,
            &DdlPreviewRequest {
                sql: "DELETE FROM users WHERE id = 1;".into(),
                rule_id: "r1".into(),
                exclude: vec![],
            },
        )
        .unwrap_err();
        assert!(
            err.contains("DELETE") || err.contains("不允许") || err.contains("危险"),
            "unexpected err: {err}"
        );
        assert!(state.ddl_previews.lock().unwrap().get("anything").is_none());
    }

    #[test]
    fn ddl_preview_accepts_alter_and_caches_token() {
        let (_dir, state) = AppState::open_temp();
        seed_rule(&state);
        let resp = ddl_preview_core(
            &state,
            &DdlPreviewRequest {
                sql: "ALTER TABLE t ADD COLUMN c int;".into(),
                rule_id: "r1".into(),
                exclude: vec![],
            },
        )
        .unwrap();
        assert!(!resp.preview_id.is_empty());
        assert_eq!(resp.statements.len(), 1);
        assert_eq!(resp.targets.len(), 1);
        assert_eq!(resp.targets[0].database, "order_lemi");
        let cache = state.ddl_previews.lock().unwrap();
        let cached = cache.get(&resp.preview_id).unwrap();
        assert_eq!(cached.statements, resp.statements);
        assert!(crate::commands::util::same_target(
            &cached.targets[0],
            &resp.targets[0]
        ));
    }

    #[test]
    fn ddl_preview_unknown_rule_fails() {
        let (_dir, state) = AppState::open_temp();
        let err = ddl_preview_core(
            &state,
            &DdlPreviewRequest {
                sql: "ALTER TABLE t ADD COLUMN c int;".into(),
                rule_id: "missing".into(),
                exclude: vec![],
            },
        )
        .unwrap_err();
        assert!(err.contains("未知规则"));
    }
}
