# Schema Sync 实现计划

> **已作废：** 请改用 `2026-08-08-schema-sync-desktop.md`（Tauri 单机桌面版）。勿再按本文执行。

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 落地本地 Web 小工具：以模板表为准，跨多 MySQL 实例扫描同源分库指定表差异，勾选后执行结构同步。

**Architecture:** FastAPI 后端负责配置（YAML + Fernet 密文密码）、多实例连接、库发现、Schema 抽取/对比/SQL 生成与执行；Vite 轻量前端提供连接/表组/同步工作台/历史四个页面。默认仅监听 `127.0.0.1`。

**Tech Stack:** Python 3.11+、FastAPI、uvicorn、PyMySQL、PyYAML、cryptography（Fernet）、pytest；前端 Vite + Vue 3 + TypeScript。

## Global Constraints

- 密码落盘必须为 `enc:v1:` 密文；API 永不返回明文密码
- 主密钥文件：`.schema-sync.key`；与 `config.yaml` 均 gitignore
- 仅结构同步，不做数据迁移；首版不做外键/分区同步
- 危险变更（DROP COLUMN/INDEX、MODIFY）默认不勾选
- 执行须勾选 + 二次确认；按目标库串行
- Git commit message 使用约定式前缀 + 中文说明
- 工作区：`/Users/delta/cursorProject/schema-sync`

---

## File Structure

```text
schema-sync/
├── .gitignore
├── README.md
├── config.example.yaml
├── backend/
│   ├── pyproject.toml          # 或 requirements.txt + pytest.ini
│   ├── app/
│   │   ├── __init__.py
│   │   ├── main.py             # FastAPI app，挂载路由，静态前端
│   │   ├── paths.py            # 配置/密钥/历史路径
│   │   ├── crypto.py           # Fernet 加解密
│   │   ├── models.py           # Pydantic 领域模型
│   │   ├── config_store.py     # YAML 读写
│   │   ├── mysql_client.py     # 连接、测连通、列库
│   │   ├── discover.py         # glob 匹配库名
│   │   ├── schema_extract.py   # 抽取表结构
│   │   ├── schema_diff.py      # 对比生成 DiffItem
│   │   ├── sql_gen.py          # DiffItem → SQL（可与 diff 合并，但保持可测）
│   │   ├── sync_exec.py        # 勾选执行
│   │   ├── history_store.py    # 执行历史 JSON
│   │   └── api/
│   │       ├── __init__.py
│   │       ├── instances.py
│   │       ├── table_groups.py
│   │       ├── sync.py
│   │       └── history.py
│   └── tests/
│       ├── conftest.py
│       ├── test_crypto.py
│       ├── test_config_store.py
│       ├── test_discover.py
│       ├── test_schema_diff.py
│       ├── test_sql_gen.py
│       └── test_sync_exec.py
└── frontend/
    ├── package.json
    ├── vite.config.ts
    ├── index.html
    └── src/
        ├── main.ts
        ├── App.vue
        ├── api.ts
        ├── styles.css
        └── views/
            ├── InstancesView.vue
            ├── TableGroupsView.vue
            ├── SyncWorkbenchView.vue
            └── HistoryView.vue
```

---

### Task 1: 仓库脚手架与密码加解密

**Files:**
- Create: `.gitignore`
- Create: `backend/pyproject.toml`
- Create: `backend/app/__init__.py`
- Create: `backend/app/paths.py`
- Create: `backend/app/crypto.py`
- Create: `backend/tests/conftest.py`
- Create: `backend/tests/test_crypto.py`

**Interfaces:**
- Produces:
  - `paths.key_path(root: Path) -> Path` → `root / ".schema-sync.key"`
  - `paths.config_path(root: Path) -> Path` → `root / "config.yaml"`
  - `class PasswordCrypto`:
    - `PasswordCrypto.load_or_create(key_path: Path) -> PasswordCrypto`
    - `encrypt(self, plaintext: str) -> str`  # 返回 `enc:v1:<token>`
    - `decrypt(self, ciphertext: str) -> str`  # 仅接受 `enc:v1:` 前缀
    - `is_encrypted(value: str) -> bool`

- [ ] **Step 1: 写失败测试**

```python
# backend/tests/test_crypto.py
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
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd backend && python -m pytest tests/test_crypto.py -v`  
Expected: FAIL（模块不存在）

