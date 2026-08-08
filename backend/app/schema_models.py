from __future__ import annotations

from typing import Literal

from pydantic import BaseModel


RiskLevel = Literal["safe", "caution", "dangerous"]
DiffKind = Literal[
    "create_table",
    "modify_table",
    "add_column",
    "modify_column",
    "drop_column",
    "add_index",
    "drop_index",
    "modify_index",
]


class ColumnDef(BaseModel):
    name: str
    col_type: str
    nullable: bool
    default: str | None = None
    comment: str = ""
    extra: str = ""


class IndexDef(BaseModel):
    name: str
    columns: list[str]
    unique: bool
    primary: bool


class TableSchema(BaseModel):
    name: str
    columns: list[ColumnDef]
    indexes: list[IndexDef]
    comment: str = ""
    create_sql: str = ""


class DiffItem(BaseModel):
    id: str
    kind: DiffKind
    risk: RiskLevel
    instance_id: str
    database: str
    table: str
    title: str
    sql: str
    selected_default: bool


def default_selected(risk: RiskLevel) -> bool:
    """仅 safe 默认勾选。"""
    return risk == "safe"


def make_diff_id(
    instance_id: str,
    database: str,
    table: str,
    kind: DiffKind,
    name: str,
) -> str:
    return f"{instance_id}|{database}|{table}|{kind}|{name}"
