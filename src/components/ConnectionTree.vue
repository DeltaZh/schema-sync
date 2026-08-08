<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  deleteConnection,
  listConnections,
  listDatabases,
  listTables,
  newId,
  pingConnection,
  upsertConnection,
} from "../lib/tauri";
import type { ConnectionConfig, TableSelection, TableSummary } from "../types";

const emit = defineEmits<{
  selectTable: [selection: TableSelection];
  connectionsChanged: [connections: ConnectionConfig[]];
}>();

interface DbNode {
  name: string;
  expanded: boolean;
  loading: boolean;
  tables: TableSummary[] | null;
  error: string;
}

interface ConnNode {
  conn: ConnectionConfig;
  expanded: boolean;
  loading: boolean;
  databases: DbNode[] | null;
  error: string;
}

const nodes = ref<ConnNode[]>([]);
const status = ref("");
const error = ref("");
const selectedKey = ref("");

const dialogOpen = ref(false);
const editingId = ref<string | null>(null);
const form = ref({
  name: "",
  host: "127.0.0.1",
  port: 3306,
  user: "root",
  password: "",
  enabled: true,
  remark: "",
});

function selectionKey(s: TableSelection): string {
  return `${s.connectionId}/${s.database}/${s.table}`;
}

async function reloadConnections() {
  error.value = "";
  status.value = "正在加载连接…";
  try {
    const list = await listConnections();
    const prev = new Map(nodes.value.map((n) => [n.conn.id, n]));
    nodes.value = list.map((conn) => {
      const old = prev.get(conn.id);
      return {
        conn,
        expanded: old?.expanded ?? false,
        loading: false,
        databases: old?.databases ?? null,
        error: "",
      };
    });
    emit("connectionsChanged", list);
    status.value = `已加载 ${list.length} 个连接`;
  } catch (e) {
    error.value = String(e);
    status.value = "";
  }
}

async function toggleConnection(node: ConnNode) {
  node.expanded = !node.expanded;
  if (!node.expanded || node.databases) return;
  node.loading = true;
  node.error = "";
  try {
    const dbs = await listDatabases(node.conn.id);
    node.databases = dbs.map((name) => ({
      name,
      expanded: false,
      loading: false,
      tables: null,
      error: "",
    }));
  } catch (e) {
    node.error = String(e);
    node.expanded = false;
  } finally {
    node.loading = false;
  }
}

async function toggleDatabase(node: ConnNode, db: DbNode) {
  db.expanded = !db.expanded;
  if (!db.expanded || db.tables) return;
  db.loading = true;
  db.error = "";
  try {
    db.tables = await listTables(node.conn.id, db.name);
  } catch (e) {
    db.error = String(e);
    db.expanded = false;
  } finally {
    db.loading = false;
  }
}

function selectTable(node: ConnNode, db: DbNode, table: TableSummary) {
  const selection: TableSelection = {
    connectionId: node.conn.id,
    connectionName: node.conn.name,
    database: db.name,
    table: table.name,
    tableComment: table.comment,
  };
  selectedKey.value = selectionKey(selection);
  emit("selectTable", selection);
}

function openCreate() {
  editingId.value = null;
  form.value = {
    name: "",
    host: "127.0.0.1",
    port: 3306,
    user: "root",
    password: "",
    enabled: true,
    remark: "",
  };
  dialogOpen.value = true;
}

function openEdit(node: ConnNode) {
  editingId.value = node.conn.id;
  form.value = {
    name: node.conn.name,
    host: node.conn.host,
    port: node.conn.port,
    user: node.conn.user,
    password: "",
    enabled: node.conn.enabled,
    remark: node.conn.remark,
  };
  dialogOpen.value = true;
}

async function saveConnection() {
  error.value = "";
  const id = editingId.value ?? newId("conn");
  const payload: ConnectionConfig = {
    id,
    name: form.value.name.trim(),
    host: form.value.host.trim(),
    port: Number(form.value.port) || 3306,
    user: form.value.user.trim(),
    password: form.value.password,
    enabled: form.value.enabled,
    remark: form.value.remark.trim(),
  };
  if (!payload.name || !payload.host || !payload.user) {
    error.value = "请填写连接名称、主机与用户名";
    return;
  }
  try {
    await upsertConnection(payload);
    dialogOpen.value = false;
    await reloadConnections();
    status.value = editingId.value ? "连接已更新" : "连接已创建";
  } catch (e) {
    error.value = String(e);
  }
}

async function removeConnection(node: ConnNode) {
  if (!confirm(`确定删除连接「${node.conn.name}」？`)) return;
  error.value = "";
  try {
    await deleteConnection(node.conn.id);
    await reloadConnections();
    status.value = "连接已删除";
  } catch (e) {
    error.value = String(e);
  }
}

async function ping(node: ConnNode) {
  error.value = "";
  status.value = `正在测试「${node.conn.name}」…`;
  try {
    await pingConnection(node.conn.id);
    status.value = `「${node.conn.name}」连通正常`;
  } catch (e) {
    error.value = String(e);
    status.value = "";
  }
}