- [ ] **Step 3: 实现脚手架与 crypto**

`.gitignore`:

```gitignore
.schema-sync.key
config.yaml
__pycache__/
*.pyc
.venv/
backend/.venv/
frontend/node_modules/
frontend/dist/
.DS_Store
data/
*.log
```

`backend/pyproject.toml`:

```toml
[project]
name = "schema-sync"
version = "0.1.0"
requires-python = ">=3.11"
dependencies = [
  "fastapi>=0.115.0",
  "uvicorn[standard]>=0.32.0",
  "pymysql>=1.1.0",
  "pyyaml>=6.0",
  "cryptography>=43.0.0",
  "pydantic>=2.0",
]

[project.optional-dependencies]
dev = ["pytest>=8.0", "httpx>=0.27"]

[tool.pytest.ini_options]
pythonpath = ["."]
testpaths = ["tests"]
```

`backend/app/paths.py`:

```python
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
```

`backend/app/crypto.py`:

```python
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
        raw = ciphertext[len(PREFIX):].encode("ascii")
        return self._fernet.decrypt(raw).decode("utf-8")
```

`backend/tests/conftest.py`: 可先为空文件，或设置 `SCHEMA_SYNC_ROOT` fixture 供后续用。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd backend && python -m venv .venv && source .venv/bin/activate && pip install -e ".[dev]" && pytest tests/test_crypto.py -v`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add .gitignore backend/pyproject.toml backend/app backend/tests
git commit -m "$(cat <<'EOF'
feat:新增密码 Fernet 加解密与项目脚手架

为配置落盘密文密码提供本地密钥能力，并建立后端测试基线。
EOF
)"
```

---

### Task 2: 配置模型与 YAML 读写（含密码加密）

**Files:**
- Create: `backend/app/models.py`
- Create: `backend/app/config_store.py`
- Create: `backend/tests/test_config_store.py`
- Create: `config.example.yaml`

**Interfaces:**
- Produces:
  - `class InstanceConfig(BaseModel)`: `id, host, port=3306, user, password, enabled=True, remark=""`
  - `class TableGroupConfig(BaseModel)`: `id, database_pattern, tables: list[str], instance_ids: list[str]`
  - `class AppConfig(BaseModel)`: `instances: list[InstanceConfig], table_groups: list[TableGroupConfig]`
  - `class ConfigStore`:
    - `__init__(self, config_path: Path, crypto: PasswordCrypto)`
    - `load(self) -> AppConfig`  # 文件不存在则空配置
    - `save(self, config: AppConfig) -> None`  # 保存前确保 password 为密文
    - `upsert_instance(self, inst: InstanceConfig, plaintext_password: str | None) -> AppConfig`
      - `plaintext_password` 非空则加密写入；`None` 表示保留原密文（更新其它字段时）
    - `public_instances(self) -> list[dict]`  # password 替换为 `********` 或省略，带 `has_password: bool`

- [ ] **Step 1: 写失败测试**

```python
# backend/tests/test_config_store.py
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
    store.save(AppConfig(
        instances=[InstanceConfig(id="main", host="h", user="u", password="p1")],
        table_groups=[],
    ))
    store.upsert_instance(
        InstanceConfig(id="main", host="h2", user="u", password=""),
        plaintext_password=None,
    )
    loaded = store.load()
    assert loaded.instances[0].host == "h2"
    assert crypto.decrypt(loaded.instances[0].password) == "p1"
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd backend && pytest tests/test_config_store.py -v`  
Expected: FAIL

- [ ] **Step 3: 实现 models 与 config_store**

```python
# backend/app/models.py（核心字段）
from pydantic import BaseModel, Field

class InstanceConfig(BaseModel):
    id: str
    host: str
    port: int = 3306
    user: str
    password: str = ""
    enabled: bool = True
    remark: str = ""

class TableGroupConfig(BaseModel):
    id: str
    database_pattern: str
    tables: list[str] = Field(default_factory=list)
    instance_ids: list[str] = Field(default_factory=list)

class AppConfig(BaseModel):
    instances: list[InstanceConfig] = Field(default_factory=list)
    table_groups: list[TableGroupConfig] = Field(default_factory=list)
```

