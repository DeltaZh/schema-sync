"""请求级依赖：从 app.state 取 ConfigStore / PasswordCrypto / HistoryStore。"""

from __future__ import annotations

from fastapi import Request

from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.history_store import HistoryStore


def get_store(request: Request) -> ConfigStore:
    return request.app.state.store


def get_crypto(request: Request) -> PasswordCrypto:
    return request.app.state.crypto


def get_history(request: Request) -> HistoryStore:
    return request.app.state.history
