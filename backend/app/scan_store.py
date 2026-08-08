"""扫描结果服务端缓存：execute 仅凭 scan_id + item_ids 取 SQL。"""

from __future__ import annotations

import time
from dataclasses import dataclass, field
from typing import Any

from app.schema_models import DiffItem

SCAN_TTL_SEC = 2 * 3600


@dataclass
class ScanRecord:
    scan_id: str
    items: list[DiffItem]
    meta: dict[str, Any] = field(default_factory=dict)
    created_at: float = field(default_factory=time.time)

    def expired(self, now: float | None = None) -> bool:
        t = time.time() if now is None else now
        return (t - self.created_at) > SCAN_TTL_SEC

    def items_by_id(self) -> dict[str, DiffItem]:
        return {i.id: i for i in self.items}


class ScanStore:
    def __init__(self) -> None:
        self._scans: dict[str, ScanRecord] = {}

    def put(
        self,
        scan_id: str,
        items: list[DiffItem],
        meta: dict[str, Any] | None = None,
    ) -> None:
        self._scans[scan_id] = ScanRecord(
            scan_id=scan_id,
            items=list(items),
            meta=dict(meta or {}),
            created_at=time.time(),
        )

    def get(self, scan_id: str) -> ScanRecord | None:
        rec = self._scans.get(scan_id)
        if rec is None:
            return None
        if rec.expired():
            del self._scans[scan_id]
            return None
        return rec

    def purge_expired(self) -> None:
        now = time.time()
        dead = [k for k, v in self._scans.items() if v.expired(now)]
        for k in dead:
            del self._scans[k]
