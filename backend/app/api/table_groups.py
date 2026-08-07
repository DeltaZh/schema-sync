"""表组配置与模板库发现。"""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Query

from app.api.deps import get_crypto, get_store
from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.discover import match_databases
from app.models import TableGroupConfig
from app.mysql_client import connect, list_databases

router = APIRouter(tags=["table-groups"])


def _plaintext(crypto: PasswordCrypto, ciphertext: str) -> str:
    if not ciphertext:
        return ""
    if PasswordCrypto.is_encrypted(ciphertext):
        return crypto.decrypt(ciphertext)
    return ciphertext


@router.get("/table-groups")
def list_table_groups(
    store: Annotated[ConfigStore, Depends(get_store)],
) -> list[TableGroupConfig]:
    return store.load().table_groups


@router.put("/table-groups")
def replace_table_groups(
    groups: list[TableGroupConfig],
    store: Annotated[ConfigStore, Depends(get_store)],
) -> list[TableGroupConfig]:
    config = store.load()
    config.table_groups = groups
    store.save(config)
    return store.load().table_groups


@router.get("/table-groups/{group_id}/databases")
def list_matched_databases(
    group_id: str,
    instance_id: Annotated[str, Query()],
    store: Annotated[ConfigStore, Depends(get_store)],
    crypto: Annotated[PasswordCrypto, Depends(get_crypto)],
) -> list[str]:
    config = store.load()
    group = next((g for g in config.table_groups if g.id == group_id), None)
    if group is None:
        raise HTTPException(status_code=404, detail=f"表组不存在：{group_id}")
    inst = next((i for i in config.instances if i.id == instance_id), None)
    if inst is None:
        raise HTTPException(status_code=404, detail=f"实例不存在：{instance_id}")
    try:
        password = _plaintext(crypto, inst.password)
        conn = connect(inst, password)
        try:
            names = list_databases(conn)
        finally:
            conn.close()
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return match_databases(names, group.database_pattern, exclude=None)
