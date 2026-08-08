<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  categoryLabel,
  categoryOfKind,
  kindLabel,
  objectFacetLabel,
  objectFacetOfKind,
  riskLabel,
  type DiffCategory,
  type ObjectFacet,
} from "../lib/labels";
import type { DiffItem, Risk } from "../types";

const props = defineProps<{
  items: DiffItem[];
  selectedIds: Set<string>;
  connName: (id: string) => string;
}>();

const emit = defineEmits<{
  toggleItem: [id: string, checked: boolean];
  selectAll: [checked: boolean];
  selectDefault: [];
}>();

type CategoryFilter = "all" | DiffCategory;
type FacetFilter = "all" | ObjectFacet;
type RiskFilter = "all" | Risk;

const categoryFilter = ref<CategoryFilter>("all");
const facetFilter = ref<FacetFilter>("all");
const riskFilter = ref<RiskFilter>("all");
const tableFilter = ref("");
const dbFilter = ref("");
const activeId = ref("");

const categoryCounts = computed(() => {
  const counts = { add: 0, modify: 0, delete: 0 };
  for (const i of props.items) {
    counts[categoryOfKind(i.kind)] += 1;
  }
  return counts;
});

const tableOptions = computed(() =>
  [...new Set(props.items.map((i) => i.table))].sort(),
);

const dbOptions = computed(() =>
  [...new Set(props.items.map((i) => i.database))].sort(),
);

const filteredItems = computed(() => {
  return props.items.filter((i) => {
    if (
      categoryFilter.value !== "all" &&
      categoryOfKind(i.kind) !== categoryFilter.value
    ) {
      return false;
    }
    if (
      facetFilter.value !== "all" &&
      objectFacetOfKind(i.kind) !== facetFilter.value
    ) {
      return false;
    }
    if (riskFilter.value !== "all" && i.risk !== riskFilter.value) {
      return false;
    }
    if (tableFilter.value && i.table !== tableFilter.value) return false;
    if (dbFilter.value && i.database !== dbFilter.value) return false;
    return true;
  });
});

const activeItem = computed(
  () =>
    filteredItems.value.find((i) => i.id === activeId.value) ??
    filteredItems.value[0] ??
    null,
);

const selectedSqlBundle = computed(() => {
  const selected = props.items.filter((i) => props.selectedIds.has(i.id));
  if (selected.length === 0) return "";
  return selected
    .map(
      (i) =>
        `-- ${i.title} @ ${props.connName(i.connection_id)} / ${i.database}\n${i.sql}`,
    )
    .join("\n\n");
});

watch(
  filteredItems,
  (list) => {
    if (!list.some((i) => i.id === activeId.value)) {
      activeId.value = list[0]?.id ?? "";
    }
  },
  { immediate: true },
);

function selectCategory(cat: CategoryFilter) {
  categoryFilter.value = cat;
}

function toggleVisible(checked: boolean) {
  for (const i of filteredItems.value) {
    emit("toggleItem", i.id, checked);
  }
}
</script>

