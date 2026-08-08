"""仅允许本机环回访问；不信任 X-Forwarded-For。"""

from __future__ import annotations

from starlette.middleware.base import BaseHTTPMiddleware
from starlette.requests import Request
from starlette.responses import JSONResponse, Response

# 生产允许的环回主机名；testing=True 时额外允许 Starlette TestClient
LOOPBACK_HOSTS = frozenset({"127.0.0.1", "::1", "localhost"})
TESTCLIENT_HOST = "testclient"


class LocalhostOnlyMiddleware(BaseHTTPMiddleware):
    async def dispatch(self, request: Request, call_next) -> Response:
        client = request.client
        host = client.host if client is not None else None
        testing = bool(getattr(request.app.state, "testing", False))
        allowed = set(LOOPBACK_HOSTS)
        if testing:
            allowed.add(TESTCLIENT_HOST)

        if host is None or host not in allowed:
            return JSONResponse(
                status_code=403,
                content={"detail": "仅允许本机访问"},
            )
        return await call_next(request)
