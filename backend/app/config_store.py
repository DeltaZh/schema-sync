from copy import deepcopy
from pathlib import Path

import yaml

from app.crypto import PasswordCrypto
from app.models import AppConfig, InstanceConfig


class ConfigStore:
    def __init__(self, config_path: Path, crypto: PasswordCrypto):
        self._config_path = config_path
        self._crypto = crypto

    def load(self) -> AppConfig:
        if not self._config_path.exists():
            return AppConfig()
        data = yaml.safe_load(self._config_path.read_text(encoding="utf-8")) or {}
        config = AppConfig.model_validate(data)
        # 加载路径：若存在明文密码则加密并原子回写
        dirty = False
        for inst in config.instances:
            if inst.password and not PasswordCrypto.is_encrypted(inst.password):
                inst.password = self._crypto.encrypt(inst.password)
                dirty = True
        if dirty:
            self._atomic_write(config)
        return config

    def save(self, config: AppConfig) -> None:
        config_to_save = deepcopy(config)
        for inst in config_to_save.instances:
            if inst.password and not PasswordCrypto.is_encrypted(inst.password):
                inst.password = self._crypto.encrypt(inst.password)
        self._atomic_write(config_to_save)

    def _atomic_write(self, config: AppConfig) -> None:
        self._config_path.parent.mkdir(parents=True, exist_ok=True)
        payload = config.model_dump(mode="json")
        text = yaml.dump(
            payload, allow_unicode=True, default_flow_style=False, sort_keys=False
        )
        tmp = self._config_path.with_suffix(self._config_path.suffix + ".tmp")
        tmp.write_text(text, encoding="utf-8")
        tmp.replace(self._config_path)

    def upsert_instance(
        self, inst: InstanceConfig, plaintext_password: str | None
    ) -> AppConfig:
        config = self.load()
        old_password = ""
        found_idx: int | None = None
        for i, existing in enumerate(config.instances):
            if existing.id == inst.id:
                old_password = existing.password
                found_idx = i
                break

        new_inst = inst.model_copy()
        if plaintext_password is not None:
            new_inst.password = (
                self._crypto.encrypt(plaintext_password) if plaintext_password else ""
            )
        else:
            new_inst.password = old_password

        if found_idx is not None:
            config.instances[found_idx] = new_inst
        else:
            config.instances.append(new_inst)

        self.save(config)
        return self.load()

    def public_instances(self) -> list[dict]:
        config = self.load()
        result: list[dict] = []
        for inst in config.instances:
            item = inst.model_dump()
            has_password = bool(inst.password)
            if has_password:
                item["password"] = "********"
            else:
                item.pop("password", None)
            item["has_password"] = has_password
            result.append(item)
        return result
