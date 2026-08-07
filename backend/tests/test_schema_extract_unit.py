"""Schema 抽取纯函数与假 cursor 单测（不连真实 MySQL）。"""

from __future__ import annotations

from typing import Any
from unittest.mock import MagicMock, patch

import pytest

from app.models import InstanceConfig
from app.schema_extract import (
    columns_from_rows,
    extract_table_schema,
    indexes_from_stats_rows,
)
from app.schema_models import ColumnDef, IndexDef


def test_columns_from_rows_maps_information_schema():
    rows = [
        {
            "COLUMN_NAME": "id",
            "COLUMN_TYPE": "bigint unsigned",
            "IS_NULLABLE": "NO",
            "COLUMN_DEFAULT": None,
            "COLUMN_COMMENT": "主键",
            "EXTRA": "auto_increment",
            "ORDINAL_POSITION": 1,
        },
        {
            "COLUMN_NAME": "name",
            "COLUMN_TYPE": "varchar(64)",
            "IS_NULLABLE": "YES",
            "COLUMN_DEFAULT": None,
            "COLUMN_COMMENT": "",
            "EXTRA": "",
            "ORDINAL_POSITION": 2,
        },
        {
            "COLUMN_NAME": "status",
            "COLUMN_TYPE": "tinyint",
            "IS_NULLABLE": "NO",
            "COLUMN_DEFAULT": "0",
            "COLUMN_COMMENT": "状态",
            "EXTRA": "",
            "ORDINAL_POSITION": 3,
        },
    ]
    cols = columns_from_rows(rows)
    assert cols == [
        ColumnDef(
            name="id",
            col_type="bigint unsigned",
            nullable=False,
            default=None,
            comment="主键",
            extra="auto_increment",
        ),
        ColumnDef(
            name="name",
            col_type="varchar(64)",
            nullable=True,
            default=None,
            comment="",
            extra="",
        ),
        ColumnDef(
            name="status",
            col_type="tinyint",
            nullable=False,
            default="0",
            comment="状态",
            extra="",
        ),
    ]


def test_indexes_from_stats_rows_groups_by_index():
    rows = [
        {
            "INDEX_NAME": "PRIMARY",
            "COLUMN_NAME": "id",
            "SEQ_IN_INDEX": 1,
            "NON_UNIQUE": 0,
        },
        {
            "INDEX_NAME": "uk_name",
            "COLUMN_NAME": "name",
            "SEQ_IN_INDEX": 1,
            "NON_UNIQUE": 0,
        },
        {
            "INDEX_NAME": "idx_a_b",
            "COLUMN_NAME": "a",
            "SEQ_IN_INDEX": 1,
            "NON_UNIQUE": 1,
        },
        {
            "INDEX_NAME": "idx_a_b",
            "COLUMN_NAME": "b",
            "SEQ_IN_INDEX": 2,
            "NON_UNIQUE": 1,
        },
    ]
    indexes = indexes_from_stats_rows(rows)
    assert indexes == [
        IndexDef(name="PRIMARY", columns=["id"], unique=True, primary=True),
        IndexDef(name="uk_name", columns=["name"], unique=True, primary=False),
        IndexDef(name="idx_a_b", columns=["a", "b"], unique=False, primary=False),
    ]


def test_indexes_from_stats_rows_respects_column_order():
    rows = [
        {
            "INDEX_NAME": "idx_pair",
            "COLUMN_NAME": "b",
            "SEQ_IN_INDEX": 2,
            "NON_UNIQUE": 1,
        },
        {
            "INDEX_NAME": "idx_pair",
            "COLUMN_NAME": "a",
            "SEQ_IN_INDEX": 1,
            "NON_UNIQUE": 1,
        },
    ]
    indexes = indexes_from_stats_rows(rows)
    assert indexes[0].columns == ["a", "b"]


class _FakeCursor:
    """按调用顺序返回预设结果的假 DictCursor。"""

    def __init__(self, script: list[Any]):
        self._script = list(script)
        self.executed: list[tuple[str, Any]] = []
        self._current: Any = None

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def execute(self, sql: str, params: Any = None):
        self.executed.append((sql, params))
        if not self._script:
            raise AssertionError(f"意外的 execute: {sql!r}")
        self._current = self._script.pop(0)

    def fetchall(self):
        if isinstance(self._current, list):
            return self._current
        raise AssertionError("当前结果不是 fetchall 列表")

    def fetchone(self):
        if isinstance(self._current, (dict, type(None))):
            return self._current
        raise AssertionError("当前结果不是 fetchone 行")


