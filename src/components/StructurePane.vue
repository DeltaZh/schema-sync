<script setup lang="ts">
import { ref, watch } from "vue";
import { getTableStructure } from "../lib/tauri";
import type { TableSchema, TableSelection } from "../types";

const props = defineProps<{
  selection: TableSelection | null;
}>();

const schema = ref<TableSchema | null>(null);
const loading = ref(false);
const error = ref("");

async function load() {
  schema.value = null;
  error.value = "";
  if (!props.selection) return;
  loading.value = true;
  try {
    schema.value = await getTableStructure(
      props.selection.connectionId,
      props.selection.database,
      props.selection.table,
    );
    if (!schema.value) {
      error.value = "未找到该表结构";
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

watch(
  () => props.selection,
  () => {
    void load();
  },
  { immediate: true, deep: true },
);

function indexKind(ix: { primary: boolean; unique: boolean }): string {
  if (ix.primary) return "主键";
  if (ix.unique) return "唯一";
  return "普通";
}
</script>

<template>
  <div class="pane-body">
    <div v-if="!selection" class="placeholder-pane">
      请在左侧选择一张表以查看结构
    </div>

    <template v-else>
      <div class="section-block">
        <h2 class="section-title">
          {{ selection.connectionName }} /
          {{ selection.database }} /
          {{ selection.table }}
        </h2>
        <p v-if="selection.tableComment || schema?.comment" class="muted">
          表注释：{{ schema?.comment || selection.tableComment || "（无）" }}
        </p>
        <p v-if="loading" class="muted">正在加载结构…</p>
        <p v-if="error" class="error-text">{{ error }}</p>
      </div>

      <template v-if="schema">
        <div class="section-block">
          <h3 class="section-title">字段</h3>
          <table class="data-table">
            <thead>
              <tr>
                <th>字段名</th>
                <th>类型</th>
                <th>可空</th>
                <th>默认值</th>
                <th>额外</th>
                <th>注释</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="col in schema.columns" :key="col.name">
                <td>{{ col.name }}</td>
                <td><code>{{ col.col_type }}</code></td>
                <td>{{ col.nullable ? "是" : "否" }}</td>
                <td>{{ col.default ?? "—" }}</td>
                <td>{{ col.extra || "—" }}</td>
                <td>{{ col.comment || "—" }}</td>
              </tr>
            </tbody>
          </table>
        </div>

        <div class="section-block">
          <h3 class="section-title">索引</h3>
          <table v-if="schema.indexes.length" class="data-table">
            <thead>
              <tr>
                <th>索引名</th>
                <th>类型</th>
                <th>列</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="ix in schema.indexes" :key="ix.name">
                <td>{{ ix.name }}</td>
                <td>{{ indexKind(ix) }}</td>
                <td>{{ ix.columns.join(", ") }}</td>
              </tr>
            </tbody>
          </table>
          <p v-else class="muted">（无索引）</p>
        </div>

        <div v-if="schema.create_sql" class="section-block">
          <h3 class="section-title">建表语句</h3>
          <pre class="pre-box">{{ schema.create_sql }}</pre>
        </div>
      </template>
    </template>
  </div>
</template>
