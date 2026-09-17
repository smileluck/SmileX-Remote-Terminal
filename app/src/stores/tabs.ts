import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { TabItem, SessionKind } from '@/types/session'
import * as sessionService from '@/services/session'
import { useMonitorStore } from '@/stores/monitor'
import {
  collectGroups,
  countGroups,
  findGroup,
  genNodeId,
  removeGroup,
  splitAtGroup,
  type GroupNode,
  type LayoutNode,
} from '@/utils/splitTree'

/** 分屏 group 数上限（ tab 栏高度固定，过多 group 可用面积不足） */
const MAX_GROUPS = 4

/**
 * 标签管理 store（Xshell 式 tab 组模型）
 *
 * 窗口为一棵可递归分割的 group 树（见 utils/splitTree）：每个 group 有
 * 自己的 tab 栏，容纳多个 tab，点击切换；tab 可跨 group 移动或拖到
 * 边缘分裂出新 group。group 最后一个 tab 被关闭/移出时折叠消失。
 *
 * 对外保持扁平 API（tabs / activeId / activeTab ...）：tabs 为全 group
 * 的 DFS 展平，activeId 为聚焦 group 的激活 tab——面板/命令面板等
 * 只读消费方无需感知 group 结构。
 *
 * closeTab 联动断开会话 + 停止监控采样，避免后端资源泄漏。
 */
