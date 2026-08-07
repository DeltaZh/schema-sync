#!/usr/bin/env bash
# 本地开发：同时启动后端 (8787) 与前端 Vite (5173)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cleanup() {
  if [[ -n "${BACKEND_PID:-}" ]]; then kill "$BACKEND_PID" 2>/dev/null || true; fi
  if [[ -n "${FRONTEND_PID:-}" ]]; then kill "$FRONTEND_PID" 2>/dev/null || true; fi
}
trap cleanup EXIT INT TERM

cd "$ROOT/backend"
if [[ ! -d .venv ]]; then
  echo "未找到 backend/.venv，请先执行: cd backend && python3 -m venv .venv && pip install -e \".[dev]\""
  exit 1
fi
# shellcheck disable=SC1091
source .venv/bin/activate
uvicorn app.main:app --host 127.0.0.1 --port 8787 &
BACKEND_PID=$!

cd "$ROOT/frontend"
if [[ ! -d node_modules ]]; then
  echo "未找到 frontend/node_modules，请先执行: cd frontend && npm install"
  exit 1
fi
npm run dev &
FRONTEND_PID=$!

echo "后端: http://127.0.0.1:8787  前端: http://127.0.0.1:5173（Vite 代理 /api）"
echo "按 Ctrl+C 停止"

wait
