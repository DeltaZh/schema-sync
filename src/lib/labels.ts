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

/** 增 / 改 / 删 三大类 */
export type DiffCategory = "add" | "modify" | "delete";

export function categoryOfKind(kind: DiffKind): DiffCategory {
  switch (kind) {
    case "create_table":
    case "add_column":
    case "add_index":
      return "add";
    case "modify_column":
    case "alter_table_comment":
      return "modify";
    case "drop_column":
    case "drop_index":
      return "delete";
  }
}

export function categoryLabel(cat: DiffCategory): string {
  switch (cat) {
    case "add":
      return "新增";
    case "modify":
      return "修改";
    case "delete":
      return "删除";
  }
}

/** 对象类型维度：表 / 字段 / 索引 / 注释 */
export type ObjectFacet = "table" | "column" | "index" | "comment";

export function objectFacetOfKind(kind: DiffKind): ObjectFacet {
  switch (kind) {
    case "create_table":
      return "table";
    case "add_column":
    case "modify_column":
    case "drop_column":
      return "column";
    case "add_index":
    case "drop_index":
      return "index";
    case "alter_table_comment":
      return "comment";
  }
}

export function objectFacetLabel(facet: ObjectFacet): string {
  switch (facet) {
    case "table":
      return "表";
    case "column":
      return "字段";
    case "index":
      return "索引";
    case "comment":
      return "注释";
  }
}

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
