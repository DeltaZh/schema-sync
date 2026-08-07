from pathlib import Path

from app.crypto import PasswordCrypto
from app.config_store import ConfigStore
from app.models import InstanceConfig, TableGroupConfig, AppConfig


def test_save_encrypts_password(tmp_path: Path):
    crypto = PasswordCrypto.load_or_create(tmp_path / ".schema-sync.key")
    store = ConfigStore(tmp_path / "config.yaml", crypto)
    cfg = AppConfig(
        instances=[InstanceConfig(id="main", host="127.0.0.1", user="u", password="plain")],
        table_groups=[],
    )
    store.save(cfg)
    text = (tmp_path / "config.yaml").read_text()
    assert "plain" not in text
    assert "enc:v1:" in text
    loaded = store.load()
    assert crypto.decrypt(loaded.instances[0].password) == "plain"


def test_upsert_keeps_password_when_none(tmp_path: Path):
    crypto = PasswordCrypto.load_or_create(tmp_path / ".schema-sync.key")
    store = ConfigStore(tmp_path / "config.yaml", crypto)
    store.save(
        AppConfig(
            instances=[InstanceConfig(id="main", host="h", user="u", password="p1")],
            table_groups=[],
        )
    )
    store.upsert_instance(
        InstanceConfig(id="main", host="h2", user="u", password=""),
        plaintext_password=None,
    )
    loaded = store.load()
    assert loaded.instances[0].host == "h2"
    assert crypto.decrypt(loaded.instances[0].password) == "p1"
