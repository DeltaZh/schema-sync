<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import {
  ddlExecute,
  ddlPreview,
  expandRuleTargets,
  listRules,
} from "../lib/tauri";
import type {
  ConnectionConfig,
  DdlPreviewResponse,
  ExecResult,
  NamingRule,
  RuleTarget,
} from "../types";

const props = defineProps<{
  connections: ConnectionConfig[];
}>();

const rules = ref<NamingRule[]>([]);
const ruleId = ref("");
const sql = ref("");
const stopOnError = ref(true);

const targets = ref<RuleTarget[]>([]);
const excludedKeys = ref<Set<string>>(new Set());

const preview = ref<DdlPreviewResponse | null>(null);
const previewing = ref(false);
const executing = ref(false);
const status = ref("");
const error = ref("");
const execResults = ref<ExecResult[]>([]);

function connName(id: string): string {
  return props.connections.find((c) => c.id === id)?.name ?? id;
}

function targetKey(t: RuleTarget): string {
  return `${t.connection_id}\0${t.database}`;
}

function toggleExclude(t: RuleTarget, exclude: boolean) {
  const next = new Set(excludedKeys.value);
  const key = targetKey(t);
  if (exclude) next.add(key);
  else next.delete(key);
  excludedKeys.value = next;
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

async function refreshTargets() {
  targets.value = [];
  excludedKeys.value = new Set();
  if (!ruleId.value) return;
  error.value = "";
  try {
    targets.value = await expandRuleTargets({
      rule_id: ruleId.value,
      probe: false,
      exclude: [],
    });
  } catch (e) {
    error.value = String(e);
  }
}

async function runPreview() {
  error.value = "";
  execResults.value = [];
  preview.value = null;
  if (!sql.value.trim()) {
    error.value = "请粘贴要投放的 SQL";
    return;
  }
  if (!ruleId.value) {
    error.value = "请选择命名规则";
    return;
  }
  previewing.value = true;
  status.value = "正在校验并展开目标…";
  try {
    const exclude = targets.value
      .filter((t) => excludedKeys.value.has(targetKey(t)))
      .map((t) => ({
        connection_id: t.connection_id,
        database: t.database,
      }));
    preview.value = await ddlPreview({
      sql: sql.value,
      rule_id: ruleId.value,
      exclude,
    });
    const warnCount = preview.value.warnings?.length ?? 0;
    status.value = `预览就绪：${preview.value.statements.length} 条语句 → ${preview.value.targets.length} 个目标${
      warnCount ? `（${warnCount} 条提示）` : ""
    }`;
  } catch (e) {
    error.value = String(e);
    status.value = "";
  } finally {
    previewing.value = false;
  }
}

async function runExecute() {
  error.value = "";
  execResults.value = [];
  if (!preview.value?.preview_id) {
    error.value = "请先完成预览";
    return;
  }
  const p = preview.value;
  if (
    !confirm(
      `即将向 ${p.targets.length} 个目标库执行已校验的 ${p.statements.length} 条语句。是否继续？`,
    )
  ) {
    return;
  }
  executing.value = true;
  status.value = "正在执行投放…";
  try {
    execResults.value = await ddlExecute({
      preview_id: p.preview_id,
      stop_on_error: stopOnError.value,
    });
    const ok = execResults.value.filter((r) => r.ok).length;
    const fail = execResults.value.length - ok;
    status.value = `投放完成：成功 ${ok}，失败 ${fail}`;
  } catch (e) {
    error.value = String(e);
    status.value = "";
  } finally {
    executing.value = false;
  }
}

watch(ruleId, () => {
  void refreshTargets();
});

onMounted(async () => {
  await reloadRules();
  await refreshTargets();
});
</script>

<template>
  <div class="pane-body">
    <div class="toolbar" style="margin: -12px -12px 12px; border-radius: 0">
      <button
        type="button"
        class="btn primary"
        :disabled="previewing"
        @click="runPreview"
      >
        {{ previewing ? "预览中…" : "预览校验" }}
      </button>
      <button
        type="button"
        class="btn primary"
        :disabled="executing || !preview"
        @click="runExecute"
      >
        {{ executing ? "执行中…" : "确认投放" }}
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
      <h2 class="section-title">DDL 投放</h2>
      <p class="muted">
        仅允许结构变更类语句（如 ALTER / CREATE INDEX 等）。危险语句会在预览阶段被拒绝。
      </p>
      <div class="form-grid" style="max-width: none">
        <label for="ddl-rule">命名规则</label>
        <select id="ddl-rule" v-model="ruleId" class="field">
          <option value="" disabled>请选择规则</option>
          <option v-for="r in rules" :key="r.id" :value="r.id">
            {{ r.logical_name || r.id }}
          </option>
        </select>

        <label for="ddl-sql">SQL</label>
        <textarea
          id="ddl-sql"
          v-model="sql"
          class="textarea ddl-sql"
          placeholder="粘贴要投放的结构变更 SQL，多条语句用分号分隔"
        />
      </div>
    </div>

    <div v-if="targets.length" class="section-block">
      <h3 class="section-title">投放目标（可剔除）</h3>
      <p class="muted">勾选「剔除」后，预览与投放不会包含该目标库。</p>
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

    <div v-if="preview" class="section-block">
      <h3 class="section-title">
        预览结果
        <span class="muted" style="font-weight: 400">
          （preview_id={{ preview.preview_id }}）
        </span>
      </h3>

      <div v-if="preview.warnings?.length" class="warn-box">
        <p class="muted" style="margin: 0 0 6px">提示（已跳过缺失或不可达目标）：</p>
        <ul>
          <li v-for="(w, i) in preview.warnings" :key="i">{{ w }}</li>
        </ul>
      </div>

      <h4 class="subsection-title">已校验语句</h4>
      <ol class="stmt-list">
        <li v-for="(s, i) in preview.statements" :key="i">
          <pre class="pre-box">{{ s }}</pre>
        </li>
      </ol>

      <h4 class="subsection-title">
        将投放的目标库（{{ preview.targets.length }}）
      </h4>
      <table class="data-table">
        <thead>
          <tr>
            <th>连接</th>
            <th>数据库</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="(t, i) in preview.targets" :key="i">
            <td>{{ connName(t.connection_id) }}</td>
            <td><code>{{ t.database }}</code></td>
          </tr>
        </tbody>
      </table>
    </div>

    <div v-if="execResults.length" class="section-block">
      <h3 class="section-title">执行结果</h3>
      <table class="data-table">
        <thead>
          <tr>
            <th>状态</th>
            <th>目标 / 语句</th>
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

<style scoped>
.warn-box {
  margin: 0 0 12px;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid var(--border, #ccc);
  background: color-mix(in srgb, var(--bg, #fff) 92%, #e6a817 8%);
}
.warn-box ul {
  margin: 0;
  padding-left: 1.2em;
}
</style>
