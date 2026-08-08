"""同步 API：execute 仅接受 scan_id + item_ids。"""

from pathlib import Path

from fastapi.testclient import TestClient

from tests.conftest import EncryptedClient


def _enc(tmp_path: Path) -> EncryptedClient:
    from app.main import create_app

    return EncryptedClient(TestClient(create_app(root=tmp_path, testing=True)))


def test_execute_rejects_client_items(tmp_path: Path):
    client = _enc(tmp_path)
    resp = client.post(
        "/api/sync/execute",
        json={
            "items": [],
            "item_ids": [],
            "stop_on_error": True,
        },
    )
    assert resp.status_code in (400, 422)
    detail = resp.json().get("detail", resp.json())
    assert "items" in str(detail) or "scan_id" in str(detail)


def test_execute_rejects_missing_scan_id(tmp_path: Path):
    client = _enc(tmp_path)
    resp = client.post("/api/sync/execute", json={"item_ids": ["x"], "stop_on_error": True})
    assert resp.status_code in (400, 422)


def test_execute_rejects_unknown_scan_id(tmp_path: Path):
    client = _enc(tmp_path)
    resp = client.post(
        "/api/sync/execute",
        json={"scan_id": "no-such", "item_ids": ["x"], "stop_on_error": True},
    )
    assert resp.status_code == 404
    assert "扫描" in str(resp.json().get("detail", ""))
