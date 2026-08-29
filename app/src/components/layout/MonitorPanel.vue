<script setup lang="ts">
/**
 * MonitorPanel - 右栏监控看版容器
 *
 * 三栏布局的右栏：
 * - 可拖拽左边缘调宽（240–560px，持久化到 layout store）
 * - 内容区由监控子组件填充（阶段 2：指标卡片 + 会话健康列表）
 */
import { ref, computed } from 'vue'
import { NIcon } from 'naive-ui'
import { X } from '@vicons/tabler'
import { useLayoutStore } from '@/stores/layout'
import MonitorDashboard from '@/components/monitor/MonitorDashboard.vue'
import AlertRules from '@/components/monitor/AlertRules.vue'

const layout = useLayoutStore()

const width = computed(() => `${layout.monitorWidth}px`)

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
  <aside class="monitor-panel" :style="{ width }">
    <div class="resize-handle" :class="{ dragging }" @mousedown="onDragStart" />
    <header class="panel-header">
      <span class="panel-title">监控看版</span>
      <button class="close-btn" title="收起面板（⌘M）" @click="layout.toggleMonitor()">
        <NIcon :component="X" :size="14" />
      </button>
    </header>
    <div class="panel-body">
      <MonitorDashboard />
      <AlertRules />
    </div>
  </aside>
</template>

<style scoped>
.monitor-panel {
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
  padding: 10px 14px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.panel-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 1px;
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
}
</style>
