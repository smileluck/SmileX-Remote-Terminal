<script setup lang="ts">
/**
 * TerminalView - SSH 终端视图（支持分屏）
 *
 * - tab 已带 sessionId（统一连接入口创建）：分屏窗格平铺绝对定位渲染（≤4 窗格）
 * - 分屏作用于当前选中窗格：在选中 pane 位置原位分割，不重排其他区域；
 *   树结构只换算为矩形，窗格组件不重建（关闭/分割不清空其他窗格的终端内容）
 * - 无 sessionId 的窗格：显示「绑定现有会话 / 新建连接」选择器（见 PaneTerminal）
 * - 会话意外断开（disconnected）：显示断开遮罩 + 重新连接按钮（原地重连）
 * - 右侧工具栏（流式竖排，不悬浮）：分屏单入口（点击弹二级菜单选
 *   分割方向）+ 四面板入口（文件管理 / 监控 / Agent / 告警），
 *   四面板互斥切换（同一时间只开一个，再点同一个收起）
 */
import { ref, computed, onMounted, onUnmounted, watch, watchEffect, h, type Component } from 'vue'
import { NButton, NDropdown, NIcon, NTooltip, useMessage } from 'naive-ui'
import {
  Refresh,
  Folder,
  ArrowsSplit2,
  LayoutRows,
  Robot,
  ChartAreaLine,
  Bell,
  Bookmarks,
  BrandDocker,
  CalendarTime,
  Package,
  ArrowsRightLeft,
} from '@vicons/tabler'
import * as sessionService from '@/services/session'
import { useConnectFlow } from '@/composables/useConnectFlow'
import { shellQuote } from '@/utils/shell'
import { useProfilesStore } from '@/stores/profiles'
import { useMonitorStore } from '@/stores/monitor'
import { useLayoutStore, type SplitRequest } from '@/stores/layout'
import { useTabsStore } from '@/stores/tabs'
import { useTabDrag } from '@/composables/useTabDrag'
import FilePanel from '@/components/sftp/FilePanel.vue'
import PaneTerminal from './PaneTerminal.vue'
import {
  countPanes,
  collectPanes,
  findPane,
  splitAtPane,
  removePane,
  computeLayout,
  genNodeId,
  type LayoutNode,
  type PaneNode,
  type DividerLayout,
} from './splitTree'
import type { TabItem } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const profiles = useProfilesStore()
const monitor = useMonitorStore()
const layout = useLayoutStore()
const tabs = useTabsStore()
const message = useMessage()
const { reconnectInTab } = useConnectFlow()
const tabDrag = useTabDrag()

/** 右栏面板入口（Agent / 监控 / 告警 / 常用记录 / Docker / 定时任务 / 环境管理 / 端口转发）：按钮高亮条件与点击切换 */
const panelEntries = [
  { key: 'monitor', label: '监控看板（⌘M）', icon: ChartAreaLine },
  { key: 'agent', label: 'AI 运维助手（⌘J）', icon: Robot },
  { key: 'alerts', label: '告警规则', icon: Bell },
  { key: 'snippets', label: '常用记录', icon: Bookmarks },
  { key: 'docker', label: 'Docker 管理', icon: BrandDocker },
  { key: 'crontab', label: '定时任务', icon: CalendarTime },
  { key: 'env', label: '环境管理', icon: Package },
  { key: 'tunnel', label: '端口转发', icon: ArrowsRightLeft },
] as const

/** 分屏下拉选项（二级选择分割方向） */
function renderIcon(icon: Component, style?: string) {
  return () => h(NIcon, null, { default: () => h(icon, style ? { style } : undefined) })
}
const splitOptions = [
  { label: '向右分屏', key: 'row', icon: renderIcon(ArrowsSplit2, 'transform: rotate(90deg)') },
  { label: '向下分屏', key: 'column', icon: renderIcon(LayoutRows) },
]

/** 重连中 */
const reconnecting = ref(false)

/** 重连目标档案（tab 带 profileId 时可用） */
const profile = computed(() =>
  props.tab.profileId ? profiles.profiles.find((p) => p.id === props.tab.profileId) : null,
)

/**
 * 分屏窗格绑定会话时写入的首行提示（主窗格有远端提示符，返回 undefined）。
 * 拖入的既有会话（dragInPanes）输出即时到达，不写占位提示行（内容也不匹配该会话）。
 */
const initialLineFor = (paneId: string): string | undefined => {
  if (paneId === primaryPaneId.value || dragInPanes.has(paneId)) return undefined
  const p = profile.value
  const label = p ? `${p.username}@${p.host}` : props.tab.title
  return `${label}:/$ `
}