`ConfigStore.save`：遍历 instances，若 `not PasswordCrypto.is_encrypted(password)` 且非空，则 `encrypt` 后写入。  
`upsert_instance`：按 `id` 替换；若 `plaintext_password is None`，从旧配置拷贝 `password` 密文。

`config.example.yaml`：无真实密码，password 写 `请在页面录入` 或留空说明。

- [ ] **Step 4: 跑测试确认通过**

Run: `cd backend && pytest tests/test_config_store.py -v`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add backend/app/models.py backend/app/config_store.py backend/tests/test_config_store.py config.example.yaml
git commit -m "$(cat <<'EOF'
feat:实现配置 YAML 读写与密码密文落盘

支持实例/表组持久化，更新连接时可不改密码保留原密文。
EOF
)"
```

---

### Task 3: 库名发现（glob）

**Files:**
- Create: `backend/app/discover.py`
- Create: `backend/tests/test_discover.py`

**Interfaces:**
- Produces:
  - `match_databases(names: list[str], pattern: str, exclude: str | None = None) -> list[str]`
  - 使用 `fnmatch.fnmatch`；结果排序；`exclude` 精确排除模板库名

- [ ] **Step 1: 写失败测试**

```python
from app.discover import match_databases

def test_order_year_tenant_pattern():
    names = ["order_2025_lemi", "order_2026_whd", "product_lemi", "mysql"]
    assert match_databases(names, "order_*_*") == ["order_2025_lemi", "order_2026_whd"]

def test_exclude_template():
    names = ["order_2025_lemi", "order_2026_whd"]
    assert match_databases(names, "order_*_*", exclude="order_2025_lemi") == ["order_2026_whd"]

def test_product_pattern():
    names = ["product_lemi", "product_whd", "order_2025_lemi"]
    assert match_databases(names, "product_*") == ["product_lemi", "product_whd"]
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd backend && pytest tests/test_discover.py -v`  
Expected: FAIL

- [ ] **Step 3: 实现**

```python
# backend/app/discover.py
import fnmatch

def match_databases(names: list[str], pattern: str, exclude: str | None = None) -> list[str]:
    out = [n for n in names if fnmatch.fnmatch(n, pattern)]
    if exclude is not None:
        out = [n for n in out if n != exclude]
    return sorted(out)
```

- [ ] **Step 4: 跑测试确认通过**

Run: `cd backend && pytest tests/test_discover.py -v`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add backend/app/discover.py backend/tests/test_discover.py
git commit -m "$(cat <<'EOF'
feat:实现按 glob 规则发现同源分库

支持 order_*_* / product_* 匹配并排除模板库。
EOF
)"
```

---

### Task 4: Schema 模型、抽取接口与 Diff/SQL 生成

**Files:**
- Create: `backend/app/schema_models.py`
- Create: `backend/app/schema_diff.py`
- Create: `backend/app/sql_gen.py`
- Create: `backend/tests/test_schema_diff.py`
- Create: `backend/tests/test_sql_gen.py`

**Interfaces:**
- Produces:
  - `ColumnDef(name, col_type, nullable, default, comment, extra="")`
  - `IndexDef(name, columns: list[str], unique: bool, primary: bool)`
  - `TableSchema(name, columns: list[ColumnDef], indexes: list[IndexDef], comment: str = "", create_sql: str = "")`
  - `RiskLevel = Literal["safe", "caution", "dangerous"]`
  - `DiffKind = Literal["create_table","add_column","modify_column","drop_column","add_index","drop_index","modify_index"]`
  - `DiffItem(id, kind, risk, instance_id, database, table, title, sql, selected_default: bool)`
  - `diff_table(template: TableSchema, target: TableSchema | None, *, instance_id, database) -> list[DiffItem]`
    - `target is None` → 一条 `create_table`，sql 用 `template.create_sql`（若空则由列拼简易 CREATE，测试中提供 create_sql）
  - `default_selected(risk) -> bool`：仅 `safe` 为 True

**对比规则（实现必须遵守）：**
- 列：按名；缺 → add；多 → drop；类型/nullable/default/comment 不同 → modify
- 索引：忽略主键名差异时以 `primary` 标志 + 列序列等价；非主键按名；列序或 unique 不同 → drop+add 两条或 `modify_index` 拆成 drop_index + add_index
- 主键：归入 indexes 且 `primary=True`

- [ ] **Step 1: 写失败测试（diff）**

