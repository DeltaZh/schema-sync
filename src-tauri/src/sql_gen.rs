//! DiffItem → MySQL DDL（标识符反引号转义）

use crate::schema::{ColumnDef, IndexDef, TableSchema};

/// 反引号转义标识符
pub fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

fn escape_string_literal(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

fn format_default(default: &Option<String>) -> Option<String> {
    let Some(default) = default else {
        return None;
    };
    let upper = default.to_ascii_uppercase();
    if upper == "NULL"
        || upper == "CURRENT_TIMESTAMP"
        || upper == "CURRENT_TIMESTAMP()"
        || upper.starts_with("CURRENT_TIMESTAMP")
    {
        return Some(default.clone());
    }
    if default.parse::<f64>().is_ok() {
        return Some(default.clone());
    }
    Some(format!("'{}'", escape_string_literal(default)))
}

/// 列定义片段：`name` type [NOT] NULL [DEFAULT ...] [extra] [COMMENT '...']
pub fn column_definition_sql(col: &ColumnDef) -> String {
    let mut parts = vec![quote_ident(&col.name), col.col_type.clone()];
    parts.push(if col.nullable {
        "NULL".into()
    } else {
        "NOT NULL".into()
    });
    if let Some(d) = format_default(&col.default) {
        parts.push(format!("DEFAULT {d}"));
    }
    if !col.extra.is_empty() {
        parts.push(col.extra.clone());
    }
    if !col.comment.is_empty() {
        parts.push(format!("COMMENT '{}'", escape_string_literal(&col.comment)));
    }
    parts.join(" ")
}

pub fn add_column_sql(table: &str, col: &ColumnDef) -> String {
    format!(
        "ALTER TABLE {} ADD COLUMN {}",
        quote_ident(table),
        column_definition_sql(col)
    )
}

pub fn modify_column_sql(table: &str, col: &ColumnDef) -> String {
    format!(
        "ALTER TABLE {} MODIFY COLUMN {}",
        quote_ident(table),
        column_definition_sql(col)
    )
}

pub fn drop_column_sql(table: &str, column_name: &str) -> String {
    format!(
        "ALTER TABLE {} DROP COLUMN {}",
        quote_ident(table),
        quote_ident(column_name)
    )
}

pub fn add_index_sql(table: &str, index: &IndexDef) -> String {
    let cols = index
        .columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    if index.primary {
        return format!(
            "ALTER TABLE {} ADD PRIMARY KEY ({cols})",
            quote_ident(table)
        );
    }
    let unique = if index.unique { "UNIQUE " } else { "" };
    format!(
        "ALTER TABLE {} ADD {unique}INDEX {} ({cols})",
        quote_ident(table),
        quote_ident(&index.name)
    )
}

pub fn drop_index_sql(table: &str, index: &IndexDef) -> String {
    if index.primary {
        return format!("ALTER TABLE {} DROP PRIMARY KEY", quote_ident(table));
    }
    format!(
        "ALTER TABLE {} DROP INDEX {}",
        quote_ident(table),
        quote_ident(&index.name)
    )
}

pub fn alter_table_comment_sql(table: &str, comment: &str) -> String {
    format!(
        "ALTER TABLE {} COMMENT='{}'",
        quote_ident(table),
        escape_string_literal(comment)
    )
}

pub fn create_table_sql(template: &TableSchema) -> String {
    if !template.create_sql.trim().is_empty() {
        return template.create_sql.clone();
    }
    let col_defs = template
        .columns
        .iter()
        .map(column_definition_sql)
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "CREATE TABLE {} ({col_defs})",
        quote_ident(&template.name)
    );
    if !template.comment.is_empty() {
        sql.push_str(&format!(
            " COMMENT='{}'",
            escape_string_literal(&template.comment)
        ));
    }
    sql
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ColumnDef;

    #[test]
    fn add_column_sql_includes_type_and_comment() {
        let col = ColumnDef {
            name: "name".into(),
            col_type: "varchar(64)".into(),
            nullable: true,
            default: None,
            comment: "n".into(),
            extra: "".into(),
        };
        let sql = add_column_sql("t", &col);
        assert!(sql.contains("ALTER TABLE `t` ADD COLUMN `name` varchar(64)"));
        assert!(sql.contains("COMMENT 'n'"));
    }

    #[test]
    fn alter_table_comment_escapes_quotes() {
        let sql = alter_table_comment_sql("t", "你好'世界");
        assert_eq!(sql, "ALTER TABLE `t` COMMENT='你好\\'世界'");
    }
}