/* ---------------- 扁平布局（绝对定位渲染） ---------------- */
/**
 * 布局树 → 窗格槽位 + 分割条矩形。窗格以 pane id 为 key 平铺渲染，
 * 树结构变化（分割/关闭/折叠）只改矩形不重建组件，xterm 实例与
 * 历史内容在关闭其他窗格时不丢失。
 */
const flat = computed(() => computeLayout(root.value))
const splitAreaEl = ref<HTMLDivElement | null>(null)

const pct = (v: number) => `${(v * 100).toFixed(4)}%`

function paneStyle(rect: { left: number; top: number; width: number; height: number }) {
  return {
    left: pct(rect.left),
    top: pct(rect.top),
    width: pct(rect.width),
    height: pct(rect.height),
  }
}

function dividerStyle(d: DividerLayout) {
  return d.dir === 'row'
    ? { left: `calc(${pct(d.center)} - 3px)`, top: pct(d.crossStart), height: pct(d.crossLength), width: '6px' }
    : { top: `calc(${pct(d.center)} - 3px)`, left: pct(d.crossStart), width: pct(d.crossLength), height: '6px' }
}

/** 分割条拖拽：只调整本 split 节点内相邻两个子节点的比例 */
function onDividerDown(e: MouseEvent, d: DividerLayout) {
  const container = splitAreaEl.value
  if (!container) return
  const horizontal = d.dir === 'row'
  const containerMain = horizontal ? container.clientWidth : container.clientHeight
  const nodeMain = containerMain * d.axisLength
  if (!nodeMain) return
  const totalRatio = d.node.ratios.reduce((s, r) => s + r, 0)
  const startX = horizontal ? e.clientX : e.clientY
  const a = d.node.ratios[d.index]
  const b = d.node.ratios[d.index + 1]
  const onMove = (ev: MouseEvent) => {
    const delta = (((horizontal ? ev.clientX : ev.clientY) - startX) / nodeMain) * totalRatio
    d.node.ratios[d.index] = Math.max(0.1, a + delta)
    d.node.ratios[d.index + 1] = Math.max(0.1, b - delta)
  }
  const onUp = () => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
  e.preventDefault()
}

/* ---------------- 分屏状态（递归布局树） ---------------- */
/** 本 tab 独立建立的会话（关闭 pane / 卸载时统一断开；绑定其他 tab 的会话不在此列） */
const ownSessions = new Set<string>()
/** 由 tab 拖入的 pane id（这些 pane 绑定的是既有会话，不写首行占位提示） */
const dragInPanes = new Set<string>()
const root = ref<LayoutNode>({ kind: 'pane', id: 'p0', sessionId: props.tab.sessionId ?? null })
/** 当前选中 pane（分屏作用目标；点击 pane 时更新） */
const activePaneId = ref('p0')
/** 主 pane（承载 tab 主会话，重连成功后更新绑定） */
const primaryPaneId = ref('p0')
const paneCount = computed(() => countPanes(root.value))

// 上报窗格数（tab 拖放区显示门控：满 4 窗格不再接受拖入）
watchEffect(() => tabDrag.setPaneCount(props.tab.id, paneCount.value))

watch(
  () => props.tab.sessionId,
  (sid) => {
    // 重连成功：主 pane 绑定新会话
    if (!sid) return
    const pane = findPane(root.value, primaryPaneId.value)
    if (pane) pane.sessionId = sid
  },
)

/** 分屏：在当前选中 pane 位置原位分割（row = 向右，column = 向下）；initialPath 时新窗格就绪后 cd 过去 */
const splitting = ref(false)
async function addPane(dir: 'row' | 'column', initialPath?: string) {
  if (paneCount.value >= 4) {
    message.warning('已达最大分屏数（4）')
    return
  }
  if (splitting.value) return
  const targetId = findPane(root.value, activePaneId.value) ? activePaneId.value : primaryPaneId.value
  const target = findPane(root.value, targetId)
  const pane: PaneNode = { kind: 'pane', id: genNodeId('p-'), sessionId: null }

  // 默认克隆选中窗格的连接为独立新会话；克隆失败/无会话则保持未绑定（可手动绑定）
  if (target?.sessionId) {
    splitting.value = true
    try {
      const sid = await sessionService.clone(target.sessionId, 80, 24)
      pane.sessionId = sid
      ownSessions.add(sid)
      // shell 就绪无信号，延时后发送 cd
      if (initialPath) {
        setTimeout(() => {
          sessionService
            .input(sid, Array.from(new TextEncoder().encode(`cd ${shellQuote(initialPath)}\r`)))
            .catch(() => {})
        }, 500)
      }
    } catch (e) {
      message.error(`克隆会话失败：${e}`)
    } finally {
      splitting.value = false
    }
  }

  root.value = splitAtPane(root.value, targetId, dir, pane)
  activePaneId.value = pane.id
}

