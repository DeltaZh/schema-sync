import { invoke } from "@tauri-apps/api/core";
import type {
  BaselineExecuteRequest,
  BaselineScanRequest,
  BaselineScanResponse,
  ConnectionConfig,
  DdlExecuteRequest,
  DdlPreviewRequest,
  DdlPreviewResponse,
  ExecResult,
  ExpandRuleTargetsRequest,
  HistoryRecord,
  NamingRule,
  RuleTarget,
  TableSchema,
  TableSummary,
} from "../types";

export function listConnections(): Promise<ConnectionConfig[]> {
  return invoke("list_connections");
}

export function upsertConnection(
  connection: ConnectionConfig,
): Promise<ConnectionConfig> {
  return invoke("upsert_connection", { connection });
}

export function deleteConnection(id: string): Promise<void> {
  return invoke("delete_connection", { id });
}

export function pingConnection(id: string): Promise<void> {
  return invoke("ping_connection", { id });
}

export function setVisibleDatabases(
  id: string,
  databases: string[],
): Promise<ConnectionConfig> {
  return invoke("set_visible_databases", { id, databases });
}

/** 树展示用：仅返回已选可见库 */
export function listDatabases(connectionId: string): Promise<string[]> {
  return invoke("list_databases", { connectionId });
}

/** 选择对话框用：服务器上全部业务库 */
export function listAllDatabases(connectionId: string): Promise<string[]> {
  return invoke("list_all_databases", { connectionId });
}

export function listTables(
  connectionId: string,
  database: string,
): Promise<TableSummary[]> {
  return invoke("list_tables", { connectionId, database });
}

export function getTableStructure(
  connectionId: string,
  database: string,
  table: string,
): Promise<TableSchema | null> {
  return invoke("get_table_structure", { connectionId, database, table });
}

export function listRules(): Promise<NamingRule[]> {
  return invoke("list_rules");
}

export function saveRules(rules: NamingRule[]): Promise<void> {
  return invoke("save_rules", { rules });
}

export function expandRuleTargets(
  req: ExpandRuleTargetsRequest,
): Promise<RuleTarget[]> {
  return invoke("expand_rule_targets", { req });
}

export function baselineScan(
  req: BaselineScanRequest,
): Promise<BaselineScanResponse> {
  return invoke("baseline_scan", { req });
}

export function baselineExecute(
  req: BaselineExecuteRequest,
): Promise<ExecResult[]> {
  return invoke("baseline_execute", { req });
}

export function ddlPreview(req: DdlPreviewRequest): Promise<DdlPreviewResponse> {
  return invoke("ddl_preview", { req });
}

export function ddlExecute(req: DdlExecuteRequest): Promise<ExecResult[]> {
  return invoke("ddl_execute", { req });
}

export function listHistory(limit?: number): Promise<HistoryRecord[]> {
  return invoke("list_history", { limit: limit ?? null });
}

/** 生成与后端风格一致的短 id */
export function newId(prefix: string): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(
    "",
  );
  return `${prefix}-${hex}`;
}
