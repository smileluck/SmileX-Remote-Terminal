<script setup lang="ts">
/**
 * MainContent - 主内容区（Xshell 式 tab 组分屏）
 *
 * tabs store 的 group 树经 computeLayout 展平为绝对定位矩形：
 * - 每个 group 顶部一个 GroupTabBar（自己的 tab 栏，可切换/拖拽）
 * - 所有 SSH tab 的 TerminalView 扁平常驻渲染（key=tab.id 不重建，
 *   xterm 与滚动历史在跨 group 拖动时不丢失），按所属 group 内容区
 *   定位，仅 group 激活 tab 可见（v-show）
 * - rdp / host / settings 按 group 激活 tab 条件渲染
 * 拖拽 tab 时每个 group 显示放置区（四边缘分裂 / 中央并入）。
 * 无 tab（group 树为空）时显示欢迎空状态。
 */
import { computed, ref } from 'vue'
import { NButton, NIcon } from 'naive-ui'
import { Terminal2, DeviceDesktop } from '@vicons/tabler'
import type { TabItem } from '@/types/session'
import { useTabsStore } from '@/stores/tabs'
import { useUiStore } from '@/stores/ui'
import { useTabDrag } from '@/composables/useTabDrag'
import { computeLayout, type DividerLayout, type Rect, type GroupNode } from '@/utils/splitTree'
import BrandMark from '@/components/common/BrandMark.vue'
import GroupTabBar from '@/components/layout/GroupTabBar.vue'
import TerminalView from '@/components/terminal/TerminalView.vue'
import DesktopView from '@/components/desktop/DesktopView.vue'
import SettingsView from '@/components/settings/SettingsView.vue'

const tabs = useTabsStore()
const ui = useUiStore()
const tabDrag = useTabDrag()

/** group tab 栏高度（px），内容区在其下方 */
const TAB_BAR_HEIGHT = 36

const mainEl = ref<HTMLDivElement | null>(null)

/** group 树 → group 槽位 + 分割条矩形（扁平绝对定位，树变化不重建组件） */
const flat = computed(() => (tabs.root ? computeLayout(tabs.root) : null))

/** groupId → rect 映射（模板取数用） */
const rectByGroup = computed(() => {
  const m = new Map<string, Rect>()
  for (const g of flat.value?.groups ?? []) m.set(g.id, g.rect)
  return m
})

/** tabId → 所属 group 映射（SSH 终端池定位/显隐用） */
const groupByTab = computed(() => {
  const m = new Map<string, GroupNode>()
  for (const g of tabs.groups) for (const t of g.tabs) m.set(t.id, g)
  return m
})

/** 所有 SSH tab：常驻渲染、按需显示，保证每个 tab 的终端实例完全独立 */
const sshTabs = computed(() => tabs.tabs.filter((t) => t.kind === 'ssh'))

const pct = (v: number) => `${(v * 100).toFixed(4)}%`

/** group tab 栏定位（group 矩形顶部，固定高度） */
function tabBarStyle(groupId: string) {
  const r = rectByGroup.value.get(groupId)
  if (!r) return {}
  return { left: pct(r.left), top: pct(r.top), width: pct(r.width), height: `${TAB_BAR_HEIGHT}px` }
}

/** group 内容区定位（tab 栏下方） */
function contentStyle(groupId: string) {
  const r = rectByGroup.value.get(groupId)
  if (!r) return {}
  return {
    left: pct(r.left),
    top: `calc(${pct(r.top)} + ${TAB_BAR_HEIGHT}px)`,
    width: pct(r.width),
    height: `calc(${pct(r.height)} - ${TAB_BAR_HEIGHT}px)`,
  }
}

/** SSH tab 是否可见：是其所属 group 的激活 tab */
function sshVisible(tab: TabItem) {
  return groupByTab.value.get(tab.id)?.activeTabId === tab.id
}

/** group 当前激活的非 SSH tab（rdp/host/settings 按需渲染） */
function activeNonSshTab(group: GroupNode): TabItem | null {
  const t = group.tabs.find((t) => t.id === group.activeTabId)
  return t && t.kind !== 'ssh' ? t : null
}

function dividerStyle(d: DividerLayout) {
  return d.dir === 'row'
    ? { left: `calc(${pct(d.center)} - 3px)`, top: pct(d.crossStart), height: pct(d.crossLength), width: '6px' }
    : { top: `calc(${pct(d.center)} - 3px)`, left: pct(d.crossStart), width: pct(d.crossLength), height: '6px' }
}