function closePane(paneId: string) {
  const pane = findPane(root.value, paneId)
  if (!pane || paneCount.value <= 1) return
  disconnectIfOwn(pane.sessionId)
  dragInPanes.delete(paneId)
  const next = removePane(root.value, paneId)
  if (next) root.value = next
  const first = collectPanes(root.value)[0]?.id
  if (first) {
    if (activePaneId.value === paneId) activePaneId.value = first
    if (primaryPaneId.value === paneId) primaryPaneId.value = first
  }
}

function bindPane(paneId: string, sid: string) {
  const pane = findPane(root.value, paneId)
  if (pane) pane.sessionId = sid
}

/* ---------------- Tab 拖拽分屏（其他 tab 的会话并入本视图） ---------------- */

/** 消费 layout store 的 pendingSplit（TabBar 放置时写入；只处理目标为本 tab 的请求） */
watch(
  () => layout.pendingSplit,
  (req) => {
    if (!req || req.targetTabId !== props.tab.id) return
    layout.clearPendingSplit()
    attachSessionPane(req)
  },
)

/**
 * 把拖入 tab 的会话作为 pane 并入分屏树（tab 已被 detachTabForSplit 移除，
 * 会话所有权转入 ownSessions：关闭 pane / 卸载本视图时负责断开）。
 * 窗格数满时恢复为原 tab（会话随 tab 重建自动重绑）。
 */
function attachSessionPane(req: SplitRequest) {
  if (paneCount.value >= 4) {
    message.warning('已达最大分屏数（4）')
    tabs.addTab('ssh', req.title, req.sessionId, req.profileId)
    return
  }
  const targetId = findPane(root.value, activePaneId.value) ? activePaneId.value : primaryPaneId.value
  const pane: PaneNode = { kind: 'pane', id: genNodeId('p-'), sessionId: req.sessionId }
  root.value = splitAtPane(root.value, targetId, req.dir, pane, req.before)
  ownSessions.add(req.sessionId)
  dragInPanes.add(pane.id)
  activePaneId.value = pane.id
}

/** 仅断开本 tab 独立建立的会话（其他 tab 的会话不受 pane 关闭影响） */
function disconnectIfOwn(sessionId: string | null) {
  if (sessionId && ownSessions.has(sessionId)) {
    monitor.stopSampling(sessionId)
    sessionService.disconnect(sessionId).catch(() => {})
    ownSessions.delete(sessionId)
  }
}

/** tab 卸载：清理 pane 独立建立的会话（tab 主会话由 closeTab 负责） */
onUnmounted(() => {
  window.removeEventListener('keydown', onPaneKeydown)
  tabDrag.clearPaneCount(props.tab.id)
  for (const sid of [...ownSessions]) disconnectIfOwn(sid)
})

/* ---------------- 分屏快捷键（仅 metaKey 系，避让 shell 的 Ctrl 控制字符） ---------------- */

/** 按 DFS 顺序循环切换 pane */
function cyclePane(delta: number) {
  const panes = collectPanes(root.value)
  if (panes.length < 2) return
  const idx = panes.findIndex((p) => p.id === activePaneId.value)
  const next = ((idx < 0 ? 0 : idx) + delta + panes.length) % panes.length
  activePaneId.value = panes[next].id
}

/**
 * 分屏快捷键（window 监听 + 本 tab 活跃门控：多 TerminalView 实例 v-show 并存时防串扰）：
 * ⌘D 向右分屏 / ⌘⇧D 向下分屏 / ⌘⇧W 关闭当前分屏 / ⌘⌥←→↑↓ 切换分屏
 */
function onPaneKeydown(e: KeyboardEvent) {
  if (tabs.activeId !== props.tab.id) return
  if (!e.metaKey) return
  const key = e.key.toLowerCase()
  if (key === 'd') {
    e.preventDefault()
    void addPane(e.shiftKey ? 'column' : 'row')
  } else if (key === 'w' && e.shiftKey) {
    // App.vue 的全局 ⌘W 已跳过 Shift 组合（见 onKeydown）
    e.preventDefault()
    closePane(activePaneId.value)
  } else if (e.altKey && (key === 'arrowleft' || key === 'arrowup')) {
    e.preventDefault()
    cyclePane(-1)
  } else if (e.altKey && (key === 'arrowright' || key === 'arrowdown')) {
    e.preventDefault()
    cyclePane(1)
  }
}

onMounted(() => window.addEventListener('keydown', onPaneKeydown))

