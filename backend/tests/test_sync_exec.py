"""扫描编排、勾选执行与历史存储单元测试（DB 用 mock/stub）。"""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock

import pytest

from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.models import AppConfig, InstanceConfig, TableGroupConfig
from app.schema_models import ColumnDef, DiffItem, TableSchema


def _diff(
    id_: str,
    kind: str,
    *,
    instance_id: str = "i1",
    database: str = "db1",
    table: str = "t",
) -> DiffItem:
    return DiffItem(
        id=id_,
        kind=kind,  # type: ignore[arg-type]
        risk="safe",
        instance_id=instance_id,
        database=database,
        table=table,
        title=id_,
        sql=f"SELECT '{id_}'",
        selected_default=True,
    )


def _tracking_connection(calls: list[str], fail_ids: set[str] | None = None):
    """返回 get_connection 可用的假连接：execute 时按 SQL 内 id 记录/失败。"""
    fail_ids = fail_ids or set()

    class FakeCursor:
        def __enter__(self):
            return self

        def __exit__(self, *args):
            return False

        def execute(self, sql: str):
            # SQL 形如 SELECT 'xxx'
            item_id = sql.split("'")[1]
            calls.append(item_id)
            if item_id in fail_ids or item_id.endswith("fail"):
                raise RuntimeError("boom")

    class FakeConn:
        def cursor(self):
            return FakeCursor()

        def close(self):
            pass

    def get_connection(instance_id: str, database: str):
        return FakeConn()

    return get_connection


def test_execute_connection_error_records_failures_and_stop():
    from app.sync_exec import execute_selected

    def boom_conn(instance_id: str, database: str):
        raise RuntimeError("conn refused")

    items = [
        _diff("a", "create_table"),
        _diff("b", "add_column"),
        _diff("c", "add_column", instance_id="i2", database="db2"),
    ]
    results = execute_selected(items, stop_on_error=True, get_connection=boom_conn)
    assert len(results) == 3
    assert all(not r.ok for r in results)
    assert "conn refused" in (results[0].error or "")
    # 同组两项都记失败；下一组因 stop 跳过
    assert results[2].error == "因前序错误跳过"


def test_execute_order_and_continue_on_error():
    from app.sync_exec import execute_selected

    calls: list[str] = []
    get_connection = _tracking_connection(calls)

    # 故意乱序传入；同库内应按 kind 序执行；fail 后仍继续
    items = [
        _diff("i1|db1|t|add_column|b", "add_column"),
        _diff("i1|db1|t|create_table|t", "create_table"),
        _diff("i1|db1|t|add_column|fail", "add_column"),
        _diff("i1|db1|t|add_column|c", "add_column"),
        _diff("i1|db1|t|modify_column|m", "modify_column"),
    ]
    results = execute_selected(items, stop_on_error=False, get_connection=get_connection)

    assert calls == [
        "i1|db1|t|create_table|t",
        "i1|db1|t|add_column|b",
        "i1|db1|t|add_column|fail",
        "i1|db1|t|add_column|c",
        "i1|db1|t|modify_column|m",
    ]
    by_id = {r.diff_id: r for r in results}
    assert by_id["i1|db1|t|add_column|fail"].ok is False
    assert "boom" in (by_id["i1|db1|t|add_column|fail"].error or "")
    assert by_id["i1|db1|t|add_column|c"].ok is True
    assert by_id["i1|db1|t|modify_column|m"].ok is True


def test_execute_stop_on_error_skips_rest():
    from app.sync_exec import execute_selected

    calls: list[str] = []
    get_connection = _tracking_connection(calls)

    items = [
        _diff("ok1", "create_table"),
        _diff("xfail", "add_column"),
        _diff("after", "add_column"),
    ]
    results = execute_selected(items, stop_on_error=True, get_connection=get_connection)

    assert calls == ["ok1", "xfail"]
    assert [r.diff_id for r in results if r.ok] == ["ok1"]
    failed = next(r for r in results if r.diff_id == "xfail")
    assert failed.ok is False
    skipped = [r for r in results if r.diff_id == "after"]
    assert len(skipped) == 1
    assert skipped[0].ok is False


def test_execute_kind_order_full_sequence():
    from app.sync_exec import execute_selected

    calls: list[str] = []
    get_connection = _tracking_connection(calls)
    kinds = [
        "add_index",
        "drop_index",
        "drop_column",
        "modify_column",
        "add_column",
        "create_table",
    ]
    items = [_diff(f"id-{k}", k) for k in kinds]
    execute_selected(items, stop_on_error=True, get_connection=get_connection)
    assert calls == [
        "id-create_table",
        "id-add_column",
        "id-modify_column",
        "id-drop_column",
        "id-drop_index",
        "id-add_index",
    ]


def test_execute_groups_by_instance_database_serially():
    from app.sync_exec import execute_selected

    opened: list[tuple[str, str]] = []
    calls: list[str] = []

    class FakeCursor:
        def __enter__(self):
            return self

        def __exit__(self, *args):
            return False

        def execute(self, sql: str):
            calls.append(sql.split("'")[1])

    class FakeConn:
        def cursor(self):
            return FakeCursor()

        def close(self):
            pass

    def get_connection(instance_id: str, database: str):
        opened.append((instance_id, database))
        return FakeConn()

    items = [
        _diff("a1", "add_column", instance_id="i1", database="db_a"),
        _diff("b1", "create_table", instance_id="i2", database="db_b"),
        _diff("a0", "create_table", instance_id="i1", database="db_a"),
    ]
    execute_selected(items, stop_on_error=True, get_connection=get_connection)

    assert opened == [("i1", "db_a"), ("i2", "db_b")]
    assert calls == ["a0", "a1", "b1"]


