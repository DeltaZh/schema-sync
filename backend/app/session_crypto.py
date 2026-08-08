"""会话层 ECDH P-256 + HKDF-SHA256 + AES-256-GCM。

公钥格式：SPKI DER 的 base64（与 Web Crypto exportKey('spki') 对齐）。
HKDF：salt=None（等价于 32 字节全零），info=b"schema-sync-v1"，输出 32 字节 AES 密钥。
密文信封：{"v":1,"nonce":"<b64 12>","ciphertext":"<b64 ct+tag>"}。
"""

from __future__ import annotations

import base64
import json
import os
import time
import uuid
from dataclasses import dataclass

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import ec
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

HKDF_INFO = b"schema-sync-v1"
SESSION_TTL_SEC = 8 * 3600
ENVELOPE_VERSION = 1


@dataclass
class SessionRecord:
    session_id: str
    aes_key: bytes
    created_at: float

    def expired(self, now: float | None = None) -> bool:
        t = time.time() if now is None else now
        return (t - self.created_at) > SESSION_TTL_SEC


class SessionStore:
    """内存会话；TTL 8 小时。"""

    def __init__(self) -> None:
        self._sessions: dict[str, SessionRecord] = {}

    def put(self, session_id: str, aes_key: bytes) -> None:
        self._sessions[session_id] = SessionRecord(
            session_id=session_id,
            aes_key=aes_key,
            created_at=time.time(),
        )

    def get(self, session_id: str) -> SessionRecord | None:
        rec = self._sessions.get(session_id)
        if rec is None:
            return None
        if rec.expired():
            del self._sessions[session_id]
            return None
        return rec

    def purge_expired(self) -> None:
        now = time.time()
        dead = [k for k, v in self._sessions.items() if v.expired(now)]
        for k in dead:
            del self._sessions[k]


def _b64e(raw: bytes) -> str:
    return base64.b64encode(raw).decode("ascii")


def _b64d(text: str) -> bytes:
    return base64.b64decode(text)


def load_spki_public(spki_b64: str):
    der = _b64d(spki_b64)
    return serialization.load_der_public_key(der)


def public_to_spki_b64(public_key) -> str:
    der = public_key.public_bytes(
        encoding=serialization.Encoding.DER,
        format=serialization.PublicFormat.SubjectPublicKeyInfo,
    )
    return _b64e(der)


def derive_aes_key(private_key, peer_public) -> bytes:
    shared = private_key.exchange(ec.ECDH(), peer_public)
    return HKDF(
        algorithm=hashes.SHA256(),
        length=32,
        salt=None,
        info=HKDF_INFO,
    ).derive(shared)


def handshake(client_public_spki_b64: str) -> tuple[str, str, bytes]:
    """完成握手，返回 (session_id, server_public_spki_b64, aes_key)。"""
    peer = load_spki_public(client_public_spki_b64)
    if not isinstance(peer, ec.EllipticCurvePublicKey):
        raise ValueError("client_public 须为 ECDH P-256 公钥")
    if peer.curve.name != "secp256r1":
        raise ValueError("仅支持 P-256 (secp256r1)")

    server_priv = ec.generate_private_key(ec.SECP256R1())
    aes_key = derive_aes_key(server_priv, peer)
    session_id = str(uuid.uuid4())
    server_public = public_to_spki_b64(server_priv.public_key())
    return session_id, server_public, aes_key


def encrypt_json(aes_key: bytes, obj: object) -> dict:
    nonce = os.urandom(12)
    plaintext = json.dumps(obj, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    ct = AESGCM(aes_key).encrypt(nonce, plaintext, None)
    return {
        "v": ENVELOPE_VERSION,
        "nonce": _b64e(nonce),
        "ciphertext": _b64e(ct),
    }


def decrypt_envelope(aes_key: bytes, envelope: dict) -> object:
    if not isinstance(envelope, dict):
        raise ValueError("密文信封须为 JSON 对象")
    if envelope.get("v") != ENVELOPE_VERSION:
        raise ValueError("不支持的信封版本")
    nonce = _b64d(envelope["nonce"])
    ct = _b64d(envelope["ciphertext"])
    if len(nonce) != 12:
        raise ValueError("nonce 须为 12 字节")
    pt = AESGCM(aes_key).decrypt(nonce, ct, None)
    return json.loads(pt.decode("utf-8"))
