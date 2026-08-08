"""本机环回、会话加密、执行仅接受 scan_id。"""

from __future__ import annotations

import base64
import json
from pathlib import Path

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from fastapi.testclient import TestClient


def _spki_b64(public_key) -> str:
    der = public_key.public_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    return base64.b64encode(der).decode("ascii")


def _derive_aes_key(private_key, peer_spki_b64: str) -> bytes:
    peer_der = base64.b64decode(peer_spki_b64)
    peer_pub = serialization.load_der_public_key(peer_der)
    shared = private_key.exchange(ec.ECDH(), peer_pub)
    return HKDF(
        algorithm=hashes.SHA256(),
        length=32,
        salt=None,
        info=b"schema-sync-v1",
    ).derive(shared)


def _encrypt(aes_key: bytes, obj: object) -> dict:
    import os

    nonce = os.urandom(12)
    ct = AESGCM(aes_key).encrypt(nonce, json.dumps(obj).encode("utf-8"), None)
    return {
        "v": 1,
        "nonce": base64.b64encode(nonce).decode("ascii"),
        "ciphertext": base64.b64encode(ct).decode("ascii"),
    }


def _decrypt(aes_key: bytes, envelope: dict) -> object:
    nonce = base64.b64decode(envelope["nonce"])
    ct = base64.b64decode(envelope["ciphertext"])
    pt = AESGCM(aes_key).decrypt(nonce, ct, None)
    return json.loads(pt.decode("utf-8"))


def test_non_loopback_client_gets_403(tmp_path: Path):
    from app.main import create_app

    app = create_app(root=tmp_path, testing=False)
    client = TestClient(app)
    # Starlette TestClient 默认 host 为 testclient；非 testing 应拒绝
    resp = client.get("/api/instances")
    assert resp.status_code == 403
    assert "本机" in resp.json()["detail"]


def test_handshake_and_encrypted_instances_roundtrip(tmp_path: Path):
    from app.main import create_app

    app = create_app(root=tmp_path, testing=True)
    client = TestClient(app)

    priv = ec.generate_private_key(ec.SECP256R1())
    hs = client.post(
        "/api/session/handshake",
        json={"client_public": _spki_b64(priv.public_key())},
    )
    assert hs.status_code == 200
    body = hs.json()
    assert "session_id" in body
    assert "server_public" in body

    aes_key = _derive_aes_key(priv, body["server_public"])
    session_id = body["session_id"]

    # 明文 GET 应失败（缺会话或未加密响应路径仍要求会话）
    bare = client.get("/api/instances")
    assert bare.status_code == 401

    enc_resp = client.get(
        "/api/instances",
        headers={"X-Schema-Sync-Session": session_id},
    )
    assert enc_resp.status_code == 200
    data = _decrypt(aes_key, enc_resp.json())
    assert data == []

    # 加密 POST 创建实例
    payload = {
        "id": "main",
        "host": "127.0.0.1",
        "port": 3306,
        "user": "root",
        "password": "s3cret",
        "remark": "",
        "enabled": True,
    }
    created = client.post(
        "/api/instances",
        headers={"X-Schema-Sync-Session": session_id},
        json=_encrypt(aes_key, payload),
    )
    assert created.status_code == 200
    created_body = _decrypt(aes_key, created.json())
    assert created_body["id"] == "main"
    assert created_body.get("password") == "********"


def test_execute_rejects_client_items_requires_scan_id(tmp_path: Path):
    from app.main import create_app

    app = create_app(root=tmp_path, testing=True)
    client = TestClient(app)

    priv = ec.generate_private_key(ec.SECP256R1())
    hs = client.post(
        "/api/session/handshake",
        json={"client_public": _spki_b64(priv.public_key())},
    )
    aes_key = _derive_aes_key(priv, hs.json()["server_public"])
    session_id = hs.json()["session_id"]
    headers = {"X-Schema-Sync-Session": session_id}

    # 携带 items（客户端 SQL）应被拒绝
    resp = client.post(
        "/api/sync/execute",
        headers=headers,
        json=_encrypt(
            aes_key,
            {
                "items": [
                    {
                        "id": "x",
                        "kind": "add_column",
                        "risk": "safe",
                        "instance_id": "i",
                        "database": "d",
                        "table": "t",
                        "title": "t",
                        "sql": "SELECT 1",
                        "selected_default": True,
                    }
                ],
                "item_ids": ["x"],
                "stop_on_error": True,
            },
        ),
    )
    assert resp.status_code in (400, 422)
    detail = _decrypt(aes_key, resp.json())
    detail_text = detail.get("detail", detail) if isinstance(detail, dict) else str(detail)
    assert "scan_id" in str(detail_text) or "items" in str(detail_text)

    # 缺 scan_id
    resp2 = client.post(
        "/api/sync/execute",
        headers=headers,
        json=_encrypt(aes_key, {"item_ids": ["x"], "stop_on_error": True}),
    )
    assert resp2.status_code in (400, 422)

    # 未知 scan_id
    resp3 = client.post(
        "/api/sync/execute",
        headers=headers,
        json=_encrypt(
            aes_key,
            {"scan_id": "missing", "item_ids": ["x"], "stop_on_error": True},
        ),
    )
    assert resp3.status_code in (400, 404)
