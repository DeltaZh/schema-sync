<script setup lang="ts">
import { ref } from "vue";
import BaselineSyncPane from "./components/BaselineSyncPane.vue";
import ConnectionTree from "./components/ConnectionTree.vue";
import DdlBroadcastPane from "./components/DdlBroadcastPane.vue";
import HistoryPane from "./components/HistoryPane.vue";
import RulesPane from "./components/RulesPane.vue";
import StructurePane from "./components/StructurePane.vue";
import type { ConnectionConfig, MainTab, TableSelection } from "./types";

const activeTab = ref<MainTab>("structure");
const selection = ref<TableSelection | null>(null);
const connections = ref<ConnectionConfig[]>([]);

function onSelectTable(s: TableSelection) {
  selection.value = s;
  if (activeTab.value === "rules") return;
  activeTab.value = "structure";
}

function onConnectionsChanged(list: ConnectionConfig[]) {
  connections.value = list;
}
</script>

<template>
  <div class="app-shell">
    <ConnectionTree
      @select-table="onSelectTable"
      @connections-changed="onConnectionsChanged"
    />

    <div class="panel panel-main">
      <div class="tabs" role="tablist">
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'structure' }"
          role="tab"
          @click="activeTab = 'structure'"
        >
          结构
        </button>
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'baseline' }"
          role="tab"
          @click="activeTab = 'baseline'"
        >
          基准同步
        </button>
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'ddl' }"
          role="tab"
          @click="activeTab = 'ddl'"
        >
          DDL 投放
        </button>
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'rules' }"
          role="tab"
          @click="activeTab = 'rules'"
        >
          规则
        </button>
        <button
          type="button"
          class="tab"
          :class="{ active: activeTab === 'history' }"
          role="tab"
          @click="activeTab = 'history'"
        >
          历史
        </button>
      </div>

      <StructurePane
        v-if="activeTab === 'structure'"
        :selection="selection"
      />
      <BaselineSyncPane
        v-else-if="activeTab === 'baseline'"
        :connections="connections"
      />
      <DdlBroadcastPane
        v-else-if="activeTab === 'ddl'"
        :connections="connections"
      />
      <RulesPane
        v-else-if="activeTab === 'rules'"
        :connections="connections"
      />
      <HistoryPane
        v-else
        :connections="connections"
      />
    </div>
  </div>
</template>
