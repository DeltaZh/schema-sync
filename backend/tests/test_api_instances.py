"""实例 API：密码不明文回传，落盘为密文。"""

from pathlib import Path

from fastapi.testclient import TestClient


def test_create_instance_get_masks_password_and_yaml_encrypted(tmp_path: Path):
    from app.main import create_app

    app = create_app(root=tmp_path)
    client = TestClient(app)

    resp = client.post(
        "/api/instances",
        json={
            "id": "main",
            "host": "127.0.0.1",
            "port": 3306,
            "user": "root",
            "password": "s3cret-plain",
            "remark": "本机",
            "enabled": True,
        },
    )
    assert resp.status_code == 200
    created = resp.json()
    assert created["id"] == "main"
    assert created.get("password") == "********"
    assert created["has_password"] is True
    assert "s3cret-plain" not in resp.text

    listed = client.get("/api/instances")
    assert listed.status_code == 200
    body = listed.json()
    assert len(body) == 1
    assert body[0]["password"] == "********"
    assert body[0]["has_password"] is True
    assert "s3cret-plain" not in listed.text

    yaml_text = (tmp_path / "config.yaml").read_text(encoding="utf-8")
    assert "s3cret-plain" not in yaml_text
    assert "enc:v1:" in yaml_text


def test_put_omitted_password_keeps_existing(tmp_path: Path):
    from app.crypto import PasswordCrypto
    from app.main import create_app

    app = create_app(root=tmp_path)
    client = TestClient(app)

    client.post(
        "/api/instances",
        json={
            "id": "main",
            "host": "127.0.0.1",
            "port": 3306,
            "user": "root",
            "password": "keep-me",
            "remark": "",
            "enabled": True,
        },
    )

    resp = client.put(
        "/api/instances/main",
        json={
            "id": "main",
            "host": "10.0.0.1",
            "port": 3307,
            "user": "admin",
            "remark": "改主机",
            "enabled": True,
        },
    )
    assert resp.status_code == 200
    assert resp.json()["host"] == "10.0.0.1"
    assert resp.json()["password"] == "********"

    crypto = PasswordCrypto.load_or_create(tmp_path / ".schema-sync.key")
    from app.config_store import ConfigStore

    store = ConfigStore(tmp_path / "config.yaml", crypto)
    loaded = store.load()
    assert loaded.instances[0].host == "10.0.0.1"
    assert crypto.decrypt(loaded.instances[0].password) == "keep-me"


def test_put_null_password_keeps_existing(tmp_path: Path):
    from app.crypto import PasswordCrypto
    from app.config_store import ConfigStore
    from app.main import create_app

    app = create_app(root=tmp_path)
    client = TestClient(app)

    client.post(
        "/api/instances",
        json={
            "id": "main",
            "host": "h",
            "port": 3306,
            "user": "u",
            "password": "original",
            "remark": "",
            "enabled": True,
        },
    )

    resp = client.put(
        "/api/instances/main",
        json={
            "id": "main",
            "host": "h2",
            "port": 3306,
            "user": "u",
            "password": None,
            "remark": "",
            "enabled": True,
        },
    )
    assert resp.status_code == 200

    crypto = PasswordCrypto.load_or_create(tmp_path / ".schema-sync.key")
    store = ConfigStore(tmp_path / "config.yaml", crypto)
    assert crypto.decrypt(store.load().instances[0].password) == "original"


def test_post_duplicate_id_returns_409(tmp_path: Path):
    from app.main import create_app

    app = create_app(root=tmp_path)
    client = TestClient(app)
    payload = {
        "id": "main",
        "host": "127.0.0.1",
        "port": 3306,
        "user": "root",
        "password": "s3cret",
        "remark": "",
        "enabled": True,
    }
    assert client.post("/api/instances", json=payload).status_code == 200
    resp = client.post("/api/instances", json=payload)
    assert resp.status_code == 409
    assert "main" in resp.json()["detail"]


def test_post_duplicate_id_cannot_clear_password(tmp_path: Path):
    from app.crypto import PasswordCrypto
    from app.config_store import ConfigStore
    from app.main import create_app

    app = create_app(root=tmp_path)
    client = TestClient(app)

    client.post(
        "/api/instances",
        json={
            "id": "main",
            "host": "127.0.0.1",
            "port": 3306,
            "user": "root",
            "password": "keep-me",
            "remark": "",
            "enabled": True,
        },
    )

    resp = client.post(
        "/api/instances",
        json={
            "id": "main",
            "host": "127.0.0.1",
            "port": 3306,
            "user": "root",
            "password": None,
            "remark": "",
            "enabled": True,
        },
    )
    assert resp.status_code == 409

    crypto = PasswordCrypto.load_or_create(tmp_path / ".schema-sync.key")
    store = ConfigStore(tmp_path / "config.yaml", crypto)
    assert crypto.decrypt(store.load().instances[0].password) == "keep-me"


def test_delete_instance(tmp_path: Path):
    from app.main import create_app

    app = create_app(root=tmp_path)
    client = TestClient(app)

    client.post(
        "/api/instances",
        json={
            "id": "gone",
            "host": "h",
            "port": 3306,
            "user": "u",
            "password": "x",
            "remark": "",
            "enabled": True,
        },
    )
    assert client.delete("/api/instances/gone").status_code == 200
    assert client.get("/api/instances").json() == []
