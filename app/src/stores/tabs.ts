import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { TabItem, SessionKind } from '@/types/session'
import * as sessionService from '@/services/session'
import { useMonitorStore } from '@/stores/monitor'

/**
 * 标签管理 store
 *
 * 管理所有打开的标签（SSH/远程桌面/AI），按 SessionKind 渲染对应视图。
 * closeTab 联动断开会话 + 停止监控采样，避免后端资源泄漏。
 */
export const useTabsStore = defineStore('tabs', () => {
  /** 所有标签 */
  const tabs = ref<TabItem[]>([])
  /** 当前活动标签 ID */
  const activeId = ref<string | null>(null)

  /** 活动标签对象 */
  const activeTab = computed(() => tabs.value.find((t) => t.id === activeId.value) || null)

  /** 新增标签 */
  function addTab(
    kind: SessionKind,
    title: string,
    sessionId?: string,
    profileId?: string,
  ): TabItem {
    const id = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    const tab: TabItem = { id, kind, title, sessionId, profileId }
    tabs.value.push(tab)
    activeId.value = id
    return tab
  }

  /** 关闭标签（联动断开会话与监控采样） */
  function closeTab(id: string) {
    const idx = tabs.value.findIndex((t) => t.id === id)
    if (idx === -1) return
    const [tab] = tabs.value.splice(idx, 1)

    // 联动清理后端会话（真实 sessionId 才处理；'connected' 为快速表单占位）
    if (tab.sessionId && tab.sessionId !== 'connected') {
      const sessionId = tab.sessionId
      useMonitorStore().stopSampling(sessionId)
      sessionService.disconnect(sessionId).catch(() => {
        /* 会话可能已断开 */
      })
    }

    // 若关闭的是活动标签，切到相邻
    if (activeId.value === id) {
      const next = tabs.value[idx] || tabs.value[idx - 1] || null
      activeId.value = next?.id ?? null
    }
  }

  /** 关闭指定标签左侧的所有标签 */
  function closeTabsToLeft(id: string) {
    const idx = tabs.value.findIndex((t) => t.id === id)
    if (idx <= 0) return
    for (const tab of tabs.value.slice(0, idx)) closeTab(tab.id)
  }

  /** 关闭指定标签右侧的所有标签 */
  function closeTabsToRight(id: string) {
    const idx = tabs.value.findIndex((t) => t.id === id)
    if (idx === -1) return
    for (const tab of tabs.value.slice(idx + 1)) closeTab(tab.id)
  }

  /** 关闭所有标签 */
  function closeAllTabs() {
    for (const tab of [...tabs.value]) closeTab(tab.id)
  }

  /** 关闭除指定标签外的所有标签 */
  function closeOtherTabs(id: string) {
    for (const tab of [...tabs.value]) {
      if (tab.id !== id) closeTab(tab.id)
    }
  }

  /** 切换活动标签 */
  function setActive(id: string) {
    activeId.value = id
  }

  /** 更新标签字段（如 sessionId/error） */
  function updateTab(id: string, patch: Partial<TabItem>) {
    const tab = tabs.value.find((t) => t.id === id)
    if (tab) Object.assign(tab, patch)
  }

  return {
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
    updateTab,
  }
})
