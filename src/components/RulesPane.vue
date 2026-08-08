<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { askConfirm } from "../lib/confirmDialog";
import {
  expandRuleTargets,
  listRules,
  newId,
  saveRules,
} from "../lib/tauri";
import type {
  ConnectionConfig,
  NamingRule,
  PartKind,
  RuleTarget,
} from "../types";

const props = defineProps<{
  connections: ConnectionConfig[];
}>();

const PLACEHOLDER_RE = /\{(年份|租户|分片|year|tenant|shard)\}/gi;

const PART_META: Record<
  PartKind,
  { label: string; placeholder: string; hint: string }
> = {
  year: {
    label: "年份取值",
    placeholder: "如 2025\n2026",
    hint: "对应模板中的 {年份}",
  },
  tenant: {
    label: "租户取值",
    placeholder: "如 demo\nacme",
    hint: "对应模板中的 {租户}",
  },
  shard: {
    label: "分片取值",
    placeholder: "如 0\n1\n2",
    hint: "对应模板中的 {分片}",
  },
};

const rules = ref<NamingRule[]>([]);
const selectedId = ref<string | null>(null);
const status = ref("");
const error = ref("");
const saving = ref(false);
const expandPreview = ref<RuleTarget[]>([]);
const expandBusy = ref(false);

const selected = computed(
  () => rules.value.find((r) => r.id === selectedId.value) ?? null,
);

const tenantsText = ref("");
const yearsText = ref("");
const shardsText = ref("");

function emptyRule(): NamingRule {
  return {
    id: newId("rule"),
    display_name: "",
    pattern: "order_{年份}_{租户}",
    logical_name: "order",
    parts_order: ["year", "tenant"],
    tenants: [],
    years: [],
    shards: [],
    connection_ids: [],
  };
}

function normalizePlaceholderToken(raw: string): PartKind | null {
  const t = raw.toLowerCase();
  if (t === "year" || raw === "年份") return "year";
  if (t === "tenant" || raw === "租户") return "tenant";
  if (t === "shard" || raw === "分片") return "shard";
  return null;
}

/** 模板中按出现顺序去重后的占位符 */
function usedParts(pattern: string): PartKind[] {
  const seen = new Set<PartKind>();
  const out: PartKind[] = [];
  const re = new RegExp(PLACEHOLDER_RE.source, "gi");
  let m: RegExpExecArray | null;
  while ((m = re.exec(pattern)) !== null) {
    const kind = normalizePlaceholderToken(m[1]);
    if (kind && !seen.has(kind)) {
      seen.add(kind);
      out.push(kind);
    }
  }
  return out;
}

const selectedUsedParts = computed(() =>
  selected.value ? usedParts(selected.value.pattern) : [],
);

function ruleTitle(rule: NamingRule): string {
  return rule.display_name || rule.pattern || rule.logical_name || "(未命名)";
}

function syncListEditors(rule: NamingRule | null) {
  tenantsText.value = rule?.tenants.join("\n") ?? "";
  yearsText.value = rule?.years.join("\n") ?? "";
  shardsText.value = rule?.shards.join("\n") ?? "";
}

watch(selected, (rule) => {
  syncListEditors(rule);
  expandPreview.value = [];
});

function parseLines(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((s) => s.trim())
    .filter(Boolean);
}

function applyListEditors(rule: NamingRule) {
  rule.tenants = parseLines(tenantsText.value);
  rule.years = parseLines(yearsText.value);
  rule.shards = parseLines(shardsText.value);
  rule.parts_order = usedParts(rule.pattern);
}

function textForPart(kind: PartKind): string {
  if (kind === "tenant") return tenantsText.value;
  if (kind === "year") return yearsText.value;
  return shardsText.value;
}

function setTextForPart(kind: PartKind, value: string) {
  if (kind === "tenant") tenantsText.value = value;
  else if (kind === "year") yearsText.value = value;
  else shardsText.value = value;
}

