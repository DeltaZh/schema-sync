"""从 information_schema / SHOW CREATE TABLE 抽取 TableSchema。"""

from __future__ import annotations

from typing import Any

from app.schema_models import ColumnDef, IndexDef, TableSchema


def columns_from_rows(rows: list[dict]) -> list[ColumnDef]:
    """将 information_schema.COLUMNS 行转为 ColumnDef（按 ORDINAL_POSITION）。"""
    ordered = sorted(rows, key=lambda r: int(r.get("ORDINAL_POSITION") or 0))
    result: list[ColumnDef] = []
    for row in ordered:
        nullable = str(row.get("IS_NULLABLE", "YES")).upper() == "YES"
        default = row.get("COLUMN_DEFAULT")
        if default is not None:
            default = str(default)
        result.append(
            ColumnDef(
                name=str(row["COLUMN_NAME"]),
                col_type=str(row["COLUMN_TYPE"]),
                nullable=nullable,
                default=default,
                comment=str(row.get("COLUMN_COMMENT") or ""),
                extra=str(row.get("EXTRA") or ""),
            )
        )
    return result


def indexes_from_stats_rows(rows: list[dict]) -> list[IndexDef]:
    """将 information_schema.STATISTICS 行按索引名聚合为 IndexDef。"""
    # name -> (non_unique, seq -> column)
    grouped: dict[str, tuple[int, dict[int, str]]] = {}
    order: list[str] = []
    for row in rows:
        name = str(row["INDEX_NAME"])
        seq = int(row["SEQ_IN_INDEX"])
        col = str(row["COLUMN_NAME"])
        non_unique = int(row.get("NON_UNIQUE", 1))
        if name not in grouped:
            grouped[name] = (non_unique, {})
            order.append(name)
        _, cols = grouped[name]
        cols[seq] = col

    result: list[IndexDef] = []
    for name in order:
        non_unique, cols = grouped[name]
        columns = [cols[i] for i in sorted(cols)]
        primary = name.upper() == "PRIMARY"
        unique = primary or non_unique == 0
        result.append(
            IndexDef(
                name=name,
                columns=columns,
                unique=unique,
                primary=primary,
            )
        )
    return result


def _fetch_column_rows(conn: Any, database: str, table: str) -> list[dict]:
    sql = """
        SELECT
            COLUMN_NAME,
            COLUMN_TYPE,
            IS_NULLABLE,
            COLUMN_DEFAULT,
            COLUMN_COMMENT,
            EXTRA,
            ORDINAL_POSITION
        FROM information_schema.COLUMNS
        WHERE TABLE_SCHEMA = %s AND TABLE_NAME = %s
        ORDER BY ORDINAL_POSITION
    """
    with conn.cursor() as cur:
        cur.execute(sql, (database, table))
        return list(cur.fetchall())


def _fetch_table_comment(conn: Any, database: str, table: str) -> str:
    sql = """
        SELECT TABLE_COMMENT
        FROM information_schema.TABLES
        WHERE TABLE_SCHEMA = %s AND TABLE_NAME = %s
    """
    with conn.cursor() as cur:
        cur.execute(sql, (database, table))
        row = cur.fetchone()
    if not row:
        return ""
    if isinstance(row, dict):
        return str(row.get("TABLE_COMMENT") or "")
    return str(row[0] or "")


def _fetch_stats_rows(conn: Any, database: str, table: str) -> list[dict]:
    sql = """
        SELECT
            INDEX_NAME,
            COLUMN_NAME,
            SEQ_IN_INDEX,
            NON_UNIQUE
        FROM information_schema.STATISTICS
        WHERE TABLE_SCHEMA = %s AND TABLE_NAME = %s
        ORDER BY INDEX_NAME, SEQ_IN_INDEX
    """
    with conn.cursor() as cur:
        cur.execute(sql, (database, table))
        return list(cur.fetchall())


def _fetch_create_sql(conn: Any, database: str, table: str) -> str:
    # 使用限定名；标识符中的反引号加倍转义
    db_q = database.replace("`", "``")
    tbl_q = table.replace("`", "``")
    with conn.cursor() as cur:
        cur.execute(f"SHOW CREATE TABLE `{db_q}`.`{tbl_q}`")
        row = cur.fetchone()
    if not row:
        return ""
    if isinstance(row, dict):
        return str(row.get("Create Table") or "")
    # 元组: (Table, Create Table)
    return str(row[1] if len(row) > 1 else "")


def extract_table_schema(
    conn: Any,
    database: str,
    table: str,
) -> TableSchema | None:
    """抽取单表结构；表不存在返回 None。"""
    col_rows = _fetch_column_rows(conn, database, table)
    if not col_rows:
        return None

    comment = _fetch_table_comment(conn, database, table)
    stats_rows = _fetch_stats_rows(conn, database, table)
    create_sql = _fetch_create_sql(conn, database, table)

    return TableSchema(
        name=table,
        columns=columns_from_rows(col_rows),
        indexes=indexes_from_stats_rows(stats_rows),
        comment=comment,
        create_sql=create_sql,
    )