```python
from app.schema_models import ColumnDef, IndexDef, TableSchema
from app.schema_diff import diff_table

def _col(name, typ="int", nullable=False, default=None, comment=""):
    return ColumnDef(name=name, col_type=typ, nullable=nullable, default=default, comment=comment)

def test_missing_column_is_safe_add():
    tmpl = TableSchema(name="t", columns=[_col("id"), _col("name", "varchar(64)")], indexes=[], create_sql="CREATE TABLE t (id int, name varchar(64))")
    tgt = TableSchema(name="t", columns=[_col("id")], indexes=[])
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    kinds = [i.kind for i in items]
    assert "add_column" in kinds
    add = next(i for i in items if i.kind == "add_column")
    assert add.risk == "safe" and add.selected_default is True
    assert "ADD COLUMN" in add.sql.upper()

def test_extra_column_dangerous_not_selected():
    tmpl = TableSchema(name="t", columns=[_col("id")], indexes=[])
    tgt = TableSchema(name="t", columns=[_col("id"), _col("legacy")], indexes=[])
    items = diff_table(tmpl, tgt, instance_id="main", database="db1")
    drop = next(i for i in items if i.kind == "drop_column")
    assert drop.risk == "dangerous" and drop.selected_default is False

def test_missing_table_create():
    tmpl = TableSchema(name="t", columns=[_col("id")], indexes=[], create_sql="CREATE TABLE `t` (`id` int)")
    items = diff_table(tmpl, None, instance_id="main", database="db1")
    assert len(items) == 1 and items[0].kind == "create_table"
    assert items[0].sql.startswith("CREATE TABLE")
```

- [ ] **Step 2: 跑测试确认失败**

Run: `cd backend && pytest tests/test_schema_diff.py -v`  
Expected: FAIL

- [ ] **Step 3: 实现 schema_models + schema_diff + sql_gen**

SQL 生成要点：
- 标识符用反引号转义
- `ADD COLUMN` 带类型、NULL/NOT NULL、DEFAULT、COMMENT
- `DROP COLUMN` / `DROP INDEX`
- `CREATE TABLE` 直接使用 `template.create_sql`（抽取时填入 `SHOW CREATE TABLE` 结果）

每个 `DiffItem.id` 用稳定字符串：`{instance_id}|{database}|{table}|{kind}|{name}`。

- [ ] **Step 4: 补充 sql_gen 单测并全部跑通**

```python
# backend/tests/test_sql_gen.py
from app.schema_models import ColumnDef
from app.sql_gen import add_column_sql

def test_add_column_sql():
    sql = add_column_sql("t", ColumnDef(name="name", col_type="varchar(64)", nullable=True, default=None, comment="n"))
    assert "ALTER TABLE `t` ADD COLUMN `name` varchar(64)" in sql
```

Run: `cd backend && pytest tests/test_schema_diff.py tests/test_sql_gen.py -v`  
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add backend/app/schema_models.py backend/app/schema_diff.py backend/app/sql_gen.py backend/tests/test_schema_diff.py backend/tests/test_sql_gen.py
git commit -m "$(cat <<'EOF'
feat:实现表结构对比与 ALTER SQL 生成

按风险分级默认勾选，覆盖缺表/缺列/多余列等核心差异。
EOF
)"
```

---

### Task 5: MySQL 客户端与 Schema 抽取

**Files:**
- Create: `backend/app/mysql_client.py`
- Create: `backend/app/schema_extract.py`
- Create: `backend/tests/test_schema_extract_unit.py`  # 用伪造 cursor/行数据，不强制真连库

**Interfaces:**
- Produces:
  - `connect(instance: InstanceConfig, password_plaintext: str, database: str | None = None) -> pymysql.Connection`
  - `ping(instance, password_plaintext) -> None`  # 失败抛清晰异常
  - `list_databases(conn) -> list[str]`
  - `extract_table_schema(conn, database: str, table: str) -> TableSchema | None`
    - 查 `information_schema.COLUMNS`、`STATISTICS`/`SHOW INDEX`，并 `SHOW CREATE TABLE`
    - 不存在返回 `None`

- [ ] **Step 1: 写失败测试（基于假数据解析）**

在 `schema_extract.py` 拆出纯函数：

```python
def columns_from_rows(rows: list[dict]) -> list[ColumnDef]: ...
def indexes_from_stats_rows(rows: list[dict]) -> list[IndexDef]: ...
```

测试传入仿 `information_schema` 行，断言 ColumnDef/IndexDef。

- [ ] **Step 2: 跑测试确认失败 → Step 3 实现 → Step 4 通过**

`mysql_client.py` 使用 `pymysql.connect(host=..., port=..., user=..., password=..., database=..., charset="utf8mb4", cursorclass=DictCursor)`。

- [ ] **Step 5: Commit**

```bash
git add backend/app/mysql_client.py backend/app/schema_extract.py backend/tests/test_schema_extract_unit.py
git commit -m "$(cat <<'EOF'
feat:实现 MySQL 连接与表结构抽取

