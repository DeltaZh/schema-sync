# schema-sync

多 MySQL 实例表结构对齐工具：按模板库对比差异、预览 SQL、勾选后执行。默认仅本机监听。

## 环境要求

- Python 3.11+
- Node.js 18+（前端开发或构建）
- 可访问的 MySQL 实例

## 安装

### 后端

```bash
cd backend
python3 -m venv .venv
source .venv/bin/activate   # Windows: .venv\Scripts\activate
pip install -e ".[dev]"
```

### 前端（开发模式或构建静态资源）

```bash
cd frontend
npm install
```

## 密钥与配置（重要）

首次启动时，**仅**会自动在项目根目录生成：

| 文件 | 说明 |
|------|------|
| `.schema-sync.key` | Fernet 主密钥，用于加密连接密码（自动创建） |

`config.yaml` **不会**自动生成。可参考示例复制后自行维护：

```bash
cp config.example.yaml config.yaml
```

| 文件 | 说明 |
|------|------|
| `config.yaml` | 实例与表组配置（密码落盘为 `enc:v1:` 密文） |

若 YAML 中仍有明文密码，加载时会自动加密并原子回写。

**请勿将上述文件提交到 Git**（已在 `.gitignore`）。换机或迁移时须**同时拷贝** `.schema-sync.key` 与 `config.yaml`，否则已加密密码无法解密。

密码请在 Web 页面录入，保存后自动加密；API 不回传明文。

## 安全模型

1. **仅本机环回** — uvicorn 必须绑定 `127.0.0.1`；中间件只接受 `127.0.0.1` / `::1` / `localhost`，**不信任** `X-Forwarded-For`。
2. **会话层加密** — 除 `POST /api/session/handshake` 外，所有 `/api/*` 请求/响应 JSON 经 ECDH P-256 + HKDF-SHA256 + AES-256-GCM 加密封装（即使 localhost 也不走明文业务体）。
3. **不信任客户端 SQL** — 扫描结果缓存在服务端（`scan_id`，TTL 2 小时）；执行只接受 `scan_id` + `item_ids`，由服务端查找 DiffItem/SQL。

握手（明文，仅本机）：

```json
POST /api/session/handshake
{ "client_public": "<SPKI base64 P-256>" }
→ { "session_id": "...", "server_public": "<SPKI base64>" }
```

后续请求头：`X-Schema-Sync-Session: <session_id>`  
业务体信封：`{"v":1,"nonce":"<b64 12>","ciphertext":"<b64>"}`。

## 启动

### 方式一：开发（前后端分离，推荐调试）

终端 1 — 后端（**必须** `--host 127.0.0.1`）：

```bash
cd backend && source .venv/bin/activate
uvicorn app.main:app --host 127.0.0.1 --port 8787
```

终端 2 — 前端（Vite 代理 `/api` → 8787）：

```bash
cd frontend && npm run dev
```

浏览器打开 Vite 提示的地址（默认 `http://127.0.0.1:5173`）。

### 方式二：一键脚本

```bash
chmod +x scripts/dev.sh
./scripts/dev.sh
```

### 方式三：单进程（构建后由 FastAPI 托管静态页）

```bash
cd frontend && npm run build
cd ../backend && source .venv/bin/activate
uvicorn app.main:app --host 127.0.0.1 --port 8787
```

访问 `http://127.0.0.1:8787`。

## 使用步骤

1. **连接管理** — 添加 2 个及以上 MySQL 实例，填写密码并测试连通；保存后 YAML 中密码为密文。
2. **表组管理** — 配置库名 glob（如 `order_*_*`、`product_*`）与要对齐的表列表，关联参与扫描的实例。
3. **同步工作台** — 选择表组、模板实例与模板库 → 扫描 → 按库查看差异 → 勾选变更（默认仅安全项）→ 预览 SQL → 二次确认 → 执行（可选遇错即停）。
4. **执行历史** — 查看最近任务、成功/失败明细与 SQL 快照。

## 验收对照

| # | 要点 |
|---|------|
| 1 | 配置多实例，YAML 密码为密文 |
| 2 | 按 glob 发现库，仅对比表组内指定表 |
| 3 | 缺列/缺索引时扫描列出正确 SQL |
| 4 | 勾选子集执行后，目标在所选变更上与模板一致 |
| 5 | 单条 SQL 失败时日志清晰，继续/停止符合选项 |

## 测试

```bash
cd backend && pytest -v
```
