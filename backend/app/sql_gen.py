from __future__ import annotations

from app.schema_models import ColumnDef, IndexDef, TableSchema


def quote_ident(name: str) -> str:
    """反引号转义标识符。"""
    return "`" + name.replace("`", "``") + "`"


def _format_default(default: str | None) -> str | None:
    if default is None:
        return None
    upper = default.upper()
    if upper in {"NULL", "CURRENT_TIMESTAMP", "CURRENT_TIMESTAMP()"} or upper.startswith(
        "CURRENT_TIMESTAMP"
    ):
        return default
    # 数字字面量不加引号
    try:
        float(default)
        return default
    except ValueError:
        pass
    escaped = default.replace("\\", "\\\\").replace("'", "\\'")
    return f"'{escaped}'"


def column_definition_sql(col: ColumnDef) -> str:
    parts = [quote_ident(col.name), col.col_type]
    parts.append("NULL" if col.nullable else "NOT NULL")
    formatted = _format_default(col.default)
    if formatted is not None:
        parts.append(f"DEFAULT {formatted}")
    if col.extra:
        parts.append(col.extra)
    if col.comment:
        escaped = col.comment.replace("\\", "\\\\").replace("'", "\\'")
        parts.append(f"COMMENT '{escaped}'")
    return " ".join(parts)


def add_column_sql(table: str, col: ColumnDef) -> str:
    return f"ALTER TABLE {quote_ident(table)} ADD COLUMN {column_definition_sql(col)}"


def modify_column_sql(table: str, col: ColumnDef) -> str:
    return f"ALTER TABLE {quote_ident(table)} MODIFY COLUMN {column_definition_sql(col)}"


def drop_column_sql(table: str, column_name: str) -> str:
    return f"ALTER TABLE {quote_ident(table)} DROP COLUMN {quote_ident(column_name)}"


def add_index_sql(table: str, index: IndexDef) -> str:
    cols = ", ".join(quote_ident(c) for c in index.columns)
    if index.primary:
        return f"ALTER TABLE {quote_ident(table)} ADD PRIMARY KEY ({cols})"
    unique = "UNIQUE " if index.unique else ""
    return (
        f"ALTER TABLE {quote_ident(table)} ADD {unique}INDEX "
        f"{quote_ident(index.name)} ({cols})"
    )


def drop_index_sql(table: str, index: IndexDef) -> str:
    if index.primary:
        return f"ALTER TABLE {quote_ident(table)} DROP PRIMARY KEY"
    return f"ALTER TABLE {quote_ident(table)} DROP INDEX {quote_ident(index.name)}"


def create_table_sql(template: TableSchema) -> str:
    if template.create_sql.strip():
        return template.create_sql
    col_defs = ", ".join(column_definition_sql(c) for c in template.columns)
    sql = f"CREATE TABLE {quote_ident(template.name)} ({col_defs})"
    if template.comment:
        escaped = template.comment.replace("\\", "\\\\").replace("'", "\\'")
        sql += f" COMMENT='{escaped}'"
    return sql
