"""勾选差异项的串行执行。"""

from __future__ import annotations

from collections import OrderedDict
from collections.abc import Callable
from typing import Any

from pydantic import BaseModel

from app.schema_models import DiffItem, DiffKind

# 组内执行顺序（未列出的 kind 排在末尾）
_KIND_ORDER: dict[DiffKind, int] = {
    "create_table": 0,
    "modify_table": 1,
    "add_column": 2,
    "modify_column": 3,
    "drop_column": 4,
    "drop_index": 5,
    "add_index": 6,
    "modify_index": 7,
}


class ExecResult(BaseModel):
    diff_id: str
    ok: bool
    error: str | None = None


def execute_selected(
    items: list[DiffItem],
    *,
    stop_on_error: bool,
    get_connection: Callable[[str, str], Any],
) -> list[ExecResult]:
    """按 (instance_id, database) 分组串行执行；组内按 kind 序。

    get_connection(instance_id, database) -> 连接（需支持 cursor()/close()）。
    """
    groups: OrderedDict[tuple[str, str], list[DiffItem]] = OrderedDict()
    for item in items:
        key = (item.instance_id, item.database)
        groups.setdefault(key, []).append(item)

    results: list[ExecResult] = []
    stopped = False

    for (instance_id, database), group_items in groups.items():
        # 同 kind 保持输入相对顺序（稳定排序）
        ordered = [
            item
            for _, item in sorted(
                enumerate(group_items),
                key=lambda pair: (_KIND_ORDER.get(pair[1].kind, 99), pair[0]),
            )
        ]
        if stopped:
            for item in ordered:
                results.append(
                    ExecResult(
                        diff_id=item.id,
                        ok=False,
                        error="因前序错误跳过",
                    )
                )
            continue

        try:
            conn = get_connection(instance_id, database)
        except Exception as exc:  # noqa: BLE001 — 连接失败记入结果，便于写历史
            err = str(exc) or "连接失败"
            for item in ordered:
                results.append(ExecResult(diff_id=item.id, ok=False, error=err))
            if stop_on_error:
                stopped = True
            continue

        try:
            for item in ordered:
                if stopped:
                    results.append(
                        ExecResult(
                            diff_id=item.id,
                            ok=False,
                            error="因前序错误跳过",
                        )
                    )
                    continue
                try:
                    with conn.cursor() as cur:
                        cur.execute(item.sql)
                    results.append(ExecResult(diff_id=item.id, ok=True, error=None))
                except Exception as exc:  # noqa: BLE001 — 逐条记录
                    results.append(
                        ExecResult(diff_id=item.id, ok=False, error=str(exc))
                    )
                    if stop_on_error:
                        stopped = True
        finally:
            close = getattr(conn, "close", None)
            if callable(close):
                close()

    return results
