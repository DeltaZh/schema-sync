from pathlib import Path

from cryptography.fernet import Fernet

PREFIX = "enc:v1:"


class PasswordCrypto:
    def __init__(self, fernet: Fernet):
        self._fernet = fernet

    @classmethod
    def load_or_create(cls, key_path: Path) -> "PasswordCrypto":
        if key_path.exists():
            key = key_path.read_bytes().strip()
        else:
            key = Fernet.generate_key()
            key_path.parent.mkdir(parents=True, exist_ok=True)
            key_path.write_bytes(key)
            key_path.chmod(0o600)
        return cls(Fernet(key))

    @staticmethod
    def is_encrypted(value: str) -> bool:
        return value.startswith(PREFIX)

    def encrypt(self, plaintext: str) -> str:
        token = self._fernet.encrypt(plaintext.encode("utf-8")).decode("ascii")
        return PREFIX + token

    def decrypt(self, ciphertext: str) -> str:
        if not self.is_encrypted(ciphertext):
            raise ValueError("password is not enc:v1 ciphertext")
        raw = ciphertext[len(PREFIX) :].encode("ascii")
        return self._fernet.decrypt(raw).decode("utf-8")
