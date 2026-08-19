import { invoke } from "@tauri-apps/api/core";
import type {
  BaselineExecuteRequest,
  BaselineScanRequest,
  BaselineScanResponse,
  SuggestRuleResponse,
  ConnectionConfig,
  DdlExecuteRequest,
  DdlPreviewRequest,
  DdlPreviewResponse,
  DdlPolicyRow,
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

/** 带超时的 invoke，避免 UI 永久停在 loading */
async function invokeWithTimeout<T>(
  cmd: string,
  args: Record<string, unknown>,
  ms: number,
  timeoutMessage: string,
): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      invoke<T>(cmd, args),
      new Promise<T>((_, reject) => {
        timer = setTimeout(() => reject(new Error(timeoutMessage)), ms);
      }),
    ]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

/** 树展示用：仅返回已选可见库 */
export function listDatabases(connectionId: string): Promise<string[]> {
  if (!connectionId) {
    return Promise.reject(new Error("连接 id 为空"));
  }
  return invokeWithTimeout<string[]>(
    "list_databases",
    { connectionId },
    15000,
    "拉取库列表超时（15s）。请确认 MySQL 可连，并使用 npm run tauri dev 启动",
  );
}

/** 选择对话框用：服务器上全部业务库 */
export function listAllDatabases(connectionId: string): Promise<string[]> {
  if (!connectionId) {
    return Promise.reject(new Error("连接 id 为空"));
  }
  return invokeWithTimeout<string[]>(
    "list_all_databases",
    { connectionId },
    15000,
    "拉取库列表超时（15s）。请确认 MySQL 可连，并使用 npm run tauri dev 启动（勿只用 Vite）",
  );
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

export function suggestRuleForDatabase(
  database: string,
): Promise<SuggestRuleResponse> {
  return invoke("suggest_rule_for_database", { database });
}

export function baselineScan(
  req: BaselineScanRequest,
): Promise<BaselineScanResponse> {
  return invoke("baseline_scan", { req });
}

export function cancelBaselineScan(jobId: string): Promise<boolean> {
  return invoke("cancel_baseline_scan", { jobId });
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

export function getDdlPolicy(): Promise<DdlPolicyRow[]> {
  return invoke("get_ddl_policy");
}

export function saveDdlPolicy(rows: DdlPolicyRow[]): Promise<DdlPolicyRow[]> {
  return invoke("save_ddl_policy", { rows });
}

export function resetDdlPolicy(): Promise<DdlPolicyRow[]> {
  return invoke("reset_ddl_policy");
}

/** 生成与后端风格一致的短 id */
export function newId(prefix: string): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(
    "",
  );
  return `${prefix}-${hex}`;
}
