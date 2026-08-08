# Schema Sync Desktop (Tauri) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 推倒旧 Web 实现，交付单机 macOS Tauri 应用：Navicat 式浏览、可组合分库规则、基准结构同步（含注释）与 DDL 投放。

**Architecture:** Vue 3 UI 经 Tauri `invoke` 调用本机 Rust；Rust 负责 MySQL、规则展开、Diff/SQL、DDL 校验、扫描缓存与执行；配置落在 Application Support，无独立 HTTP 服务端。

**Tech Stack:** Tauri 2、Rust 2021、Vue 3 + TypeScript + Vite、`mysql_async` 或 `sqlx`（MySQL）、`serde`/`serde_json`、`aes-gcm`+本地密钥或 `keyring`（密码密文）、`tokio`、Rust 单测 + `cargo test`。

## Global Constraints

- 单机 App，不对外监听 HTTP；无浏览器版 / 独立服务端
- 密码密文落盘；UI 不回显明文
- 执行只认 Rust 侧缓存的 `scan_id` + `item_ids`（或不接受客户端 SQL）
- 模式 2 拒绝 `DROP`/`DELETE`/`UPDATE`/`TRUNCATE`
- 同步预览须展示表/字段注释
- 命名规则：逻辑名固定前缀，部件可排序组合 `{tenant, year, shard}`
- 仓库路径：`/Users/delta/cursorProject/schema-sync`；推倒 `backend/`、`frontend/` 旧实现
- Git commit：约定式前缀 + 中文说明
- 首版打包目标：macOS `.app`

---

## File Structure（目标）

```text
schema-sync/
├── README.md
├── docs/superpowers/specs/...          # 已有设计，保留
├── docs/superpowers/plans/...          # 本计划
├── package.json                        # 根：前端 scripts + tauri
├── src/                                # Vue 前端
│   ├── main.ts
│   ├── App.vue
│   ├── styles.css
│   ├── lib/tauri.ts                    # invoke 封装
│   ├── types.ts
│   └── components/
│       ├── ConnectionTree.vue
│       ├── StructurePane.vue
│       ├── BaselineSyncPane.vue
│       ├── DdlBroadcastPane.vue
│       ├── RulesPane.vue
│       └── HistoryPane.vue
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── capabilities/
│   └── src/
│       ├── lib.rs / main.rs
│       ├── error.rs
│       ├── paths.rs
│       ├── crypto.rs
│       ├── config.rs
│       ├── models.rs
│       ├── naming.rs                   # 规则展开
│       ├── mysql.rs
│       ├── schema.rs                   # 抽取
│       ├── diff.rs
│       ├── sql_gen.rs
│       ├── ddl_guard.rs                # 模式2白名单
│       ├── scan_cache.rs
│       ├── history.rs
│       ├── exec.rs
│       └── commands/                   # Tauri commands
│           ├── mod.rs
│           ├── connections.rs
│           ├── browse.rs
│           ├── rules.rs
│           ├── sync.rs
│           └── ddl.rs
└── src-tauri/tests/ 或各模块 #[cfg(test)]
```

删除：`backend/`、旧根级 `frontend/`（被 `src/` + `src-tauri/` 取代）、旧 `scripts/dev.sh`（改为 `npm run tauri dev`）。

---

### Task 1: 清理旧代码并脚手架 Tauri 2 + Vue 3

**Files:**
- Delete: `backend/`, `frontend/`, `scripts/dev.sh`, `config.example.yaml`（稍后用新示例替换）
- Create: Tauri+Vite+Vue 工程文件（`package.json`, `src/*`, `src-tauri/*`）
- Modify: `.gitignore`（保留密钥/配置忽略；加入 `src-tauri/target`）
- Keep: `docs/superpowers/**`

**Interfaces:**
- Produces: `npm run tauri dev` 可打开空窗口；`cargo check` 通过

- [ ] **Step 1: 备份确认后删除旧实现**

