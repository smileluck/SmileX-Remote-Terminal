<script setup lang="ts">
/**
 * TopBar - 顶部工具栏
 *
 * 品牌标识 + 右侧布局开关（左栏折叠 / 监控看版）。
 */
import { NButton, NIcon, NTooltip } from 'naive-ui'
import { LayoutSidebarLeftCollapse, ChartAreaLine } from '@vicons/tabler'
import { useLayoutStore } from '@/stores/layout'

const layout = useLayoutStore()
</script>

<template>
  <header class="top-bar">
    <div class="brand">
      <span class="brand-mark">▣</span>
      <span class="brand-name">SmileX <span class="brand-sub">Remote Terminal</span></span>
    </div>

    <div class="top-actions">
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton quaternary circle size="small" @click="layout.toggleSidebar()">
            <NIcon :component="LayoutSidebarLeftCollapse" />
          </NButton>
        </template>
        折叠/展开会话栏（⌘B）
      </NTooltip>
      <NTooltip placement="bottom">
        <template #trigger>
          <NButton
            quaternary
            circle
            size="small"
            :type="layout.monitorVisible ? 'primary' : 'default'"
            @click="layout.toggleMonitor()"
          >
            <NIcon :component="ChartAreaLine" />
          </NButton>
        </template>
        监控看版（⌘M）
      </NTooltip>
    </div>
  </header>
</template>

<style scoped>
.top-bar {
  height: 44px;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 12px 0 16px;
  background: var(--bg-sidebar);
  border-bottom: 1px solid var(--border-color);
}
.brand {
  display: flex;
  align-items: center;
  gap: 8px;
}
.brand-mark {
  color: var(--primary);
  font-size: 18px;
  line-height: 1;
}
.brand-name {
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: 0.3px;
}
.brand-sub {
  color: var(--text-secondary);
  font-weight: 400;
}
.top-actions {
  display: flex;
  align-items: center;
  gap: 4px;
}
</style>
