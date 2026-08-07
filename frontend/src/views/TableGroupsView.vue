<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue'
import { api, type Instance, type TableGroup } from '../api'

const groups = ref<TableGroup[]>([])
const instances = ref<Instance[]>([])
const error = ref('')
const info = ref('')
const loading = ref(false)
const editingIndex = ref<number | null>(null)

const form = reactive({
  id: '',
  database_pattern: '',
  tablesText: '',
  instance_ids: [] as string[],
})

const enabledInstances = computed(() => instances.value.filter((i) => i.enabled))

function parseTables(text: string): string[] {
  return text
    .split(/[,，\s\n]+/)
    .map((s) => s.trim())
    .filter(Boolean)
}

function resetForm() {
  editingIndex.value = null
  form.id = ''
  form.database_pattern = ''
  form.tablesText = ''
  form.instance_ids = []
}

function startEdit(index: number) {
  const g = groups.value[index]
  if (!g) return
  editingIndex.value = index
  form.id = g.id
  form.database_pattern = g.database_pattern
  form.tablesText = g.tables.join(', ')
  form.instance_ids = [...g.instance_ids]
  error.value = ''
  info.value = ''
}

function toggleInstance(id: string, checked: boolean) {
  if (checked) {
    if (!form.instance_ids.includes(id)) form.instance_ids.push(id)
  } else {
    form.instance_ids = form.instance_ids.filter((x) => x !== id)
  }
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    const [g, i] = await Promise.all([api.listTableGroups(), api.listInstances()])
    groups.value = g
    instances.value = i
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

async function persist(next: TableGroup[]) {
  groups.value = await api.saveTableGroups(next)
}

async function save() {
  error.value = ''
  info.value = ''
  const id = form.id.trim()
  if (!id || !form.database_pattern.trim()) {
    error.value = '请填写表组 ID 与库名 pattern'
    return
  }
  const item: TableGroup = {
    id,
    database_pattern: form.database_pattern.trim(),
    tables: parseTables(form.tablesText),
    instance_ids: [...form.instance_ids],
  }
  try {
    const next = [...groups.value]
    if (editingIndex.value != null) {
      next[editingIndex.value] = item
    } else {
      if (next.some((g) => g.id === id)) {
        error.value = `表组已存在：${id}`
        return
      }
      next.push(item)
    }
    await persist(next)
    info.value = editingIndex.value != null ? '已更新表组' : '已添加表组'
    resetForm()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function remove(index: number) {
  const g = groups.value[index]
  if (!g) return
  if (!confirm(`确认删除表组「${g.id}」？`)) return
  error.value = ''
  info.value = ''
  try {
    const next = groups.value.filter((_, i) => i !== index)
    await persist(next)
    if (editingIndex.value === index) resetForm()
    else if (editingIndex.value != null && editingIndex.value > index) {
      editingIndex.value -= 1
    }
    info.value = '已删除'
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

onMounted(load)
</script>

<template>
  <div>
    <h1 class="page-title">表组配置</h1>
    <p class="page-desc">
      配置库名 pattern、同步表清单与参与实例。pattern 支持通配（如 shop_*）。
    </p>

    <div v-if="error" class="msg msg-error">{{ error }}</div>
    <div v-if="info" class="msg msg-ok">{{ info }}</div>

    <section class="card">
      <h2 style="margin: 0 0 0.75rem; font-size: 1rem">
        {{ editingIndex != null ? `编辑：${form.id}` : '新建表组' }}
      </h2>
      <div class="form-grid">
        <div class="field">
          <label>表组 ID</label>
          <input v-model="form.id" :disabled="editingIndex != null" placeholder="如 order-core" />
        </div>
        <div class="field span-2">
          <label>库名 pattern</label>
          <input v-model="form.database_pattern" placeholder="shop_*" />
        </div>
        <div class="field span-2">
          <label>表名（逗号或空格分隔）</label>
          <textarea
            v-model="form.tablesText"
            placeholder="orders, order_items, payments"
          />
        </div>
        <div class="field span-2">
          <label>参与实例（多选）</label>
          <div class="check-list">
            <label v-for="inst in enabledInstances" :key="inst.id">
              <input
                type="checkbox"
                :checked="form.instance_ids.includes(inst.id)"
                @change="
                  toggleInstance(
                    inst.id,
                    ($event.target as HTMLInputElement).checked,
                  )
                "
              />
              <span>
                <strong>{{ inst.id }}</strong>
                <span class="muted"> — {{ inst.host }}:{{ inst.port }}</span>
              </span>
            </label>
            <p v-if="enabledInstances.length === 0" class="muted">
              暂无已启用实例，请先在「连接实例」中配置。
            </p>
          </div>
        </div>
      </div>
      <div class="toolbar" style="margin-top: 0.85rem; margin-bottom: 0">
        <button type="button" class="btn btn-primary" @click="save">
          {{ editingIndex != null ? '保存修改' : '添加' }}
        </button>
        <button v-if="editingIndex != null" type="button" class="btn" @click="resetForm">
          取消编辑
        </button>
        <button type="button" class="btn" :disabled="loading" @click="load">刷新</button>
      </div>
    </section>

    <section class="card">
      <div class="table-wrap">
        <table class="data">
          <thead>
            <tr>
              <th>ID</th>
              <th>Pattern</th>
              <th>表</th>
              <th>实例</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(g, idx) in groups" :key="g.id">
              <td class="mono">{{ g.id }}</td>
              <td class="mono">{{ g.database_pattern }}</td>
              <td>{{ g.tables.join(', ') || '—' }}</td>
              <td>{{ g.instance_ids.join(', ') || '—' }}</td>
              <td>
                <div class="toolbar" style="margin: 0">
                  <button type="button" class="btn btn-sm" @click="startEdit(idx)">编辑</button>
                  <button type="button" class="btn btn-sm btn-danger" @click="remove(idx)">
                    删除
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="!loading && groups.length === 0" class="empty">暂无表组。</p>
      </div>
    </section>
  </div>
</template>
