<script setup lang="ts">
/**
 * TerminalView - SSH 终端视图（支持分屏）
 *
 * - tab 已带 sessionId（统一连接入口创建）：分屏窗格平铺绝对定位渲染（≤4 窗格）
 * - 分屏作用于当前选中窗格：在选中 pane 位置原位分割，不重排其他区域；
 *   树结构只换算为矩形，窗格组件不重建（关闭/分割不清空其他窗格的终端内容）
 * - 无 sessionId 的窗格：显示「绑定现有会话 / 新建连接」选择器（见 PaneTerminal）
 * - 会话意外断开（disconnected）：显示断开遮罩 + 重新连接按钮（原地重连）
 * - 右侧工具栏（流式竖排，不悬浮）：SFTP 文件面板 / 分屏 / 监控看板 / AI 运维助手
 */
import { ref, computed, onUnmounted, watch } from 'vue'
import { NButton, NIcon, NTooltip, useMessage } from 'naive-ui'
import {
  Refresh,
  Folder,
  ArrowsSplit2,
  LayoutRows,
  ChartAreaLine,
  Robot,
} from '@vicons/tabler'
import * as sessionService from '@/services/session'
import { useConnectFlow } from '@/composables/useConnectFlow'
import { useProfilesStore } from '@/stores/profiles'
import { useMonitorStore } from '@/stores/monitor'
import { useLayoutStore } from '@/stores/layout'
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
const message = useMessage()
const { reconnectInTab } = useConnectFlow()

/** AI 运维助手开关（⌘J 同语义：右栏已在 Agent 页签时收起，否则打开并切换） */
function toggleAgent() {
  if (layout.monitorVisible && layout.rightTab === 'agent') layout.toggleMonitor()
  else layout.openRightPanel('agent')
}

/** SFTP 文件面板开关 */
const showFiles = ref(false)
/** 重连中 */
const reconnecting = ref(false)

/** 重连目标档案（tab 带 profileId 时可用） */
const profile = computed(() =>
  props.tab.profileId ? profiles.profiles.find((p) => p.id === props.tab.profileId) : null,
)

/** 分屏窗格绑定会话时写入的首行提示（主窗格有远端提示符，返回 undefined） */
const initialLineFor = (paneId: string): string | undefined => {
  if (paneId === primaryPaneId.value) return undefined
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
const root = ref<LayoutNode>({ kind: 'pane', id: 'p0', sessionId: props.tab.sessionId ?? null })
/** 当前选中 pane（分屏作用目标；点击 pane 时更新） */
const activePaneId = ref('p0')
/** 主 pane（承载 tab 主会话，重连成功后更新绑定） */
const primaryPaneId = ref('p0')
const paneCount = computed(() => countPanes(root.value))

watch(
  () => props.tab.sessionId,
  (sid) => {
    // 重连成功：主 pane 绑定新会话
    if (!sid) return
    const pane = findPane(root.value, primaryPaneId.value)
    if (pane) pane.sessionId = sid
  },
)

/** 分屏：在当前选中 pane 位置原位分割（row = 向右，column = 向下） */
const splitting = ref(false)
async function addPane(dir: 'row' | 'column') {
  if (paneCount.value >= 4 || splitting.value) return
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
  for (const sid of [...ownSessions]) disconnectIfOwn(sid)
})

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
          <NTooltip v-if="tab.sessionId" placement="left">
            <template #trigger>
              <NButton
                quaternary
                circle
                size="small"
                :type="showFiles ? 'primary' : 'default'"
                @click="showFiles = !showFiles"
              >
                <NIcon :component="Folder" />
              </NButton>
            </template>
            文件管理（SFTP）
          </NTooltip>
          <NTooltip v-if="paneCount < 4 && tab.sessionId" placement="left">
            <template #trigger>
              <NButton quaternary circle size="small" :loading="splitting" @click="addPane('row')">
                <NIcon :component="ArrowsSplit2" style="transform: rotate(90deg)" />
              </NButton>
            </template>
            向右分屏（克隆当前选中窗格的连接）
          </NTooltip>
          <NTooltip v-if="paneCount < 4 && tab.sessionId" placement="left">
            <template #trigger>
              <NButton quaternary circle size="small" :loading="splitting" @click="addPane('column')">
                <NIcon :component="LayoutRows" />
              </NButton>
            </template>
            向下分屏（克隆当前选中窗格的连接）
          </NTooltip>
          <NTooltip placement="left">
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
            监控看板（⌘M）
          </NTooltip>
          <NTooltip placement="left">
            <template #trigger>
              <NButton
                quaternary
                circle
                size="small"
                :type="layout.monitorVisible && layout.rightTab === 'agent' ? 'primary' : 'default'"
                @click="toggleAgent()"
              >
                <NIcon :component="Robot" />
              </NButton>
            </template>
            AI 运维助手（⌘J）
          </NTooltip>
      </div>
      <!-- SFTP 文件面板 -->
      <FilePanel
        v-if="showFiles && tab.sessionId && !tab.disconnected"
        :session-id="tab.sessionId"
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
