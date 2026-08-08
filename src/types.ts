/** 与 Rust `PartKind`（snake_case）对齐 */
export type PartKind = "tenant" | "year" | "shard";

export interface ConnectionConfig {
  id: string;
  name: string;
  host: string;
  port: number;
  user: string;
  /** 列表回显为掩码；提交时空串/`********` 表示不改密 */
  password: string;
  enabled: boolean;
  remark: string;
}

export interface NamingRule {
  id: string;
  logical_name: string;
  parts_order: PartKind[];
  tenants: string[];
  years: string[];
  shards: string[];
  connection_ids: string[];
}

export interface TableSummary {
  name: string;
  comment: string;
}

export interface ColumnDef {
  name: string;
  col_type: string;
  nullable: boolean;
  default: string | null;
  comment: string;
  extra: string;
}

export interface IndexDef {
  name: string;
  columns: string[];
  unique: boolean;
  primary: boolean;
}

export interface TableSchema {
  name: string;
  comment: string;
  columns: ColumnDef[];
  indexes: IndexDef[];
  create_sql: string;
}

export interface RuleTarget {
  connection_id: string;
  database: string;
  exists?: boolean | null;
}

export interface ExpandRuleTargetsRequest {
  rule_id: string;
  probe?: boolean;
  exclude?: RuleTarget[];
}

/** 树选中表时的定位 */
export interface TableSelection {
  connectionId: string;
  connectionName: string;
  database: string;
  table: string;
  tableComment: string;
}

export type MainTab = "structure" | "baseline" | "ddl" | "rules" | "history";