```bash
cd /Users/delta/cursorProject/schema-sync
# 确认 docs 保留
git rm -r backend frontend scripts/dev.sh config.example.yaml 2>/dev/null || rm -rf backend frontend scripts
```

- [ ] **Step 2: 脚手架**

优先：

```bash
npm create tauri-app@latest . -- --template vue-ts
```

若目录非空失败：手动创建 `package.json` / Vite Vue-TS / `src-tauri` 最小 Cargo 工程，`tauri.conf.json` productName=`schema-sync`。

- [ ] **Step 3: 验证**

```bash
npm install
cd src-tauri && cargo check
# 可选：npm run tauri dev（需 GUI）
```

Expected: `cargo check` OK；窗口能起即可。

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "$(cat <<'EOF'
chore:推倒旧 Web 实现并初始化 Tauri Vue 工程

按桌面版设计改为单机应用脚手架，移除 FastAPI/分端前端。
EOF
)"
```

---

### Task 2: 路径、密码加密与配置读写（Rust）

**Files:**
- Create: `src-tauri/src/paths.rs`, `crypto.rs`, `models.rs`, `config.rs`
- Test: `src-tauri/src/crypto.rs` 与 `config.rs` 内 `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `app_data_dir() -> PathBuf` — 开发期可用 `SCHEMA_SYNC_DATA` 覆盖；生产用 Tauri `app.path().app_data_dir()`
  - `struct ConnectionConfig { id, name, host, port, user, password, enabled, remark }`
  - `struct NamingRule { id, logical_name, parts_order: Vec<PartKind>, tenants, years, shards, connection_ids }`
  - `enum PartKind { Tenant, Year, Shard }`
  - `struct AppConfig { connections, rules }`
  - `PasswordCrypto::load_or_create(key_path) / encrypt / decrypt`（`enc:v1:` 前缀，AES-GCM 或等价）
  - `ConfigStore::load/save`；保存时加密明文密码；`public_connections()` 掩码

- [ ] **Step 1: 写失败测试（crypto roundtrip）**

```rust
#[test]
fn encrypt_decrypt_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let crypto = PasswordCrypto::load_or_create(&dir.path().join("key")).unwrap();
    let t = crypto.encrypt("s3cret").unwrap();
    assert!(t.starts_with("enc:v1:"));
    assert_eq!(crypto.decrypt(&t).unwrap(), "s3cret");
}
```

- [ ] **Step 2: `cargo test` 确认失败 → Step 3 实现 → Step 4 通过**

- [ ] **Step 5: Commit** `feat:实现本机配置与密码密文存储`

---

### Task 3: 命名规则展开（纯函数）

**Files:**
- Create: `src-tauri/src/naming.rs`
- Test: 同文件 `#[cfg(test)]`

**Interfaces:**
- Produces:
  - `fn expand_database_names(rule: &NamingRule) -> Vec<String>`
  - 逻辑名固定前缀；`parts_order` 空 → 仅 `logical_name`
  - 笛卡尔积；部件间 `_` 连接
  - `years` 支持显式列表；`shards` 支持 `1..=n` 由配置侧先展开为 `Vec<String>`

- [ ] **Step 1: 失败测试**

```rust
#[test]
fn only_logical_name() {
    let r = NamingRule { logical_name: "order".into(), parts_order: vec![], tenants: vec![], years: vec![], shards: vec![], ..Default::default() };
    assert_eq!(expand_database_names(&r), vec!["order"]);
}

#[test]
fn tenant_year_shard_order() {
    // parts: Tenant, Year, Shard → order_demo_2025_1
    ...
}

#[test]
fn shard_only() {
    // order_1, order_2
    ...
}
```

- [ ] **Step 2–4: TDD 实现并通过**

- [ ] **Step 5: Commit** `feat:实现可组合分库命名规则展开`

---

### Task 4: MySQL 连接、浏览与 Schema 抽取

