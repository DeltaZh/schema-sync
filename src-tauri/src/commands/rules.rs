//! 命名规则与目标展开

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::config::ConfigStore;
use crate::models::{AppConfig, NamingRule};
use crate::mysql;
use crate::naming::expand_database_names;
use crate::preview_cache::RuleTarget;

use super::state::AppState;
use super::util::{decrypt_password, find_connection, same_target};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandRuleTargetsRequest {
    pub rule_id: String,
    /// 是否探测库是否存在（需连 MySQL）；为 true 时仅返回存在的库
    #[serde(default)]
    pub probe: bool,
    /// 剔除的目标
    #[serde(default)]
    pub exclude: Vec<RuleTarget>,
}

/// 纯逻辑：按规则 × 绑定连接展开目标（不含探测、不过滤禁用）
pub fn expand_targets_offline(
    rule: &NamingRule,
    exclude: &[RuleTarget],
) -> Vec<RuleTarget> {
    let names = expand_database_names(rule);
    let mut out = Vec::new();
    for conn_id in &rule.connection_ids {
        for db in &names {
            let t = RuleTarget {
                connection_id: conn_id.clone(),
                database: db.clone(),
                exists: None,
            };
            if exclude.iter().any(|e| same_target(e, &t)) {
                continue;
            }
            out.push(t);
        }
    }
    out
}

/// 展开目标并跳过 `enabled == false` 的连接；返回 (targets, warnings)
pub fn expand_targets_enabled(
    rule: &NamingRule,
    config: &AppConfig,
    exclude: &[RuleTarget],
) -> (Vec<RuleTarget>, Vec<String>) {
    let enabled_ids: HashSet<&str> = config
        .connections
        .iter()
        .filter(|c| c.enabled)
        .map(|c| c.id.as_str())
        .collect();

    let mut warnings = Vec::new();
    let mut seen_disabled = HashSet::new();
    for id in &rule.connection_ids {
        if let Some(conn) = config.connections.iter().find(|c| c.id == *id) {
            if !conn.enabled && seen_disabled.insert(id.as_str()) {
                warnings.push(format!("已跳过禁用连接: {} ({})", conn.name, id));
            }
        }
    }

    let targets = expand_targets_offline(rule, exclude)
        .into_iter()
        .filter(|t| enabled_ids.contains(t.connection_id.as_str()))
        .collect();
    (targets, warnings)
}

/// 按连接探测 `list_databases`，仅保留真实存在的库；缺失库跳过并记警告
pub async fn filter_targets_existing(
    targets: Vec<RuleTarget>,
    config: &AppConfig,
    store: &ConfigStore,
) -> (Vec<RuleTarget>, Vec<String>) {
    let mut warnings = Vec::new();
    let mut by_conn: HashMap<String, Vec<RuleTarget>> = HashMap::new();
    for t in targets {
        by_conn.entry(t.connection_id.clone()).or_default().push(t);
    }

    let mut kept = Vec::new();
    for (conn_id, group) in by_conn {
        let conn = match find_connection(config, &conn_id) {
            Ok(c) => {
                if !c.enabled {
                    warnings.push(format!("已跳过禁用连接: {}", c.name));
                    continue;
                }
                c.clone()
            }
            Err(e) => {
                warnings.push(format!("跳过连接 {conn_id}: {e}"));
                continue;
            }
        };
        let password = match decrypt_password(store, &conn) {
            Ok(p) => p,
            Err(e) => {
                warnings.push(format!("跳过连接 {}（解密失败）: {e}", conn.name));
                continue;
            }
        };
        let dbs = match mysql::list_databases(&conn, &password).await {
            Ok(d) => d,
            Err(e) => {
                warnings.push(format!("跳过连接 {}（列举库失败）: {e}", conn.name));
                continue;
            }
        };
        let db_set: HashSet<String> = dbs.into_iter().collect();
        for mut t in group {
            if db_set.contains(&t.database) {
                t.exists = Some(true);
                kept.push(t);
            } else {
                warnings.push(format!(
                    "目标库不存在，已跳过: {} / {}",
                    conn.name, t.database
                ));
            }
        }
    }
    (kept, warnings)
}

