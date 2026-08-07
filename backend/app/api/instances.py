"""实例 CRUD 与测连通。"""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException
from pydantic import BaseModel

from app.api.deps import get_crypto, get_store
from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.models import InstanceConfig
from app.mysql_client import ping as mysql_ping

router = APIRouter(tags=["instances"])


class InstanceWrite(BaseModel):
    id: str
    host: str
    port: int = 3306
    user: str
    password: str | None = None
    remark: str = ""
    enabled: bool = True


def _public_one(store: ConfigStore, instance_id: str) -> dict:
    for item in store.public_instances():
        if item["id"] == instance_id:
            return item
    raise HTTPException(status_code=404, detail=f"实例不存在：{instance_id}")


@router.get("/instances")
def list_instances(store: Annotated[ConfigStore, Depends(get_store)]) -> list[dict]:
    return store.public_instances()


@router.post("/instances")
def create_instance(
    body: InstanceWrite,
    store: Annotated[ConfigStore, Depends(get_store)],
) -> dict:
    config = store.load()
    if any(i.id == body.id for i in config.instances):
        raise HTTPException(status_code=409, detail=f"实例已存在：{body.id}")
    plaintext = body.password if body.password is not None else ""
    inst = InstanceConfig(
        id=body.id,
        host=body.host,
        port=body.port,
        user=body.user,
        password="",
        remark=body.remark,
        enabled=body.enabled,
    )
    store.upsert_instance(inst, plaintext_password=plaintext)
    return _public_one(store, body.id)


@router.put("/instances/{instance_id}")
def update_instance(
    instance_id: str,
    body: InstanceWrite,
    store: Annotated[ConfigStore, Depends(get_store)],
) -> dict:
    config = store.load()
    if not any(i.id == instance_id for i in config.instances):
        raise HTTPException(status_code=404, detail=f"实例不存在：{instance_id}")
    # password 省略或 null → 保留原密文；显式字符串（含空）则更新
    plaintext: str | None = body.password
    inst = InstanceConfig(
        id=instance_id,
        host=body.host,
        port=body.port,
        user=body.user,
        password="",
        remark=body.remark,
        enabled=body.enabled,
    )
    store.upsert_instance(inst, plaintext_password=plaintext)
    return _public_one(store, instance_id)


@router.delete("/instances/{instance_id}")
def delete_instance(
    instance_id: str,
    store: Annotated[ConfigStore, Depends(get_store)],
) -> dict:
    config = store.load()
    before = len(config.instances)
    config.instances = [i for i in config.instances if i.id != instance_id]
    if len(config.instances) == before:
        raise HTTPException(status_code=404, detail=f"实例不存在：{instance_id}")
    store.save(config)
    return {"ok": True}


@router.post("/instances/{instance_id}/ping")
def ping_instance(
    instance_id: str,
    store: Annotated[ConfigStore, Depends(get_store)],
    crypto: Annotated[PasswordCrypto, Depends(get_crypto)],
) -> dict:
    config = store.load()
    inst = next((i for i in config.instances if i.id == instance_id), None)
    if inst is None:
        raise HTTPException(status_code=404, detail=f"实例不存在：{instance_id}")
    try:
        if inst.password and PasswordCrypto.is_encrypted(inst.password):
            password = crypto.decrypt(inst.password)
        else:
            password = inst.password or ""
        mysql_ping(inst, password)
    except Exception as exc:  # noqa: BLE001
        raise HTTPException(status_code=400, detail=str(exc)) from exc
    return {"ok": True}
