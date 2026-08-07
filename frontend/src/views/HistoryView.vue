<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { api, type HistoryRecord } from '../api'

const records = ref<HistoryRecord[]>([])
const expanded = ref<Set<string>>(new Set())
const loading = ref(false)
const error = ref('')

async function load() {
  loading.value = true
  error.value = ''
  try {
    records.value = await api.listHistory(100)
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

function toggle(id: string) {
  const next = new Set(expanded.value)
  if (next.has(id)) next.delete(id)
  else next.add(id)
  expanded.value = next
}

function formatTs(ts: string) {
  try {
    return new Date(ts).toLocaleString()
  } catch {
    return ts
  }
}

function summary(r: HistoryRecord) {
  const ok = r.results.filter((x) => x.ok).length
  const fail = r.results.length - ok
  return `成功 ${ok} / 失败 ${fail} / 共 ${r.results.length}`
}

onMounted(load)
</script>

<template>
  <div>
    <h1 class="page-title">执行历史</h1>
    <p class="page-desc">查看近期同步执行记录与明细。</p>

    <div v-if="error" class="msg msg-error">{{ error }}</div>

    <div class="toolbar">
      <button type="button" class="btn" :disabled="loading" @click="load">
        {{ loading ? '加载中…' : '刷新' }}
      </button>
    </div>

    <p v-if="!loading && records.length === 0" class="empty">暂无历史记录。</p>

    <div v-for="r in records" :key="r.id" class="history-item">
      <button type="button" class="history-summary" @click="toggle(r.id)">
        <span class="mono">{{ formatTs(r.ts) }}</span>
        <span>表组 <strong>{{ r.group_id || '—' }}</strong></span>
        <span>
          模板 {{ r.template_instance_id || '—' }} /
          {{ r.template_database || '—' }}
        </span>
        <span>{{ summary(r) }}</span>
        <span class="muted">{{ expanded.has(r.id) ? '收起' : '展开' }}</span>
      </button>
      <div v-if="expanded.has(r.id)" class="history-detail">
        <p class="muted" style="margin-top: 0">
          记录 ID：<span class="mono">{{ r.id }}</span>
          · 遇错停止：{{ r.stop_on_error ? '是' : '否' }}
        </p>
        <h3 style="margin: 0.5rem 0; font-size: 0.95rem">执行结果</h3>
        <div class="table-wrap">
          <table class="data">
            <thead>
              <tr>
                <th>Diff ID</th>
                <th>状态</th>
                <th>错误</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="res in r.results" :key="res.diff_id">
                <td class="mono">{{ res.diff_id }}</td>
                <td>{{ res.ok ? '成功' : '失败' }}</td>
                <td>{{ res.error || '—' }}</td>
              </tr>
              <tr v-if="r.results.length === 0">
                <td colspan="3" class="muted">无结果</td>
              </tr>
            </tbody>
          </table>
        </div>
        <h3 style="margin: 0.85rem 0 0.5rem; font-size: 0.95rem">提交的差异快照</h3>
        <div v-if="r.item_snapshots.length === 0" class="muted">无快照</div>
        <div v-for="item in r.item_snapshots" :key="item.id" style="margin-bottom: 0.6rem">
          <div>
            <span :class="`badge badge-${item.risk}`">{{ item.risk }}</span>
            <strong style="margin-left: 0.35rem">{{ item.title }}</strong>
          </div>
          <div class="muted" style="font-size: 0.85rem">
            {{ item.instance_id }} / {{ item.database }} / {{ item.table }} · {{ item.kind }}
          </div>
          <pre class="mono">{{ item.sql }}</pre>
        </div>
      </div>
    </div>
  </div>
</template>