def test_history_store_append_and_list_recent(tmp_path: Path):
    from app.history_store import HistoryRecord, HistoryStore
    from app.sync_exec import ExecResult

    store = HistoryStore(tmp_path / "history.json")
    item = _diff("d1", "add_column")
    rec = HistoryRecord(
        id="h1",
        ts="2026-08-07T10:00:00",
        group_id="g1",
        template_instance_id="tmpl",
        template_database="tpl_db",
        stop_on_error=False,
        results=[ExecResult(diff_id="d1", ok=True, error=None)],
        item_snapshots=[item],
    )
    store.append(rec)
    store.append(
        HistoryRecord(
            id="h2",
            ts="2026-08-07T11:00:00",
            group_id="g1",
            template_instance_id="tmpl",
            template_database="tpl_db",
            stop_on_error=True,
            results=[ExecResult(diff_id="d2", ok=False, error="x")],
            item_snapshots=[],
        )
    )
    recent = store.list_recent(limit=1)
    assert len(recent) == 1
    assert recent[0].id == "h2"
    all_recs = store.list_recent(limit=50)
    assert [r.id for r in all_recs] == ["h2", "h1"]


def test_scan_continues_on_instance_failure(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    from app import sync_service
    from app.sync_service import ScanRequest, scan_differences

    crypto = PasswordCrypto.load_or_create(tmp_path / ".key")
    store = ConfigStore(tmp_path / "config.yaml", crypto)
    store.save(
        AppConfig(
            instances=[
                InstanceConfig(
                    id="tmpl",
                    host="h",
                    user="u",
                    password=crypto.encrypt("p"),
                ),
                InstanceConfig(
                    id="bad",
                    host="h",
                    user="u",
                    password=crypto.encrypt("p"),
                ),
                InstanceConfig(
                    id="good",
                    host="h",
                    user="u",
                    password=crypto.encrypt("p"),
                ),
            ],
            table_groups=[
                TableGroupConfig(
                    id="g1",
                    database_pattern="shop_*",
                    tables=["orders"],
                    instance_ids=["bad", "good"],
                )
            ],
        )
    )

    tmpl_schema = TableSchema(
        name="orders",
        columns=[ColumnDef(name="id", col_type="int", nullable=False)],
        indexes=[],
        create_sql="CREATE TABLE `orders` (`id` int)",
    )

    def fake_connect(instance, password_plaintext, database=None):
        if instance.id == "bad":
            raise RuntimeError("conn refused")
        return MagicMock(name=f"conn-{instance.id}")

    def fake_list_databases(conn):
        return ["shop_a", "other"]

    def fake_extract(conn, database, table):
        if database == "tpl_db":
            return tmpl_schema
        # 目标库缺表 → create_table
        return None

    monkeypatch.setattr(sync_service, "connect", fake_connect)
    monkeypatch.setattr(sync_service, "list_databases", fake_list_databases)
    monkeypatch.setattr(sync_service, "extract_table_schema", fake_extract)

    result = scan_differences(
        store,
        crypto,
        ScanRequest(
            group_id="g1",
            template_instance_id="tmpl",
            template_database="tpl_db",
        ),
    )
    assert any(e.instance_id == "bad" for e in result.errors)
    assert any(i.kind == "create_table" and i.instance_id == "good" for i in result.items)


def test_scan_fails_when_template_table_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    from app import sync_service
    from app.sync_service import ScanRequest, scan_differences

    crypto = PasswordCrypto.load_or_create(tmp_path / ".key")
    store = ConfigStore(tmp_path / "config.yaml", crypto)
    store.save(
        AppConfig(
            instances=[
                InstanceConfig(
                    id="tmpl",
                    host="h",
                    user="u",
                    password=crypto.encrypt("p"),
                ),
            ],
            table_groups=[
                TableGroupConfig(
                    id="g1",
                    database_pattern="shop_*",
                    tables=["orders", "missing_tbl"],
                    instance_ids=[],
                )
            ],
        )
    )

    monkeypatch.setattr(sync_service, "connect", lambda *a, **k: MagicMock())
    monkeypatch.setattr(
        sync_service,
        "extract_table_schema",
        lambda conn, database, table: (
            TableSchema(
                name="orders",
                columns=[ColumnDef(name="id", col_type="int", nullable=False)],
                indexes=[],
            )
            if table == "orders"
            else None
        ),
    )

    with pytest.raises(RuntimeError, match="missing_tbl"):
        scan_differences(
            store,
            crypto,
            ScanRequest(
                group_id="g1",
                template_instance_id="tmpl",
                template_database="tpl_db",
            ),
        )


def test_scan_rejects_empty_tables(tmp_path: Path):
    from app.sync_service import ScanRequest, scan_differences

    crypto = PasswordCrypto.load_or_create(tmp_path / ".key")
    store = ConfigStore(tmp_path / "config.yaml", crypto)
    store.save(
        AppConfig(
            instances=[],
            table_groups=[
                TableGroupConfig(
                    id="g1",
                    database_pattern="*",
                    tables=[],
                    instance_ids=[],
                )
            ],
        )
    )
    with pytest.raises(ValueError, match="tables"):
        scan_differences(
            store,
            crypto,
            ScanRequest(
                group_id="g1",
                template_instance_id="tmpl",
                template_database="tpl_db",
            ),
        )
