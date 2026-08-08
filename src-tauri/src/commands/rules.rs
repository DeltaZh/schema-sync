//! 命名规则与目标展开

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::models::NamingRule;
use crate::mysql;
use crate::naming::expand_database_names;
use crate::preview_cache::RuleTarget;

use super::state::AppState;
use super::util::{decrypt_password, find_connection, same_target};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandRuleTargetsRequest {
    pub rule_id: String,
    /// 是否探测库是否存在（需连 MySQL）
    #[serde(default)]
    pub probe: bool,
    /// 剔除的目标
    #[serde(default)]
    pub exclude: Vec<RuleTarget>,
}

/// 纯逻辑：按规则 × 绑定连接展开目标（不含探测）
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

    let mut targets = expand_targets_offline(&rule, &req.exclude);

    if req.probe {
        for t in &mut targets {
            let conn = match find_connection(&config_snapshot, &t.connection_id) {
                Ok(c) => c.clone(),
                Err(_) => {
                    t.exists = Some(false);
                    continue;
                }
            };
            let password = match decrypt_password(&state.store, &conn) {
                Ok(p) => p,
                Err(_) => {
                    t.exists = Some(false);
                    continue;
                }
            };
            match mysql::list_databases(&conn, &password).await {
                Ok(dbs) => t.exists = Some(dbs.iter().any(|d| d == &t.database)),
                Err(_) => t.exists = Some(false),
            }
        }
    }

    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{NamingRule, PartKind};

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
}
