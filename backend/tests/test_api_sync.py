"""同步 API：execute 请求校验。"""

from pathlib import Path

from fastapi.testclient import TestClient


def _client(tmp_path: Path) -> TestClient:
    from app.main import create_app

    return TestClient(create_app(root=tmp_path))


def test_execute_rejects_null_items(tmp_path: Path):
    client = _client(tmp_path)
    resp = client.post("/api/sync/execute", json={"items": None})
    assert resp.status_code == 400
    assert "items" in resp.json()["detail"]


def test_execute_rejects_missing_items(tmp_path: Path):
    client = _client(tmp_path)
    resp = client.post("/api/sync/execute", json={})
    assert resp.status_code == 400
    assert "items" in resp.json()["detail"]


def test_execute_rejects_empty_items(tmp_path: Path):
    client = _client(tmp_path)
    resp = client.post("/api/sync/execute", json={"items": []})
    assert resp.status_code == 400
    assert "items" in resp.json()["detail"]
