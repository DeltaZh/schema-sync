<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { askConfirm } from "../lib/confirmDialog";
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

/** 筛选后的库视图：tables 为 null 表示尚未加载 */
interface FilteredDbView {
  db: DbNode;
  tables: TableSummary[] | null;
  /** 因表名命中而展示时，即使未展开也列出表 */
  revealTables: boolean;
}

interface FilteredConnView {
  node: ConnNode;
  databases: FilteredDbView[] | null;
}

const nodes = ref<ConnNode[]>([]);
const status = ref("");
const error = ref("");
const selectedKey = ref("");
const treeFilter = ref("");

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
const pickerFilter = ref("");

/** 连接行右键 / 「⋯」菜单 */
const connMenu = ref<{
  x: number;
  y: number;
  node: ConnNode;
} | null>(null);

function selectionKey(s: TableSelection): string {
  return `${s.connectionId}/${s.database}/${s.table}`;
}

function hasVisibleDbs(conn: ConnectionConfig): boolean {
  return (conn.visible_databases?.length ?? 0) > 0;
}

function includesIgnoreCase(haystack: string, needle: string): boolean {
  return haystack.toLowerCase().includes(needle);
}

const filteredTree = computed((): FilteredConnView[] => {
  const q = treeFilter.value.trim().toLowerCase();
  if (!q) {
    return nodes.value.map((node) => ({
      node,
      databases:
        node.databases?.map((db) => ({
          db,
          tables: db.tables,
          revealTables: false,
        })) ?? null,
    }));
  }

  const out: FilteredConnView[] = [];
  for (const node of nodes.value) {
    const connHit =
      includesIgnoreCase(node.conn.name, q) ||
      includesIgnoreCase(node.conn.host, q) ||
      includesIgnoreCase(node.conn.remark ?? "", q);

    if (!node.databases) {
      if (connHit) out.push({ node, databases: null });
      continue;
    }

    const dbs: FilteredDbView[] = [];
    for (const db of node.databases) {
      const dbHit = includesIgnoreCase(db.name, q);
      if (dbHit) {
        dbs.push({ db, tables: db.tables, revealTables: false });
        continue;
      }
      if (db.tables) {
        const tables = db.tables.filter(
          (t) =>
            includesIgnoreCase(t.name, q) ||
            includesIgnoreCase(t.comment ?? "", q),
        );
        if (tables.length > 0) {
          dbs.push({ db, tables, revealTables: true });
        }
      }
    }

    if (connHit && dbs.length === 0) {
      // 仅连接命中：保留其下全部库
      out.push({
        node,
        databases: node.databases.map((db) => ({
          db,
          tables: db.tables,
          revealTables: false,
        })),
      });
    } else if (dbs.length > 0) {
      out.push({ node, databases: dbs });
    }
  }
  return out;
});

const treeFilterActive = computed(() => treeFilter.value.trim().length > 0);

