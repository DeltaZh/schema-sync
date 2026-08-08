//! 表结构对比：模板 vs 目标 → DiffItem 列表（含注释 detail）

use serde::{Deserialize, Serialize};

use crate::schema::{ColumnDef, IndexDef, TableSchema};
use crate::sql_gen;

/// 风险等级；仅 Safe 默认勾选
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Safe,
    Caution,
    Dangerous,
}

/// 差异类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    CreateTable,
    AddColumn,
    ModifyColumn,
    DropColumn,
    AddIndex,
    DropIndex,
    AlterTableComment,
}

impl DiffKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateTable => "create_table",
            Self::AddColumn => "add_column",
            Self::ModifyColumn => "modify_column",
            Self::DropColumn => "drop_column",
            Self::AddIndex => "add_index",
            Self::DropIndex => "drop_index",
            Self::AlterTableComment => "alter_table_comment",
        }
    }
}

/// 对比上下文（目标连接与库）
#[derive(Debug, Clone)]
pub struct DiffCtx {
    pub connection_id: String,
    pub database: String,
}

/// 单条结构差异
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffItem {
    pub id: String,
    pub kind: DiffKind,
    pub risk: Risk,
    pub connection_id: String,
    pub database: String,
    pub table: String,
    pub title: String,
    /// 人类可读说明，须含注释信息（如「字段注释: xxx」）
    pub detail: String,
    pub sql: String,
    pub selected_default: bool,
}

pub fn default_selected(risk: Risk) -> bool {
    matches!(risk, Risk::Safe)
}

pub fn make_diff_id(
    connection_id: &str,
    database: &str,
    table: &str,
    kind: DiffKind,
    name: &str,
) -> String {
    format!(
        "{connection_id}|{database}|{table}|{}|{name}",
        kind.as_str()
    )
}

fn risk_for(kind: DiffKind) -> Risk {
    match kind {
        DiffKind::DropColumn | DiffKind::DropIndex => Risk::Dangerous,
        DiffKind::ModifyColumn | DiffKind::AlterTableComment => Risk::Caution,
        DiffKind::CreateTable | DiffKind::AddColumn | DiffKind::AddIndex => Risk::Safe,
    }
}

fn format_comment(comment: &str) -> String {
    if comment.is_empty() {
        "（空）".into()
    } else {
        comment.to_string()
    }
}

fn build_item(
    kind: DiffKind,
    ctx: &DiffCtx,
    table: &str,
    name: &str,
    title: String,
    detail: String,
    sql: String,
    as_replacement: bool,
) -> DiffItem {
    // 替换对中的 add 半边标 caution，避免默认勾选造成半改
    let risk = if as_replacement && kind == DiffKind::AddIndex {
        Risk::Caution
    } else {
        risk_for(kind)
    };
    DiffItem {
        id: make_diff_id(&ctx.connection_id, &ctx.database, table, kind, name),
        kind,
        risk,
        connection_id: ctx.connection_id.clone(),
        database: ctx.database.clone(),
        table: table.to_string(),
        title,
        detail,
        sql,
        selected_default: default_selected(risk),
    }
}

fn columns_equal(a: &ColumnDef, b: &ColumnDef) -> bool {
    a.col_type == b.col_type
        && a.nullable == b.nullable
        && a.default == b.default
        && a.comment == b.comment
        && a.extra == b.extra
}

fn primary_of(indexes: &[IndexDef]) -> Option<&IndexDef> {
    indexes.iter().find(|i| i.primary)
}

fn non_primary_map(indexes: &[IndexDef]) -> std::collections::BTreeMap<&str, &IndexDef> {
    indexes
        .iter()
        .filter(|i| !i.primary)
        .map(|i| (i.name.as_str(), i))
        .collect()
}

fn index_equiv(a: &IndexDef, b: &IndexDef) -> bool {
    a.columns == b.columns && a.unique == b.unique && a.primary == b.primary
}

fn column_modify_detail(tmpl: &ColumnDef, tgt: &ColumnDef) -> String {
    let mut parts = Vec::new();
    if tmpl.col_type != tgt.col_type {
        parts.push(format!("类型: {} → {}", tgt.col_type, tmpl.col_type));
    }
    if tmpl.nullable != tgt.nullable {
        parts.push(format!(
            "可空: {} → {}",
            if tgt.nullable { "是" } else { "否" },
            if tmpl.nullable { "是" } else { "否" }
        ));
    }
    if tmpl.default != tgt.default {
        parts.push(format!(
            "默认值: {} → {}",
            tgt.default.as_deref().unwrap_or("（无）"),
            tmpl.default.as_deref().unwrap_or("（无）")
        ));
    }
    if tmpl.comment != tgt.comment {
        parts.push(format!(
            "字段注释: {} → {}",
            format_comment(&tgt.comment),
            format_comment(&tmpl.comment)
        ));
    } else {
        parts.push(format!("字段注释: {}", format_comment(&tmpl.comment)));
    }
    if tmpl.extra != tgt.extra {
        parts.push(format!("额外: {} → {}", tgt.extra, tmpl.extra));
    }
    parts.join("；")
}