class _FakeConn:
    def __init__(self, cursor: _FakeCursor):
        self._cursor = cursor

    def cursor(self):
        return self._cursor


def test_extract_table_schema_returns_none_when_missing():
    cur = _FakeCursor(
        [
            [],  # COLUMNS 空 → 表不存在
        ]
    )
    conn = _FakeConn(cur)
    assert extract_table_schema(conn, "db1", "missing") is None


def test_extract_table_schema_assembles_table_schema():
    create_sql = (
        "CREATE TABLE `t` (\n"
        "  `id` bigint unsigned NOT NULL AUTO_INCREMENT COMMENT '主键',\n"
        "  `name` varchar(64) DEFAULT NULL,\n"
        "  PRIMARY KEY (`id`),\n"
        "  KEY `idx_name` (`name`)\n"
        ") ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COMMENT='示例表'"
    )
    cur = _FakeCursor(
        [
            [
                {
                    "COLUMN_NAME": "id",
                    "COLUMN_TYPE": "bigint unsigned",
                    "IS_NULLABLE": "NO",
                    "COLUMN_DEFAULT": None,
                    "COLUMN_COMMENT": "主键",
                    "EXTRA": "auto_increment",
                    "ORDINAL_POSITION": 1,
                },
                {
                    "COLUMN_NAME": "name",
                    "COLUMN_TYPE": "varchar(64)",
                    "IS_NULLABLE": "YES",
                    "COLUMN_DEFAULT": None,
                    "COLUMN_COMMENT": "",
                    "EXTRA": "",
                    "ORDINAL_POSITION": 2,
                },
            ],
            {"TABLE_COMMENT": "示例表"},
            [
                {
                    "INDEX_NAME": "PRIMARY",
                    "COLUMN_NAME": "id",
                    "SEQ_IN_INDEX": 1,
                    "NON_UNIQUE": 0,
                },
                {
                    "INDEX_NAME": "idx_name",
                    "COLUMN_NAME": "name",
                    "SEQ_IN_INDEX": 1,
                    "NON_UNIQUE": 1,
                },
            ],
            {"Table": "t", "Create Table": create_sql},
        ]
    )
    conn = _FakeConn(cur)
    schema = extract_table_schema(conn, "db1", "t")
    assert schema is not None
    assert schema.name == "t"
    assert schema.comment == "示例表"
    assert schema.create_sql == create_sql
    assert [c.name for c in schema.columns] == ["id", "name"]
    assert schema.columns[0].extra == "auto_increment"
    assert [i.name for i in schema.indexes] == ["PRIMARY", "idx_name"]


def test_connect_uses_pymysql_dict_cursor():
    from app.mysql_client import connect

    inst = InstanceConfig(id="a", host="127.0.0.1", port=3307, user="u")
    fake = MagicMock(name="conn")
    with patch("app.mysql_client.pymysql.connect", return_value=fake) as m:
        conn = connect(inst, "secret", database="db1")
    assert conn is fake
    kwargs = m.call_args.kwargs
    assert kwargs["host"] == "127.0.0.1"
    assert kwargs["port"] == 3307
    assert kwargs["user"] == "u"
    assert kwargs["password"] == "secret"
    assert kwargs["database"] == "db1"
    assert kwargs["charset"] == "utf8mb4"
    import pymysql.cursors

    assert kwargs["cursorclass"] is pymysql.cursors.DictCursor


def test_list_databases_filters_system_schemas():
    from app.mysql_client import list_databases

    cur = _FakeCursor(
        [
            [
                {"Database": "information_schema"},
                {"Database": "mysql"},
                {"Database": "performance_schema"},
                {"Database": "sys"},
                {"Database": "order_2025_lemi"},
                {"Database": "product_lemi"},
            ]
        ]
    )
    names = list_databases(_FakeConn(cur))
    assert names == ["order_2025_lemi", "product_lemi"]


def test_ping_raises_clear_error_on_failure():
    from app.mysql_client import ping

    inst = InstanceConfig(id="a", host="bad.host", port=3306, user="u")
    with patch(
        "app.mysql_client.connect",
        side_effect=OSError("连接被拒绝"),
    ):
        with pytest.raises(RuntimeError, match="无法连接实例|连接失败|bad.host"):
            ping(inst, "pw")