const pickerFiltered = computed(() => {
  const q = pickerFilter.value.trim().toLowerCase();
  if (!q) return pickerAll.value;
  return pickerAll.value.filter((name) => includesIgnoreCase(name, q));
});

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
  const ok = await askConfirm(`确定删除连接「${node.conn.name}」？`, {
    title: "删除连接",
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
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
  pickerFilter.value = "";
  pickerSelected.value = new Set(node.conn.visible_databases ?? []);
  try {
    status.value = `正在拉取「${node.conn.name}」的库列表…`;
    const all = await listAllDatabases(node.conn.id);
    pickerAll.value = all;
    // 若尚未选择过，默认不勾选，由用户自选
    if ((node.conn.visible_databases?.length ?? 0) === 0) {
      pickerSelected.value = new Set();
    }
    status.value = `已拉取 ${all.length} 个业务库，请勾选后保存`;
  } catch (e) {
    pickerError.value = formatErr(e);
    status.value = "";
  } finally {
    pickerLoading.value = false;
  }
}

function formatErr(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

function togglePickerDb(name: string) {
  const next = new Set(pickerSelected.value);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  pickerSelected.value = next;
}

function selectAllPicker() {
  // 全选仅作用于当前筛出结果（已勾选的其它库保留）
  const next = new Set(pickerSelected.value);
  for (const name of pickerFiltered.value) next.add(name);
  pickerSelected.value = next;
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

function closeConnMenu() {
  connMenu.value = null;
}

function openConnMenu(e: MouseEvent, node: ConnNode) {
  e.preventDefault();
  e.stopPropagation();
  const pad = 8;
  const menuW = 168;
  const menuH = 220;
  const x = Math.min(e.clientX, window.innerWidth - menuW - pad);
  const y = Math.min(e.clientY, window.innerHeight - menuH - pad);
  connMenu.value = { x: Math.max(pad, x), y: Math.max(pad, y), node };
}

function runConnMenu(action: (node: ConnNode) => void | Promise<void>) {
  const node = connMenu.value?.node;
  closeConnMenu();
  if (!node) return;
  void action(node);
}

function onGlobalPointerDown(e: PointerEvent) {
  if (!connMenu.value) return;
  const el = e.target;
  if (el instanceof Element && el.closest(".ctx-menu")) return;
  closeConnMenu();
}

onMounted(() => {
  void reloadConnections();
  window.addEventListener("pointerdown", onGlobalPointerDown, true);
  window.addEventListener("resize", closeConnMenu);
  window.addEventListener("blur", closeConnMenu);
});

onBeforeUnmount(() => {
  window.removeEventListener("pointerdown", onGlobalPointerDown, true);
  window.removeEventListener("resize", closeConnMenu);
  window.removeEventListener("blur", closeConnMenu);
});

defineExpose({ reloadConnections });
</script>

<template>
  <div class="panel">
    <div class="brand-bar">
      <img class="brand-logo" src="/app-icon.png" alt="" />
      <div>
        <div class="brand-title">结构同步</div>
        <div class="brand-sub muted">多库表结构对齐</div>
      </div>
    </div>

    <div class="toolbar">
      <button type="button" class="btn primary" @click="openCreate">新建</button>
      <button type="button" class="btn" @click="reloadConnections">刷新</button>
      <span class="spacer" />
    </div>

    <div class="filter-bar">
      <input
        v-model="treeFilter"
        class="filter-input"
        type="search"
        placeholder="筛选库 / 表…"
        aria-label="筛选库或表"
      />
    </div>

    <div v-if="error" class="toolbar error-text">{{ error }}</div>

    <div class="tree">
      <div v-if="nodes.length === 0" class="muted" style="padding: 10px">
        暂无连接，请点击「新建」添加
      </div>
      <div
        v-else-if="treeFilterActive && filteredTree.length === 0"
        class="muted"
        style="padding: 10px"
      >
        没有符合的库或表
      </div>

      <div
        v-for="view in filteredTree"
        :key="view.node.conn.id"
        class="tree-node"
      >
        <div
          class="tree-row conn-row"
          @click="toggleConnection(view.node)"
          @contextmenu="openConnMenu($event, view.node)"
        >
          <span class="tree-toggle">{{
            view.node.loading
              ? "…"
              : view.node.expanded || treeFilterActive
                ? "▼"
                : "▶"
          }}</span>
          <span
            class="tree-label"
            :title="view.node.conn.remark || view.node.conn.host"
          >
            {{ view.node.conn.name }}
            <span class="muted"
              >({{ view.node.conn.host }}:{{ view.node.conn.port }})</span
            >
            <span
              v-if="!hasVisibleDbs(view.node.conn)"
              class="muted"
              style="margin-left: 6px"
              >未选库</span
            >
            <span
              v-else
              class="muted"
              style="margin-left: 6px"
              >{{ view.node.conn.visible_databases.length }} 库</span
            >
          </span>
          <button
            type="button"
            class="row-more"
            title="连接操作"
            aria-label="连接操作"
            @click.stop="openConnMenu($event, view.node)"
          >
            ⋯
          </button>
        </div>
        <div
          v-if="view.node.error"
          class="error-text"
          style="padding: 0 10px 6px 26px"
        >
          {{ view.node.error }}
        </div>

        <div
          v-if="view.node.expanded || treeFilterActive"
          class="tree-children"
        >
          <div
            v-if="!hasVisibleDbs(view.node.conn)"
            class="muted"
            style="padding: 6px 10px"
          >
            尚未选择可见库。
            <button
              type="button"
              class="btn ghost"
              @click="openDbPicker(view.node)"
            >
              选择库
            </button>
          </div>
          <template v-else-if="view.databases">
            <div
              v-for="dbView in view.databases"
              :key="dbView.db.name"
              class="tree-node"
            >
              <div
                class="tree-row"
                @click="toggleDatabase(view.node, dbView.db)"
              >
                <span class="tree-toggle">{{
                  dbView.db.loading
                    ? "…"
                    : dbView.db.expanded || dbView.revealTables
                      ? "▼"
                      : "▶"
                }}</span>
                <span class="tree-label">{{ dbView.db.name }}</span>
              </div>
              <div
                v-if="dbView.db.error"
                class="error-text"
                style="padding: 0 10px 6px 26px"
              >
                {{ dbView.db.error }}
              </div>
              <div
                v-if="
                  (dbView.db.expanded || dbView.revealTables) &&
                  dbView.tables
                "
                class="tree-children"
              >
                <div
                  v-for="table in dbView.tables"
                  :key="table.name"
                  class="tree-row"
                  :class="{
                    active:
                      selectedKey ===
                      `${view.node.conn.id}/${dbView.db.name}/${table.name}`,
                  }"
                  @click="selectTable(view.node, dbView.db, table)"
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
                  v-if="dbView.tables.length === 0"
                  class="muted"
                  style="padding: 4px 10px"
                >
                  （无表）
                </div>
              </div>
            </div>
            <div
              v-if="view.databases.length === 0"
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
      v-if="connMenu"
      class="ctx-menu"
      role="menu"
      :style="{ left: `${connMenu.x}px`, top: `${connMenu.y}px` }"
      @pointerdown.stop
    >
      <button
        type="button"
        class="ctx-item"
        role="menuitem"
        @click="runConnMenu(ping)"
      >
        测连通
      </button>
      <button
        type="button"
        class="ctx-item"
        role="menuitem"
        @click="runConnMenu(openDbPicker)"
      >
        选择库
      </button>
      <button
        type="button"
        class="ctx-item"
        role="menuitem"
        @click="runConnMenu(refreshNode)"
      >
        重载库列表
      </button>
      <button
        type="button"
        class="ctx-item"
        role="menuitem"
        @click="runConnMenu(openEdit)"
      >
        编辑连接
      </button>
      <div class="ctx-sep" />
      <button
        type="button"
        class="ctx-item danger"
        role="menuitem"
        @click="runConnMenu(removeConnection)"
      >
        删除连接
      </button>
    </div>

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
          <button
            v-if="pickerNode && !pickerLoading"
            type="button"
            class="btn ghost"
            style="margin-left: 8px"
            @click="openDbPicker(pickerNode)"
          >
            重试
          </button>
        </div>
        <div v-if="pickerLoading" class="muted">
          正在从服务器拉取库列表（最多约 15 秒）…
        </div>
        <template v-else>
          <div class="filter-bar" style="padding: 0 0 8px; border: none">
            <input
              v-model="pickerFilter"
              class="filter-input"
              type="search"
              placeholder="筛选库名…"
              aria-label="筛选库名"
            />
          </div>
          <div class="toolbar" style="border: none; padding: 0 0 8px">
            <button type="button" class="btn ghost" @click="selectAllPicker">
              全选当前结果
            </button>
            <button type="button" class="btn ghost" @click="clearPicker">
              清空
            </button>
            <span class="spacer" />
            <span class="muted"
              >已选 {{ pickerSelected.size }} / 共 {{ pickerAll.length }}
              <template v-if="pickerFilter.trim()"
                >（显示 {{ pickerFiltered.length }}）</template
              ></span
            >
          </div>
          <div class="picker-list">
            <label
              v-for="name in pickerFiltered"
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
            <div
              v-else-if="pickerFiltered.length === 0"
              class="muted"
            >
              没有符合的库名
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