async function reload() {
  error.value = "";
  try {
    rules.value = await listRules();
    if (
      selectedId.value &&
      !rules.value.some((r) => r.id === selectedId.value)
    ) {
      selectedId.value = rules.value[0]?.id ?? null;
    } else if (!selectedId.value) {
      selectedId.value = rules.value[0]?.id ?? null;
    }
    syncListEditors(selected.value);
    status.value = `已加载 ${rules.value.length} 条规则`;
  } catch (e) {
    error.value = String(e);
  }
}

function addRule() {
  const rule = emptyRule();
  rule.display_name = "新规则";
  rules.value.push(rule);
  selectedId.value = rule.id;
  status.value = "已新增规则（尚未保存）";
}

async function removeSelected() {
  if (!selected.value) return;
  const ok = await askConfirm(`确定删除规则「${ruleTitle(selected.value)}」？`, {
    title: "删除规则",
    confirmText: "删除",
    danger: true,
  });
  if (!ok) return;
  rules.value = rules.value.filter((r) => r.id !== selectedId.value);
  selectedId.value = rules.value[0]?.id ?? null;
  status.value = "已从列表移除（需点保存才落盘）";
}

function toggleConnection(id: string, checked: boolean) {
  const rule = selected.value;
  if (!rule) return;
  if (checked) {
    if (!rule.connection_ids.includes(id)) {
      rule.connection_ids.push(id);
    }
  } else {
    rule.connection_ids = rule.connection_ids.filter((c) => c !== id);
  }
}