/** 重新连接（基于档案，复用当前 tab） */
async function handleReconnect() {
  if (!profile.value) {
    message.warning('该会话无关联配置，请从左侧列表重新连接')
    return
  }
  reconnecting.value = true
  try {
    await reconnectInTab(props.tab, profile.value)
    message.success(`已重新连接「${profile.value.name}」`)
  } catch (e) {
    message.error(`重连失败：${e}`)
  } finally {
    reconnecting.value = false
  }
}
</script>

<template>
  <div class="terminal-view">
    <div class="terminal-main">
      <div ref="splitAreaEl" class="split-area">
        <div
          v-for="p in flat.panes"
          :key="p.id"
          class="pane-slot"
          :style="paneStyle(p.rect)"
        >
          <PaneTerminal
            :session-id="p.sessionId"
            :active="p.id === activePaneId"
            :closable="paneCount > 1"
            :initial-line="initialLineFor(p.id)"
            @focus="activePaneId = p.id"
            @close="closePane(p.id)"
            @bind="(sid: string) => bindPane(p.id, sid)"
          />
        </div>
        <div
          v-for="d in flat.dividers"
          :key="d.key"
          class="split-divider"
          :class="d.dir"
          :style="dividerStyle(d)"
          @mousedown="onDividerDown($event, d)"
        />
      </div>
      <!-- 工具栏：常规流式竖排（终端区与文件面板之间），不悬浮遮挡任何内容 -->
      <div class="view-tools">
          <!-- 分屏：单入口，二级菜单选择分割方向 -->
          <NDropdown
            v-if="paneCount < 4 && tab.sessionId"
            trigger="click"
            placement="bottom-end"
            :options="splitOptions"
            @select="(key: string | number) => addPane(key as 'row' | 'column')"
          >
            <NButton quaternary circle size="small" :loading="splitting">
              <NIcon :component="ArrowsSplit2" />
            </NButton>
          </NDropdown>
          <!-- 四面板入口：文件管理 / 监控 / Agent / 告警，互斥切换 -->
          <NTooltip v-if="tab.sessionId" placement="left">
            <template #trigger>
              <NButton
                quaternary
                circle
                size="small"
                :type="layout.filesVisible ? 'primary' : 'default'"
                @click="layout.toggleFiles()"
              >
                <NIcon :component="Folder" />
              </NButton>
            </template>
            文件管理（SFTP）
          </NTooltip>
          <NTooltip v-for="entry in panelEntries" :key="entry.key" placement="left">
            <template #trigger>
              <NButton
                quaternary
                circle
                size="small"
                :type="layout.monitorVisible && layout.rightTab === entry.key ? 'primary' : 'default'"
                @click="layout.toggleRightPanel(entry.key)"
              >
                <NIcon :component="entry.icon" />
              </NButton>
            </template>
            {{ entry.label }}
          </NTooltip>
      </div>
      <!-- SFTP 文件面板 -->
      <FilePanel
        v-if="layout.filesVisible && tab.sessionId && !tab.disconnected"
        :session-id="tab.sessionId"
        :nav-path="layout.filesNavPath"
        @open-split-at="(p: string) => addPane('row', p)"
      />
    </div>
    <!-- 断开遮罩 -->
    <div v-if="tab.disconnected" class="disconnect-overlay">
      <div class="disconnect-card">
        <p class="disconnect-text">会话已断开</p>
        <NButton
          type="primary"
          size="small"
          :loading="reconnecting"
          @click="handleReconnect"
        >
          <template #icon><NIcon :component="Refresh" /></template>
          重新连接
        </NButton>
      </div>
    </div>
  </div>
</template>

<style scoped>
.terminal-view {
  position: relative;
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-app);
}
.terminal-main {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 0;
}
.split-area {
  position: relative;
  flex: 1;
  min-width: 0;
  min-height: 0;
}
.pane-slot {
  position: absolute;
  display: flex;
  min-width: 0;
  min-height: 0;
}
.pane-slot :deep(.pane) {
  flex: 1;
  min-width: 0;
  min-height: 0;
}
.split-divider {
  position: absolute;
  z-index: 5;
  background: var(--border-color);
}
.split-divider.row {
  cursor: col-resize;
}
.split-divider.column {
  cursor: row-resize;
}
.split-divider:hover {
  background: var(--primary);
}
.view-tools {
  flex-shrink: 0;
  width: 38px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 8px 0;
  border-left: 1px solid var(--border-color);
  background: var(--bg-app);
}
.disconnect-overlay {
  position: absolute;
  inset: 0;
  background: rgba(15, 20, 25, 0.72);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
}
.disconnect-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 20px 28px;
  background: var(--bg-panel, #161c24);
  border: 1px solid var(--border-color);
  border-radius: 8px;
}
.disconnect-text {
  margin: 0;
  font-size: 13px;
  color: var(--text-secondary);
}
</style>
