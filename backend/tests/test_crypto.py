from pathlib import Path

from app.crypto import PasswordCrypto


def test_encrypt_decrypt_roundtrip(tmp_path: Path):
    crypto = PasswordCrypto.load_or_create(tmp_path / ".schema-sync.key")
    token = crypto.encrypt("s3cret")
    assert token.startswith("enc:v1:")
    assert "s3cret" not in token
    assert crypto.decrypt(token) == "s3cret"


def test_load_existing_key_stable(tmp_path: Path):
    key = tmp_path / ".schema-sync.key"
    a = PasswordCrypto.load_or_create(key)
    token = a.encrypt("x")
    b = PasswordCrypto.load_or_create(key)
    assert b.decrypt(token) == "x"
