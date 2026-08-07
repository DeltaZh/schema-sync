"""执行历史 JSON 落盘。"""

from __future__ import annotations

import json
from pathlib import Path

from pydantic import BaseModel, Field

from app.schema_models import DiffItem
from app.sync_exec import ExecResult


class HistoryRecord(BaseModel):
    id: str
    ts: str
    group_id: str
    template_instance_id: str
    template_database: str
    stop_on_error: bool
    results: list[ExecResult] = Field(default_factory=list)
    item_snapshots: list[DiffItem] = Field(default_factory=list)


class HistoryStore:
    def __init__(self, path: Path):
        self._path = path

    def _load_all(self) -> list[HistoryRecord]:
        if not self._path.exists():
            return []
        raw = json.loads(self._path.read_text(encoding="utf-8") or "[]")
        if not isinstance(raw, list):
            return []
        return [HistoryRecord.model_validate(item) for item in raw]

    def _save_all(self, records: list[HistoryRecord]) -> None:
        self._path.parent.mkdir(parents=True, exist_ok=True)
        payload = [r.model_dump(mode="json") for r in records]
        self._path.write_text(
            json.dumps(payload, ensure_ascii=False, indent=2),
            encoding="utf-8",
        )

    def append(self, record: HistoryRecord) -> None:
        records = self._load_all()
        records.append(record)
        self._save_all(records)

    def list_recent(self, limit: int = 50) -> list[HistoryRecord]:
        records = self._load_all()
        # 新记录在后；返回最近的在前
        return list(reversed(records[-limit:]))
