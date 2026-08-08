"""差异扫描与勾选执行。"""

from __future__ import annotations

import uuid
from datetime import datetime, timezone
from typing import Annotated, Any

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, Field, model_validator

from app.api.deps import get_crypto, get_history, get_store
from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.history_store import HistoryRecord, HistoryStore
from app.mysql_client import connect
from app.scan_store import ScanStore
from app.schema_models import DiffItem
from app.sync_exec import ExecResult, execute_selected
from app.sync_service import ScanError, ScanRequest, scan_differences

router = APIRouter(tags=["sync"])


class ScanApiResult(BaseModel):
    scan_id: str
    items: list[DiffItem] = Field(default_factory=list)
    errors: list[ScanError] = Field(default_factory=list)


class ExecuteRequest(BaseModel):
    scan_id: str
    item_ids: list[str] = Field(default_factory=list)
    stop_on_error: bool = True
    group_id: str = ""
    template_instance_id: str = ""
    template_database: str = ""

    @model_validator(mode="before")
    @classmethod
    def reject_client_items(cls, data: Any) -> Any:
        if isinstance(data, dict) and "items" in data and data["items"] is not None:
            raise ValueError(
                "不允许客户端提交 items/SQL，请使用 scan_id + item_ids"
            )
        return data


def get_scans(request: Request) -> ScanStore:
    return request.app.state.scans


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
    scans: Annotated[ScanStore, Depends(get_scans)],
) -> ScanApiResult:
    try:
        result = scan_differences(store, crypto, req)
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    except RuntimeError as exc:
        raise HTTPException(status_code=400, detail=str(exc)) from exc

    scan_id = str(uuid.uuid4())
    scans.put(
        scan_id,
        result.items,
        meta={
            "group_id": req.group_id,
            "template_instance_id": req.template_instance_id,
            "template_database": req.template_database,
        },
    )
    return ScanApiResult(scan_id=scan_id, items=result.items, errors=result.errors)


@router.post("/sync/execute")
def sync_execute(
    body: ExecuteRequest,
    store: Annotated[ConfigStore, Depends(get_store)],
    crypto: Annotated[PasswordCrypto, Depends(get_crypto)],
    history: Annotated[HistoryStore, Depends(get_history)],
    scans: Annotated[ScanStore, Depends(get_scans)],
) -> list[ExecResult]:
    record = scans.get(body.scan_id)
    if record is None:
        raise HTTPException(status_code=404, detail="扫描结果不存在或已过期，请重新扫描")

    by_id = record.items_by_id()
    if not body.item_ids:
        raise HTTPException(status_code=400, detail="未选中任何差异项")
    missing = [i for i in body.item_ids if i not in by_id]
    if missing:
        raise HTTPException(
            status_code=400,
            detail=f"差异项不在本次扫描中：{', '.join(missing[:5])}",
        )
    selected = [by_id[i] for i in body.item_ids]

    config = store.load()
    instances = {i.id: i for i in config.instances}

    def get_connection(instance_id: str, database: str) -> Any:
        inst = instances.get(instance_id)
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
    history.append(
        HistoryRecord(
            id=str(uuid.uuid4()),
            ts=datetime.now(timezone.utc).isoformat(),
            group_id=body.group_id,
            template_instance_id=body.template_instance_id,
            template_database=body.template_database,
            stop_on_error=body.stop_on_error,
            results=results,
            item_snapshots=selected,
        )
    )
    return results