/** 分割条拖拽：只调整本 split 节点内相邻两个子节点的比例 */
function onDividerDown(e: MouseEvent, d: DividerLayout) {
  const container = mainEl.value
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

/** 放置区高亮判断（命中本 group 且 zone 一致） */
function zoneOn(groupId: string, zone: string) {
  const t = tabDrag.dropTarget.value
  return t?.groupId === groupId && t.zone === zone
}
</script>

<template>
  <div ref="mainEl" class="main-content">
    <template v-if="flat">
      <!-- 每个 group 的 tab 栏 -->
      <div
        v-for="g in flat.groups"
        :key="'bar-' + g.id"
        class="group-tabbar-slot"
        :style="tabBarStyle(g.id)"
      >
        <GroupTabBar :group-id="g.id" />
      </div>

      <!-- SSH 终端池：所有 ssh tab 常驻渲染，跨 group 拖动不重建 xterm -->
      <div
        v-for="t in sshTabs"
        :key="t.id"
        v-show="sshVisible(t)"
        class="group-content"
        :style="contentStyle(groupByTab.get(t.id)?.id ?? '')"
      >
        <TerminalView :tab="t" />
      </div>

      <!-- 非 SSH 视图：按 group 激活 tab 条件渲染 -->
      <template v-for="g in tabs.groups" :key="'view-' + g.id">
        <div v-if="activeNonSshTab(g)" class="group-content" :style="contentStyle(g.id)">
          <DesktopView
            v-if="activeNonSshTab(g)!.kind === 'rdp' || activeNonSshTab(g)!.kind === 'host'"
            :tab="activeNonSshTab(g)!"
          />
          <SettingsView v-else />
        </div>
      </template>

      <!-- 分割条 -->
      <div
        v-for="d in flat.dividers"
        :key="d.key"
        class="split-divider"
        :class="d.dir"
        :style="dividerStyle(d)"
        @mousedown="onDividerDown($event, d)"
      />

      <!-- 拖拽放置区：每个 group 一份（四边缘分裂 / 中央并入） -->
      <div
        v-for="g in flat.groups"
        v-show="tabDrag.draggingTab.value"
        :key="'drop-' + g.id"
        class="group-drop-overlay"
        :data-group-drop="g.id"
        :style="contentStyle(g.id)"
      >
        <div class="drop-zone zone-left" :class="{ on: zoneOn(g.id, 'left') }" />
        <div class="drop-zone zone-right" :class="{ on: zoneOn(g.id, 'right') }" />
        <div class="drop-zone zone-top" :class="{ on: zoneOn(g.id, 'top') }" />
        <div class="drop-zone zone-bottom" :class="{ on: zoneOn(g.id, 'bottom') }" />
        <div class="drop-zone zone-merge" :class="{ on: zoneOn(g.id, 'merge') }" />
      </div>
    </template>

    <div v-else class="empty-wrap">
      <div class="empty-body">
        <BrandMark :size="64" class="empty-logo" />
        <h2>开始你的第一个连接</h2>
        <p class="empty-hint">SSH 终端 · 远程桌面 · AI 运维助手</p>
        <div class="empty-actions">
          <NButton type="primary" size="large" @click="ui.openConnectDialog()">
            <template #icon><NIcon :component="Terminal2" /></template>
            新建 SSH
          </NButton>
          <NButton size="large" @click="ui.openDesktopConnectDialog()">
            <template #icon><NIcon :component="DeviceDesktop" /></template>
            远程桌面
          </NButton>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.main-content {
  flex: 1;
  overflow: hidden;
  display: flex;
  position: relative;
  background: var(--bg-app);
}
.group-tabbar-slot {
  position: absolute;
  z-index: 5;
}
.group-content {
  position: absolute;
  display: flex;
  min-width: 0;
  min-height: 0;
}
.split-divider {
  position: absolute;
  z-index: 10;
  background: transparent;
  transition: background 0.15s;
}
.split-divider:hover {
  background: var(--primary);
}
.split-divider.row {
  cursor: col-resize;
}
.split-divider.column {
  cursor: row-resize;
}
.group-drop-overlay {
  position: absolute;
  z-index: 20;
}
.drop-zone {
  position: absolute;
  pointer-events: none;
  background: rgba(76, 141, 255, 0.1);
  border: 1px dashed rgba(76, 141, 255, 0.4);
  border-radius: 6px;
  transition:
    background 0.12s,
    border-color 0.12s;
}
.drop-zone.on {
  background: rgba(76, 141, 255, 0.28);
  border: 1px solid var(--primary);
}
.zone-left {
  left: 4px;
  top: 4px;
  bottom: 4px;
  width: 30%;
}
.zone-right {
  right: 4px;
  top: 4px;
  bottom: 4px;
  width: 30%;
}
.zone-top {
  left: calc(30% + 8px);
  right: calc(30% + 8px);
  top: 4px;
  height: 30%;
}
.zone-bottom {
  left: calc(30% + 8px);
  right: calc(30% + 8px);
  bottom: 4px;
  height: 30%;
}
.zone-merge {
  left: calc(30% + 8px);
  right: calc(30% + 8px);
  top: calc(30% + 8px);
  bottom: calc(30% + 8px);
  background: transparent;
  border-color: transparent;
}
.zone-merge.on {
  background: rgba(76, 141, 255, 0.12);
  border: 1px dashed rgba(76, 141, 255, 0.4);
}
.empty-wrap {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  /* 微弱的径向光晕，给空状态一点空间感 */
  background: radial-gradient(
    ellipse 60% 50% at 50% 40%,
    rgba(76, 141, 255, 0.06),
    transparent 70%
  );
}
.empty-body {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
}
.empty-logo {
  margin-bottom: 24px;
  filter: drop-shadow(0 8px 24px rgba(76, 141, 255, 0.35));
}
.empty-body h2 {
  font-size: 20px;
  font-weight: 600;
  color: var(--text-primary);
  letter-spacing: 0.2px;
  margin-bottom: 8px;
}
.empty-hint {
  font-size: 13px;
  color: var(--text-secondary);
  margin-bottom: 32px;
}
.empty-actions {
  display: flex;
  gap: 12px;
}
</style>
