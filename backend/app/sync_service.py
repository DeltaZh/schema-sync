"""差异扫描编排：模板抽取 → 多实例匹配库 → 逐表 diff。"""

from __future__ import annotations

from pydantic import BaseModel, Field

from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.discover import match_databases
from app.mysql_client import connect, list_databases
from app.schema_diff import diff_table
from app.schema_extract import extract_table_schema
from app.schema_models import DiffItem, TableSchema


class ScanRequest(BaseModel):
    group_id: str
    template_instance_id: str
    template_database: str


class ScanError(BaseModel):
    instance_id: str
    database: str | None = None
    message: str


class ScanResult(BaseModel):
    items: list[DiffItem] = Field(default_factory=list)
    errors: list[ScanError] = Field(default_factory=list)


def _plaintext_password(crypto: PasswordCrypto, ciphertext: str) -> str:
    if not ciphertext:
        return ""
    if PasswordCrypto.is_encrypted(ciphertext):
        return crypto.decrypt(ciphertext)
    return ciphertext


def scan_differences(
    store: ConfigStore,
    crypto: PasswordCrypto,
    req: ScanRequest,
) -> ScanResult:
    """扫描表组在各实例上的结构差异。

    - 模板表任一缺失：整体失败（抛 RuntimeError）
    - 单实例失败：记入 errors，继续其他实例
    """
    config = store.load()
    group = next((g for g in config.table_groups if g.id == req.group_id), None)
    if group is None:
        raise ValueError(f"表组不存在：{req.group_id}")
    if not group.tables:
        raise ValueError("表组 tables 不能为空")

    instances_by_id = {i.id: i for i in config.instances}
    tmpl_inst = instances_by_id.get(req.template_instance_id)
    if tmpl_inst is None:
        raise ValueError(f"模板实例不存在：{req.template_instance_id}")

    tmpl_password = _plaintext_password(crypto, tmpl_inst.password)
    tmpl_conn = connect(tmpl_inst, tmpl_password, database=req.template_database)
    templates: dict[str, TableSchema] = {}
    try:
        for table in group.tables:
            schema = extract_table_schema(tmpl_conn, req.template_database, table)
            if schema is None:
                raise RuntimeError(
                    f"模板库 {req.template_database} 缺少表 {table}"
                )
            templates[table] = schema
    finally:
        tmpl_conn.close()

    result = ScanResult()
    for instance_id in group.instance_ids:
        inst = instances_by_id.get(instance_id)
        if inst is None:
            result.errors.append(
                ScanError(
                    instance_id=instance_id,
                    database=None,
                    message=f"实例不存在：{instance_id}",
                )
            )
            continue
        try:
            password = _plaintext_password(crypto, inst.password)
            conn = connect(inst, password)
        except Exception as exc:  # noqa: BLE001
            result.errors.append(
                ScanError(
                    instance_id=instance_id,
                    database=None,
                    message=str(exc),
                )
            )
            continue

        try:
            try:
                names = list_databases(conn)
            except Exception as exc:  # noqa: BLE001
                result.errors.append(
                    ScanError(
                        instance_id=instance_id,
                        database=None,
                        message=str(exc),
                    )
                )
                continue

            exclude = (
                req.template_database
                if instance_id == req.template_instance_id
                else None
            )
            databases = match_databases(
                names, group.database_pattern, exclude=exclude
            )
            for database in databases:
                for table in group.tables:
                    try:
                        target = extract_table_schema(conn, database, table)
                        items = diff_table(
                            templates[table],
                            target,
                            instance_id=instance_id,
                            database=database,
                        )
                        result.items.extend(items)
                    except Exception as exc:  # noqa: BLE001
                        result.errors.append(
                            ScanError(
                                instance_id=instance_id,
                                database=database,
                                message=f"{table}: {exc}",
                            )
                        )
        finally:
            conn.close()

    return result
