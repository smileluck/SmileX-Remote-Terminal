<script setup lang="ts">
/**
 * RightPanel - 右栏容器（Agent 助手 / 监控看板 / 告警规则）
 *
 * 三栏布局的右栏（收起时整体隐藏，打开/切换入口在终端工具栏列）：
 * - 顶部页签切换，单页签显示（不同时叠加多个面板）
 * - 可拖拽左边缘调宽（240–560px，持久化到 layout store）
 */
import { ref, computed } from 'vue'
import { NIcon } from 'naive-ui'
import { X } from '@vicons/tabler'
import { useLayoutStore, type RightPanelTab } from '@/stores/layout'
import MonitorDashboard from '@/components/monitor/MonitorDashboard.vue'
import AlertRules from '@/components/monitor/AlertRules.vue'
import ChatPanel from '@/components/ai/ChatPanel.vue'

const layout = useLayoutStore()

const width = computed(() => `${layout.monitorWidth}px`)

/** 页签定义（一次只显示一个面板） */
const panelTabs: { key: RightPanelTab; label: string }[] = [
  { key: 'agent', label: 'Agent' },
  { key: 'monitor', label: '监控' },
  { key: 'alerts', label: '告警' },
]

/** 拖拽调宽状态 */
const dragging = ref(false)
function onDragStart(e: MouseEvent) {
  dragging.value = true
  const startX = e.clientX
  const startWidth = layout.monitorWidth
  const onMove = (ev: MouseEvent) => {
    // 向左拖增大宽度
    const w = Math.min(560, Math.max(240, startWidth + (startX - ev.clientX)))
    layout.monitorWidth = w
  }
  const onUp = () => {
    dragging.value = false
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
  e.preventDefault()
}
</script>

<template>
  <aside v-if="layout.monitorVisible" class="right-panel" :style="{ width }">
    <div class="resize-handle" :class="{ dragging }" @mousedown="onDragStart" />
    <header class="panel-header">
      <div class="tab-switch">
        <button
          v-for="t in panelTabs"
          :key="t.key"
          class="tab-btn"
          :class="{ active: layout.rightTab === t.key }"
          @click="layout.setRightTab(t.key)"
        >
          {{ t.label }}
        </button>
      </div>
      <button class="close-btn" title="收起面板" @click="layout.toggleMonitor()">
        <NIcon :component="X" :size="14" />
      </button>
    </header>
    <div class="panel-body" :class="{ agent: layout.rightTab === 'agent' }">
      <ChatPanel v-if="layout.rightTab === 'agent'" />
      <MonitorDashboard v-else-if="layout.rightTab === 'monitor'" />
      <AlertRules v-else />
    </div>
  </aside>
</template>

<style scoped>
.right-panel {
  position: relative;
  flex-shrink: 0;
  background: var(--bg-sidebar);
  border-left: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.resize-handle {
  position: absolute;
  left: 0;
  top: 0;
  bottom: 0;
  width: 4px;
  cursor: col-resize;
  z-index: 10;
}
.resize-handle:hover,
.resize-handle.dragging {
  background: var(--primary);
  opacity: 0.6;
}
.panel-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 10px 8px 14px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.tab-switch {
  display: flex;
  gap: 2px;
  background: var(--bg-elevated);
  border-radius: 6px;
  padding: 2px;
}
.tab-btn {
  border: none;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  padding: 3px 14px;
  border-radius: 4px;
  cursor: pointer;
  transition: color 0.12s, background-color 0.12s;
}
.tab-btn:hover {
  color: var(--text-primary);
}
.tab-btn.active {
  background: var(--bg-panel);
  color: var(--text-primary);
  font-weight: 600;
}
.close-btn {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: 4px;
}
.close-btn:hover {
  color: var(--text-primary);
  background: var(--bg-elevated);
}
.panel-body {
  flex: 1;
  overflow-y: auto;
  padding: 10px;
  min-height: 0;
}
/* Agent 面板自带内边距与滚动区，撑满高度 */
.panel-body.agent {
  display: flex;
  padding: 0;
  overflow: hidden;
}
</style>
