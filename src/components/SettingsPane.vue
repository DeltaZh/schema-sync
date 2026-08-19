<script setup lang="ts">
import { onMounted, ref } from "vue";
import {
  getDdlPolicy,
  resetDdlPolicy,
  saveDdlPolicy,
} from "../lib/tauri";
import type { DdlPolicyLevel, DdlPolicyRow } from "../types";

const rows = ref<DdlPolicyRow[]>([]);
const loading = ref(false);
const saving = ref(false);
const status = ref("");
const error = ref("");

const levelOptions: { value: DdlPolicyLevel; label: string }[] = [
  { value: "normal", label: "常规放行" },
  { value: "high", label: "高风险（需二次确认）" },
  { value: "forbidden", label: "不允许执行" },
];

async function load() {
  loading.value = true;
  error.value = "";
  try {
    rows.value = await getDdlPolicy();
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

async function save() {
  saving.value = true;
  error.value = "";
  status.value = "";
  try {
    rows.value = await saveDdlPolicy(rows.value);
    status.value = "已保存。下次 DDL 投放预览将按新策略校验。";
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

async function reset() {
  saving.value = true;
  error.value = "";
  status.value = "";
  try {
    rows.value = await resetDdlPolicy();
    status.value = "已恢复默认策略并保存。";
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}

onMounted(load);
</script>

<template>
  <div class="pane-body settings-pane">
    <header class="settings-header">
      <h2 class="settings-title">DDL 投放策略</h2>
      <p class="muted settings-desc">
        控制「DDL 投放」里哪些语句可执行、哪些算高风险、哪些直接禁止。策略只保存在本机，不影响基准同步。
        删除库等操作风险极高，请谨慎放开。
      </p>
    </header>

    <p v-if="loading" class="muted">加载中…</p>
    <p v-if="error" class="error-text">{{ error }}</p>
    <p v-if="status" class="ok-text">{{ status }}</p>

    <table v-if="rows.length" class="settings-table">
      <thead>
        <tr>
          <th>语句类型</th>
          <th>说明</th>
          <th>策略</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="row in rows" :key="row.kind">
          <td>{{ row.label }}</td>
          <td class="muted">{{ row.hint }}</td>
          <td>
            <select v-model="row.level" class="settings-select">
              <option
                v-for="opt in levelOptions"
                :key="opt.value"
                :value="opt.value"
              >
                {{ opt.label }}
              </option>
            </select>
          </td>
        </tr>
      </tbody>
    </table>

    <div class="settings-actions">
      <button type="button" class="btn primary" :disabled="saving || loading" @click="save">
        保存
      </button>
      <button type="button" class="btn ghost" :disabled="saving || loading" @click="reset">
        恢复默认
      </button>
    </div>
  </div>
</template>
