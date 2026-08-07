<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  api,
  type DiffItem,
  type ExecResult,
  type Instance,
  type ScanError,
  type TableGroup,
} from '../api'

const groups = ref<TableGroup[]>([])
const instances = ref<Instance[]>([])
const groupId = ref('')
const templateInstanceId = ref('')
const templateDatabase = ref('')
const databases = ref<string[]>([])
const items = ref<DiffItem[]>([])
const scanErrors = ref<ScanError[]>([])
const selected = ref<Set<string>>(new Set())
const stopOnError = ref(true)
const loading = ref(false)
const scanning = ref(false)
const executing = ref(false)
const error = ref('')
const info = ref('')
const execResults = ref<ExecResult[] | null>(null)
const showConfirm = ref(false)

const selectedGroup = computed(() => groups.value.find((g) => g.id === groupId.value))

const templateInstances = computed(() => {
  const g = selectedGroup.value
  if (!g) return instances.value.filter((i) => i.enabled)
  const set = new Set(g.instance_ids)
  return instances.value.filter((i) => i.enabled && (set.size === 0 || set.has(i.id)))
})

const grouped = computed(() => {
  const map = new Map<string, DiffItem[]>()
  for (const item of items.value) {
    const key = `${item.instance_id} / ${item.database}`
    const list = map.get(key) ?? []
    list.push(item)
    map.set(key, list)
  }
  return [...map.entries()].map(([key, list]) => ({ key, list }))
})

const selectedCount = computed(() => selected.value.size)

const selectedItems = computed(() =>
  items.value.filter((i) => selected.value.has(i.id)),
)

const riskSummary = computed(() => {
  const counts = { safe: 0, caution: 0, dangerous: 0 }
  for (const item of selectedItems.value) {
    counts[item.risk] += 1
  }
  return counts
})

function riskClass(risk: string) {
  return `badge badge-${risk}`
}

function riskLabel(risk: string) {
  if (risk === 'safe') return '安全'
  if (risk === 'caution') return '谨慎'
  return '危险'
}