**Files:**
- Create: `src-tauri/src/mysql.rs`, `schema.rs`
- Test: 解析函数单测（假行数据）；真连库测试标 `#[ignore]`

**Interfaces:**
- Produces:
  - `async fn ping(conn_cfg, password_plain) -> Result<()>`
  - `async fn list_databases(...) -> Result<Vec<String>>`
  - `async fn list_tables(..., database) -> Result<Vec<TableSummary>>` — 含 `name`, `comment`
  - `async fn fetch_table_schema(..., database, table) -> Result<Option<TableSchema>>`
  - `TableSchema { name, comment, columns: Vec<ColumnDef>, indexes: Vec<IndexDef>, create_sql }`
  - `ColumnDef { name, col_type, nullable, default, comment, extra }`

- [ ] **Step 1–4: TDD 解析 + 实现 sqlx/mysql_async 访问**

- [ ] **Step 5: Commit** `feat:实现 MySQL 浏览与表结构抽取`

---

### Task 5: Diff / SQL 生成（含表与字段注释）

**Files:**
- Create: `src-tauri/src/diff.rs`, `sql_gen.rs`
- Test: 缺列、注释变更、表注释、危险默认不选

**Interfaces:**
- Produces:
  - `enum Risk { Safe, Caution, Dangerous }`
  - `struct DiffItem { id, kind, risk, connection_id, database, table, title, detail, sql, selected_default }`
  - `detail` 须含人类可读注释信息（如「字段注释: xxx」）
  - `fn diff_table(template, target, ctx) -> Vec<DiffItem>`
  - 列注释不同 → `modify_column`（caution）
  - 表注释不同 → `alter_table_comment`（caution）

- [ ] **Step 1–4: TDD**

- [ ] **Step 5: Commit** `feat:实现含注释的表结构对比与 SQL 生成`

---

### Task 6: 扫描缓存、执行器、历史

**Files:**
- Create: `src-tauri/src/scan_cache.rs`, `exec.rs`, `history.rs`

**Interfaces:**
- Produces:
  - `ScanCache::put(scan_id, items) / get(scan_id)`
  - `async fn execute_by_ids(cache, scan_id, item_ids, stop_on_error) -> Vec<ExecResult>` — **禁止**接受客户端 sql
  - `HistoryStore` JSON 文件追加

- [ ] **Step 1: 测试 execute 拒绝未知 id；连接失败记入结果**

- [ ] **Step 2–4: 实现**

- [ ] **Step 5: Commit** `feat:实现扫描缓存与按 id 安全执行`

---

### Task 7: DDL 白名单校验（模式 2）

**Files:**
- Create: `src-tauri/src/ddl_guard.rs`

**Interfaces:**
- Produces:
  - `fn split_statements(sql: &str) -> Vec<String>`
  - `fn validate_structure_ddl(sql: &str) -> Result<Vec<String>, String>`
  - 允许：`ALTER TABLE`（ADD/MODIFY COLUMN、ADD INDEX/UNIQUE/KEY）、`CREATE INDEX`、`CREATE UNIQUE INDEX`
  - 拒绝：含 `DROP`/`DELETE`/`UPDATE`/`TRUNCATE`/`INSERT`/`REPLACE` 等（大小写不敏感，按语句前缀/关键词扫描）

- [ ] **Step 1: 测试合法 ALTER 通过；DELETE/DROP 失败**

- [ ] **Step 2–4: 实现**

- [ ] **Step 5: Commit** `feat:实现 DDL 投放语句白名单校验`

---

### Task 8: Tauri Commands API

**Files:**
- Create: `src-tauri/src/commands/*.rs`；在 `lib.rs` 注册
- Modify: `capabilities` 按需

