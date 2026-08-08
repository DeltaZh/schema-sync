<script setup lang="ts">
import { onMounted, ref } from "vue";
import ExecResultsTable from "./ExecResultsTable.vue";
import { formatHistoryTime, kindLabel, riskLabel } from "../lib/labels";
import { listHistory } from "../lib/tauri";
import type { ConnectionConfig, HistoryRecord } from "../types";

const props = defineProps<{
  connections: ConnectionConfig[];
}>();

function connName(id: string): string {
  return props.connections.find((c) => c.id === id)?.name ?? id;
}

const records = ref<HistoryRecord[]>([]);
const expanded = ref<Set<string>>(new Set());
const loading = ref(false);
const error = ref("");
const status = ref("");

function toggle(id: string) {
  const next = new Set(expanded.value);
  if (next.has(id)) next.delete(id);
  else next.add(id);
  expanded.value = next;
}

function summary(r: HistoryRecord): string {
  const ok = r.results.filter((x) => x.ok).length;
  const fail = r.results.length - ok;
  return `${r.results.length} 项（成功 ${ok} / 失败 ${fail}）`;
}

async function reload() {
  loading.value = true;
  error.value = "";
  try {
    records.value = await listHistory(50);
    status.value = `最近 ${records.value.length} 条记录`;
  } catch (e) {
    error.value = String(e);
    status.value = "";
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void reload();
});
</script>

<template>
  <div class="pane-body">
    <div class="toolbar" style="margin: -12px -12px 12px; border-radius: 0">
      <button type="button" class="btn" :disabled="loading" @click="reload">
        {{ loading ? "加载中…" : "刷新" }}
      </button>
      <span class="spacer" />
      <span v-if="status" class="muted">{{ status }}</span>
    </div>

    <p v-if="error" class="error-text">{{ error }}</p>

    <div v-if="!loading && records.length === 0" class="placeholder-pane">
      暂无执行历史
    </div>

    <div v-else class="history-list">
      <div v-for="r in records" :key="r.id" class="history-card">
        <button
          type="button"
          class="history-head"
          @click="toggle(r.id)"
        >
          <span class="tree-toggle">{{
            expanded.has(r.id) ? "▼" : "▶"
          }}</span>
          <span class="history-meta">
            <strong>{{ formatHistoryTime(r.ts) }}</strong>
            <span class="muted">{{ summary(r) }}</span>
            <code class="muted">{{ r.scan_id }}</code>
            <span v-if="r.stop_on_error" class="chip">遇错即停</span>
          </span>
        </button>

        <div v-if="expanded.has(r.id)" class="history-body">
          <h4 class="subsection-title">执行结果</h4>
          <ExecResultsTable :results="r.results" :conn-name="connName" />

          <template v-if="r.item_snapshots.length">
            <h4 class="subsection-title">差异快照</h4>
            <table class="data-table">
              <thead>
                <tr>
                  <th>风险</th>
                  <th>类型</th>
                  <th>目标</th>
                  <th>说明</th>
                </tr>
              </thead>
              <tbody>
                <tr v-for="item in r.item_snapshots" :key="item.id">
                  <td>
                    <span class="risk-badge" :data-risk="item.risk">
                      {{ riskLabel(item.risk) }}
                    </span>
                  </td>
                  <td>{{ kindLabel(item.kind) }}</td>
                  <td>
                    <code
                      >{{ item.connection_id }}/{{ item.database }}.{{
                        item.table
                      }}</code
                    >
                  </td>
                  <td>
                    <div>{{ item.title }}</div>
                    <div class="muted">{{ item.detail }}</div>
                  </td>
                </tr>
              </tbody>
            </table>
          </template>
        </div>
      </div>
    </div>
  </div>
</template>
