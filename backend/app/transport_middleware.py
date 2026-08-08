"""/api/* 应用层加解密（握手除外）——纯 ASGI，可靠替换请求体。"""

from __future__ import annotations

import json

from starlette.types import ASGIApp, Message, Receive, Scope, Send

from app.session_crypto import SessionStore, decrypt_envelope, encrypt_json

SESSION_HEADER = b"x-schema-sync-session"
HANDSHAKE_PATH = "/api/session/handshake"


def _header(scope: Scope, name: bytes) -> str | None:
    for key, value in scope.get("headers", []):
        if key.lower() == name:
            return value.decode("latin-1")
    return None


def _set_content_length(scope: Scope, length: int) -> None:
    headers = [
        (k, v)
        for k, v in scope.get("headers", [])
        if k.lower() != b"content-length"
    ]
    headers.append((b"content-length", str(length).encode("latin-1")))
    scope["headers"] = headers


async def _read_body(receive: Receive) -> bytes:
    chunks: list[bytes] = []
    while True:
        message = await receive()
        if message["type"] != "http.request":
            continue
        chunks.append(message.get("body", b""))
        if not message.get("more_body", False):
            break
    return b"".join(chunks)


class SessionCryptoMiddleware:
    def __init__(self, app: ASGIApp) -> None:
        self.app = app

    async def __call__(self, scope: Scope, receive: Receive, send: Send) -> None:
        if scope["type"] != "http":
            await self.app(scope, receive, send)
            return

        path = scope.get("path", "")
        if not path.startswith("/api/") or path == HANDSHAKE_PATH:
            await self.app(scope, receive, send)
            return

        app_state = scope["app"].state
        sessions: SessionStore = app_state.sessions
        session_id = _header(scope, SESSION_HEADER)
        if not session_id:
            await _send_json(send, 401, {"detail": "缺少会话头 X-Schema-Sync-Session"})
            return
        rec = sessions.get(session_id)
        if rec is None:
            await _send_json(send, 401, {"detail": "会话无效或已过期，请重新握手"})
            return

        aes_key = rec.aes_key
        method = scope.get("method", "GET").upper()
        new_receive = receive

        if method in ("POST", "PUT", "PATCH"):
            raw = await _read_body(receive)
            if raw:
                try:
                    envelope = json.loads(raw.decode("utf-8"))
                    payload = decrypt_envelope(aes_key, envelope)
                except Exception as exc:  # noqa: BLE001
                    await _send_json(send, 400, {"detail": f"请求解密失败：{exc}"})
                    return
                new_body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
            else:
                new_body = b""

            _set_content_length(scope, len(new_body))

            async def new_receive() -> Message:  # noqa: F811
                return {"type": "http.request", "body": new_body, "more_body": False}

        response_started = False
        status_code = 200
        response_headers: list[tuple[bytes, bytes]] = []
        body_chunks: list[bytes] = []

        async def send_wrapper(message: Message) -> None:
            nonlocal response_started, status_code, response_headers
            if message["type"] == "http.response.start":
                response_started = True
                status_code = message["status"]
                response_headers = list(message.get("headers", []))
                return
            if message["type"] == "http.response.body":
                body_chunks.append(message.get("body", b""))
                if message.get("more_body", False):
                    return
                body_bytes = b"".join(body_chunks)
                envelope_obj = _plain_to_envelope_obj(body_bytes, status_code)
                envelope = encrypt_json(aes_key, envelope_obj)
                out = json.dumps(envelope, ensure_ascii=False).encode("utf-8")
                headers = [
                    (k, v)
                    for k, v in response_headers
                    if k.lower() not in (b"content-length", b"content-type")
                ]
                headers.append((b"content-type", b"application/json"))
                headers.append((b"content-length", str(len(out)).encode("latin-1")))
                # 空 204 改为带信封的 200，便于客户端统一解密
                out_status = 200 if status_code == 204 else status_code
                await send({"type": "http.response.start", "status": out_status, "headers": headers})
                await send({"type": "http.response.body", "body": out, "more_body": False})
                return
            await send(message)

        await self.app(scope, new_receive, send_wrapper)


def _plain_to_envelope_obj(body_bytes: bytes, status_code: int) -> object:
    if status_code == 204 or not body_bytes:
        return None if status_code == 204 else {}
    try:
        if body_bytes[:1] in (b"{", b"["):
            return json.loads(body_bytes.decode("utf-8"))
        return {"detail": body_bytes.decode("utf-8", errors="replace")}
    except Exception:
        return {"detail": body_bytes.decode("utf-8", errors="replace")}


async def _send_json(send: Send, status: int, obj: object) -> None:
    body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
    await send(
        {
            "type": "http.response.start",
            "status": status,
            "headers": [
                (b"content-type", b"application/json"),
                (b"content-length", str(len(body)).encode("latin-1")),
            ],
        }
    )
    await send({"type": "http.response.body", "body": body, "more_body": False})
