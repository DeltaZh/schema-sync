"""MySQL 连接、测连通与列库。"""

from __future__ import annotations

import pymysql
from pymysql.cursors import DictCursor

from app.models import InstanceConfig

# 列库时排除的系统库
_SYSTEM_DATABASES = frozenset(
    {
        "information_schema",
        "mysql",
        "performance_schema",
        "sys",
    }
)


def connect(
    instance: InstanceConfig,
    password_plaintext: str,
    database: str | None = None,
) -> pymysql.Connection:
    """使用明文密码建立连接（DictCursor）。"""
    return pymysql.connect(
        host=instance.host,
        port=instance.port,
        user=instance.user,
        password=password_plaintext,
        database=database,
        charset="utf8mb4",
        cursorclass=DictCursor,
    )


def ping(instance: InstanceConfig, password_plaintext: str) -> None:
    """测连通；失败抛出含实例信息的清晰异常。"""
    try:
        conn = connect(instance, password_plaintext)
    except Exception as exc:  # noqa: BLE001 — 统一包装为可读错误
        raise RuntimeError(
            f"无法连接实例 {instance.id}（{instance.host}:{instance.port}）：{exc}"
        ) from exc
    try:
        with conn.cursor() as cur:
            cur.execute("SELECT 1")
            cur.fetchone()
    except Exception as exc:  # noqa: BLE001
        raise RuntimeError(
            f"连接失败：实例 {instance.id}（{instance.host}:{instance.port}）：{exc}"
        ) from exc
    finally:
        conn.close()


def list_databases(conn: pymysql.Connection) -> list[str]:
    """列出业务库名（排除系统库），按名称排序。"""
    with conn.cursor() as cur:
        cur.execute("SHOW DATABASES")
        rows = cur.fetchall()
    names: list[str] = []
    for row in rows:
        # DictCursor: {"Database": "..."}；兼容元组形态
        if isinstance(row, dict):
            name = row.get("Database") or next(iter(row.values()))
        else:
            name = row[0]
        if name and name not in _SYSTEM_DATABASES:
            names.append(str(name))
    return sorted(names)
