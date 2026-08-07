"""执行历史查询。"""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends, Query

from app.api.deps import get_history
from app.history_store import HistoryRecord, HistoryStore

router = APIRouter(tags=["history"])


@router.get("/history")
def list_history(
    history: Annotated[HistoryStore, Depends(get_history)],
    limit: Annotated[int, Query(ge=1, le=500)] = 50,
) -> list[HistoryRecord]:
    return history.list_recent(limit=limit)
