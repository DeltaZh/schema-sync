"""差异扫描与勾选执行。"""

from __future__ import annotations

import uuid
from datetime import datetime, timezone
from typing import Annotated, Any

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel, Field

from app.api.deps import get_crypto, get_history, get_store
from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.history_store import HistoryRecord, HistoryStore
from app.mysql_client import connect
from app.schema_models import DiffItem
from app.sync_exec import ExecResult, execute_selected
from app.sync_service import ScanRequest, ScanResult, scan_differences

router = APIRouter(tags=["sync"])


class ExecuteRequest(BaseModel):
    item_ids: list[str] = Field(default_factory=list)
    items: list[DiffItem] | None = None
    stop_on_error: bool = True
    # 写入历史用；前端二次确认后一并提交
    group_id: str = ""
    template_instance_id: str = ""
    template_database: str = ""


def _plaintext(crypto: PasswordCrypto, ciphertext: str) -> str:
    if not ciphertext:
        return ""
    if PasswordCrypto.is_encrypted(ciphertext):
        return crypto.decrypt(ciphertext)
    return ciphertext


@router.post("/sync/scan")
def sync_scan(
    req: ScanRequest,
    store: Annotated[ConfigStore, Depends(get_store)],
    crypto: Annotated[PasswordCrypto, Depends(get_crypto)],
) -> ScanResult:
    try:
        return scan_differences(store, crypto, req)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except RuntimeError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc


@router.post("/sync/execute")
def sync_execute(
    body: ExecuteRequest,
    store: Annotated[ConfigStore, Depends(get_store)],
    crypto: Annotated[PasswordCrypto, Depends(get_crypto)],
    history: Annotated[HistoryStore, Depends(get_history)],
) -> list[ExecResult]:
    if not body.items:
        raise HTTPException(
            status_code=400,
            detail="execute 需携带完整勾选 items（含 sql），服务端不缓存扫描态",
        )
    id_set = set(body.item_ids) if body.item_ids else {i.id for i in body.items}
    selected = [item for item in body.items if item.id in id_set]
    if not selected:
        raise HTTPException(status_code=400, detail="未选中任何差异项")

    config = store.load()
    by_id = {i.id: i for i in config.instances}

    def get_connection(instance_id: str, database: str) -> Any:
        inst = by_id.get(instance_id)
        if inst is None:
            raise RuntimeError(f"实例不存在：{instance_id}")
        password = _plaintext(crypto, inst.password)
        conn = connect(inst, password, database=database)
        conn.autocommit(True)
        return conn

    results = execute_selected(
        selected,
        stop_on_error=body.stop_on_error,
        get_connection=get_connection,
    )
    record = HistoryRecord(
        id=str(uuid.uuid4()),
        ts=datetime.now(timezone.utc).isoformat(),
        group_id=body.group_id,
        template_instance_id=body.template_instance_id,
        template_database=body.template_database,
        stop_on_error=body.stop_on_error,
        results=results,
        item_snapshots=selected,
    )
    history.append(record)
    return results
