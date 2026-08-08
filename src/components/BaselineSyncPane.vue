<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { kindLabel, riskLabel } from "../lib/labels";
import {
  baselineExecute,
  baselineScan,
  expandRuleTargets,
  listDatabases,
  listRules,
  listTables,
} from "../lib/tauri";
import type {
  ConnectionConfig,
  DiffItem,
  ExecResult,
  NamingRule,
  RuleTarget,
  TableSummary,
} from "../types";

const props = defineProps<{
  connections: ConnectionConfig[];
}>();

const rules = ref<NamingRule[]>([]);
const ruleId = ref("");
const baselineConnId = ref("");
const baselineDb = ref("");
const databases = ref<string[]>([]);
const tables = ref<TableSummary[]>([]);
const selectedTables = ref<Set<string>>(new Set());
const targets = ref<RuleTarget[]>([]);
const excludedKeys = ref<Set<string>>(new Set());

const scanId = ref("");
const items = ref<DiffItem[]>([]);
const selectedIds = ref<Set<string>>(new Set());
const stopOnError = ref(true);

const loadingDbs = ref(false);
const loadingTables = ref(false);
const scanning = ref(false);
const executing = ref(false);
const status = ref("");
const error = ref("");
const execResults = ref<ExecResult[]>([]);

const enabledConnections = computed(() =>
  props.connections.filter((c) => c.enabled),
);

function targetKey(t: RuleTarget): string {
  return `${t.connection_id}|${t.database}`;
}

function connName(id: string): string {
  return props.connections.find((c) => c.id === id)?.name ?? id;
}

async function reloadRules() {
  try {
    rules.value = await listRules();
    if (!ruleId.value || !rules.value.some((r) => r.id === ruleId.value)) {
      ruleId.value = rules.value[0]?.id ?? "";
    }
  } catch (e) {
    error.value = String(e);
  }
}

async function loadDatabases() {
  databases.value = [];
  baselineDb.value = "";
  tables.value = [];
  selectedTables.value = new Set();
  if (!baselineConnId.value) return;
  loadingDbs.value = true;
  error.value = "";
  try {
    databases.value = await listDatabases(baselineConnId.value);
  } catch (e) {
    error.value = String(e);
  } finally {
    loadingDbs.value = false;
  }
}

async function loadTables() {
  tables.value = [];
  selectedTables.value = new Set();
  if (!baselineConnId.value || !baselineDb.value) return;
  loadingTables.value = true;
  error.value = "";
  try {
    tables.value = await listTables(baselineConnId.value, baselineDb.value);
  } catch (e) {
    error.value = String(e);
  } finally {
    loadingTables.value = false;
  }
}

function toggleTable(name: string, checked: boolean) {
  const next = new Set(selectedTables.value);
  if (checked) next.add(name);
  else next.delete(name);
  selectedTables.value = next;
}

function selectAllTables(checked: boolean) {
  selectedTables.value = checked
    ? new Set(tables.value.map((t) => t.name))
    : new Set();
}

function toggleExclude(t: RuleTarget, exclude: boolean) {
  const next = new Set(excludedKeys.value);
  const key = targetKey(t);
  if (exclude) next.add(key);
  else next.delete(key);
  excludedKeys.value = next;
}

async function refreshTargets() {
  targets.value = [];
  excludedKeys.value = new Set();
  if (!ruleId.value) return;
  error.value = "";
  try {
    targets.value = await expandRuleTargets({
      rule_id: ruleId.value,
      probe: false,
      exclude: [
        {
          connection_id: baselineConnId.value,
          database: baselineDb.value,
        },
      ].filter((t) => t.connection_id && t.database),
    });
  } catch (e) {
    error.value = String(e);
  }
}

function toggleItem(id: string, checked: boolean) {
  const next = new Set(selectedIds.value);
  if (checked) next.add(id);
  else next.delete(id);
  selectedIds.value = next;
}

function selectAllItems(checked: boolean) {
  selectedIds.value = checked
    ? new Set(items.value.map((i) => i.id))
    : new Set();
}

function selectDefaultItems() {
  selectedIds.value = new Set(
    items.value.filter((i) => i.selected_default).map((i) => i.id),
  );
}

