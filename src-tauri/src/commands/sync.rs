//! 模式 1：基准扫描与按 id 执行

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};

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
    /// 前端生成的任务 id，用于进度推送与取消
    #[serde(default)]
    pub job_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineScanResponse {
    pub scan_id: String,
    pub items: Vec<DiffItem>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineScanProgress {
    pub job_id: String,
    pub done: u32,
    pub total: u32,
    pub message: String,
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

fn emit_progress(app: &AppHandle, progress: &BaselineScanProgress) {
    let _ = app.emit("baseline-scan-progress", progress);
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        Err("扫描已终止".into())
    } else {
        Ok(())
    }
}

#[tauri::command]
pub async fn baseline_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    req: BaselineScanRequest,
) -> Result<BaselineScanResponse, String> {
    if req.tables.is_empty() {
        return Err("请至少选择一张表".into());
    }

    let job_id = if req.job_id.trim().is_empty() {
        new_id("job")
    } else {
        req.job_id.trim().to_string()
    };
    let cancel = state.begin_scan_job(&job_id)?;

    let result = run_baseline_scan(&app, &state, &req, &job_id, &cancel).await;
    state.end_scan_job(&job_id);

    match result {
        Err(msg) if msg.contains("已终止") => {
            emit_progress(
                &app,
                &BaselineScanProgress {
                    job_id,
                    done: 0,
                    total: 0,
                    message: "已终止".into(),
                },
            );
            Err(msg)
        }
        other => other,
    }
}

async fn run_baseline_scan(
    app: &AppHandle,
    state: &AppState,
    req: &BaselineScanRequest,
    job_id: &str,
    cancel: &Arc<AtomicBool>,
) -> Result<BaselineScanResponse, String> {
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

    check_cancel(cancel)?;
    emit_progress(
        app,
        &BaselineScanProgress {
            job_id: job_id.into(),
            done: 0,
            total: 1,
            message: "正在展开目标库…".into(),
        },
    );

    let mut exclude = req.exclude_targets.clone();
    exclude.push(RuleTarget {
        connection_id: req.baseline_connection_id.clone(),
        database: req.baseline_database.clone(),
        exists: None,
    });
    let (targets, mut warnings) = expand_targets_enabled(&rule, &config, &exclude);

    check_cancel(cancel)?;
    emit_progress(
        app,
        &BaselineScanProgress {
            job_id: job_id.into(),
            done: 0,
            total: 1,
            message: "正在确认目标库是否存在…".into(),
        },
    );

    let (targets, probe_warnings) =
        filter_targets_existing(targets, &config, &state.store).await;
    warnings.extend(probe_warnings);

    let total = (req.tables.len() + targets.len() * req.tables.len()) as u32;
    let total = total.max(1);
    let mut done = 0u32;

    let mut templates = Vec::new();
    for table in &req.tables {
        check_cancel(cancel)?;
        emit_progress(
            app,
            &BaselineScanProgress {
                job_id: job_id.into(),
                done,
                total,
                message: format!("抽取基准表 {}.{}", req.baseline_database, table),
            },
        );
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
        done += 1;
        emit_progress(
            app,
            &BaselineScanProgress {
                job_id: job_id.into(),
                done,
                total,
                message: format!("已抽取基准表 {table}"),
            },
        );
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
            check_cancel(cancel)?;
            emit_progress(
                app,
                &BaselineScanProgress {
                    job_id: job_id.into(),
                    done,
                    total,
                    message: format!(
                        "对比 {} / {}.{}",
                        conn.name, target.database, template.name
                    ),
                },
            );
            let target_schema = mysql::fetch_table_schema(
                &conn,
                &password,
                &target.database,
                &template.name,
            )
            .await
            .map_err(|e| e.to_string())?;
            items.extend(diff_table(template, target_schema.as_ref(), &ctx));
            done += 1;
            emit_progress(
                app,
                &BaselineScanProgress {
                    job_id: job_id.into(),
                    done,
                    total,
                    message: format!(
                        "已对比 {} / {}.{}",
                        conn.name, target.database, template.name
                    ),
                },
            );
        }
    }

    check_cancel(cancel)?;

    let scan_id = new_id("scan");
    state
        .cache
        .lock()
        .map_err(|e| e.to_string())?
        .put(scan_id.clone(), items.clone());

    emit_progress(
        app,
        &BaselineScanProgress {
            job_id: job_id.into(),
            done: total,
            total,
            message: "扫描完成".into(),
        },
    );

    Ok(BaselineScanResponse {
        scan_id,
        items,
        warnings,
        cancelled: false,
    })
}

/// 请求终止进行中的基准扫描
#[tauri::command]
pub fn cancel_baseline_scan(
    state: State<'_, AppState>,
    job_id: String,
) -> Result<bool, String> {
    if job_id.trim().is_empty() {
        return Err("jobId 为空".into());
    }
    state.request_cancel_scan(job_id.trim())
}

#[tauri::command]
pub async fn baseline_execute(
    state: State<'_, AppState>,
    req: BaselineExecuteRequest,
) -> Result<Vec<ExecResult>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?.clone();

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

    let mut results = execute_by_ids(
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

    // 补全连接显示名，便于结果列表阅读
    for r in &mut results {
        if r.connection_name.is_empty() && !r.connection_id.is_empty() {
            if let Ok(c) = find_connection(&config, &r.connection_id) {
                r.connection_name = c.name.clone();
            }
        }
    }

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
