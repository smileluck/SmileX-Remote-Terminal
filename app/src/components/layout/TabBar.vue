<script setup lang="ts">
/**
 * TabBar - 主区顶部标签栏
 *
 * 横向展示已打开的 Tab，支持切换 / 关闭。
 * 激活态：底部 2px 主色边框 + 背景提亮（VSCode 编辑器标签风）。
 */
import { NIcon, NButton } from 'naive-ui'
import { Terminal2, DeviceDesktop, Robot, Settings, X } from '@vicons/tabler'
import { useTabsStore } from '@/stores/tabs'
import type { SessionKind } from '@/types/session'
import type { Component } from 'vue'

const tabs = useTabsStore()

/** kind → 图标组件映射（host 暂复用桌面图标，待 macOS 协议落地再换 BrandApple） */
const kindIcon: Record<SessionKind, Component> = {
  ssh: Terminal2,
  rdp: DeviceDesktop,
  host: DeviceDesktop,
  chat: Robot,
  settings: Settings,
}
</script>

<template>
  <div v-if="tabs.tabs.length" class="tab-bar">
    <div
      v-for="tab in tabs.tabs"
      :key="tab.id"
      class="tab"
      :class="{ active: tab.id === tabs.activeId }"
      :title="tab.title"
      @click="tabs.setActive(tab.id)"
    >
      <NIcon :component="kindIcon[tab.kind]" class="tab-icon" />
      <span class="tab-title" :class="{ disconnected: tab.disconnected }">
        {{ tab.title }}
      </span>
      <span v-if="tab.disconnected" class="tab-dot" title="已断开" />
      <NButton text class="tab-close" @click.stop="tabs.closeTab(tab.id)">
        <NIcon :component="X" />
      </NButton>
    </div>
  </div>
</template>

<style scoped>
.tab-bar {
  display: flex;
  align-items: stretch;
  height: 36px;
  background: var(--bg-app);
  border-bottom: 1px solid var(--border-color);
  overflow-x: auto;
  flex-shrink: 0;
}
.tab {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 8px 0 12px;
  min-width: 130px;
  max-width: 220px;
  cursor: pointer;
  border-right: 1px solid var(--border-color);
  color: var(--text-secondary);
  position: relative;
  white-space: nowrap;
}
.tab:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.tab.active {
  background: var(--bg-sidebar);
  color: var(--text-primary);
}
.tab.active::after {
  content: '';
  position: absolute;
  left: 0;
  right: 0;
  bottom: 0;
  height: 2px;
  background: var(--primary);
}
.tab-icon {
  font-size: 15px;
  flex-shrink: 0;
  opacity: 0.85;
}
.tab-title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 13px;
}
.tab-title.disconnected {
  color: var(--danger, #f87171);
  text-decoration: line-through;
  text-decoration-thickness: 1px;
  opacity: 0.75;
}
.tab-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--danger, #f87171);
  flex-shrink: 0;
}
.tab-close {
  flex-shrink: 0;
  --n-text-color: var(--text-tertiary);
  opacity: 0;
  transition: opacity 0.15s;
}
.tab:hover .tab-close,
.tab.active .tab-close {
  opacity: 1;
}
.tab-close:hover {
  --n-text-color: var(--danger);
}
</style>
