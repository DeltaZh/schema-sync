import type { DiffKind, Risk } from "../types";

const KIND_LABELS: Record<DiffKind, string> = {
  create_table: "新建表",
  add_column: "新增字段",
  modify_column: "修改字段",
  drop_column: "删除字段",
  add_index: "新增索引",
  drop_index: "删除索引",
  alter_table_comment: "修改表注释",
};

const RISK_LABELS: Record<Risk, string> = {
  safe: "安全",
  caution: "谨慎",
  dangerous: "危险",
};

export function kindLabel(kind: DiffKind): string {
  return KIND_LABELS[kind] ?? kind;
}

export function riskLabel(risk: Risk): string {
  return RISK_LABELS[risk] ?? risk;
}

export function formatHistoryTime(ts: number): string {
  try {
    return new Date(ts).toLocaleString("zh-CN", { hour12: false });
  } catch {
    return String(ts);
  }
}