**Interfaces（invoke 名）：**
- `list_connections` / `upsert_connection` / `delete_connection` / `ping_connection`
- `list_databases` / `list_tables` / `get_table_structure`
- `list_rules` / `save_rules`
- `expand_rule_targets` — 返回 `{connection_id, database}[]`（可探测库是否存在）
- `baseline_scan` — 入参：baseline conn/db、tables、rule_id、可选目标剔选 → 返回 `{scan_id, items}`（items 含 detail/注释，**可含 sql 供预览**；执行仍只认 id）
- `baseline_execute` — `{scan_id, item_ids, stop_on_error}`
- `ddl_preview` / `ddl_execute` — preview 校验+目标列表；execute 用服务端保存的 preview token 或再次校验后执行
- `list_history`

State：`AppState { config, crypto, cache, history }` 用 `Mutex`/`RwLock`。

- [ ] **Step 1: 为 `expand_rule_targets` 与 `ddl_preview` 拒绝危险 SQL 写 command 级测试或集成测试**

- [ ] **Step 2–4: 实现并 `cargo test`**

- [ ] **Step 5: Commit** `feat:暴露桌面端 Tauri 命令接口`

---

### Task 9: Vue 壳 — 连接树 + 结构页 + 规则页

**Files:**
- Create/Modify: `src/App.vue`, `ConnectionTree.vue`, `StructurePane.vue`, `RulesPane.vue`, `lib/tauri.ts`, `types.ts`, `styles.css`

**行为：**
- 左树：连接 → 库 → 表；右「结构」显示列（含注释列）、索引
- 连接 CRUD 对话框；密码编辑留空=不改
- 规则页：逻辑名、部件多选排序（tenant/year/shard）、租户/年/分片、绑定连接

- [ ] **Step 1: 实现 UI 并 `npm run build`**
- [ ] **Step 2: `npm run tauri dev` 手工点通浏览（需本机 MySQL 时用真实库；无库时至少连接表单与规则保存）**
- [ ] **Step 3: Commit** `feat:新增连接树结构浏览与规则配置界面`

---

### Task 10: 基准同步 + DDL 投放 + 历史 UI

**Files:**
- Create: `BaselineSyncPane.vue`, `DdlBroadcastPane.vue`, `HistoryPane.vue`

**行为：**
- 模式 1：选基准库/勾表（显示表注释）→ 选规则 → 扫差异（展示 detail 注释）→ 勾选 → 确认 → `baseline_execute`
- 模式 2：粘贴 SQL → preview → 确认执行；危险语句错误提示
- 历史列表可展开

- [ ] **Step 1–2: 实现 + build**
- [ ] **Step 3: Commit** `feat:完成基准同步与 DDL 投放界面`

---

### Task 11: README、示例配置与 macOS 打包

**Files:**
- Create: `README.md`（重写）、`config.example.json`（或首启空配置说明）
- Modify: `src-tauri/tauri.conf.json` 图标/bundle identifier

- [ ] **Step 1: 文档写明：开发 `npm run tauri dev`；打包 `npm run tauri build`；数据目录；安全模型**
- [ ] **Step 2: 本机执行 `npm run tauri build`，确认生成 `.app`**
- [ ] **Step 3: `cargo test` 全绿**
- [ ] **Step 4: Commit** `docs:补充 Tauri 桌面版启动与打包说明`

---

## Spec Coverage Checklist

| Spec 要求 | 任务 |
|-----------|------|
| 推倒 Web、Tauri 单机 | Task 1 |
| 密码密文 / App 数据目录 | Task 2 |
| 可组合规则 | Task 3, 9 |
| Navicat 浏览含注释 | Task 4, 9 |
| 模式 1 基准+勾表+注释 diff | Task 5–6, 8, 10 |
| 执行只认缓存 id | Task 6, 8 |
| 模式 2 DDL 白名单 | Task 7, 8, 10 |
| 历史 | Task 6, 10 |
| Mac `.app` | Task 11 |
| 无独立 HTTP 服务端 | Task 1, 8 |

旧计划 `2026-08-07-schema-sync.md` 作废，勿再执行。