export const useTabsStore = defineStore('tabs', () => {
  /** group 树根（null = 无 tab，显示欢迎页） */
  const root = ref<LayoutNode | null>(null)
  /** 聚焦 group（其激活 tab 即全局 activeTab，右栏面板跟随） */
  const focusedGroupId = ref<string | null>(null)

  /** 所有 group（DFS 序） */
  const groups = computed<GroupNode[]>(() => (root.value ? collectGroups(root.value) : []))
  /** 所有标签（全 group DFS 展平；⌘1-9 / Ctrl+Tab 按此顺序） */
  const tabs = computed<TabItem[]>(() => groups.value.flatMap((g) => g.tabs))
  /** 聚焦 group */
  const focusedGroup = computed(
    () => groups.value.find((g) => g.id === focusedGroupId.value) ?? null,
  )
  /** 当前活动标签 ID（聚焦 group 的激活 tab） */
  const activeId = computed(() => focusedGroup.value?.activeTabId ?? null)
  /** 活动标签对象 */
  const activeTab = computed(() => tabs.value.find((t) => t.id === activeId.value) || null)

  function createGroup(tabItems: TabItem[]): GroupNode {
    return { kind: 'group', id: genNodeId('g'), tabs: tabItems, activeTabId: tabItems[0]?.id ?? null }
  }

  /** tab 所属 group */
  function groupOf(tabId: string): GroupNode | null {
    return groups.value.find((g) => g.tabs.some((t) => t.id === tabId)) ?? null
  }

  /** group 空了则折叠，并修正聚焦 group */
  function collapseIfEmpty(group: GroupNode) {
    if (group.tabs.length > 0 || !root.value) return
    root.value = removeGroup(root.value, group.id)
    if (focusedGroupId.value === group.id || !findGroupSafe(focusedGroupId.value)) {
      focusedGroupId.value = root.value ? (collectGroups(root.value)[0]?.id ?? null) : null
    }
  }

  function findGroupSafe(groupId: string | null): GroupNode | null {
    return root.value && groupId ? findGroup(root.value, groupId) : null
  }

  /** 新增标签（进聚焦 group 并激活；无 group 时创建首个） */
  function addTab(
    kind: SessionKind,
    title: string,
    sessionId?: string,
    profileId?: string,
  ): TabItem {
    const id = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    const tab: TabItem = { id, kind, title, sessionId, profileId }
    if (!root.value) {
      const g = createGroup([tab])
      root.value = g
      focusedGroupId.value = g.id
    } else {
      const g = focusedGroup.value ?? groups.value[0]
      g.tabs.push(tab)
      g.activeTabId = tab.id
      focusedGroupId.value = g.id
    }
    return tab
  }

  /** 关闭标签（联动断开会话与监控采样；group 空了折叠） */
  function closeTab(id: string) {
    const g = groupOf(id)
    if (!g) return
    const idx = g.tabs.findIndex((t) => t.id === id)
    const [tab] = g.tabs.splice(idx, 1)

    // 联动清理后端会话（真实 sessionId 才处理；'connected' 为快速表单占位）
    if (tab.sessionId && tab.sessionId !== 'connected') {
      const sessionId = tab.sessionId
      useMonitorStore().stopSampling(sessionId)
      sessionService.disconnect(sessionId).catch(() => {
        /* 会话可能已断开 */
      })
    }

    // 若关闭的是 group 激活 tab，切到相邻
    if (g.activeTabId === id) {
      const next = g.tabs[idx] || g.tabs[idx - 1] || null
      g.activeTabId = next?.id ?? null
    }
    collapseIfEmpty(g)
  }

  /** 切换活动标签（同时聚焦其所属 group） */
  function setActive(id: string) {
    const g = groupOf(id)
    if (!g) return
    g.activeTabId = id
    focusedGroupId.value = g.id
  }

  /**
   * 移动 tab 到目标 group（拖到其 tab 栏 / 中央区）。
   * 同 group 时仅激活；源 group 空了折叠。
   */
  function moveTabToGroup(tabId: string, groupId: string) {
    if (!root.value) return
    const src = groupOf(tabId)
    const dst = findGroup(root.value, groupId)
    if (!src || !dst) return
    if (src.id === dst.id) {
      setActive(tabId)
      return
    }
    const idx = src.tabs.findIndex((t) => t.id === tabId)
    const [tab] = src.tabs.splice(idx, 1)
    if (src.activeTabId === tabId) src.activeTabId = src.tabs[idx]?.id ?? src.tabs[idx - 1]?.id ?? null
    dst.tabs.push(tab)
    dst.activeTabId = tab.id
    focusedGroupId.value = dst.id
    collapseIfEmpty(src)
  }

  /**
   * 拖 tab 到目标 group 边缘：分裂出新 group 容纳该 tab（Xshell 式移动，非克隆）。
   * 返回 false 表示未执行（目标不存在 / 已达上限 / 源 group 仅此 tab 且拖到自身边缘），
   * 调用方可据此提示。
   */
  function splitWithTab(
    tabId: string,
    targetGroupId: string,
    dir: 'row' | 'column',
    before: boolean,
  ): boolean {
    if (!root.value) return false
    const src = groupOf(tabId)
    const target = findGroup(root.value, targetGroupId)
    if (!src || !target) return false
    if (src.id === target.id && src.tabs.length === 1) return false
    if (countGroups(root.value) >= MAX_GROUPS) return false

    const idx = src.tabs.findIndex((t) => t.id === tabId)
    const [tab] = src.tabs.splice(idx, 1)
    if (src.activeTabId === tabId) src.activeTabId = src.tabs[idx]?.id ?? src.tabs[idx - 1]?.id ?? null
    const g = createGroup([tab])
    root.value = splitAtGroup(root.value, targetGroupId, dir, g, before)
    focusedGroupId.value = g.id
    collapseIfEmpty(src)
    return true
  }

  /** 聚焦上/下一个 group（DFS 序循环） */
  function cycleGroup(delta: number) {
    const gs = groups.value
    if (gs.length < 2) return
    const idx = gs.findIndex((g) => g.id === focusedGroupId.value)
    const next = ((idx < 0 ? 0 : idx) + delta + gs.length) % gs.length
    focusedGroupId.value = gs[next].id
  }

  /** 关闭指定标签左侧的所有标签（同 group 内） */
  function closeTabsToLeft(id: string) {
    const g = groupOf(id)
    if (!g) return
    const idx = g.tabs.findIndex((t) => t.id === id)
    if (idx <= 0) return
    for (const tab of g.tabs.slice(0, idx)) closeTab(tab.id)
  }

  /** 关闭指定标签右侧的所有标签（同 group 内） */
  function closeTabsToRight(id: string) {
    const g = groupOf(id)
    if (!g) return
    const idx = g.tabs.findIndex((t) => t.id === id)
    if (idx === -1) return
    for (const tab of g.tabs.slice(idx + 1)) closeTab(tab.id)
  }

  /** 关闭所有标签 */
  function closeAllTabs() {
    for (const tab of [...tabs.value]) closeTab(tab.id)
  }

  /** 关闭同 group 内除指定标签外的所有标签 */
  function closeOtherTabs(id: string) {
    const g = groupOf(id)
    if (!g) return
    for (const tab of [...g.tabs]) {
      if (tab.id !== id) closeTab(tab.id)
    }
  }

  /** 按序号切换（快捷键 ⌘1~9；index 越界时钳制到边界，9 常用于跳到最后一个） */
  function setActiveByIndex(index: number) {
    const n = tabs.value.length
    if (!n) return
    const i = Math.max(0, Math.min(index, n - 1))
    setActive(tabs.value[i].id)
  }

  /** 循环切换（快捷键 Ctrl+Tab / Ctrl+Shift+Tab；delta=±1） */
  function cycleActive(delta: number) {
    const n = tabs.value.length
    if (n < 2) return
    const cur = tabs.value.findIndex((t) => t.id === activeId.value)
    const next = ((cur < 0 ? 0 : cur) + delta + n) % n
    setActive(tabs.value[next].id)
  }

  /** 更新标签字段（如 sessionId/error） */
  function updateTab(id: string, patch: Partial<TabItem>) {
    const tab = tabs.value.find((t) => t.id === id)
    if (tab) Object.assign(tab, patch)
  }

  return {
    root,
    focusedGroupId,
    groups,
    tabs,
    activeId,
    activeTab,
    addTab,
    closeTab,
    closeTabsToLeft,
    closeTabsToRight,
    closeAllTabs,
    closeOtherTabs,
    setActive,
    setActiveByIndex,
    cycleActive,
    updateTab,
    groupOf,
    moveTabToGroup,
    splitWithTab,
    cycleGroup,
  }
})
