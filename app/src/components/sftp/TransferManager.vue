<script setup lang="ts">
/**
 * TransferManager - 全局传输管理（兜底入口）
 *
 * - 传输进度主入口在文件面板底部传输区（TransferList）；
 *   本组件仅作兜底：文件面板可见时隐藏，且平时不再常驻——
 *   仅在有活跃传输时显示悬浮球；传输成功后悬浮球带红点提示，
 *   点击打开抽屉（内容同为 TransferList）即视为已读、红点消失
 * - 数据来自 stores/transfer.ts（transfer_event 事件 upsert）
 */
import { computed, ref, watch } from 'vue'
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

/** 已读的成功完成传输（groupId）；组被清除或重新活跃时自动出列 */
const seenCompleted = new Set<string>()
const hasUnseenCompleted = ref(false)

function recomputeDot() {
  hasUnseenCompleted.value = store.groups.some(
    (g) => g.status === 'completed' && !seenCompleted.has(g.groupId),
  )
}

watch(
  () => store.groups,
  (groups) => {
    for (const id of [...seenCompleted]) {
      const g = groups.find((x) => x.groupId === id)
      // 仅保留仍是 completed 的组：被清除/重试后可再次提示
      if (!g || g.status !== 'completed') seenCompleted.delete(id)
    }
    recomputeDot()
  },
  { deep: true },
)

/** 打开抽屉即标记全部成功任务已读（红点消失） */
function openManager() {
  store.managerVisible = true
  for (const g of store.groups) {
    if (g.status === 'completed') seenCompleted.add(g.groupId)
  }
  recomputeDot()
}

/** 悬浮球显示：文件面板不可见，且有活跃传输或未读的成功完成 */
const fabVisible = computed(
  () => !filePanelVisible.value && (store.activeCount > 0 || hasUnseenCompleted.value),
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
  <!-- 平时不占位：仅传输中/有未读成功提示时出现，且文件面板可见时隐藏；速率标签在左、悬浮球贴右边缘 -->
  <div v-if="fabVisible" class="tm-fab" @click="openManager">
    <span v-if="store.totalSpeedBps > 0" class="tm-fab-speed">{{ fmtSpeed(store.totalSpeedBps) }}</span>
    <NBadge :value="store.activeCount" :max="99" :show="store.activeCount > 0">
      <NButton circle size="large" type="primary" secondary>
        <NIcon :component="ArrowsLeftRight" :size="20" />
      </NButton>
    </NBadge>
    <span v-if="hasUnseenCompleted" class="tm-fab-dot" />
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
  top: 12px;
  z-index: 200;
  display: flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
}
.tm-fab-dot {
  position: absolute;
  top: 0;
  right: 0;
  width: 11px;
  height: 11px;
  border-radius: 50%;
  background: var(--error-color, #f87171);
  border: 2px solid var(--bg-app);
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
