//! 模式 2：DDL 投放 preview / execute

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::ddl_guard;
use crate::exec::ExecResult;
use crate::history::{new_record_meta, HistoryRecord};
use crate::mysql;
use crate::preview_cache::{DdlPreviewEntry, FrozenConnection, RuleTarget};

use super::rules::{expand_targets_enabled, filter_targets_existing};
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
    #[serde(default)]
    pub warnings: Vec<String>,
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

/// 在已确定 targets 后写入预览缓存（含连接端点快照）
pub fn ddl_preview_store(
    state: &AppState,
    statements: Vec<String>,
    targets: Vec<RuleTarget>,
    warnings: Vec<String>,
) -> Result<DdlPreviewResponse, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    let mut connections = HashMap::new();
    for t in &targets {
        if connections.contains_key(&t.connection_id) {
            continue;
        }
        let conn = find_connection(&config, &t.connection_id)?;
        connections.insert(
            t.connection_id.clone(),
            FrozenConnection::from_config(conn),
        );
    }
    drop(config);

    let preview_id = new_id("preview");
    let entry = DdlPreviewEntry {
        statements: statements.clone(),
        targets: targets.clone(),
        connections,
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
        warnings,
    })
}

/// 校验 SQL + 展开/探测目标并写入预览缓存（供 command / 集成路径）
pub async fn ddl_preview_core(
    state: &AppState,
    req: &DdlPreviewRequest,
) -> Result<DdlPreviewResponse, String> {
    let statements = ddl_guard::validate_structure_ddl(&req.sql)?;
    let (rule, config) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let rule = config
            .rules
            .iter()
            .find(|r| r.id == req.rule_id)
            .cloned()
            .ok_or_else(|| format!("未知规则: {}", req.rule_id))?;
        (rule, config.clone())
    };

    let (targets, mut warnings) = expand_targets_enabled(&rule, &config, &req.exclude);
    let (targets, probe_warnings) =
        filter_targets_existing(targets, &config, &state.store).await;
    warnings.extend(probe_warnings);

    ddl_preview_store(state, statements, targets, warnings)
}

#[tauri::command]
pub async fn ddl_preview(
    state: State<'_, AppState>,
    req: DdlPreviewRequest,
) -> Result<DdlPreviewResponse, String> {
    ddl_preview_core(&state, &req).await
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

            // 使用预览时冻结的连接端点，而非当前配置
            let frozen = match entry.connections.get(&target.connection_id) {
                Some(f) => f,
                None => {
                    results.push(ExecResult {
                        diff_id,
                        ok: false,
                        error: Some(format!(
                            "预览快照中缺少连接: {}",
                            target.connection_id
                        )),
                    });
                    if req.stop_on_error {
                        stopped = true;
                    }
                    continue;
                }
            };
            let conn = frozen.to_connection_config(&target.connection_id);
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
    use crate::models::{ConnectionConfig, NamingRule, PartKind};
    use crate::preview_cache::FrozenConnection;

    fn seed_rule(state: &AppState) {
        let mut cfg = state.config.lock().unwrap();
        cfg.connections = vec![ConnectionConfig {
            id: "c1".into(),
            name: "local".into(),
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: "secret".into(),
            enabled: true,
            remark: String::new(),
        }];
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
        // 危险语句在探测前即拒绝（同步校验路径）
        let err = ddl_guard::validate_structure_ddl("DELETE FROM users WHERE id = 1;")
            .unwrap_err();
        assert!(
            err.contains("DELETE") || err.contains("不允许") || err.contains("危险"),
            "unexpected err: {err}"
        );
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(ddl_preview_core(
                &state,
                &DdlPreviewRequest {
                    sql: "DELETE FROM users WHERE id = 1;".into(),
                    rule_id: "r1".into(),
                    exclude: vec![],
                },
            ))
            .unwrap_err();
        assert!(
            err.contains("DELETE") || err.contains("不允许") || err.contains("危险"),
            "unexpected err: {err}"
        );
    }

    #[test]
    fn ddl_preview_store_freezes_connection_snapshot() {
        let (_dir, state) = AppState::open_temp();
        seed_rule(&state);
        let targets = vec![RuleTarget {
            connection_id: "c1".into(),
            database: "order_lemi".into(),
            exists: Some(true),
        }];
        let resp = ddl_preview_store(
            &state,
            vec!["ALTER TABLE t ADD COLUMN c int".into()],
            targets.clone(),
            vec![],
        )
        .unwrap();
        assert!(!resp.preview_id.is_empty());
        assert_eq!(resp.targets.len(), 1);

        // 事后改配置：execute 仍应使用快照中的端点
        {
            let mut cfg = state.config.lock().unwrap();
            cfg.connections[0].host = "10.0.0.9".into();
            cfg.connections[0].password = "new-secret".into();
        }

        let cache = state.ddl_previews.lock().unwrap();
        let cached = cache.get(&resp.preview_id).unwrap();
        assert_eq!(cached.connections["c1"].host, "127.0.0.1");
        assert_eq!(cached.connections["c1"].password, "secret");
        assert_eq!(cached.statements, resp.statements);
    }

    #[test]
    fn ddl_preview_unknown_rule_fails() {
        let (_dir, state) = AppState::open_temp();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let err = rt
            .block_on(ddl_preview_core(
                &state,
                &DdlPreviewRequest {
                    sql: "ALTER TABLE t ADD COLUMN c int;".into(),
                    rule_id: "missing".into(),
                    exclude: vec![],
                },
            ))
            .unwrap_err();
        assert!(err.contains("未知规则"));
    }

    #[test]
    fn frozen_snapshot_used_over_live_config() {
        let frozen = FrozenConnection {
            host: "preview-host".into(),
            port: 3307,
            user: "u".into(),
            password: "p".into(),
        };
        let live_looks_different = ConnectionConfig {
            id: "c1".into(),
            name: "x".into(),
            host: "edited-host".into(),
            port: 9999,
            user: "other".into(),
            password: "other".into(),
            enabled: true,
            remark: String::new(),
        };
        let from_snap = frozen.to_connection_config("c1");
        assert_ne!(from_snap.host, live_looks_different.host);
        assert_eq!(from_snap.host, "preview-host");
        assert_eq!(from_snap.port, 3307);
    }
}
