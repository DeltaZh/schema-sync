<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  deleteConnection,
  listAllDatabases,
  listConnections,
  listDatabases,
  listTables,
  newId,
  pingConnection,
  setVisibleDatabases,
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

/** 选择可见库对话框 */
const pickerOpen = ref(false);
const pickerNode = ref<ConnNode | null>(null);
const pickerLoading = ref(false);
const pickerError = ref("");
const pickerAll = ref<string[]>([]);
const pickerSelected = ref<Set<string>>(new Set());

function selectionKey(s: TableSelection): string {
  return `${s.connectionId}/${s.database}/${s.table}`;
}

function hasVisibleDbs(conn: ConnectionConfig): boolean {
  return (conn.visible_databases?.length ?? 0) > 0;
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
        conn: {
          ...conn,
          visible_databases: conn.visible_databases ?? [],
        },
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

async function loadDatabasesForNode(node: ConnNode) {
  node.loading = true;
  node.error = "";
  try {
    if (!hasVisibleDbs(node.conn)) {
      node.databases = [];
      node.expanded = true;
      status.value = `「${node.conn.name}」尚未选择可见库，请点击「选择库」`;
      return;
    }
    const dbs = await listDatabases(node.conn.id);
    node.databases = dbs.map((name) => ({
      name,
      expanded: false,
      loading: false,
      tables: null,
      error: "",
    }));
    node.expanded = true;
    if (dbs.length === 0) {
      status.value = `「${node.conn.name}」已选库在服务器上均不存在，请重新「选择库」`;
    } else {
      status.value = `「${node.conn.name}」已加载 ${dbs.length} 个库`;
    }
  } catch (e) {
    // 展开失败时保持展开，便于看到错误（旧逻辑会收起，看起来像「没展开」）
    node.error = String(e);
    node.expanded = true;
    node.databases = node.databases ?? [];
    status.value = "";
  } finally {
    node.loading = false;
  }
}

async function toggleConnection(node: ConnNode) {
  if (node.expanded) {
    node.expanded = false;
    return;
  }
  if (!hasVisibleDbs(node.conn)) {
    node.expanded = true;
    node.databases = [];
    await openDbPicker(node);
    return;
  }
  if (node.databases) {
    node.expanded = true;
    return;
  }
  await loadDatabasesForNode(node);
}

async function toggleDatabase(node: ConnNode, db: DbNode) {
  if (db.expanded) {
    db.expanded = false;
    return;
  }
  if (db.tables) {
    db.expanded = true;
    return;
  }
  db.loading = true;
  db.error = "";
  try {
    db.tables = await listTables(node.conn.id, db.name);
    db.expanded = true;
  } catch (e) {
    db.error = String(e);
    db.expanded = true;
    db.tables = db.tables ?? [];
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
  const existing = nodes.value.find((n) => n.conn.id === id)?.conn;
  const payload: ConnectionConfig = {
    id,
    name: form.value.name.trim(),
    host: form.value.host.trim(),
    port: Number(form.value.port) || 3306,
    user: form.value.user.trim(),
    password: form.value.password,
    enabled: form.value.enabled,
    remark: form.value.remark.trim(),
    visible_databases: existing?.visible_databases ?? [],
  };
  if (!payload.name || !payload.host || !payload.user) {
    error.value = "请填写连接名称、主机与用户名";
    return;
  }
  try {
    await upsertConnection(payload);
    dialogOpen.value = false;
    await reloadConnections();
    status.value = editingId.value ? "连接已更新" : "连接已创建；请点击「选择库」后再展开";
    if (!editingId.value) {
      const created = nodes.value.find((n) => n.conn.id === id);
      if (created) {
        await openDbPicker(created);
      }
    }
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
  if (!hasVisibleDbs(node.conn)) {
    node.expanded = true;
    await openDbPicker(node);
    return;
  }
  await loadDatabasesForNode(node);
}

async function openDbPicker(node: ConnNode) {
  pickerNode.value = node;
  pickerOpen.value = true;
  pickerLoading.value = true;
  pickerError.value = "";
  pickerAll.value = [];
  pickerSelected.value = new Set(node.conn.visible_databases ?? []);
  try {
    const all = await listAllDatabases(node.conn.id);
    pickerAll.value = all;
    // 若尚未选择过，默认不勾选，由用户自选
    if ((node.conn.visible_databases?.length ?? 0) === 0) {
      pickerSelected.value = new Set();
    }
  } catch (e) {
    pickerError.value = String(e);
  } finally {
    pickerLoading.value = false;
  }
}

function togglePickerDb(name: string) {
  const next = new Set(pickerSelected.value);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  pickerSelected.value = next;
}

function selectAllPicker() {
  pickerSelected.value = new Set(pickerAll.value);
}

function clearPicker() {
  pickerSelected.value = new Set();
}

async function savePicker() {
  const node = pickerNode.value;
  if (!node) return;
  pickerError.value = "";
  try {
    const selected = Array.from(pickerSelected.value).sort();
    if (selected.length === 0) {
      pickerError.value = "请至少选择一个库，否则连接树不会展示任何库";
      return;
    }
    const updated = await setVisibleDatabases(node.conn.id, selected);
    node.conn = { ...updated, visible_databases: updated.visible_databases ?? selected };
    pickerOpen.value = false;
    node.databases = null;
    await loadDatabasesForNode(node);
    await reloadConnections();
    // 恢复展开状态
    const fresh = nodes.value.find((n) => n.conn.id === node.conn.id);
    if (fresh) {
      fresh.databases = null;
      await loadDatabasesForNode(fresh);
    }
    status.value = `已保存 ${selected.length} 个可见库`;
  } catch (e) {
    pickerError.value = String(e);
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
            <span
              v-if="!hasVisibleDbs(node.conn)"
              class="muted"
              style="margin-left: 6px"
              >未选库</span
            >
            <span
              v-else
              class="muted"
              style="margin-left: 6px"
              >{{ node.conn.visible_databases.length }} 库</span
            >
          </span>
        </div>
        <div class="toolbar" style="padding-left: 22px; background: transparent; border: none">
          <button type="button" class="btn ghost" @click.stop="ping(node)">
            测连通
          </button>
          <button type="button" class="btn ghost" @click.stop="openDbPicker(node)">
            选择库
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

        <div v-if="node.expanded" class="tree-children">
          <div
            v-if="!hasVisibleDbs(node.conn)"
            class="muted"
            style="padding: 6px 10px"
          >
            尚未选择可见库。
            <button type="button" class="btn ghost" @click="openDbPicker(node)">
              选择库
            </button>
          </div>
          <template v-else-if="node.databases">
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
              （已选库均不存在或未加载）
            </div>
          </template>
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

    <div
      v-if="pickerOpen"
      class="dialog-backdrop"
      @click.self="pickerOpen = false"
    >
      <div class="dialog" role="dialog" aria-modal="true" style="max-width: 480px">
        <h3>
          选择可见库
          <span v-if="pickerNode" class="muted">— {{ pickerNode.conn.name }}</span>
        </h3>
        <p class="muted" style="margin: 0 0 10px; font-size: 12px">
          未勾选的库不会在连接树中展示，也不会加载其表结构。
        </p>
        <div v-if="pickerError" class="error-text" style="margin-bottom: 8px">
          {{ pickerError }}
        </div>
        <div v-if="pickerLoading" class="muted">正在从服务器拉取库列表…</div>
        <template v-else>
          <div class="toolbar" style="border: none; padding: 0 0 8px">
            <button type="button" class="btn ghost" @click="selectAllPicker">
              全选
            </button>
            <button type="button" class="btn ghost" @click="clearPicker">
              清空
            </button>
            <span class="spacer" />
            <span class="muted">已选 {{ pickerSelected.size }} / {{ pickerAll.length }}</span>
          </div>
          <div class="picker-list">
            <label
              v-for="name in pickerAll"
              :key="name"
              class="picker-item"
            >
              <input
                type="checkbox"
                :checked="pickerSelected.has(name)"
                @change="togglePickerDb(name)"
              />
              <span>{{ name }}</span>
            </label>
            <div v-if="pickerAll.length === 0" class="muted">
              （服务器上没有可展示的业务库）
            </div>
          </div>
        </template>
        <div class="dialog-actions">
          <button type="button" class="btn" @click="pickerOpen = false">
            取消
          </button>
          <button
            type="button"
            class="btn primary"
            :disabled="pickerLoading"
            @click="savePicker"
          >
            保存并加载
          </button>
        </div>
      </div>
    </div>
  </div>
</template>
