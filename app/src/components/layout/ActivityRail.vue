<script setup lang="ts">
/**
 * ActivityRail - 左侧活动栏
 *
 * - 顶部「会话」Tab：展开/收起侧边栏（会话列表；折叠按钮在侧栏头部）
 * - 底部设置入口（单例 tab）
 * 新建会话 / 远程桌面统一由 SideBar「+」下拉弹窗承担。
 */
import { NButton, NIcon, NTooltip } from 'naive-ui'
import { Terminal2, Settings } from '@vicons/tabler'
import { useTabsStore } from '@/stores/tabs'
import { useLayoutStore } from '@/stores/layout'

const tabs = useTabsStore()
const layout = useLayoutStore()

/** 打开设置 Tab（单例：已存在则激活，否则新建） */
function openSettings() {
  const existing = tabs.tabs.find((t) => t.kind === 'settings')
  if (existing) tabs.setActive(existing.id)
  else tabs.addTab('settings', '设置')
}
</script>

<template>
  <nav class="rail">
    <div class="rail-group">
      <NTooltip placement="right">
        <template #trigger>
          <NButton
            quaternary
            circle
            class="rail-btn"
            :class="{ active: !layout.sidebarCollapsed }"
            @click="layout.toggleSidebar()"
          >
            <template #icon><NIcon :component="Terminal2" /></template>
          </NButton>
        </template>
        会话
      </NTooltip>
    </div>

    <div class="rail-group">
      <NTooltip placement="right">
        <template #trigger>
          <NButton quaternary circle class="rail-btn" @click="openSettings">
            <template #icon><NIcon :component="Settings" /></template>
          </NButton>
        </template>
        设置
      </NTooltip>
    </div>
  </nav>
</template>

<style scoped>
.rail {
  width: 52px;
  flex-shrink: 0;
  background: var(--bg-rail);
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  justify-content: space-between;
  align-items: center;
  padding: 12px 0;
}
.rail-group {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.rail-btn {
  font-size: 19px;
  color: var(--text-secondary);
  transition: color 0.15s ease, background-color 0.15s ease;
}
.rail-btn:hover {
  color: var(--text-primary);
}
.rail-btn.active {
  color: var(--primary);
}
</style>
