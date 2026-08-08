//! 模式 1：基准扫描与按 id 执行

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::diff::{diff_table, DiffCtx, DiffItem};
use crate::exec::{execute_by_ids, ExecResult};
use crate::history::{new_record_meta, HistoryRecord};
use crate::mysql;
use crate::preview_cache::RuleTarget;

use super::rules::{expand_targets_enabled, filter_targets_existing};
use super::state::AppState;
use super::util::{decrypt_password, find_connection, new_id};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineScanRequest {
    pub baseline_connection_id: String,
    pub baseline_database: String,
    pub tables: Vec<String>,
    pub rule_id: String,
    /// 可选：剔除的目标库
    #[serde(default)]
    pub exclude_targets: Vec<RuleTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineScanResponse {
    pub scan_id: String,
    pub items: Vec<DiffItem>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineExecuteRequest {
    pub scan_id: String,
    pub item_ids: Vec<String>,
    #[serde(default = "default_true")]
    pub stop_on_error: bool,
}

fn default_true() -> bool {
    true
}

#[tauri::command]
pub async fn baseline_scan(
    state: State<'_, AppState>,
    req: BaselineScanRequest,
) -> Result<BaselineScanResponse, String> {
    if req.tables.is_empty() {
        return Err("请至少选择一张表".into());
    }

    let config = state.config.lock().map_err(|e| e.to_string())?.clone();
    let rule = config
        .rules
        .iter()
        .find(|r| r.id == req.rule_id)
        .cloned()
        .ok_or_else(|| format!("未知规则: {}", req.rule_id))?;

    let baseline_conn = find_connection(&config, &req.baseline_connection_id)?.clone();
    if !baseline_conn.enabled {
        return Err("基准连接已禁用".into());
    }
    let baseline_password = decrypt_password(&state.store, &baseline_conn)?;

    // 展开目标，排除基准自身 + 用户剔选；跳过禁用连接
    let mut exclude = req.exclude_targets.clone();
    exclude.push(RuleTarget {
        connection_id: req.baseline_connection_id.clone(),
        database: req.baseline_database.clone(),
        exists: None,
    });
    let (targets, mut warnings) = expand_targets_enabled(&rule, &config, &exclude);

    // 仅保留真实存在的库；缺失库跳过，不拖垮整次扫描
    let (targets, probe_warnings) =
        filter_targets_existing(targets, &config, &state.store).await;
    warnings.extend(probe_warnings);

    // 抽取基准表结构
    let mut templates = Vec::new();
    for table in &req.tables {
        let schema = mysql::fetch_table_schema(
            &baseline_conn,
            &baseline_password,
            &req.baseline_database,
            table,
        )
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("基准库中不存在表: {table}"))?;
        templates.push(schema);
    }

    let mut items = Vec::new();
    for target in &targets {
        let conn = find_connection(&config, &target.connection_id)?.clone();
        let password = decrypt_password(&state.store, &conn)?;
        let ctx = DiffCtx {
            connection_id: target.connection_id.clone(),
            database: target.database.clone(),
        };
        for template in &templates {
            let target_schema = mysql::fetch_table_schema(
                &conn,
                &password,
                &target.database,
                &template.name,
            )
            .await
            .map_err(|e| e.to_string())?;
            items.extend(diff_table(
                template,
                target_schema.as_ref(),
                &ctx,
            ));
        }
    }

    let scan_id = new_id("scan");
    state
        .cache
        .lock()
        .map_err(|e| e.to_string())?
        .put(scan_id.clone(), items.clone());

    Ok(BaselineScanResponse {
        scan_id,
        items,
        warnings,
    })
}

#[tauri::command]
pub async fn baseline_execute(
    state: State<'_, AppState>,
    req: BaselineExecuteRequest,
) -> Result<Vec<ExecResult>, String> {
    // 仅 scan_id + item_ids；禁止接受客户端 SQL
    let config = state.config.lock().map_err(|e| e.to_string())?.clone();

    // 拷贝扫描快照后立刻释放锁，避免跨 await 持有 MutexGuard
    let (local_cache, snapshots) = {
        let cache = state.cache.lock().map_err(|e| e.to_string())?;
        let mut local = crate::scan_cache::ScanCache::new();
        if let Some(items) = cache.get(&req.scan_id) {
            local.put(&req.scan_id, items.to_vec());
        }
        let snapshots: Vec<DiffItem> = req
            .item_ids
            .iter()
            .filter_map(|id| local.get_item(&req.scan_id, id).cloned())
            .collect();
        (local, snapshots)
    };

    let results = execute_by_ids(
        &local_cache,
        &req.scan_id,
        &req.item_ids,
        req.stop_on_error,
        |item| {
            let config = config.clone();
            let conn_id = item.connection_id.clone();
            let database = item.database.clone();
            let sql = item.sql.clone();
            let password_result = find_connection(&config, &conn_id).and_then(|c| {
                decrypt_password(&state.store, c).map(|p| (c.clone(), p))
            });
            async move {
                let (conn, password) = password_result?;
                mysql::execute_sql(&conn, &password, &database, &sql)
                    .await
                    .map_err(|e| e.to_string())
            }
        },
    )
    .await;

    let (id, ts) = new_record_meta();
    let record = HistoryRecord {
        id,
        ts,
        scan_id: req.scan_id.clone(),
        stop_on_error: req.stop_on_error,
        results: results.clone(),
        item_snapshots: snapshots,
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

    #[test]
    fn baseline_execute_request_has_no_sql_field() {
        // 契约：执行入参只认 scan_id + item_ids，不含客户端 SQL
        let req = BaselineExecuteRequest {
            scan_id: "scan-1".into(),
            item_ids: vec!["a".into()],
            stop_on_error: true,
        };
        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("sql").is_none());
        assert!(json.get("statements").is_none());
    }
}