<template>
  <div class="diff-review">
    <div class="diff-toolbar">
      <div class="chip-row">
        <button
          type="button"
          class="chip-btn"
          :class="{ active: categoryFilter === 'all' }"
          @click="selectCategory('all')"
        >
          全部 {{ items.length }}
        </button>
        <button
          type="button"
          class="chip-btn"
          :class="{ active: categoryFilter === 'add' }"
          @click="selectCategory('add')"
        >
          {{ categoryLabel("add") }} {{ categoryCounts.add }}
        </button>
        <button
          type="button"
          class="chip-btn"
          :class="{ active: categoryFilter === 'modify' }"
          @click="selectCategory('modify')"
        >
          {{ categoryLabel("modify") }} {{ categoryCounts.modify }}
        </button>
        <button
          type="button"
          class="chip-btn"
          :class="{ active: categoryFilter === 'delete' }"
          @click="selectCategory('delete')"
        >
          {{ categoryLabel("delete") }} {{ categoryCounts.delete }}
        </button>
      </div>

      <div class="chip-row" style="margin-top: 8px">
        <label class="filter-inline">
          对象
          <select v-model="facetFilter" class="field field-sm">
            <option value="all">全部</option>
            <option value="table">{{ objectFacetLabel("table") }}</option>
            <option value="column">{{ objectFacetLabel("column") }}</option>
            <option value="index">{{ objectFacetLabel("index") }}</option>
            <option value="comment">{{ objectFacetLabel("comment") }}</option>
          </select>
        </label>
        <label class="filter-inline">
          风险
          <select v-model="riskFilter" class="field field-sm">
            <option value="all">全部</option>
            <option value="safe">{{ riskLabel("safe") }}</option>
            <option value="caution">{{ riskLabel("caution") }}</option>
            <option value="dangerous">{{ riskLabel("dangerous") }}</option>
          </select>
        </label>
        <label class="filter-inline">
          表
          <select v-model="tableFilter" class="field field-sm">
            <option value="">全部</option>
            <option v-for="t in tableOptions" :key="t" :value="t">{{ t }}</option>
          </select>
        </label>
        <label class="filter-inline">
          目标库
          <select v-model="dbFilter" class="field field-sm">
            <option value="">全部</option>
            <option v-for="d in dbOptions" :key="d" :value="d">{{ d }}</option>
          </select>
        </label>
      </div>

      <div class="toolbar" style="border: none; background: transparent; padding: 8px 0 0">
        <button type="button" class="btn ghost" @click="emit('selectDefault')">
          恢复默认勾选
        </button>
        <button type="button" class="btn ghost" @click="toggleVisible(true)">
          勾选当前列表
        </button>
        <button type="button" class="btn ghost" @click="toggleVisible(false)">
          取消当前列表
        </button>
        <button type="button" class="btn ghost" @click="emit('selectAll', true)">
          全选全部
        </button>
        <span class="muted">已选 {{ selectedIds.size }} / 共 {{ items.length }}</span>
      </div>
    </div>

    <div class="diff-main">
      <div class="diff-list">
        <button
          v-for="item in filteredItems"
          :key="item.id"
          type="button"
          class="diff-list-item"
          :class="{ active: activeItem?.id === item.id }"
          @click="activeId = item.id"
        >
          <input
            type="checkbox"
            :checked="selectedIds.has(item.id)"
            @click.stop
            @change="
              emit(
                'toggleItem',
                item.id,
                ($event.target as HTMLInputElement).checked,
              )
            "
          />
          <div class="diff-list-body">
            <div class="diff-list-title">
              <span class="risk-badge" :data-risk="item.risk">{{
                riskLabel(item.risk)
              }}</span>
              <span>{{ kindLabel(item.kind) }}</span>
            </div>
            <div class="muted">
              {{ item.table }}
              <template v-if="item.object_name">
                · {{ item.object_name }}
              </template>
            </div>
            <div class="muted" style="font-size: 11px">
              {{ connName(item.connection_id) }} / {{ item.database }}
            </div>
          </div>
        </button>
        <div v-if="filteredItems.length === 0" class="muted" style="padding: 12px">
          当前筛选下没有差异
        </div>
      </div>

      <div v-if="activeItem" class="diff-detail">
        <div class="compare-grid">
          <div class="compare-pane">
            <div class="compare-head">基准（期望）</div>
            <pre class="compare-body">{{
              activeItem.baseline_view || activeItem.detail || "（无）"
            }}</pre>
          </div>
          <div class="compare-pane">
            <div class="compare-head">
              目标（现状）
              <span class="muted" style="font-weight: 400">
                — {{ connName(activeItem.connection_id) }} /
                {{ activeItem.database }}
              </span>
            </div>
            <pre class="compare-body">{{
              activeItem.target_view || "（无）"
            }}</pre>
          </div>
        </div>
        <div class="ddl-pane">
          <div class="compare-head">
            将要执行的语句
            <span class="muted" style="font-weight: 400">（当前选中项）</span>
          </div>
          <pre class="compare-body ddl-current">{{ activeItem.sql }}</pre>
        </div>
      </div>
    </div>

    <div class="ddl-bundle">
      <div class="compare-head">
        已勾选将执行的 DDL（{{ selectedIds.size }}）
      </div>
      <pre class="compare-body">{{
        selectedSqlBundle || "（尚未勾选差异）"
      }}</pre>
    </div>
  </div>
</template>

<style scoped>
.diff-review {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
}

.diff-main {
  display: grid;
  grid-template-columns: minmax(220px, 280px) 1fr;
  gap: 10px;
  min-height: 280px;
}

.diff-list {
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: auto;
  max-height: 420px;
  background: var(--bg);
}

.diff-list-item {
  display: flex;
  gap: 8px;
  width: 100%;
  text-align: left;
  border: none;
  border-bottom: 1px solid var(--border);
  background: transparent;
  padding: 8px 10px;
  color: inherit;
  align-items: flex-start;
}

.diff-list-item:hover {
  background: var(--bg-hover);
}

.diff-list-item.active {
  background: var(--bg-active);
}

.diff-list-title {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
  font-weight: 600;
  font-size: 12px;
}

.diff-detail {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
}

.compare-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  min-height: 160px;
}

.compare-pane,
.ddl-pane,
.ddl-bundle {
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
  background: var(--bg-panel);
  min-width: 0;
}

.compare-head {
  padding: 6px 10px;
  font-size: 12px;
  font-weight: 600;
  background: var(--bg-muted);
  border-bottom: 1px solid var(--border);
}

.compare-body {
  margin: 0;
  padding: 10px;
  font-family: var(--mono);
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 220px;
  overflow: auto;
}

.ddl-current {
  max-height: 120px;
}

.ddl-bundle .compare-body {
  max-height: 180px;
}

.chip-btn {
  border: 1px solid var(--border);
  background: var(--bg-panel);
  border-radius: 999px;
  padding: 4px 10px;
  color: var(--text-secondary);
}

.chip-btn.active {
  border-color: var(--accent);
  color: var(--accent);
  background: color-mix(in srgb, var(--accent) 12%, var(--bg-panel));
  font-weight: 600;
}

.filter-inline {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--text-secondary);
  font-size: 12px;
}

.field-sm {
  width: auto;
  min-width: 96px;
  padding: 3px 6px;
}

@media (max-width: 900px) {
  .diff-main,
  .compare-grid {
    grid-template-columns: 1fr;
  }
}
</style>
