"""会话握手：明文 JSON，仅本机；建立 ECDH P-256 会话密钥。"""

from __future__ import annotations

from typing import Annotated

from fastapi import APIRouter, Depends, HTTPException, Request
from pydantic import BaseModel, Field

from app.session_crypto import SessionStore, handshake

router = APIRouter(tags=["session"])


class HandshakeRequest(BaseModel):
    client_public: str = Field(..., description="SPKI DER base64（P-256）")


class HandshakeResponse(BaseModel):
    session_id: str
    server_public: str


def get_sessions(request: Request) -> SessionStore:
    return request.app.state.sessions


@router.post("/session/handshake", response_model=HandshakeResponse)
def session_handshake(
    body: HandshakeRequest,
    sessions: Annotated[SessionStore, Depends(get_sessions)],
) -> HandshakeResponse:
    try:
        session_id, server_public, aes_key = handshake(body.client_public)
    except Exception as exc:  # noqa: BLE001 — 握手失败统一 400
        raise HTTPException(status_code=400, detail=f"握手失败：{exc}") from exc
    sessions.put(session_id, aes_key)
    return HandshakeResponse(session_id=session_id, server_public=server_public)
