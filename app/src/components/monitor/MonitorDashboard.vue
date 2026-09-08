<script setup lang="ts">
/**
 * MonitorDashboard - 监控看板内容区
 *
 * - 顶部：监控目标切换（多会话下拉；默认跟随激活 SSH tab）
 * - 分类页签：通用 / CPU / 内存 / 磁盘 / GPU（各 Tab 组件自行从 store 取数）
 *
 * 数据来源：monitor store（`monitor_metrics` 事件 + session_stats_all 轮询）
 */
import { computed, ref, watch, onMounted } from 'vue'
import { NSelect, NTabs, NTabPane } from 'naive-ui'
import { useMonitorStore } from '@/stores/monitor'
import { useTabsStore } from '@/stores/tabs'
import { segmentTabThemeOverrides } from '@/components/common/segmentTabTheme'
import GeneralTab from './tabs/GeneralTab.vue'
import CpuTab from './tabs/CpuTab.vue'
import MemoryTab from './tabs/MemoryTab.vue'
import DiskTab from './tabs/DiskTab.vue'
import GpuTab from './tabs/GpuTab.vue'

const monitor = useMonitorStore()
const tabs = useTabsStore()

/** 监控目标下拉选项（SSH 会话） */
const targetOptions = computed(() =>
  tabs.tabs
    .filter((t) => t.kind === 'ssh' && t.sessionId)
    .map((t) => ({ label: t.title, value: t.sessionId! })),
)

/** 跟随激活的 SSH tab 自动切换监控目标 */
watch(
  () => tabs.activeTab,
  (tab) => {
    if (tab?.kind === 'ssh' && tab.sessionId) {
      monitor.setActive(tab.sessionId)
    }
  },
  { immediate: true },
)

onMounted(() => monitor.init())

/** 当前分类页签 */
const activeTab = ref('general')
</script>

<template>
  <div class="monitor-dashboard">
    <!-- 监控目标切换 -->
    <div v-if="targetOptions.length > 0" class="target-row">
      <NSelect
        size="small"
        :value="monitor.activeSessionId"
        :options="targetOptions"
        placeholder="选择监控目标"
        @update:value="(v: string) => monitor.setActive(v)"
      />
    </div>

    <template v-if="monitor.latest">
      <NTabs
        v-model:value="activeTab"
        type="segment"
        size="small"
        :theme-overrides="segmentTabThemeOverrides"
      >
        <NTabPane name="general" tab="通用">
          <GeneralTab />
        </NTabPane>
        <NTabPane name="cpu" tab="CPU">
          <CpuTab />
        </NTabPane>
        <NTabPane name="memory" tab="内存">
          <MemoryTab />
        </NTabPane>
        <NTabPane name="disk" tab="磁盘">
          <DiskTab />
        </NTabPane>
        <NTabPane name="gpu" tab="GPU">
          <GpuTab />
        </NTabPane>
      </NTabs>
    </template>

    <p v-else class="placeholder">
      {{ targetOptions.length ? '正在采集监控数据…' : '连接 SSH 会话后展示监控数据' }}
    </p>
  </div>
</template>

<style scoped>
.monitor-dashboard {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.target-row {
  display: flex;
}
.placeholder {
  font-size: 12px;
  color: var(--text-tertiary);
  text-align: center;
  padding: 24px 0;
}
</style>
