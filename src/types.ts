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
  /** 连接树可见库白名单；空表示尚未选择，不展示/加载库 */
  visible_databases: string[];
}

export interface NamingRule {
  id: string;
  /** 列表显示名 */
  display_name: string;
  /** 库名模板，如 order_{年份}_{租户} */
  pattern: string;
  /** 兼容旧字段；可由模板推导 */
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

/** 与 Rust `Risk`（snake_case）对齐 */
export type Risk = "safe" | "caution" | "dangerous";

/** 与 Rust `DiffKind`（snake_case）对齐 */
export type DiffKind =
  | "create_table"
  | "add_column"
  | "modify_column"
  | "drop_column"
  | "add_index"
  | "drop_index"
  | "alter_table_comment";

export interface DiffItem {
  id: string;
  kind: DiffKind;
  risk: Risk;
  connection_id: string;
  database: string;
  table: string;
  /** 对象名（列/索引/表） */
  object_name?: string;
  title: string;
  /** 人类可读说明，含注释信息 */
  detail: string;
  /** 左侧：基准侧对照 */
  baseline_view?: string;
  /** 右侧：目标侧对照 */
  target_view?: string;
  /** 仅供预览；执行只认服务端缓存 id */
  sql: string;
  selected_default: boolean;
}

export interface ExecResult {
  diff_id: string;
  ok: boolean;
  error: string | null;
  connection_id?: string;
  connection_name?: string;
  database?: string;
  /** 人类可读说明 */
  summary?: string;
  /** 语句摘要 */
  sql_preview?: string;
}

export interface BaselineScanRequest {
  baseline_connection_id: string;
  baseline_database: string;
  tables: string[];
  rule_id: string;
  exclude_targets?: RuleTarget[];
  /** 进度/取消用任务 id */
  job_id?: string;
}

export interface BaselineScanResponse {
  scan_id: string;
  items: DiffItem[];
  warnings?: string[];
  cancelled?: boolean;
}

export interface BaselineScanProgress {
  job_id: string;
  done: number;
  total: number;
  message: string;
}

export interface SuggestRuleResponse {
  rule_id: string | null;
  display_name: string;
  pattern: string;
  match_count: number;
}

export interface BaselineExecuteRequest {
  scan_id: string;
  item_ids: string[];
  stop_on_error?: boolean;
}

export interface DdlPreviewRequest {
  sql: string;
  rule_id: string;
  exclude?: RuleTarget[];
}

export interface DdlPreviewResponse {
  preview_id: string;
  statements: string[];
  /** 与 statements 等长 */
  statement_high_risk?: boolean[];
  has_high_risk?: boolean;
  targets: RuleTarget[];
  warnings?: string[];
}

export interface DdlExecuteRequest {
  preview_id: string;
  stop_on_error?: boolean;
}

export interface HistoryRecord {
  id: string;
  /** Unix 毫秒 */
  ts: number;
  scan_id: string;
  stop_on_error: boolean;
  results: ExecResult[];
  item_snapshots: DiffItem[];
}