async function refreshNode(node: ConnNode) {
  node.databases = null;
  if (node.expanded) {
    node.expanded = false;
    await toggleConnection(node);
  }
}

onMounted(() => {
  void reloadConnections();
});

defineExpose({ reloadConnections });
</script>

<template>
  <div class="panel">
    <div class="toolbar">
      <button type="button" class="btn primary" @click="openCreate">新建</button>
      <button type="button" class="btn" @click="reloadConnections">刷新</button>
      <span class="spacer" />
    </div>

    <div v-if="error" class="toolbar error-text">{{ error }}</div>

    <div class="tree">
      <div v-if="nodes.length === 0" class="muted" style="padding: 10px">
        暂无连接，请点击「新建」添加
      </div>

      <div v-for="node in nodes" :key="node.conn.id" class="tree-node">
        <div class="tree-row" @click="toggleConnection(node)">
          <span class="tree-toggle">{{
            node.loading ? "…" : node.expanded ? "▼" : "▶"
          }}</span>
          <span class="tree-label" :title="node.conn.remark || node.conn.host">
            {{ node.conn.name }}
            <span class="muted">({{ node.conn.host }}:{{ node.conn.port }})</span>
          </span>
        </div>
        <div class="toolbar" style="padding-left: 22px; background: transparent; border: none">
          <button type="button" class="btn ghost" @click.stop="ping(node)">
            测连通
          </button>
          <button type="button" class="btn ghost" @click.stop="refreshNode(node)">
            重载库
          </button>
          <button type="button" class="btn ghost" @click.stop="openEdit(node)">
            编辑
          </button>
          <button
            type="button"
            class="btn ghost danger"
            @click.stop="removeConnection(node)"
          >
            删除
          </button>
        </div>
        <div v-if="node.error" class="error-text" style="padding: 0 10px 6px 26px">
          {{ node.error }}
        </div>

        <div v-if="node.expanded && node.databases" class="tree-children">
          <div v-for="db in node.databases" :key="db.name" class="tree-node">
            <div class="tree-row" @click="toggleDatabase(node, db)">
              <span class="tree-toggle">{{
                db.loading ? "…" : db.expanded ? "▼" : "▶"
              }}</span>
              <span class="tree-label">{{ db.name }}</span>
            </div>
            <div v-if="db.error" class="error-text" style="padding: 0 10px 6px 26px">
              {{ db.error }}
            </div>
            <div v-if="db.expanded && db.tables" class="tree-children">
              <div
                v-for="table in db.tables"
                :key="table.name"
                class="tree-row"
                :class="{
                  active:
                    selectedKey ===
                    `${node.conn.id}/${db.name}/${table.name}`,
                }"
                @click="selectTable(node, db, table)"
              >
                <span class="tree-toggle">·</span>
                <span class="tree-label">
                  {{ table.name }}
                  <span v-if="table.comment" class="tree-comment">{{
                    table.comment
                  }}</span>
                </span>
              </div>
              <div
                v-if="db.tables.length === 0"
                class="muted"
                style="padding: 4px 10px"
              >
                （无表）
              </div>
            </div>
          </div>
          <div
            v-if="node.databases.length === 0"
            class="muted"
            style="padding: 4px 10px"
          >
            （无库）
          </div>
        </div>
      </div>
    </div>

    <div class="status-bar">{{ status || "就绪" }}</div>

    <div
      v-if="dialogOpen"
      class="dialog-backdrop"
      @click.self="dialogOpen = false"
    >
      <div class="dialog" role="dialog" aria-modal="true">
        <h3>{{ editingId ? "编辑连接" : "新建连接" }}</h3>
        <div class="form-grid">
          <label for="conn-name">显示名称</label>
          <input id="conn-name" v-model="form.name" class="field" />

          <label for="conn-host">主机</label>
          <input id="conn-host" v-model="form.host" class="field" />

          <label for="conn-port">端口</label>
          <input
            id="conn-port"
            v-model.number="form.port"
            class="field"
            type="number"
            min="1"
            max="65535"
          />

          <label for="conn-user">用户名</label>
          <input id="conn-user" v-model="form.user" class="field" />

          <label for="conn-password">密码</label>
          <input
            id="conn-password"
            v-model="form.password"
            class="field"
            type="password"
            autocomplete="new-password"
            :placeholder="editingId ? '留空表示不修改' : '请输入密码'"
          />

          <label for="conn-remark">备注</label>
          <input id="conn-remark" v-model="form.remark" class="field" />

          <label class="full">
            <input v-model="form.enabled" type="checkbox" />
            启用此连接
          </label>
        </div>
        <div class="dialog-actions">
          <button type="button" class="btn" @click="dialogOpen = false">
            取消
          </button>
          <button type="button" class="btn primary" @click="saveConnection">
            保存
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