从 information_schema 与 SHOW CREATE TABLE 得到可对比的 TableSchema。
EOF
)"
```

---

### Task 6: 扫描编排与执行器

**Files:**
- Create: `backend/app/sync_service.py`
- Create: `backend/app/sync_exec.py`
- Create: `backend/app/history_store.py`
- Create: `backend/tests/test_sync_exec.py`

**Interfaces:**
- Produces:
  - `class ScanRequest`: `group_id, template_instance_id, template_database`
  - `class ScanResult`: `items: list[DiffItem], errors: list[ScanError]`
  - `class ScanError`: `instance_id, database | None, message`
  - `scan_differences(store, crypto, req) -> ScanResult`
    1. 加载表组；校验 tables 非空
    2. 连模板实例，对每个 table `extract`；任一模板表缺失则整体失败（抛/返回错误）
    3. 对每个 instance_id：list_databases → match_databases → 对每个目标库每张表 diff
    4. 单实例失败记入 `errors`，继续
  - `execute_selected(items: list[DiffItem], *, stop_on_error: bool, get_connection) -> list[ExecResult]`
    - 按 `(instance_id, database)` 分组串行
    - 组内排序：create_table → add_column → modify_column → drop_column → drop_index → add_index
    - `ExecResult(diff_id, ok, error: str | None)`
  - `HistoryStore.append(record) / list_recent(limit=50)`

- [ ] **Step 1: 写执行器单元测试（mock 执行函数）**

```python
def test_execute_order_and_continue_on_error():
    calls = []
    def run_sql(item):
        calls.append(item.id)
        if item.id.endswith("fail"):
            raise RuntimeError("boom")
        return None
    # 构造 items：同库 create / add / 一条会 fail / 后续 add
    # stop_on_error=False → 后续仍执行
    ...
```

- [ ] **Step 2–4: TDD 实现 sync_exec / history_store / sync_service（scan 可用 stub extract）**

History 记录字段：`id, ts, group_id, template_instance_id, template_database, stop_on_error, results: list[ExecResult], item_snapshots`

- [ ] **Step 5: Commit**

```bash
git add backend/app/sync_service.py backend/app/sync_exec.py backend/app/history_store.py backend/tests/test_sync_exec.py
git commit -m "$(cat <<'EOF'
feat:实现差异扫描编排与勾选执行

支持按库串行、遇错继续/停止，并写入执行历史。
EOF
)"
```

---

### Task 7: FastAPI 路由

**Files:**
- Create: `backend/app/main.py`
- Create: `backend/app/api/__init__.py`
- Create: `backend/app/api/instances.py`
- Create: `backend/app/api/table_groups.py`
- Create: `backend/app/api/sync.py`
- Create: `backend/app/api/history.py`
- Create: `backend/tests/test_api_instances.py`

**Interfaces（HTTP）：**
- `GET /api/instances` → 无明文密码
- `POST /api/instances` body: `{id,host,port,user,password,remark,enabled}` password 明文仅此写入
- `PUT /api/instances/{id}` body 同；`password` 省略或 null 表示不改
- `DELETE /api/instances/{id}`
- `POST /api/instances/{id}/ping` → `{ok: true}` / 400
- `GET/PUT /api/table-groups`（PUT 整体替换列表或提供 CRUD，任选整体替换以简化）
- `GET /api/table-groups/{id}/databases?instance_id=` → 该实例匹配到的库列表（供选模板）
- `POST /api/sync/scan` → ScanResult
- `POST /api/sync/execute` body: `{item_ids: list[str], items: list[DiffItem] | null, stop_on_error: bool}`  
  推荐：execute 携带完整勾选 `items`（含 sql），避免服务端缓存扫描态
- `GET /api/history`

绑定：`uvicorn` host=`127.0.0.1` port=`8787`。

- [ ] **Step 1: 写 API 测试（httpx + TestClient）** — 创建实例后 GET 不见明文，YAML 为密文

- [ ] **Step 2–4: 实现路由与 main**

```python
# main.py 关键
app = FastAPI(title="schema-sync")
app.include_router(instances.router, prefix="/api")
...
# 若 frontend/dist 存在则挂载 StaticFiles，SPA fallback
```

- [ ] **Step 5: Commit**

```bash
git add backend/app/main.py backend/app/api backend/tests/test_api_instances.py
git commit -m "$(cat <<'EOF'
feat:暴露实例/表组/扫描执行的 HTTP API

