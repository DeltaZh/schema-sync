from __future__ import annotations

from app.schema_models import (
    DiffItem,
    DiffKind,
    IndexDef,
    RiskLevel,
    TableSchema,
    default_selected,
    make_diff_id,
)
from app import sql_gen


def _risk_for(kind: DiffKind) -> RiskLevel:
    if kind in ("drop_column", "drop_index"):
        return "dangerous"
    if kind in ("modify_column", "modify_index"):
        return "caution"
    return "safe"


def _item(
    *,
    kind: DiffKind,
    instance_id: str,
    database: str,
    table: str,
    name: str,
    title: str,
    sql: str,
) -> DiffItem:
    risk = _risk_for(kind)
    return DiffItem(
        id=make_diff_id(instance_id, database, table, kind, name),
        kind=kind,
        risk=risk,
        instance_id=instance_id,
        database=database,
        table=table,
        title=title,
        sql=sql,
        selected_default=default_selected(risk),
    )


def _columns_equal(a, b) -> bool:
    return (
        a.col_type == b.col_type
        and a.nullable == b.nullable
        and a.default == b.default
        and a.comment == b.comment
    )


def _primary_of(indexes: list[IndexDef]) -> IndexDef | None:
    for idx in indexes:
        if idx.primary:
            return idx
    return None


def _non_primary_map(indexes: list[IndexDef]) -> dict[str, IndexDef]:
    return {idx.name: idx for idx in indexes if not idx.primary}


def _index_equiv(a: IndexDef, b: IndexDef) -> bool:
    return a.columns == b.columns and a.unique == b.unique and a.primary == b.primary


def diff_table(
    template: TableSchema,
    target: TableSchema | None,
    *,
    instance_id: str,
    database: str,
) -> list[DiffItem]:
    table = template.name

    if target is None:
        return [
            _item(
                kind="create_table",
                instance_id=instance_id,
                database=database,
                table=table,
                name=table,
                title=f"创建表 {table}",
                sql=sql_gen.create_table_sql(template),
            )
        ]

    items: list[DiffItem] = []

    tmpl_cols = {c.name: c for c in template.columns}
    tgt_cols = {c.name: c for c in target.columns}

    for name, col in tmpl_cols.items():
        if name not in tgt_cols:
            items.append(
                _item(
                    kind="add_column",
                    instance_id=instance_id,
                    database=database,
                    table=table,
                    name=name,
                    title=f"新增列 {table}.{name}",
                    sql=sql_gen.add_column_sql(table, col),
                )
            )
        elif not _columns_equal(col, tgt_cols[name]):
            items.append(
                _item(
                    kind="modify_column",
                    instance_id=instance_id,
                    database=database,
                    table=table,
                    name=name,
                    title=f"修改列 {table}.{name}",
                    sql=sql_gen.modify_column_sql(table, col),
                )
            )

    for name in tgt_cols:
        if name not in tmpl_cols:
            items.append(
                _item(
                    kind="drop_column",
                    instance_id=instance_id,
                    database=database,
                    table=table,
                    name=name,
                    title=f"删除列 {table}.{name}",
                    sql=sql_gen.drop_column_sql(table, name),
                )
            )

    # 主键：按 primary 标志 + 列序列等价，忽略名称差异
    tmpl_pk = _primary_of(template.indexes)
    tgt_pk = _primary_of(target.indexes)
    if tmpl_pk and not tgt_pk:
        items.append(
            _item(
                kind="add_index",
                instance_id=instance_id,
                database=database,
                table=table,
                name=tmpl_pk.name or "PRIMARY",
                title=f"新增主键 {table}",
                sql=sql_gen.add_index_sql(table, tmpl_pk),
            )
        )
    elif not tmpl_pk and tgt_pk:
        items.append(
            _item(
                kind="drop_index",
                instance_id=instance_id,
                database=database,
                table=table,
                name=tgt_pk.name or "PRIMARY",
                title=f"删除主键 {table}",
                sql=sql_gen.drop_index_sql(table, tgt_pk),
            )
        )
    elif tmpl_pk and tgt_pk and not _index_equiv(tmpl_pk, tgt_pk):
        items.append(
            _item(
                kind="drop_index",
                instance_id=instance_id,
                database=database,
                table=table,
                name=tgt_pk.name or "PRIMARY",
                title=f"删除主键 {table}",
                sql=sql_gen.drop_index_sql(table, tgt_pk),
            )
        )
        items.append(
            _item(
                kind="add_index",
                instance_id=instance_id,
                database=database,
                table=table,
                name=tmpl_pk.name or "PRIMARY",
                title=f"新增主键 {table}",
                sql=sql_gen.add_index_sql(table, tmpl_pk),
            )
        )

    # 非主键：按名对比；列序或 unique 不同 → drop + add
    tmpl_idx = _non_primary_map(template.indexes)
    tgt_idx = _non_primary_map(target.indexes)

    for name, idx in tmpl_idx.items():
        if name not in tgt_idx:
            items.append(
                _item(
                    kind="add_index",
                    instance_id=instance_id,
                    database=database,
                    table=table,
                    name=name,
                    title=f"新增索引 {table}.{name}",
                    sql=sql_gen.add_index_sql(table, idx),
                )
            )
        elif not _index_equiv(idx, tgt_idx[name]):
            items.append(
                _item(
                    kind="drop_index",
                    instance_id=instance_id,
                    database=database,
                    table=table,
                    name=name,
                    title=f"删除索引 {table}.{name}",
                    sql=sql_gen.drop_index_sql(table, tgt_idx[name]),
                )
            )
            items.append(
                _item(
                    kind="add_index",
                    instance_id=instance_id,
                    database=database,
                    table=table,
                    name=name,
                    title=f"新增索引 {table}.{name}",
                    sql=sql_gen.add_index_sql(table, idx),
                )
            )

    for name, idx in tgt_idx.items():
        if name not in tmpl_idx:
            items.append(
                _item(
                    kind="drop_index",
                    instance_id=instance_id,
                    database=database,
                    table=table,
                    name=name,
                    title=f"删除索引 {table}.{name}",
                    sql=sql_gen.drop_index_sql(table, idx),
                )
            )

    return items