async function persist() {
  error.value = "";
  saving.value = true;
  try {
    if (selected.value) {
      applyListEditors(selected.value);
    }
    for (const r of rules.value) {
      r.pattern = r.pattern.trim();
      r.display_name = r.display_name.trim();
      if (!r.pattern) {
        throw new Error(`请为规则填写库名模板（如 order_{年份}_{租户}）`);
      }
      if (!r.display_name) {
        r.display_name = r.pattern;
      }
      r.parts_order = usedParts(r.pattern);
    }
    await saveRules(rules.value);
    status.value = "规则已保存";
    await reload();
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

async function previewExpand(probe: boolean) {
  const rule = selected.value;
  if (!rule) return;
  error.value = "";
  expandBusy.value = true;
  try {
    applyListEditors(rule);
    await saveRules(rules.value);
    expandPreview.value = await expandRuleTargets({
      rule_id: rule.id,
      probe,
      exclude: [],
    });
    status.value = probe
      ? `已展开并探测 ${expandPreview.value.length} 个目标`
      : `已展开 ${expandPreview.value.length} 个目标`;
  } catch (e) {
    error.value = String(e);
  } finally {
    expandBusy.value = false;
  }
}

function connName(id: string): string {
  return props.connections.find((c) => c.id === id)?.name ?? id;
}

onMounted(() => {
  void reload();
});
</script>

<template>
  <div class="pane-body">
    <div class="toolbar" style="margin: -12px -12px 12px; border-radius: 0">
      <button type="button" class="btn primary" @click="addRule">新建规则</button>
      <button
        type="button"
        class="btn danger"
        :disabled="!selected"
        @click="removeSelected"
      >
        删除当前
      </button>
      <button type="button" class="btn" @click="reload">重新加载</button>
      <button
        type="button"
        class="btn primary"
        :disabled="saving"
        @click="persist"
      >
        {{ saving ? "保存中…" : "保存全部" }}
      </button>
      <span class="spacer" />
      <span v-if="status" class="muted">{{ status }}</span>
    </div>

    <p v-if="error" class="error-text">{{ error }}</p>

    <div class="rules-layout">
      <div class="rules-list">
        <button
          v-for="rule in rules"
          :key="rule.id"
          type="button"
          :class="{ active: rule.id === selectedId }"
          @click="selectedId = rule.id"
        >
          <div>{{ ruleTitle(rule) }}</div>
          <div class="muted" style="font-size: 11px; font-family: var(--mono)">
            {{ rule.pattern || "（无模板）" }}
          </div>
        </button>
        <div v-if="rules.length === 0" class="muted" style="padding: 10px">
          暂无规则
        </div>
      </div>

      <div v-if="selected" class="rules-editor">
        <div class="form-grid" style="max-width: none">
          <label for="rule-display">显示名称</label>
          <input
            id="rule-display"
            v-model="selected.display_name"
            class="field"
            placeholder="如 乐米订单（按年）"
          />

          <label for="rule-pattern">库名模板</label>
          <div>
            <input
              id="rule-pattern"
              v-model="selected.pattern"
              class="field"
              style="font-family: var(--mono)"
              placeholder="如 order_{年份}_{租户}"
            />
            <p class="muted" style="margin: 6px 0 0; font-size: 12px">
              连接符直接写在模板里。可用占位符：
              <code>{年份}</code>、<code>{租户}</code>、<code>{分片}</code>
              （也支持 year / tenant / shard）
            </p>
            <p
              v-if="selected.pattern.trim()"
              class="muted"
              style="margin: 4px 0 0; font-size: 12px"
            >
              预览形态：
              <code>{{ selected.pattern.trim() }}</code>
            </p>
          </div>

          <template v-for="kind in selectedUsedParts" :key="kind">
            <label :for="`rule-${kind}`">{{ PART_META[kind].label }}</label>
            <div>
              <textarea
                :id="`rule-${kind}`"
                class="textarea"
                :placeholder="PART_META[kind].placeholder"
                :value="textForPart(kind)"
                @input="
                  setTextForPart(
                    kind,
                    ($event.target as HTMLTextAreaElement).value,
                  )
                "
              />
              <p class="muted" style="margin: 4px 0 0; font-size: 12px">
                {{ PART_META[kind].hint }}；每行一个，或逗号分隔
              </p>
            </div>
          </template>

          <template v-if="selectedUsedParts.length === 0">
            <label>取值</label>
            <p class="muted" style="margin: 0">
              当前模板没有占位符，将只生成一个固定库名。
            </p>
          </template>

          <label>绑定连接</label>
          <div class="checkbox-list">
            <label v-for="conn in connections" :key="conn.id">
              <input
                type="checkbox"
                :checked="selected.connection_ids.includes(conn.id)"
                @change="
                  toggleConnection(
                    conn.id,
                    ($event.target as HTMLInputElement).checked,
                  )
                "
              />
              <span>{{ conn.name }}</span>
            </label>
            <p v-if="connections.length === 0" class="muted">
              暂无连接，请先在左侧新建
            </p>
          </div>
        </div>

        <div
          class="toolbar"
          style="
            margin-top: 12px;
            border: none;
            background: transparent;
            padding: 0;
          "
        >
          <button
            type="button"
            class="btn"
            :disabled="expandBusy"
            @click="previewExpand(false)"
          >
            预览展开
          </button>
          <button
            type="button"
            class="btn"
            :disabled="expandBusy"
            @click="previewExpand(true)"
          >
            展开并探测存在性
          </button>
        </div>

        <div
          v-if="expandPreview.length"
          class="section-block"
          style="margin-top: 12px"
        >
          <h3 class="section-title">展开结果（{{ expandPreview.length }}）</h3>
          <table class="data-table">
            <thead>
              <tr>
                <th>连接</th>
                <th>数据库</th>
                <th>是否存在</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(t, i) in expandPreview" :key="i">
                <td>{{ connName(t.connection_id) }}</td>
                <td>
                  <code>{{ t.database }}</code>
                </td>
                <td>
                  <span v-if="t.exists === true" class="ok-text">存在</span>
                  <span v-else-if="t.exists === false" class="error-text"
                    >不存在</span
                  >
                  <span v-else class="muted">未探测</span>
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>

      <div v-else class="rules-editor placeholder-pane">
        请选择或新建一条规则
      </div>
    </div>
  </div>
</template>
