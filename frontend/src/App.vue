<script setup lang="ts">
import { ref } from 'vue'
import InstancesView from './views/InstancesView.vue'
import TableGroupsView from './views/TableGroupsView.vue'
import SyncWorkbenchView from './views/SyncWorkbenchView.vue'
import HistoryView from './views/HistoryView.vue'

type Page = 'instances' | 'groups' | 'sync' | 'history'

const page = ref<Page>('instances')

const nav: { id: Page; label: string }[] = [
  { id: 'instances', label: '连接实例' },
  { id: 'groups', label: '表组配置' },
  { id: 'sync', label: '同步工作台' },
  { id: 'history', label: '执行历史' },
]
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">Schema Sync</div>
      <nav class="nav">
        <button
          v-for="item in nav"
          :key="item.id"
          type="button"
          :class="{ active: page === item.id }"
          @click="page = item.id"
        >
          {{ item.label }}
        </button>
      </nav>
    </aside>
    <main class="main">
      <InstancesView v-if="page === 'instances'" />
      <TableGroupsView v-else-if="page === 'groups'" />
      <SyncWorkbenchView v-else-if="page === 'sync'" />
      <HistoryView v-else />
    </main>
  </div>
</template>
