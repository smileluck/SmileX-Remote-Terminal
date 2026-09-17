<script setup lang="ts">
/**
 * GroupTabBar - 分屏 group 的标签栏
 *
 * 每个 group 一个（Xshell 式）：展示本 group 的 tab，支持切换 / 关闭。
 * 右键菜单：关闭 / 关闭其他会话 / 关闭左侧会话 / 关闭右侧会话 / 关闭所有会话 / 重新连接
 * （左/右/其他按本 group 内顺序，关闭所有为全局）。
 * 拖拽：拖 tab 到其他 group 的 tab 栏 / 中央区 = 并入；拖到四边缘 = 分裂新 group。
 * 激活态：底部 2px 主色边框 + 背景提亮（VSCode 编辑器标签风）。
 */
import { NIcon, NButton, NDropdown, useMessage } from 'naive-ui'
import { Terminal2, DeviceDesktop, Settings, X } from '@vicons/tabler'
import { useTabsStore } from '@/stores/tabs'
import { useProfilesStore } from '@/stores/profiles'
import { useConnectFlow } from '@/composables/useConnectFlow'
import { useTabDrag, type DropTarget } from '@/composables/useTabDrag'
import type { SessionKind, TabItem } from '@/types/session'
import { computed, ref, type Component } from 'vue'

const props = defineProps<{ groupId: string }>()

const tabs = useTabsStore()
const profiles = useProfilesStore()
const tabDrag = useTabDrag()
const message = useMessage()
const { reconnectInTab } = useConnectFlow()

/** 本 group 节点（store 保证空 group 即折叠，正常恒存在） */
const group = computed(() => tabs.groups.find((g) => g.id === props.groupId) ?? null)

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
  const list = group.value?.tabs ?? []
  const idx = list.findIndex((t) => t.id === tab.id)
  return [
    { label: '关闭', key: 'close' },
    { label: '关闭其他会话', key: 'close-others', disabled: list.length <= 1 },
    { label: '关闭左侧会话', key: 'close-left', disabled: idx <= 0 },
    { label: '关闭右侧会话', key: 'close-right', disabled: idx >= list.length - 1 },
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
    case 'close-others':
      tabs.closeOtherTabs(tab.id)
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

/* ---------------- Tab 拖拽分屏（Pointer Events 自实现，参照 SnippetPanel） ---------------- */

/** 拖拽判定阈值（px）：位移内视为点击 */
const DRAG_THRESHOLD = 6

interface TabDragInfo {
  tab: TabItem
  startX: number
  startY: number
  started: boolean
}

let dragInfo: TabDragInfo | null = null
/** 拖拽结束的 pointerup 会紧跟一次 click，需吞掉防止误切换 active tab */
let suppressClick = false

function onTabPointerDown(e: PointerEvent, tab: TabItem) {
  if (e.button !== 0) return
  // 点在关闭按钮上时不启动拖拽
  if ((e.target as HTMLElement).closest('.tab-close')) return
  e.preventDefault() // 阻止文本选中；不影响 click
  dragInfo = { tab, startX: e.clientX, startY: e.clientY, started: false }
  window.addEventListener('pointermove', onTabPointerMove)
  window.addEventListener('pointerup', onTabPointerUp)
  window.addEventListener('pointercancel', onTabPointerUp)
}

function onTabPointerMove(e: PointerEvent) {
  const d = dragInfo
  if (!d) return
  if (!d.started) {
    if (Math.hypot(e.clientX - d.startX, e.clientY - d.startY) < DRAG_THRESHOLD) return
    d.started = true
    tabDrag.startDrag(d.tab)
  }
  tabDrag.moveGhost(e.clientX, e.clientY)
  tabDrag.setDropTarget(hitDropTarget(e.clientX, e.clientY))
}

/**
 * 命中检测：优先 group tab 栏（并入），其次 group 内容区覆盖层
 * （data-group-drop，按相对位置折算四边缘；中央 40% 为并入，无死区）。
 */
function hitDropTarget(x: number, y: number): DropTarget | null {
  const el = document.elementFromPoint(x, y)
  const bar = el?.closest('[data-group-tabbar]') as HTMLElement | null
  if (bar?.dataset.groupTabbar) return { groupId: bar.dataset.groupTabbar, zone: 'merge' }
  const overlay = el?.closest('[data-group-drop]') as HTMLElement | null
  if (!overlay?.dataset.groupDrop) return null
  const r = overlay.getBoundingClientRect()
  const fx = (x - r.left) / r.width
  const fy = (y - r.top) / r.height
  let zone: DropTarget['zone'] = 'merge'
  if (fx < 0.3) zone = 'left'
  else if (fx > 0.7) zone = 'right'
  else if (fy < 0.3) zone = 'top'
  else if (fy > 0.7) zone = 'bottom'
  return { groupId: overlay.dataset.groupDrop, zone }
}

function onTabPointerUp() {
  window.removeEventListener('pointermove', onTabPointerMove)
  window.removeEventListener('pointerup', onTabPointerUp)
  window.removeEventListener('pointercancel', onTabPointerUp)
  const d = dragInfo
  dragInfo = null
  if (!d?.started) return
  suppressClick = true
  // click 紧跟 pointerup 派发（同一事件序列）；若无 click（落点在 tab 外）则下一拍复位，避免吞掉后续正常点击
  setTimeout(() => (suppressClick = false), 0)
  const result = tabDrag.endDrag()
  if (!result) return
  const { tab, target } = result
  if (target.zone === 'merge') {
    tabs.moveTabToGroup(tab.id, target.groupId)
    return
  }
  const dir = target.zone === 'left' || target.zone === 'right' ? 'row' : 'column'
  const before = target.zone === 'left' || target.zone === 'top'
  const ok = tabs.splitWithTab(tab.id, target.groupId, dir, before)
  if (!ok && tabs.groups.length >= 4) message.warning('已达最大分屏数（4）')
}

function onTabClick(tab: TabItem) {
  if (suppressClick) {
    suppressClick = false
    return
  }
  tabs.setActive(tab.id)
}
</script>

<template>
  <div v-if="group" class="tab-bar" :data-group-tabbar="groupId">
    <div
      v-for="tab in group.tabs"
      :key="tab.id"
      class="tab"
      :class="{ active: tab.id === group.activeTabId, dragging: tabDrag.draggingTab.value?.id === tab.id }"
      :title="tab.title"
      @click="onTabClick(tab)"
      @pointerdown="(e: PointerEvent) => onTabPointerDown(e, tab)"
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
.tab.dragging {
  opacity: 0.45;
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
