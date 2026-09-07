import { defineStore } from 'pinia'
import { ref, watch } from 'vue'

const STORAGE_KEY = 'smilex-layout-v1'

/** 右栏面板页签（单页签显示，切换制，不同时叠加多个面板） */
export type RightPanelTab = 'agent' | 'monitor' | 'alerts'

/** 侧栏列表页签（SSH 会话 / 远程桌面） */
export type SidebarTab = 'ssh' | 'desktop'

interface LayoutState {
  sidebarCollapsed: boolean
  sidebarTab: SidebarTab
  monitorVisible: boolean
  monitorWidth: number
  rightTab: RightPanelTab
  filesWidth: number
}

/** localStorage 读取（容错：损坏时回退默认值） */
function loadState(): LayoutState {
  const defaults: LayoutState = {
    sidebarCollapsed: false,
    sidebarTab: 'ssh',
    monitorVisible: true,
    monitorWidth: 320,
    rightTab: 'monitor',
    filesWidth: 240,
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
 * - 右栏（Agent 助手 / 监控看版）显示/隐藏 + 宽度 + 页签
 * 状态持久化到 localStorage。
 */
export const useLayoutStore = defineStore('layout', () => {
  const initial = loadState()

  /** 左栏（SideBar）是否折叠（折叠后仅 ActivityRail 图标列） */
  const sidebarCollapsed = ref(initial.sidebarCollapsed)
  /** 左栏当前页签（SSH 会话 / 远程桌面） */
  const sidebarTab = ref<SidebarTab>(initial.sidebarTab)
  /** 右栏是否可见 */
  const monitorVisible = ref(initial.monitorVisible)
  /** 右栏宽度（px，拖拽调宽） */
  const monitorWidth = ref(initial.monitorWidth)
  /** 右栏当前页签（Agent 助手 / 监控） */
  const rightTab = ref<RightPanelTab>(initial.rightTab)
  /** 终端文件面板（SFTP）宽度（px，拖拽调宽） */
  const filesWidth = ref(initial.filesWidth)
  /** 终端文件面板（SFTP）是否打开（与右栏三面板互斥；不持久化） */
  const filesVisible = ref(false)

  /** 切换左栏折叠 */
  function toggleSidebar() {
    sidebarCollapsed.value = !sidebarCollapsed.value
  }

  /** 切换左栏页签（VSCode 式：已在当前页签且展开时折叠，否则切换并展开） */
  function toggleSidebarTab(tab: SidebarTab) {
    if (!sidebarCollapsed.value && sidebarTab.value === tab) {
      sidebarCollapsed.value = true
    } else {
      sidebarTab.value = tab
      sidebarCollapsed.value = false
    }
  }

  /** 切换右栏显示/隐藏 */
  function toggleMonitor() {
    monitorVisible.value = !monitorVisible.value
  }

  /** 切换右栏页签 */
  function setRightTab(tab: RightPanelTab) {
    rightTab.value = tab
  }

  /** 打开右栏（可同时指定页签） */
  function openRightPanel(tab?: RightPanelTab) {
    if (tab) rightTab.value = tab
    monitorVisible.value = true
    filesVisible.value = false
  }

  /** 单页签切换语义：已在目标页签时收起右栏，否则切到该页签（⌘M/⌘J 与工具栏按钮共用） */
  function toggleRightPanel(tab: RightPanelTab) {
    if (monitorVisible.value && rightTab.value === tab) {
      monitorVisible.value = false
    } else {
      rightTab.value = tab
      monitorVisible.value = true
      filesVisible.value = false
    }
  }

  /** 切换文件面板（与右栏三面板互斥：打开时收起右栏） */
  function toggleFiles() {
    if (filesVisible.value) {
      filesVisible.value = false
    } else {
      filesVisible.value = true
      monitorVisible.value = false
    }
  }

  // 持久化（watch 深度变化写入 localStorage）
  watch(
    [sidebarCollapsed, sidebarTab, monitorVisible, monitorWidth, rightTab, filesWidth],
    () => {
      try {
        localStorage.setItem(
          STORAGE_KEY,
          JSON.stringify({
            sidebarCollapsed: sidebarCollapsed.value,
            sidebarTab: sidebarTab.value,
            monitorVisible: monitorVisible.value,
            monitorWidth: monitorWidth.value,
            rightTab: rightTab.value,
            filesWidth: filesWidth.value,
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
    sidebarTab,
    monitorVisible,
    monitorWidth,
    rightTab,
    filesWidth,
    filesVisible,
    toggleSidebar,
    toggleSidebarTab,
    toggleMonitor,
    setRightTab,
    openRightPanel,
    toggleRightPanel,
    toggleFiles,
  }
})
