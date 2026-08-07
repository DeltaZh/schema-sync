from pathlib import Path


def data_root() -> Path:
    # 默认项目根（backend 的上一级）；可用环境变量 SCHEMA_SYNC_ROOT 覆盖
    import os

    if env := os.environ.get("SCHEMA_SYNC_ROOT"):
        return Path(env)
    return Path(__file__).resolve().parents[2]


def key_path(root: Path | None = None) -> Path:
    return (root or data_root()) / ".schema-sync.key"


def config_path(root: Path | None = None) -> Path:
    return (root or data_root()) / "config.yaml"


def history_path(root: Path | None = None) -> Path:
    return (root or data_root()) / "data" / "history.json"
