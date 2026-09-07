<script setup lang="ts">
/**
 * TabBar - 主区顶部标签栏
 *
 * 横向展示已打开的 Tab，支持切换 / 关闭。
 * 右键菜单：关闭 / 关闭左侧会话 / 关闭右侧会话 / 关闭所有会话 / 重新连接。
 * 激活态：底部 2px 主色边框 + 背景提亮（VSCode 编辑器标签风）。
 */
import { NIcon, NButton, NDropdown, useMessage } from 'naive-ui'
import { Terminal2, DeviceDesktop, Settings, X } from '@vicons/tabler'
import { useTabsStore } from '@/stores/tabs'
import { useProfilesStore } from '@/stores/profiles'
import { useConnectFlow } from '@/composables/useConnectFlow'
import type { SessionKind, TabItem } from '@/types/session'
import { ref, type Component } from 'vue'

const tabs = useTabsStore()
const profiles = useProfilesStore()
const message = useMessage()
const { reconnectInTab } = useConnectFlow()

/** kind → 图标组件映射（host 暂复用桌面图标，待 macOS 协议落地再换 BrandApple） */
const kindIcon: Record<SessionKind, Component> = {
  ssh: Terminal2,
  rdp: DeviceDesktop,
  host: DeviceDesktop,
  settings: Settings,
}

/** 右键菜单状态（参照 FilePanel 的 manual NDropdown 模式） */
const menuShow = ref(false)
const menuX = ref(0)
const menuY = ref(0)
let menuTab: TabItem | null = null

function onContextMenu(e: MouseEvent, tab: TabItem) {
  menuTab = tab
  menuX.value = e.clientX
  menuY.value = e.clientY
  menuShow.value = true
}

function menuOptions(tab: TabItem) {
  const idx = tabs.tabs.findIndex((t) => t.id === tab.id)
  return [
    { label: '关闭', key: 'close' },
    { label: '关闭左侧会话', key: 'close-left', disabled: idx <= 0 },
    { label: '关闭右侧会话', key: 'close-right', disabled: idx >= tabs.tabs.length - 1 },
    { label: '关闭所有会话', key: 'close-all' },
    { type: 'divider', key: 'd1' },
    { label: '重新连接', key: 'reconnect', disabled: tab.kind !== 'ssh' },
  ]
}

function onMenuSelect(key: string) {
  menuShow.value = false
  const tab = menuTab
  if (!tab) return
  switch (key) {
    case 'close':
      tabs.closeTab(tab.id)
      break
    case 'close-left':
      tabs.closeTabsToLeft(tab.id)
      break
    case 'close-right':
      tabs.closeTabsToRight(tab.id)
      break
    case 'close-all':
      tabs.closeAllTabs()
      break
    case 'reconnect':
      void reconnectTab(tab)
      break
  }
}

/** 重新连接（基于档案，复用当前 tab，与 TerminalView 断线重连同逻辑） */
async function reconnectTab(tab: TabItem) {
  const profile = tab.profileId
    ? profiles.profiles.find((p) => p.id === tab.profileId)
    : null
  if (!profile) {
    message.warning('该会话无关联配置，请从左侧列表重新连接')
    return
  }
  try {
    await reconnectInTab(tab, profile)
    tabs.setActive(tab.id)
    message.success(`已重新连接「${profile.name}」`)
  } catch (e) {
    message.error(`重连失败：${e}`)
  }
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
      @contextmenu.prevent="(e: MouseEvent) => onContextMenu(e, tab)"
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
    <NDropdown
      trigger="manual"
      :show="menuShow"
      :x="menuX"
      :y="menuY"
      placement="bottom-start"
      :options="menuTab ? menuOptions(menuTab) : []"
      @select="onMenuSelect"
      @clickoutside="menuShow = false"
    />
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