async function runScan() {
  error.value = "";
  execResults.value = [];
  scanId.value = "";
  items.value = [];
  selectedIds.value = new Set();

  if (!baselineConnId.value || !baselineDb.value) {
    error.value = "请选择基准连接与数据库";
    return;
  }
  if (selectedTables.value.size === 0) {
    error.value = "请至少勾选一张表";
    return;
  }
  if (!ruleId.value) {
    error.value = "请选择命名规则";
    return;
  }

  scanning.value = true;
  status.value = "正在扫描差异…";
  try {
    const exclude_targets = targets.value
      .filter((t) => excludedKeys.value.has(targetKey(t)))
      .map((t) => ({
        connection_id: t.connection_id,
        database: t.database,
      }));
    const resp = await baselineScan({
      baseline_connection_id: baselineConnId.value,
      baseline_database: baselineDb.value,
      tables: [...selectedTables.value],
      rule_id: ruleId.value,
      exclude_targets,
    });
    scanId.value = resp.scan_id;
    items.value = resp.items;
    selectDefaultItems();
    const warnCount = resp.warnings?.length ?? 0;
    status.value = `扫描完成：${resp.items.length} 条差异（scan_id=${resp.scan_id}）${
      warnCount ? `；${warnCount} 条提示（已跳过缺失库等）` : ""
    }`;
  } catch (e) {
    error.value = String(e);
    status.value = "";
  } finally {
    scanning.value = false;
  }
}

async function runExecute() {
  error.value = "";
  execResults.value = [];
  if (!scanId.value) {
    error.value = "请先扫描差异";
    return;
  }
  const ids = [...selectedIds.value];
  if (ids.length === 0) {
    error.value = "请至少勾选一条差异";
    return;
  }
  if (
    !confirm(
      `即将按已勾选的 ${ids.length} 条差异执行同步（仅使用服务端缓存 id，不提交客户端 SQL）。是否继续？`,
    )
  ) {
    return;
  }

  executing.value = true;
  status.value = "正在执行…";
  try {
    execResults.value = await baselineExecute({
      scan_id: scanId.value,
      item_ids: ids,
      stop_on_error: stopOnError.value,
    });
    const ok = execResults.value.filter((r) => r.ok).length;
    const fail = execResults.value.length - ok;
    status.value = `执行完成：成功 ${ok}，失败 ${fail}`;
  } catch (e) {
    error.value = String(e);
    status.value = "";
  } finally {
    executing.value = false;
  }
}

watch(baselineConnId, () => {
  void loadDatabases();
});

watch(baselineDb, () => {
  void loadTables();
});

watch(ruleId, () => {
  void refreshTargets();
});

watch(
  () => [baselineConnId.value, baselineDb.value] as const,
  () => {
    void refreshTargets();
  },
);

onMounted(() => {
  void reloadRules();
  if (enabledConnections.value[0]) {
    baselineConnId.value = enabledConnections.value[0].id;
  }
});
</script>