/// 纯逻辑：按已知库名集合过滤（便于单测）
pub fn keep_targets_in_sets(
    targets: Vec<RuleTarget>,
    existing_by_conn: &HashMap<String, HashSet<String>>,
) -> (Vec<RuleTarget>, Vec<String>) {
    let mut kept = Vec::new();
    let mut warnings = Vec::new();
    for mut t in targets {
        match existing_by_conn.get(&t.connection_id) {
            Some(set) if set.contains(&t.database) => {
                t.exists = Some(true);
                kept.push(t);
            }
            Some(_) => {
                warnings.push(format!(
                    "目标库不存在，已跳过: {} / {}",
                    t.connection_id, t.database
                ));
            }
            None => {
                warnings.push(format!(
                    "目标库不存在，已跳过: {} / {}",
                    t.connection_id, t.database
                ));
            }
        }
    }
    (kept, warnings)
}

#[tauri::command]
pub fn list_rules(state: State<'_, AppState>) -> Result<Vec<NamingRule>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.rules.clone())
}

#[tauri::command]
pub fn save_rules(state: State<'_, AppState>, rules: Vec<NamingRule>) -> Result<(), String> {
    let mut config = state.config.lock().map_err(|e| e.to_string())?;
    config.rules = rules;
    state.store.save(config.clone()).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn expand_rule_targets(
    state: State<'_, AppState>,
    req: ExpandRuleTargetsRequest,
) -> Result<Vec<RuleTarget>, String> {
    let (rule, config_snapshot) = {
        let config = state.config.lock().map_err(|e| e.to_string())?;
        let rule = config
            .rules
            .iter()
            .find(|r| r.id == req.rule_id)
            .cloned()
            .ok_or_else(|| format!("未知规则: {}", req.rule_id))?;
        (rule, config.clone())
    };

    let (targets, _warnings) = expand_targets_enabled(&rule, &config_snapshot, &req.exclude);

    if req.probe {
        let (kept, _) =
            filter_targets_existing(targets, &config_snapshot, &state.store).await;
        return Ok(kept);
    }

    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ConnectionConfig, NamingRule, PartKind};

    fn sample_rule() -> NamingRule {
        NamingRule {
            id: "r1".into(),
            logical_name: "order".into(),
            parts_order: vec![PartKind::Tenant],
            tenants: vec!["lemi".into(), "yr".into()],
            years: vec![],
            shards: vec![],
            connection_ids: vec!["c1".into(), "c2".into()],
        }
    }

    fn conn(id: &str, enabled: bool) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: id.into(),
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: String::new(),
            enabled,
            remark: String::new(),
            visible_databases: Vec::new(),
        }
    }

    #[test]
    fn expand_rule_targets_cartesian_and_exclude() {
        let rule = sample_rule();
        let exclude = vec![RuleTarget {
            connection_id: "c1".into(),
            database: "order_lemi".into(),
            exists: None,
        }];
        let targets = expand_targets_offline(&rule, &exclude);
        assert_eq!(targets.len(), 3); // c1/yr + c2/lemi + c2/yr
        assert!(!targets.iter().any(|t| {
            t.connection_id == "c1" && t.database == "order_lemi"
        }));
        assert!(targets.iter().any(|t| {
            t.connection_id == "c2" && t.database == "order_lemi"
        }));
    }

    #[test]
    fn expand_targets_enabled_skips_disabled_connections() {
        let rule = sample_rule();
        let config = AppConfig {
            connections: vec![conn("c1", true), conn("c2", false)],
            rules: vec![],
        };
        let (targets, warnings) = expand_targets_enabled(&rule, &config, &[]);
        assert!(targets.iter().all(|t| t.connection_id == "c1"));
        assert_eq!(targets.len(), 2);
        assert!(warnings.iter().any(|w| w.contains("禁用") && w.contains("c2")));
    }

    #[test]
    fn keep_targets_in_sets_skips_missing_dbs() {
        let targets = vec![
            RuleTarget {
                connection_id: "c1".into(),
                database: "order_lemi".into(),
                exists: None,
            },
            RuleTarget {
                connection_id: "c1".into(),
                database: "order_missing".into(),
                exists: None,
            },
            RuleTarget {
                connection_id: "c2".into(),
                database: "order_lemi".into(),
                exists: None,
            },
        ];
        let mut sets = HashMap::new();
        sets.insert(
            "c1".into(),
            HashSet::from(["order_lemi".into()]),
        );
        // c2 完全无库集合 → 全部跳过
        let (kept, warnings) = keep_targets_in_sets(targets, &sets);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].database, "order_lemi");
        assert_eq!(kept[0].exists, Some(true));
        assert_eq!(warnings.len(), 2);
    }
}
