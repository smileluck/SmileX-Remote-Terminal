import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

const STORAGE_KEY = 'smilex-layout-v1'

interface LayoutState {
  sidebarCollapsed: boolean
  monitorVisible: boolean
  monitorWidth: number
}

/** localStorage 读取（容错：损坏时回退默认值） */
function loadState(): LayoutState {
  const defaults: LayoutState = {
    sidebarCollapsed: false,
    monitorVisible: true,
    monitorWidth: 320,
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return defaults
    return { ...defaults, ...JSON.parse(raw) }
  } catch {
    return defaults
  }
}

/**
 * 布局状态 store
 *
 * 管理三栏布局的折叠状态：
 * - 左栏（会话管理）折叠/展开
 * - 右栏（监控看版）显示/隐藏 + 宽度
 * 状态持久化到 localStorage。
 */
export const useLayoutStore = defineStore('layout', () => {
  const initial = loadState()

  /** 左栏（SideBar）是否折叠（折叠后仅 ActivityRail 图标列） */
  const sidebarCollapsed = ref(initial.sidebarCollapsed)
  /** 右栏（监控看版）是否可见 */
  const monitorVisible = ref(initial.monitorVisible)
  /** 右栏宽度（px，拖拽调宽） */
  const monitorWidth = ref(initial.monitorWidth)

  /** 切换左栏折叠 */
  function toggleSidebar() {
    sidebarCollapsed.value = !sidebarCollapsed.value
  }

  /** 切换右栏监控面板 */
  function toggleMonitor() {
    monitorVisible.value = !monitorVisible.value
  }

  // 持久化（watch 深度变化写入 localStorage）
  watch(
    [sidebarCollapsed, monitorVisible, monitorWidth],
    () => {
      try {
        localStorage.setItem(
          STORAGE_KEY,
          JSON.stringify({
            sidebarCollapsed: sidebarCollapsed.value,
            monitorVisible: monitorVisible.value,
            monitorWidth: monitorWidth.value,
          }),
        )
      } catch {
        /* 存储不可用时忽略 */
      }
    },
    { deep: true },
  )

  return {
    sidebarCollapsed,
    monitorVisible,
    monitorWidth,
    toggleSidebar,
    toggleMonitor,
  }
})