<template>
  <div class="pane-body">
    <div class="toolbar" style="margin: -12px -12px 12px; border-radius: 0">
      <button
        type="button"
        class="btn primary"
        :disabled="scanning"
        @click="runScan"
      >
        {{ scanning ? "扫描中…" : "扫描差异" }}
      </button>
      <button
        type="button"
        class="btn primary"
        :disabled="executing || !scanId"
        @click="runExecute"
      >
        {{ executing ? "执行中…" : "确认执行" }}
      </button>
      <label class="inline-check">
        <input v-model="stopOnError" type="checkbox" />
        遇错即停
      </label>
      <span class="spacer" />
      <span v-if="status" class="muted">{{ status }}</span>
    </div>

    <p v-if="error" class="error-text">{{ error }}</p>

    <div class="section-block">
      <h2 class="section-title">基准库与表</h2>
      <div class="form-grid" style="max-width: none">
        <label for="bl-conn">基准连接</label>
        <select id="bl-conn" v-model="baselineConnId" class="field">
          <option value="" disabled>请选择连接</option>
          <option
            v-for="c in enabledConnections"
            :key="c.id"
            :value="c.id"
          >
            {{ c.name }}
          </option>
        </select>

        <label for="bl-db">基准数据库</label>
        <select
          id="bl-db"
          v-model="baselineDb"
          class="field"
          :disabled="loadingDbs || !baselineConnId"
        >
          <option value="" disabled>
            {{ loadingDbs ? "加载中…" : "请选择数据库" }}
          </option>
          <option v-for="db in databases" :key="db" :value="db">
            {{ db }}
          </option>
        </select>

        <label for="bl-rule">命名规则</label>
        <select id="bl-rule" v-model="ruleId" class="field">
          <option value="" disabled>请选择规则</option>
          <option v-for="r in rules" :key="r.id" :value="r.id">
            {{ r.logical_name || r.id }}
          </option>
        </select>
      </div>

      <div v-if="tables.length" class="table-pick" style="margin-top: 12px">
        <div class="toolbar" style="border: none; background: transparent; padding: 0 0 8px">
          <label class="inline-check">
            <input
              type="checkbox"
              :checked="
                tables.length > 0 && selectedTables.size === tables.length
              "
              @change="
                selectAllTables(($event.target as HTMLInputElement).checked)
              "
            />
            全选表（{{ selectedTables.size }}/{{ tables.length }}）
          </label>
          <span v-if="loadingTables" class="muted">加载表列表…</span>
        </div>
        <div class="checkbox-list pick-scroll">
          <label v-for="t in tables" :key="t.name">
            <input
              type="checkbox"
              :checked="selectedTables.has(t.name)"
              @change="
                toggleTable(
                  t.name,
                  ($event.target as HTMLInputElement).checked,
                )
              "
            />
            <code>{{ t.name }}</code>
            <span class="muted">{{ t.comment || "（无注释）" }}</span>
          </label>
        </div>
      </div>
      <p v-else-if="baselineDb && !loadingTables" class="muted">
        该库暂无表
      </p>
    </div>

    <div v-if="targets.length" class="section-block">
      <h3 class="section-title">投放目标（可剔除）</h3>
      <p class="muted">勾选「剔除」后，扫描时不会对比该目标库。</p>
      <table class="data-table">
        <thead>
          <tr>
            <th>剔除</th>
            <th>连接</th>
            <th>数据库</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="t in targets" :key="targetKey(t)">
            <td>
              <input
                type="checkbox"
                :checked="excludedKeys.has(targetKey(t))"
                @change="
                  toggleExclude(
                    t,
                    ($event.target as HTMLInputElement).checked,
                  )
                "
              />
            </td>
            <td>{{ connName(t.connection_id) }}</td>
            <td>
              <code>{{ t.database }}</code>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="items.length" class="section-block">
      <h3 class="section-title">差异列表（{{ items.length }}）</h3>
      <div class="toolbar" style="border: none; background: transparent; padding: 0 0 8px">
        <button type="button" class="btn ghost" @click="selectDefaultItems">
          恢复默认勾选
        </button>
        <button type="button" class="btn ghost" @click="selectAllItems(true)">
          全选
        </button>
        <button type="button" class="btn ghost" @click="selectAllItems(false)">
          全不选
        </button>
        <span class="muted">已选 {{ selectedIds.size }} 条</span>
      </div>
      <table class="data-table">
        <thead>
          <tr>
            <th></th>
            <th>风险</th>
            <th>类型</th>
            <th>目标</th>
            <th>说明</th>
            <th>SQL 预览</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="item in items" :key="item.id">
            <td>
              <input
                type="checkbox"
                :checked="selectedIds.has(item.id)"
                @change="
                  toggleItem(
                    item.id,
                    ($event.target as HTMLInputElement).checked,
                  )
                "
              />
            </td>
            <td>
              <span class="risk-badge" :data-risk="item.risk">
                {{ riskLabel(item.risk) }}
              </span>
            </td>
            <td>{{ kindLabel(item.kind) }}</td>
            <td>
              <div>{{ connName(item.connection_id) }}</div>
              <code>{{ item.database }}.{{ item.table }}</code>
            </td>
            <td>
              <div class="diff-title">{{ item.title }}</div>
              <div class="muted">{{ item.detail }}</div>
            </td>
            <td>
              <pre class="sql-cell">{{ item.sql }}</pre>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
    <div v-else-if="scanId" class="section-block muted">
      扫描完成，未发现差异
    </div>

    <div v-if="execResults.length" class="section-block">
      <h3 class="section-title">执行结果</h3>
      <table class="data-table">
        <thead>
          <tr>
            <th>状态</th>
            <th>差异 ID</th>
            <th>错误</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(r, i) in execResults" :key="i">
            <td>
              <span v-if="r.ok" class="ok-text">成功</span>
              <span v-else class="error-text">失败</span>
            </td>
            <td><code>{{ r.diff_id }}</code></td>
            <td>{{ r.error || "—" }}</td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>