async function loadMeta() {
  loading.value = true
  error.value = ''
  try {
    const [g, i] = await Promise.all([api.listTableGroups(), api.listInstances()])
    groups.value = g
    instances.value = i
    if (!groupId.value && g.length) groupId.value = g[0].id
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function loadDatabases() {
  databases.value = []
  templateDatabase.value = ''
  if (!groupId.value || !templateInstanceId.value) return
  error.value = ''
  try {
    databases.value = await api.listMatchedDatabases(
      groupId.value,
      templateInstanceId.value,
    )
    if (databases.value.length === 1) {
      templateDatabase.value = databases.value[0]
    }
  } catch (e) {
    error.value = `加载模板库失败：${e instanceof Error ? e.message : String(e)}`
  }
}

watch(groupId, () => {
  items.value = []
  scanErrors.value = []
  selected.value = new Set()
  execResults.value = null
  const opts = templateInstances.value
  if (!opts.some((i) => i.id === templateInstanceId.value)) {
    templateInstanceId.value = opts[0]?.id ?? ''
  }
  void loadDatabases()
})

watch(templateInstanceId, () => {
  void loadDatabases()
})

function applyDefaultSelection(list: DiffItem[]) {
  selected.value = new Set(list.filter((i) => i.selected_default).map((i) => i.id))
}

function toggle(id: string, checked: boolean) {
  const next = new Set(selected.value)
  if (checked) next.add(id)
  else next.delete(id)
  selected.value = next
}

function selectAllSafe() {
  const next = new Set(selected.value)
  for (const item of items.value) {
    if (item.risk === 'safe') next.add(item.id)
  }
  selected.value = next
  info.value = `已勾选全部安全项（当前共 ${selected.value.size} 项）`
}

function selectGroupAll(list: DiffItem[], checked: boolean) {
  const next = new Set(selected.value)
  for (const item of list) {
    if (checked) next.add(item.id)
    else next.delete(item.id)
  }
  selected.value = next
}

async function scan() {
  error.value = ''
  info.value = ''
  execResults.value = null
  if (!groupId.value || !templateInstanceId.value || !templateDatabase.value) {
    error.value = '请选择表组、模板实例与模板库'
    return
  }
  scanning.value = true
  try {
    const result = await api.scan({
      group_id: groupId.value,
      template_instance_id: templateInstanceId.value,
      template_database: templateDatabase.value,
    })
    items.value = result.items
    scanErrors.value = result.errors
    applyDefaultSelection(result.items)
    info.value = `扫描完成：${result.items.length} 条差异，${result.errors.length} 条错误`
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    scanning.value = false
  }
}

function openConfirm() {
  error.value = ''
  info.value = ''
  if (selectedCount.value === 0) {
    error.value = '请先勾选要执行的差异项'
    return
  }
  showConfirm.value = true
}

async function execute() {
  showConfirm.value = false
  executing.value = true
  error.value = ''
  info.value = ''
  try {
    const results = await api.execute({
      items: selectedItems.value,
      item_ids: selectedItems.value.map((i) => i.id),
      stop_on_error: stopOnError.value,
      group_id: groupId.value,
      template_instance_id: templateInstanceId.value,
      template_database: templateDatabase.value,
    })
    execResults.value = results
    const ok = results.filter((r) => r.ok).length
    const fail = results.length - ok
    info.value = `执行完成：成功 ${ok}，失败 ${fail}`
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    executing.value = false
  }
}

onMounted(async () => {
  await loadMeta()
  if (templateInstances.value.length && !templateInstanceId.value) {
    templateInstanceId.value = templateInstances.value[0].id
  }
})
</script>

<template>
  <div>
    <h1 class="page-title">同步工作台</h1>
    <p class="page-desc">
      选择表组与模板库，扫描差异后勾选执行。执行会携带完整 DiffItem（含 SQL）。
    </p>

    <div v-if="error" class="msg msg-error">{{ error }}</div>
    <div v-if="info" class="msg msg-ok">{{ info }}</div>

    <section class="card">
      <div class="form-grid">
        <div class="field">
          <label>表组</label>
          <select v-model="groupId" :disabled="loading">
            <option value="" disabled>请选择</option>
            <option v-for="g in groups" :key="g.id" :value="g.id">{{ g.id }}</option>
          </select>
        </div>
        <div class="field">
          <label>模板实例</label>
          <select v-model="templateInstanceId" :disabled="!groupId">
            <option value="" disabled>请选择</option>
            <option v-for="i in templateInstances" :key="i.id" :value="i.id">
              {{ i.id }}
            </option>
          </select>
        </div>
        <div class="field">
          <label>模板库</label>
          <select v-model="templateDatabase" :disabled="!templateInstanceId">
            <option value="" disabled>请选择</option>
            <option v-for="db in databases" :key="db" :value="db">{{ db }}</option>
          </select>
        </div>
        <div class="field">
          <label>遇错停止</label>
          <label style="flex-direction: row; align-items: center; gap: 0.4rem">
            <input v-model="stopOnError" type="checkbox" />
            {{ stopOnError ? '是' : '否' }}
          </label>
        </div>
      </div>
      <div class="toolbar" style="margin-top: 0.85rem; margin-bottom: 0">
        <button
          type="button"
          class="btn btn-primary"
          :disabled="scanning || !groupId || !templateInstanceId || !templateDatabase"
          @click="scan"
        >
          {{ scanning ? '扫描中…' : '扫描差异' }}
        </button>
        <button type="button" class="btn" :disabled="!items.length" @click="selectAllSafe">
          全选安全项
        </button>
        <button
          type="button"
          class="btn btn-primary"
          :disabled="executing || selectedCount === 0"
          @click="openConfirm"
        >
          {{ executing ? '执行中…' : `确认执行（${selectedCount}）` }}
        </button>
        <button type="button" class="btn" :disabled="loading" @click="loadMeta">刷新配置</button>
      </div>
    </section>

    <section v-if="scanErrors.length" class="card">
      <h2 style="margin: 0 0 0.5rem; font-size: 1rem">扫描错误</h2>
      <ul>
        <li v-for="(err, idx) in scanErrors" :key="idx" class="muted">
          {{ err.instance_id }}
          <template v-if="err.database"> / {{ err.database }}</template>
          — {{ err.message }}
        </li>
      </ul>
    </section>

    <section class="card">
      <h2 style="margin: 0 0 0.75rem; font-size: 1rem">
        差异列表
        <span class="muted" style="font-weight: 400">（按库分组）</span>
      </h2>
      <p v-if="!items.length" class="empty">尚未扫描，或无差异。</p>
      <div v-for="group in grouped" :key="group.key" class="db-group">
        <div class="db-group-head">
          <span>{{ group.key }}（{{ group.list.length }}）</span>
          <span>
            <button
              type="button"
              class="btn btn-sm"
              @click="selectGroupAll(group.list, true)"
            >
              全选本组
            </button>
            <button
              type="button"
              class="btn btn-sm"
              @click="selectGroupAll(group.list, false)"
            >
              清空本组
            </button>
          </span>
        </div>
        <div class="db-group-body">
          <div v-for="item in group.list" :key="item.id" class="diff-row">
            <input
              type="checkbox"
              :checked="selected.has(item.id)"
              @change="toggle(item.id, ($event.target as HTMLInputElement).checked)"
            />
            <div>
              <div>
                <span :class="riskClass(item.risk)">{{ riskLabel(item.risk) }}</span>
                <strong style="margin-left: 0.4rem">{{ item.title }}</strong>
              </div>
              <div class="muted" style="font-size: 0.85rem">
                {{ item.kind }} · {{ item.table }}
              </div>
              <pre class="mono">{{ item.sql }}</pre>
            </div>
            <span class="muted mono" style="font-size: 0.75rem">{{ item.id }}</span>
          </div>
        </div>
      </div>
    </section>

    <section v-if="execResults" class="card">
      <h2 style="margin: 0 0 0.5rem; font-size: 1rem">执行结果</h2>
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
            <tr v-for="r in execResults" :key="r.diff_id">
              <td class="mono">{{ r.diff_id }}</td>
              <td>{{ r.ok ? '成功' : '失败' }}</td>
              <td>{{ r.error || '—' }}</td>
            </tr>
          </tbody>
        </table>
      </div>
    </section>

    <div v-if="showConfirm" class="dialog-backdrop" @click.self="showConfirm = false">
      <div class="dialog" role="dialog" aria-modal="true">
        <h3>确认执行同步</h3>
        <p>
          即将对 <strong>{{ selectedCount }}</strong> 项差异执行 SQL。
          安全 {{ riskSummary.safe }} / 谨慎 {{ riskSummary.caution }} / 危险
          {{ riskSummary.dangerous }}。
          遇错停止：{{ stopOnError ? '是' : '否' }}。
        </p>
        <p v-if="riskSummary.dangerous > 0" style="color: var(--dangerous)">
          包含危险操作（如删列/删索引），请仔细核对。
        </p>
        <div class="dialog-actions">
          <button type="button" class="btn" @click="showConfirm = false">取消</button>
          <button type="button" class="btn btn-primary" @click="execute">确认执行</button>
        </div>
      </div>
    </div>
  </div>
</template>
