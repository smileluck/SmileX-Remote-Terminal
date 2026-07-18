import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { TabItem, SessionKind } from '@/types/session'

/**
 * 标签管理 store
 *
 * 管理所有打开的标签（SSH/远程桌面/AI），按 SessionKind 渲染对应视图。
 */
export const useTabsStore = defineStore('tabs', () => {
  /** 所有标签 */
  const tabs = ref<TabItem[]>([])
  /** 当前活动标签 ID */
  const activeId = ref<string | null>(null)

  /** 活动标签对象 */
  const activeTab = computed(() => tabs.value.find((t) => t.id === activeId.value) || null)

  /** 新增标签 */
  function addTab(kind: SessionKind, title: string, sessionId?: string): TabItem {
    const id = `tab-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
    const tab: TabItem = { id, kind, title, sessionId }
    tabs.value.push(tab)
    activeId.value = id
    return tab
  }

  /** 关闭标签 */
  function closeTab(id: string) {
    const idx = tabs.value.findIndex((t) => t.id === id)
    if (idx === -1) return
    tabs.value.splice(idx, 1)
    // 若关闭的是活动标签，切到相邻
    if (activeId.value === id) {
      const next = tabs.value[idx] || tabs.value[idx - 1] || null
      activeId.value = next?.id ?? null
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
    setActive,
    updateTab,
  }
})
