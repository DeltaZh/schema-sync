"""FastAPI 入口。

默认本机监听（文档约定）：
  uvicorn app.main:app --host 127.0.0.1 --port 8787
"""

from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from app.api import history, instances, sync, table_groups
from app.config_store import ConfigStore
from app.crypto import PasswordCrypto
from app.history_store import HistoryStore
from app.paths import config_path, data_root, history_path, key_path


def create_app(root: Path | None = None) -> FastAPI:
    """创建应用；可注入 root 便于测试隔离配置/密钥/历史。"""
    app_root = root if root is not None else data_root()
    crypto = PasswordCrypto.load_or_create(key_path(app_root))
    store = ConfigStore(config_path(app_root), crypto)
    hist = HistoryStore(history_path(app_root))

    app = FastAPI(title="schema-sync")
    app.state.root = app_root
    app.state.crypto = crypto
    app.state.store = store
    app.state.history = hist

    app.include_router(instances.router, prefix="/api")
    app.include_router(table_groups.router, prefix="/api")
    app.include_router(sync.router, prefix="/api")
    app.include_router(history.router, prefix="/api")

    frontend_dist = app_root / "frontend" / "dist"
    if frontend_dist.is_dir():
        assets = frontend_dist / "assets"
        if assets.is_dir():
            app.mount("/assets", StaticFiles(directory=assets), name="assets")

        index_html = frontend_dist / "index.html"

        @app.get("/{full_path:path}")
        async def spa_fallback(full_path: str):  # noqa: ARG001
            # API 已由路由优先匹配；其余走 SPA
            if index_html.is_file():
                return FileResponse(index_html)
            return {"detail": "frontend dist missing index.html"}

    return app


app = create_app()