/// 以模板为准对比目标表；`target == None` 表示缺表，生成 create_table
pub fn diff_table(
    template: &TableSchema,
    target: Option<&TableSchema>,
    ctx: &DiffCtx,
) -> Vec<DiffItem> {
    let table = template.name.as_str();

    let Some(target) = target else {
        let detail = format!("表注释: {}", format_comment(&template.comment));
        return vec![build_item(
            DiffKind::CreateTable,
            ctx,
            table,
            table,
            format!("创建表 {table}"),
            detail,
            sql_gen::create_table_sql(template),
            false,
        )];
    };

    let mut items = Vec::new();

    if template.comment != target.comment {
        let detail = format!(
            "表注释: {} → {}",
            format_comment(&target.comment),
            format_comment(&template.comment)
        );
        items.push(build_item(
            DiffKind::AlterTableComment,
            ctx,
            table,
            "COMMENT",
            format!("修改表注释 {table}"),
            detail,
            sql_gen::alter_table_comment_sql(table, &template.comment),
            false,
        ));
    }

    let tmpl_cols: std::collections::HashMap<&str, &ColumnDef> = template
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();
    let tgt_cols: std::collections::HashMap<&str, &ColumnDef> = target
        .columns
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    // 按模板列序产出 add/modify
    for col in &template.columns {
        let name = col.name.as_str();
        match tgt_cols.get(name) {
            None => {
                let detail = format!("字段注释: {}", format_comment(&col.comment));
                items.push(build_item(
                    DiffKind::AddColumn,
                    ctx,
                    table,
                    name,
                    format!("新增列 {table}.{name}"),
                    detail,
                    sql_gen::add_column_sql(table, col),
                    false,
                ));
            }
            Some(tgt_col) if !columns_equal(col, tgt_col) => {
                let detail = column_modify_detail(col, tgt_col);
                items.push(build_item(
                    DiffKind::ModifyColumn,
                    ctx,
                    table,
                    name,
                    format!("修改列 {table}.{name}"),
                    detail,
                    sql_gen::modify_column_sql(table, col),
                    false,
                ));
            }
            _ => {}
        }
    }

    // 按目标列序产出多余列 drop
    for col in &target.columns {
        let name = col.name.as_str();
        if !tmpl_cols.contains_key(name) {
            let detail = format!("字段注释: {}", format_comment(&col.comment));
            items.push(build_item(
                DiffKind::DropColumn,
                ctx,
                table,
                name,
                format!("删除列 {table}.{name}"),
                detail,
                sql_gen::drop_column_sql(table, name),
                false,
            ));
        }
    }

    // 主键：按 primary 标志 + 列序列等价，忽略名称差异
    let tmpl_pk = primary_of(&template.indexes);
    let tgt_pk = primary_of(&target.indexes);
    match (tmpl_pk, tgt_pk) {
        (Some(pk), None) => {
            items.push(build_item(
                DiffKind::AddIndex,
                ctx,
                table,
                if pk.name.is_empty() {
                    "PRIMARY"
                } else {
                    pk.name.as_str()
                },
                format!("新增主键 {table}"),
                "主键索引".into(),
                sql_gen::add_index_sql(table, pk),
                false,
            ));
        }
        (None, Some(pk)) => {
            items.push(build_item(
                DiffKind::DropIndex,
                ctx,
                table,
                if pk.name.is_empty() {
                    "PRIMARY"
                } else {
                    pk.name.as_str()
                },
                format!("删除主键 {table}"),
                "主键索引".into(),
                sql_gen::drop_index_sql(table, pk),
                false,
            ));
        }
        (Some(tmpl_pk), Some(tgt_pk)) if !index_equiv(tmpl_pk, tgt_pk) => {
            items.push(build_item(
                DiffKind::DropIndex,
                ctx,
                table,
                if tgt_pk.name.is_empty() {
                    "PRIMARY"
                } else {
                    tgt_pk.name.as_str()
                },
                format!("删除主键 {table}"),
                "主键索引".into(),
                sql_gen::drop_index_sql(table, tgt_pk),
                false,
            ));
            items.push(build_item(
                DiffKind::AddIndex,
                ctx,
                table,
                if tmpl_pk.name.is_empty() {
                    "PRIMARY"
                } else {
                    tmpl_pk.name.as_str()
                },
                format!("新增主键 {table}"),
                "主键索引".into(),
                sql_gen::add_index_sql(table, tmpl_pk),
                true,
            ));
        }
        _ => {}
    }

    let tmpl_idx = non_primary_map(&template.indexes);
    let tgt_idx = non_primary_map(&target.indexes);

    for (name, idx) in &tmpl_idx {
        if !tgt_idx.contains_key(name) {
            items.push(build_item(
                DiffKind::AddIndex,
                ctx,
                table,
                name,
                format!("新增索引 {table}.{name}"),
                format!("索引列: {}", idx.columns.join(", ")),
                sql_gen::add_index_sql(table, idx),
                false,
            ));
        } else if !index_equiv(idx, tgt_idx[name]) {
            items.push(build_item(
                DiffKind::DropIndex,
                ctx,
                table,
                name,
                format!("删除索引 {table}.{name}"),
                format!("索引列: {}", tgt_idx[name].columns.join(", ")),
                sql_gen::drop_index_sql(table, tgt_idx[name]),
                false,
            ));
            items.push(build_item(
                DiffKind::AddIndex,
                ctx,
                table,
                name,
                format!("新增索引 {table}.{name}"),
                format!("索引列: {}", idx.columns.join(", ")),
                sql_gen::add_index_sql(table, idx),
                true,
            ));
        }
    }

    for (name, idx) in &tgt_idx {
        if !tmpl_idx.contains_key(name) {
            items.push(build_item(
                DiffKind::DropIndex,
                ctx,
                table,
                name,
                format!("删除索引 {table}.{name}"),
                format!("索引列: {}", idx.columns.join(", ")),
                sql_gen::drop_index_sql(table, idx),
                false,
            ));
        }
    }

    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{ColumnDef, IndexDef, TableSchema};

    fn col(name: &str, typ: &str, comment: &str) -> ColumnDef {
        ColumnDef {
            name: name.into(),
            col_type: typ.into(),
            nullable: false,
            default: None,
            comment: comment.into(),
            extra: "".into(),
        }
    }

    fn col_full(
        name: &str,
        typ: &str,
        nullable: bool,
        default: Option<&str>,
        comment: &str,
    ) -> ColumnDef {
        ColumnDef {
            name: name.into(),
            col_type: typ.into(),
            nullable,
            default: default.map(str::to_string),
            comment: comment.into(),
            extra: "".into(),
        }
    }

    fn idx(name: &str, columns: &[&str], unique: bool, primary: bool) -> IndexDef {
        IndexDef {
            name: name.into(),
            columns: columns.iter().map(|s| (*s).to_string()).collect(),
            unique,
            primary,
        }
    }

    fn ctx() -> DiffCtx {
        DiffCtx {
            connection_id: "main".into(),
            database: "db1".into(),
        }
    }

    #[test]
    fn missing_column_is_safe_add_with_comment_detail() {
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![
                col("id", "int", "主键"),
                col_full("name", "varchar(64)", true, None, "名称"),
            ],
            indexes: vec![],
            create_sql: "CREATE TABLE t (id int, name varchar(64))".into(),
        };
        let tgt = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![col("id", "int", "主键")],
            indexes: vec![],
            create_sql: "".into(),
        };
        let items = diff_table(&tmpl, Some(&tgt), &ctx());
        let add = items
            .iter()
            .find(|i| i.kind == DiffKind::AddColumn)
            .expect("add_column");
        assert_eq!(add.risk, Risk::Safe);
        assert!(add.selected_default);
        assert!(add.sql.to_uppercase().contains("ADD COLUMN"));
        assert!(add.detail.contains("字段注释: 名称"));
    }

    #[test]
    fn extra_column_dangerous_not_selected() {
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![col("id", "int", "")],
            indexes: vec![],
            create_sql: "".into(),
        };
        let tgt = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![col("id", "int", ""), col("legacy", "int", "遗留")],
            indexes: vec![],
            create_sql: "".into(),
        };
        let items = diff_table(&tmpl, Some(&tgt), &ctx());
        let drop = items
            .iter()
            .find(|i| i.kind == DiffKind::DropColumn)
            .expect("drop_column");
        assert_eq!(drop.risk, Risk::Dangerous);
        assert!(!drop.selected_default);
        assert!(drop.detail.contains("字段注释: 遗留"));
    }

    #[test]
    fn missing_table_create() {
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "新表".into(),
            columns: vec![col("id", "int", "")],
            indexes: vec![],
            create_sql: "CREATE TABLE `t` (`id` int)".into(),
        };
        let items = diff_table(&tmpl, None, &ctx());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, DiffKind::CreateTable);
        assert!(items[0].sql.starts_with("CREATE TABLE"));
        assert!(items[0].detail.contains("表注释: 新表"));
        assert!(items[0].selected_default);
    }

    #[test]
    fn column_comment_change_is_modify_caution() {
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![col("id", "int", "新注释")],
            indexes: vec![],
            create_sql: "".into(),
        };
        let tgt = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![col("id", "int", "旧注释")],
            indexes: vec![],
            create_sql: "".into(),
        };
        let items = diff_table(&tmpl, Some(&tgt), &ctx());
        let mod_item = items
            .iter()
            .find(|i| i.kind == DiffKind::ModifyColumn)
            .expect("modify_column");
        assert_eq!(mod_item.risk, Risk::Caution);
        assert!(!mod_item.selected_default);
        assert!(mod_item.sql.to_uppercase().contains("MODIFY COLUMN"));
        assert!(mod_item.detail.contains("字段注释: 旧注释 → 新注释"));
    }

    #[test]
    fn table_comment_diff_is_alter_table_comment_caution() {
        let cols = vec![col("id", "int", "")];
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "模板注释".into(),
            columns: cols.clone(),
            indexes: vec![],
            create_sql: "".into(),
        };
        let tgt = TableSchema {
            name: "t".into(),
            comment: "旧注释".into(),
            columns: cols,
            indexes: vec![],
            create_sql: "".into(),
        };
        let items = diff_table(&tmpl, Some(&tgt), &ctx());
        assert_eq!(items.len(), 1);
        let item = &items[0];
        assert_eq!(item.kind, DiffKind::AlterTableComment);
        assert_eq!(item.risk, Risk::Caution);
        assert!(!item.selected_default);
        assert!(item.sql.to_uppercase().contains("ALTER TABLE"));
        assert!(item.sql.to_uppercase().contains("COMMENT="));
        assert!(item.sql.contains("模板注释"));
        assert!(item.detail.contains("表注释: 旧注释 → 模板注释"));
    }

    #[test]
    fn primary_key_name_only_diff_is_noop() {
        let cols = vec![col("id", "int", "")];
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: cols.clone(),
            indexes: vec![idx("PRIMARY", &["id"], true, true)],
            create_sql: "".into(),
        };
        let tgt = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: cols,
            indexes: vec![idx("pk_id", &["id"], true, true)],
            create_sql: "".into(),
        };
        assert!(diff_table(&tmpl, Some(&tgt), &ctx()).is_empty());
    }

    #[test]
    fn non_primary_index_change_drop_then_add_not_selected() {
        let cols = vec![col("id", "int", ""), col("name", "varchar(64)", "")];
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: cols.clone(),
            indexes: vec![idx("idx_name", &["name"], true, false)],
            create_sql: "".into(),
        };
        let tgt = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: cols,
            indexes: vec![idx("idx_name", &["name"], false, false)],
            create_sql: "".into(),
        };
        let items = diff_table(&tmpl, Some(&tgt), &ctx());
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, DiffKind::DropIndex);
        assert_eq!(items[1].kind, DiffKind::AddIndex);
        assert_eq!(items[0].risk, Risk::Dangerous);
        assert!(!items[0].selected_default);
        assert_eq!(items[1].risk, Risk::Caution);
        assert!(!items[1].selected_default);
    }

    #[test]
    fn diff_item_id_uses_connection_id() {
        let tmpl = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![col("id", "int", ""), col("name", "varchar(64)", "n")],
            indexes: vec![],
            create_sql: "".into(),
        };
        let tgt = TableSchema {
            name: "t".into(),
            comment: "".into(),
            columns: vec![col("id", "int", "")],
            indexes: vec![],
            create_sql: "".into(),
        };
        let ctx = DiffCtx {
            connection_id: "inst-1".into(),
            database: "shop_db".into(),
        };
        let items = diff_table(&tmpl, Some(&tgt), &ctx);
        let add = items
            .iter()
            .find(|i| i.kind == DiffKind::AddColumn)
            .expect("add");
        assert_eq!(add.id, "inst-1|shop_db|t|add_column|name");
    }
}
