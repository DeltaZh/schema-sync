# Schema Sync

多 MySQL 实例结构同步桌面应用（Tauri 2 + Vue 3）。单机进程，无独立 HTTP 服务端；UI 仅通过 Tauri `invoke` 调用本机 Rust。

## 开发

```bash
npm install
npm run tauri dev
```

可选：用环境变量覆盖数据目录（便于隔离测试配置）：

```bash
export SCHEMA_SYNC_DATA="$PWD/.schema-sync-data"
npm run tauri dev
```

## 打包（macOS）

```bash
npm run tauri build
```

产物（本机架构）：

- 应用包：`src-tauri/target/release/bundle/macos/schema-sync.app`
- 可选安装包：`src-tauri/target/release/bundle/dmg/`（若目标开启）

### 未签名本地运行

未配置 Apple Developer 签名 / 公证时，构建仍可产出 `.app`，但首次打开可能被 Gatekeeper 拦截。本机可任选：

1. **右键打开**：Finder 中对 `schema-sync.app` → 右键 →「打开」→ 确认。
2. **清除隔离属性**（仅信任的本机构建产物）：

```bash
xattr -cr "src-tauri/target/release/bundle/macos/schema-sync.app"
open "src-tauri/target/release/bundle/macos/schema-sync.app"
```

正式分发需配置签名与公证（Tauri / Apple 文档），本仓库首版以本机可运行 `.app` 为准。

## 数据目录

| 场景 | 路径 |
|---|---|
| 生产（Tauri） | `~/Library/Application Support/com.schemasync.desktop/` |
| 回落默认 | `~/Library/Application Support/schema-sync/`（`dirs::data_dir`） |
| 开发覆盖 | `$SCHEMA_SYNC_DATA` |

目录内主要文件：

| 文件 | 说明 |
|---|---|
| `config.json` | 连接与命名规则（密码为密文） |
| `.schema-sync.key` | 本地密钥（权限应收紧，勿提交） |
| `history.jsonl` | 执行历史 |

首次启动若无配置，应用内创建连接与规则即可；也可参考仓库根目录 `config.example.json` 手工放入数据目录后，在应用内补全密码（保存时会加密）。

## 安全模型

1. **无独立 HTTP 服务**：不对外监听；无浏览器直连数据库。
2. **密码密文落盘**：连接密码以 `enc:v1:` 密文写入 `config.json`；密钥在应用数据目录；UI 不回显明文（展示掩码）。
3. **执行只认缓存 id**：
   - 模式 1：`baseline_execute` 仅接受 `scan_id` + `item_ids`，SQL 来自 Rust 侧扫描缓存。
   - 模式 2：`ddl_execute` 仅接受预览阶段下发的 `preview_id`，禁止客户端篡改待执行 SQL。
4. **二次确认**：执行前须预览并确认。
5. **模式 2 DDL 白名单**：仅允许结构类语句；拒绝 `DROP` / `DELETE` / `UPDATE` / `TRUNCATE` 等。

## 使用说明

### 连接与浏览

1. 在连接管理中新增实例（主机、端口、用户、密码）。
2. 左侧树：连接 → 库 → 表；右侧查看字段、索引与注释（类 Navicat 浏览）。

### 命名规则

规则由「逻辑名」+ 可排序部件（租户 / 年 / 分片）组成，笛卡尔展开为物理库名，并绑定到若干连接。用于模式 1 / 模式 2 的目标库集合。

### 模式 1：基准对齐

1. 选择基准连接与已存在库 → 勾选要同步的表（可见表注释）。
2. 选择命名规则 → 展开目标库（可剔除）→ 扫描差异（含字段/表注释）。
3. 勾选执行项（危险项默认不勾）→ 确认 → 按缓存 id 执行并写入历史。

### 模式 2：DDL 投放

1. 选择规则（可剔库）→ 粘贴结构 DDL（`;` 分隔）。
2. 预览：白名单校验通过后展示目标库与语句；危险语句会被拒绝。
3. 确认后按 `preview_id` 串行执行并记录结果。

### 历史

「历史」页签可查看本机执行记录与结果摘要。

## 文档

- 设计：`docs/superpowers/specs/2026-08-08-schema-sync-desktop-design.md`
- 计划：`docs/superpowers/plans/2026-08-08-schema-sync-desktop.md`

## 技术栈

- 前端：Vue 3 + TypeScript + Vite
- 桌面壳：Tauri 2
- 后端逻辑：Rust（MySQL / Diff / DDL 校验 / 缓存执行）
