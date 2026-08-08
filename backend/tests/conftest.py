"""pytest fixtures：加密 TestClient（握手 + AES-GCM 信封）。"""

from __future__ import annotations

import base64
import json
import os
from pathlib import Path
from typing import Any

import pytest
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


class EncryptedClient:
    """包装 TestClient：自动握手，加解密 JSON 请求/响应。"""

    def __init__(self, client: TestClient):
        self.raw = client
        priv = ec.generate_private_key(ec.SECP256R1())
        hs = client.post(
            "/api/session/handshake",
            json={"client_public": _spki_b64(priv.public_key())},
        )
        assert hs.status_code == 200, hs.text
        body = hs.json()
        self.session_id = body["session_id"]
        self._aes_key = _derive_aes_key(priv, body["server_public"])

    def _headers(self, extra: dict | None = None) -> dict[str, str]:
        h = {"X-Schema-Sync-Session": self.session_id}
        if extra:
            h.update(extra)
        return h

    def _encrypt(self, obj: Any) -> dict:
        nonce = os.urandom(12)
        ct = AESGCM(self._aes_key).encrypt(
            nonce, json.dumps(obj, ensure_ascii=False).encode("utf-8"), None
        )
        return {
            "v": 1,
            "nonce": base64.b64encode(nonce).decode("ascii"),
            "ciphertext": base64.b64encode(ct).decode("ascii"),
        }

    def _decrypt_response(self, resp) -> Any:
        if not resp.content:
            return None
        envelope = resp.json()
        if not isinstance(envelope, dict) or "ciphertext" not in envelope:
            return envelope
        nonce = base64.b64decode(envelope["nonce"])
        ct = base64.b64decode(envelope["ciphertext"])
        pt = AESGCM(self._aes_key).decrypt(nonce, ct, None)
        if not pt or pt == b"null":
            return None
        return json.loads(pt.decode("utf-8"))

    class _Resp:
        def __init__(self, status_code: int, data: Any, text: str):
            self.status_code = status_code
            self._data = data
            self.text = text

        def json(self):
            return self._data

    def get(self, path: str, **kwargs):
        resp = self.raw.get(path, headers=self._headers(kwargs.pop("headers", None)), **kwargs)
        return self._Resp(resp.status_code, self._decrypt_response(resp), resp.text)

    def post(self, path: str, json: Any = None, **kwargs):
        body = self._encrypt(json) if json is not None else None
        resp = self.raw.post(
            path,
            headers=self._headers(kwargs.pop("headers", None)),
            json=body,
            **kwargs,
        )
        return self._Resp(resp.status_code, self._decrypt_response(resp), resp.text)

    def put(self, path: str, json: Any = None, **kwargs):
        body = self._encrypt(json) if json is not None else None
        resp = self.raw.put(
            path,
            headers=self._headers(kwargs.pop("headers", None)),
            json=body,
            **kwargs,
        )
        return self._Resp(resp.status_code, self._decrypt_response(resp), resp.text)

    def delete(self, path: str, **kwargs):
        resp = self.raw.delete(
            path, headers=self._headers(kwargs.pop("headers", None)), **kwargs
        )
        return self._Resp(resp.status_code, self._decrypt_response(resp), resp.text)


@pytest.fixture
def enc_client(tmp_path: Path) -> EncryptedClient:
    from app.main import create_app

    return EncryptedClient(TestClient(create_app(root=tmp_path, testing=True)))
