<script setup lang="ts">
import { onMounted, reactive, ref } from 'vue'
import { api, type Instance, type InstanceWrite } from '../api'

const instances = ref<Instance[]>([])
const error = ref('')
const info = ref('')
const loading = ref(false)
const editingId = ref<string | null>(null)

const form = reactive({
  id: '',
  host: '',
  port: 3306,
  user: '',
  password: '',
  remark: '',
  enabled: true,
})

function resetForm() {
  editingId.value = null
  form.id = ''
  form.host = ''
  form.port = 3306
  form.user = ''
  form.password = ''
  form.remark = ''
  form.enabled = true
}

function startEdit(inst: Instance) {
  editingId.value = inst.id
  form.id = inst.id
  form.host = inst.host
  form.port = inst.port
  form.user = inst.user
  form.password = ''
  form.remark = inst.remark
  form.enabled = inst.enabled
  info.value = '编辑中：密码留空表示保留原密码'
  error.value = ''
}

async function load() {
  loading.value = true
  error.value = ''
  try {
    instances.value = await api.listInstances()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  } finally {
    loading.value = false
  }
}

function toWrite(): InstanceWrite {
  const body: InstanceWrite = {
    id: form.id.trim(),
    host: form.host.trim(),
    port: Number(form.port) || 3306,
    user: form.user.trim(),
    enabled: form.enabled,
    remark: form.remark.trim(),
  }
  if (editingId.value) {
    // 编辑：仅当用户填写了密码才发送；空字符串表示不改 → 传 null / 省略
    if (form.password !== '') {
      body.password = form.password
    } else {
      body.password = null
    }
  } else {
    body.password = form.password
  }
  return body
}

async function save() {
  error.value = ''
  info.value = ''
  if (!form.id.trim() || !form.host.trim() || !form.user.trim()) {
    error.value = '请填写实例 ID、主机与用户名'
    return
  }
  try {
    if (editingId.value) {
      await api.updateInstance(editingId.value, toWrite())
      info.value = '已更新实例'
    } else {
      await api.createInstance(toWrite())
      info.value = '已创建实例'
    }
    resetForm()
    await load()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function remove(id: string) {
  if (!confirm(`确认删除实例「${id}」？`)) return
  error.value = ''
  info.value = ''
  try {
    await api.deleteInstance(id)
    if (editingId.value === id) resetForm()
    info.value = '已删除'
    await load()
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e)
  }
}

async function ping(id: string) {
  error.value = ''
  info.value = ''
  try {
    await api.pingInstance(id)
    info.value = `实例「${id}」连通正常`
  } catch (e) {
    error.value = `测连通失败：${e instanceof Error ? e.message : String(e)}`
  }
}

onMounted(load)
</script>

<template>
  <div>
    <h1 class="page-title">连接实例</h1>
    <p class="page-desc">管理 MySQL 连接；密码加密存本地，列表永不展示明文。</p>

    <div v-if="error" class="msg msg-error">{{ error }}</div>
    <div v-if="info" class="msg msg-ok">{{ info }}</div>

    <section class="card">
      <h2 style="margin: 0 0 0.75rem; font-size: 1rem">
        {{ editingId ? `编辑：${editingId}` : '新建实例' }}
      </h2>
      <div class="form-grid">
        <div class="field">
          <label>实例 ID</label>
          <input v-model="form.id" :disabled="!!editingId" placeholder="如 prod-a" />
        </div>
        <div class="field">
          <label>主机</label>
          <input v-model="form.host" placeholder="127.0.0.1" />
        </div>
        <div class="field">
          <label>端口</label>
          <input v-model.number="form.port" type="number" min="1" max="65535" />
        </div>
        <div class="field">
          <label>用户名</label>
          <input v-model="form.user" />
        </div>
        <div class="field">
          <label>{{ editingId ? '密码（留空不改）' : '密码' }}</label>
          <input v-model="form.password" type="password" autocomplete="new-password" />
        </div>
        <div class="field">
          <label>备注</label>
          <input v-model="form.remark" />
        </div>
        <div class="field">
          <label>启用</label>
          <label style="flex-direction: row; align-items: center; gap: 0.4rem">
            <input v-model="form.enabled" type="checkbox" />
            {{ form.enabled ? '已启用' : '已禁用' }}
          </label>
        </div>
      </div>
      <div class="toolbar" style="margin-top: 0.85rem; margin-bottom: 0">
        <button type="button" class="btn btn-primary" @click="save">
          {{ editingId ? '保存修改' : '创建' }}
        </button>
        <button v-if="editingId" type="button" class="btn" @click="resetForm">取消编辑</button>
        <button type="button" class="btn" :disabled="loading" @click="load">刷新列表</button>
      </div>
    </section>

    <section class="card">
      <div class="table-wrap">
        <table class="data">
          <thead>
            <tr>
              <th>ID</th>
              <th>主机</th>
              <th>用户</th>
              <th>密码</th>
              <th>启用</th>
              <th>备注</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="inst in instances" :key="inst.id">
              <td class="mono">{{ inst.id }}</td>
              <td>{{ inst.host }}:{{ inst.port }}</td>
              <td>{{ inst.user }}</td>
              <td>{{ inst.has_password ? '已设置' : '未设置' }}</td>
              <td>{{ inst.enabled ? '是' : '否' }}</td>
              <td>{{ inst.remark || '—' }}</td>
              <td>
                <div class="toolbar" style="margin: 0">
                  <button type="button" class="btn btn-sm" @click="startEdit(inst)">编辑</button>
                  <button type="button" class="btn btn-sm" @click="ping(inst.id)">测连通</button>
                  <button type="button" class="btn btn-sm btn-danger" @click="remove(inst.id)">
                    删除
                  </button>
                </div>
              </td>
            </tr>
          </tbody>
        </table>
        <p v-if="!loading && instances.length === 0" class="empty">暂无实例，请先创建。</p>
      </div>
    </section>
  </div>
</template>