保证密码不回传明文，并固定本机监听入口。
EOF
)"
```

---

### Task 8: 前端四页

**Files:**
- Create: `frontend/package.json`, `vite.config.ts`, `index.html`, `src/*` 如上结构

**Interfaces:**
- Vite dev proxy：`/api` → `http://127.0.0.1:8787`
- 生产构建输出到 `frontend/dist`，由 FastAPI 托管

**页面行为（必须）：**
1. InstancesView：表单增改删、测连通；密码输入框；编辑时密码留空表示不改
2. TableGroupsView：编辑 pattern、tables（逗号或标签）、instance_ids 多选
3. SyncWorkbenchView：选 group → 选 template instance/db → 扫描 → 按库分组 checkbox → 全选安全项 → 确认执行
4. HistoryView：列表 + 展开明细

视觉：功能清晰即可；CSS 变量统一；非营销落地页，无需重型动效。

- [ ] **Step 1: scaffold Vite Vue-TS**

Run: `cd frontend && npm create vite@latest . -- --template vue-ts`（若目录非空则手动建文件）  
安装依赖后配置 proxy。

- [ ] **Step 2: 实现 `api.ts` 与四个 View + 简单导航**

- [ ] **Step 3: 手动烟测**

Run backend: `cd backend && uvicorn app.main:app --host 127.0.0.1 --port 8787`  
Run frontend: `cd frontend && npm run dev`  
Expected: 能打开四页；无真实 MySQL 时 ping 失败信息可读

- [ ] **Step 4: Commit**

```bash
git add frontend
git commit -m "$(cat <<'EOF'
feat:新增同步工作台等前端页面

覆盖连接、表组、差异勾选执行与历史查看主流程。
EOF
)"
```

---

### Task 9: README、示例配置与启动串联

**Files:**
- Create: `README.md`
- Modify: `config.example.yaml`（若需补全）
- Create: `scripts/dev.sh`（可选）

**README 必须包含：**
- 安装：backend venv + frontend npm
- 启动：两个命令或一键脚本
- 说明 `.schema-sync.key` / `config.yaml` 勿提交；换机需同时拷贝
- 简要使用步骤对应验收标准

- [ ] **Step 1: 写 README 与 `config.example.yaml`**
- [ ] **Step 2: 全量单测**

Run: `cd backend && pytest -v`  
Expected: 全部 PASS

- [ ] **Step 3: Commit**

```bash
git add README.md config.example.yaml scripts/dev.sh
git commit -m "$(cat <<'EOF'
docs:补充 schema-sync 启动与配置说明

说明密文密钥用法与本地 Web 启动方式，便于验收。
EOF
)"
```

---

## Spec Coverage Checklist（自检）

| Spec 要求 | 任务 |
|-----------|------|
| 多实例 MySQL | Task 5–7 |
| 库名 glob（order/product） | Task 3, 6 |
| 仅指定表 | Task 2 表组, Task 6 扫描 |
| 模板表为真理 | Task 4, 6 |
| 预览+勾选执行 | Task 6–8 |
| YAML+页面双写 | Task 2, 7–8 |
| 密码密文 Fernet | Task 1–2, 7 |
| 风险默认不勾选 | Task 4 |
| 按库串行 / 遇错策略 | Task 6 |
| 执行历史 | Task 6–8 |
| 本机监听 | Task 7 |
| 验收 1–5 | Task 7–9 烟测 + 单测 |

无 TBD/TODO 占位；类型名以 Task 1–4 的 Interfaces 为准。
