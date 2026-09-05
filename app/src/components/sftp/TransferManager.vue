<script setup lang="ts">
/**
 * TransferManager - 全局传输管理（兜底入口）
 *
 * - 传输进度主入口在文件面板底部传输区（TransferList）；
 *   文件面板可见时本组件悬浮球隐藏，面板关闭时从右下角 FAB 兜底
 * - FAB：活跃传输数徽标 + 总速度，点击打开抽屉（内容同为 TransferList）
 * - 数据来自 stores/transfer.ts（transfer_event 事件 upsert）
 */
import { computed } from 'vue'
import { NBadge, NButton, NDrawer, NDrawerContent, NIcon } from 'naive-ui'
import { ArrowsLeftRight } from '@vicons/tabler'

import { useTransferStore } from '@/stores/transfer'
import { useTabsStore } from '@/stores/tabs'
import { useLayoutStore } from '@/stores/layout'
import TransferList from './TransferList.vue'

const store = useTransferStore()
const tabs = useTabsStore()
const layout = useLayoutStore()

/** 文件面板当前可见（与 TerminalView 渲染 FilePanel 条件一致）→ 底部已有传输区 */
const filePanelVisible = computed(
  () => layout.filesVisible && !!tabs.activeTab?.sessionId && !tabs.activeTab.disconnected,
)

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`
  return `${(n / 1024 ** 3).toFixed(2)} GB`
}

function fmtSpeed(bps: number): string {
  if (bps <= 0) return ''
  return `${fmtBytes(bps)}/s`
}
</script>

<template>
  <!-- 文件面板可见时隐藏 FAB，避免与面板内传输区重复 -->
  <div v-if="!filePanelVisible" class="tm-fab" @click="store.managerVisible = true">
    <NBadge :value="store.activeCount" :max="99" :show="store.activeCount > 0">
      <NButton circle size="large" type="primary" secondary>
        <NIcon :component="ArrowsLeftRight" :size="20" />
      </NButton>
    </NBadge>
    <span v-if="store.totalSpeedBps > 0" class="tm-fab-speed">{{ fmtSpeed(store.totalSpeedBps) }}</span>
  </div>

  <NDrawer v-model:show="store.managerVisible" placement="right" :width="440" :z-index="300">
    <NDrawerContent title="传输管理" closable>
      <TransferList />
    </NDrawerContent>
  </NDrawer>
</template>

<style scoped>
.tm-fab {
  position: fixed;
  right: 20px;
  bottom: 20px;
  z-index: 200;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.tm-fab-speed {
  font-size: 12px;
  color: var(--primary);
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  padding: 1px 8px;
}
</style>
