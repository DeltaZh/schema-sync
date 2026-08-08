<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
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

const ALL_PARTS: { kind: PartKind; label: string }[] = [
  { kind: "tenant", label: "租户" },
  { kind: "year", label: "年份" },
  { kind: "shard", label: "分片" },
];

const rules = ref<NamingRule[]>([]);
const selectedId = ref<string | null>(null);
const status = ref("");
const error = ref("");
const saving = ref(false);
const expandPreview = ref<RuleTarget[]>([]);
const expandBusy = ref(false);

const selected = computed(() =>
  rules.value.find((r) => r.id === selectedId.value) ?? null,
);

const tenantsText = ref("");
const yearsText = ref("");
const shardsText = ref("");

function emptyRule(): NamingRule {
  return {
    id: newId("rule"),
    logical_name: "",
    parts_order: [],
    tenants: [],
    years: [],
    shards: [],
    connection_ids: [],
  };
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
  rule.logical_name = "new_db";
  rules.value.push(rule);
  selectedId.value = rule.id;
  status.value = "已新增规则（尚未保存）";
}

function removeSelected() {
  if (!selected.value) return;
  if (!confirm(`确定删除规则「${selected.value.logical_name || selected.value.id}」？`)) {
    return;
  }
  rules.value = rules.value.filter((r) => r.id !== selectedId.value);
  selectedId.value = rules.value[0]?.id ?? null;
  status.value = "已从列表移除（需点保存才落盘）";
}

function togglePart(kind: PartKind, enabled: boolean) {
  const rule = selected.value;
  if (!rule) return;
  if (enabled) {
    if (!rule.parts_order.includes(kind)) {
      rule.parts_order.push(kind);
    }
  } else {
    rule.parts_order = rule.parts_order.filter((p) => p !== kind);
  }
}

function movePart(kind: PartKind, delta: -1 | 1) {
  const rule = selected.value;
  if (!rule) return;
  const idx = rule.parts_order.indexOf(kind);
  if (idx < 0) return;
  const next = idx + delta;
  if (next < 0 || next >= rule.parts_order.length) return;
  const arr = [...rule.parts_order];
  const [item] = arr.splice(idx, 1);
  arr.splice(next, 0, item);
  rule.parts_order = arr;
}

function partLabel(kind: PartKind): string {
  return ALL_PARTS.find((p) => p.kind === kind)?.label ?? kind;
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
      if (!r.logical_name.trim()) {
        throw new Error(`规则 ${r.id} 缺少逻辑名`);
      }
      r.logical_name = r.logical_name.trim();
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
          <div>{{ rule.logical_name || "(未命名)" }}</div>
          <div class="muted" style="font-size: 11px">{{ rule.id }}</div>
        </button>
        <div v-if="rules.length === 0" class="muted" style="padding: 10px">
          暂无规则
        </div>
      </div>

      <div v-if="selected" class="rules-editor">
        <div class="form-grid" style="max-width: none">
          <label for="rule-logical">逻辑名</label>
          <input
            id="rule-logical"
            v-model="selected.logical_name"
            class="field"
            placeholder="如 order"
          />

          <label>部件顺序</label>
          <div class="parts-order">
            <p class="muted" style="margin: 0">
              勾选后参与库名拼接；可用上下调整顺序（逻辑名固定在前）
            </p>
            <label
              v-for="part in ALL_PARTS"
              :key="part.kind"
              class="part-row"
            >
              <input
                type="checkbox"
                :checked="selected.parts_order.includes(part.kind)"
                @change="
                  togglePart(
                    part.kind,
                    ($event.target as HTMLInputElement).checked,
                  )
                "
              />
              <span>{{ part.label }}（{{ part.kind }}）</span>
              <span class="part-actions">
                <button
                  type="button"
                  class="btn ghost"
                  :disabled="!selected.parts_order.includes(part.kind)"
                  @click.prevent="movePart(part.kind, -1)"
                >
                  上移
                </button>
                <button
                  type="button"
                  class="btn ghost"
                  :disabled="!selected.parts_order.includes(part.kind)"
                  @click.prevent="movePart(part.kind, 1)"
                >
                  下移
                </button>
              </span>
            </label>
            <div class="chip-row">
              <span class="muted">当前顺序：</span>
              <span class="chip">逻辑名</span>
              <span
                v-for="kind in selected.parts_order"
                :key="kind"
                class="chip"
              >
                {{ partLabel(kind) }}
              </span>
              <span v-if="selected.parts_order.length === 0" class="muted"
                >（仅逻辑名）</span
              >
            </div>
          </div>

          <label for="rule-tenants">租户列表</label>
          <textarea
            id="rule-tenants"
            v-model="tenantsText"
            class="textarea"
            placeholder="每行一个，或逗号分隔"
          />

          <label for="rule-years">年份列表</label>
          <textarea
            id="rule-years"
            v-model="yearsText"
            class="textarea"
            placeholder="如 2024&#10;2025"
          />

          <label for="rule-shards">分片列表</label>
          <textarea
            id="rule-shards"
            v-model="shardsText"
            class="textarea"
            placeholder="如 0&#10;1&#10;2"
          />

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
              <span class="muted">({{ conn.id }})</span>
            </label>
            <p v-if="connections.length === 0" class="muted">
              暂无连接，请先在左侧新建
            </p>
          </div>
        </div>

        <div class="toolbar" style="margin-top: 12px; border: none; background: transparent; padding: 0">
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

        <div v-if="expandPreview.length" class="section-block" style="margin-top: 12px">
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
                <td><code>{{ t.database }}</code></td>
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
