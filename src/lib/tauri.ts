import { invoke } from "@tauri-apps/api/core";
import type {
  ConnectionConfig,
  ExpandRuleTargetsRequest,
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

export function listDatabases(connectionId: string): Promise<string[]> {
  return invoke("list_databases", { connectionId });
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

/** 生成与后端风格一致的短 id */
export function newId(prefix: string): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(
    "",
  );
  return `${prefix}-${hex}`;
}
