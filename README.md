# Schema Sync

多 MySQL 实例**表结构**同步桌面应用（Tauri 2 + Vue 3）。

单机进程，无独立 HTTP 服务；界面通过 Tauri `invoke` 调用本机 Rust，不对外监听端口，也不把连接信息上传到任何远端。

**作者**：[DeltaZh](https://github.com/DeltaZh)  
**协议**：[MIT License](./LICENSE)（开源免费，可商用 / 修改 / 二次分发，保留版权与许可声明即可）

---

## 能做什么

| 能力 | 说明 |
|---|---|
| 连接与浏览 | 多实例连接树：连接 → 库 → 表；查看字段、索引与注释 |
| 可见库白名单 | 每个连接可自选要展示的库，避免一次拉全库 |
| 命名规则 | 模板如 `order_{年份}_{租户}` + 取值列表，展开目标库并绑定连接 |
| 模式 1：基准对齐 | 以某库为基准扫描差异，勾选生成/执行同步 SQL |
| 模式 2：DDL 投放 | 将结构类（及受控的高风险）SQL 预览后投放到多个目标库 |
| 历史 | 本机记录执行结果摘要 |

高风险语句（如 `DROP` / `DELETE` / `UPDATE` / `INSERT` 等）会标出风险，并要求二次确认（输入「确认执行」）；仍禁止 `DROP DATABASE`。

---

## 隐私与本机数据（开源不影响你的环境）

公开本仓库**不会**读取、上传或改动你本机已安装应用里的配置。

| 内容 | 位置 | 是否进仓库 |
|---|---|---|
| 连接密码密文、规则、密钥 | 系统应用数据目录（见下表） | **否**（已 gitignore） |
| 执行历史 | 同上 `history.jsonl` | **否** |
| 示例配置 | 仓库内 `config.example.json`（空密码占位） | 是 |

| 场景 | 数据目录 |
|---|---|
| macOS 生产（Tauri） | `~/Library/Application Support/com.schemasync.desktop/` |
| 回落默认 | `~/Library/Application Support/schema-sync/` |
| 开发覆盖 | 环境变量 `$SCHEMA_SYNC_DATA`（如项目下 `.schema-sync-data/`） |

目录内：`config.json`（密码为 `enc:v1:` 密文）、`.schema-sync.key`（本地密钥）、`history.jsonl`。  
克隆或 fork 本仓库拿不到你的库密码；别人跑起来也只会写到**他们自己**机器的数据目录。

---

## 环境要求

- Node.js 18+（建议 LTS）
- Rust（与 [Tauri 2 前置条件](https://v2.tauri.app/start/prerequisites/) 一致）
- 可访问的 MySQL 实例（开发联调时）

---

## 开发

```bash
npm install
npm run tauri dev
```

可选：隔离测试配置，避免写进正式数据目录：

```bash
export SCHEMA_SYNC_DATA="$PWD/.schema-sync-data"
npm run tauri dev
```

---

## 打包

```bash
npm run tauri build
```

常见产物：

- macOS：`src-tauri/target/release/bundle/macos/schema-sync.app`
- 安装包：`src-tauri/target/release/bundle/dmg/`（若目标开启）
- Windows：`src-tauri/target/release/bundle/msi/` 或 `nsis/`（在对应平台构建）

### macOS 未签名本地运行

未配置 Apple Developer 签名 / 公证时，首次打开可能被 Gatekeeper 拦截，可任选：

1. Finder 中对 `schema-sync.app` → 右键 →「打开」→ 确认  
2. 仅对本机构建产物清除隔离属性：

```bash
xattr -cr "src-tauri/target/release/bundle/macos/schema-sync.app"
open "src-tauri/target/release/bundle/macos/schema-sync.app"
```

正式对外分发需自行配置签名与公证。

---

## 安全模型（简要）

1. **无独立 HTTP 服务**：不对外监听；浏览器不能直连数据库。  
2. **密码密文落盘**：UI 展示掩码；密钥仅在本机数据目录。  
3. **执行只认缓存 id**：基准执行用 `scan_id` + `item_ids`；DDL 执行用预览下发的 `preview_id`，客户端无法篡改待执行 SQL。  
4. **二次确认**：预览后确认；高风险另需输入「确认执行」。  
5. **DDL 投放策略**：默认允许建表与常规结构变更；删库默认禁止。可在「设置」页调整各语句类型为常规 / 高风险 / 不允许。  
6. **勿提交本机数据**：切勿把真实 `config.json` / `.schema-sync.key` / `history.jsonl` 放进 git。

---

## 使用流程（概要）

### 连接与浏览

1. 新增连接（主机、端口、用户、密码）。  
2. 配置该连接的可见库（可选）。  
3. 左侧树展开库表，右侧查看结构。

### 命名规则

1. 填写展示名与模板（如 `order_{年份}_{租户}`）。  
2. 为占位符填写取值列表，绑定到若干连接。  
3. 用于模式 1 / 模式 2 的目标库展开。

### 模式 1：基准对齐

1. 选基准连接与库 → 勾选要同步的表。  
2. 选命名规则 → 展开目标库（可剔除）→ 扫描差异。  
3. 勾选执行项（危险项默认不勾）→ 确认 → 执行并写入历史。

### 模式 2：DDL 投放

1. 选规则（可剔库）→ 粘贴 SQL（`;` 分隔，支持 `CREATE TABLE` 等）。  
2. 预览：按「设置」中的策略校验；高风险会标出。  
3. 确认后按 `preview_id` 串行执行。

### 设置

在「设置」页配置 DDL 投放各语句类型的策略，并可恢复默认。

---

## 文档

- 桌面版设计：`docs/superpowers/specs/2026-08-08-schema-sync-desktop-design.md`
- 实现计划：`docs/superpowers/plans/2026-08-08-schema-sync-desktop.md`
- 专题规格：`docs/superpowers/specs/2026-08-08-*.md`（命名规则、扫描进度、高风险确认、Logo/筛选等）

---

## 技术栈

- 前端：Vue 3 + TypeScript + Vite  
- 桌面壳：Tauri 2  
- 核心逻辑：Rust（MySQL / Diff / DDL 校验 / 扫描与预览缓存）

---

## License

本项目采用 [MIT License](./LICENSE)。

允许商业使用、修改与二次分发；使用时请保留版权与许可声明（标注作者即可）。

Copyright (c) 2026 DeltaZh
