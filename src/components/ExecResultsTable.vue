<script setup lang="ts">
import type { ExecResult } from "../types";

defineProps<{
  results: ExecResult[];
  /** 可选：用当前连接列表补全显示名 */
  connName?: (id: string) => string;
}>();

function displayConn(r: ExecResult, connName?: (id: string) => string): string {
  if (r.connection_name?.trim()) return r.connection_name;
  if (r.connection_id && connName) {
    const n = connName(r.connection_id);
    if (n && n !== r.connection_id) return n;
  }
  return r.connection_id || "—";
}
</script>

<template>
  <table class="data-table">
    <thead>
      <tr>
        <th>状态</th>
        <th>连接</th>
        <th>数据库</th>
        <th>说明</th>
        <th>语句摘要</th>
        <th>错误</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="(r, i) in results" :key="i">
        <td>
          <span v-if="r.ok" class="ok-text">成功</span>
          <span v-else class="error-text">失败</span>
        </td>
        <td>{{ displayConn(r, connName) }}</td>
        <td>
          <code v-if="r.database">{{ r.database }}</code>
          <span v-else class="muted">—</span>
        </td>
        <td>{{ r.summary || "—" }}</td>
        <td>
          <pre v-if="r.sql_preview" class="sql-cell">{{ r.sql_preview }}</pre>
          <span v-else class="muted">—</span>
        </td>
        <td>
          <span v-if="r.error" class="error-text">{{ r.error }}</span>
          <span v-else class="muted">—</span>
        </td>
      </tr>
      <tr v-if="results.length === 0">
        <td colspan="6" class="muted">无结果项</td>
      </tr>
    </tbody>
  </table>
</template>
