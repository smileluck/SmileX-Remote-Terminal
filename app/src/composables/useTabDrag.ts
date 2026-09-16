/**
 * useTabDrag - 会话 Tab 拖拽分屏（共享状态）
 *
 * TabBar 发起拖拽（pointer 事件 + ghost，与 SnippetPanel 同模式），
 * MainContent 依据本模块状态渲染放置区；放置结果由 TabBar 在
 * pointerup 时消费（detachTabForSplit + layout.requestSplit）。
 */
import { reactive, ref } from 'vue'
import type { TabItem } from '@/types/session'

export type DropZone = 'left' | 'right' | 'top' | 'bottom'

/** 正在拖拽的 tab（null = 未拖拽） */
const draggingTab = ref<TabItem | null>(null)
/** 当前命中的放置区（由拖拽方按指针位置计算） */
const dropZone = ref<DropZone | null>(null)
/** 各 tab 分屏窗格数（TerminalView 上报，放置区显示门控用） */
const paneCounts = reactive(new Map<string, number>())

let ghost: HTMLElement | null = null

function createGhost(title: string): HTMLElement {
  const el = document.createElement('div')
  el.textContent = title
  el.style.cssText = [
    'position:fixed',
    'z-index:9999',
    'pointer-events:none',
    'padding:4px 12px',
    'border-radius:6px',
    'font-size:12px',
    'white-space:nowrap',
    'max-width:220px',
    'overflow:hidden',
    'text-overflow:ellipsis',
    'background:var(--bg-elevated, #1c2530)',
    'color:var(--text-primary, #e6e9ef)',
    'border:1px solid var(--border-color, #2a3442)',
    'box-shadow:0 4px 16px rgba(0,0,0,0.35)',
    'opacity:0.9',
  ].join(';')
  document.body.appendChild(el)
  return el
}

export function useTabDrag() {
  /** 开始拖拽：记录源 tab 并创建跟随光标的 ghost */
  function startDrag(tab: TabItem) {
    draggingTab.value = tab
    dropZone.value = null
    ghost = createGhost(tab.title)
  }

  function moveGhost(x: number, y: number) {
    if (ghost) {
      ghost.style.left = `${x + 12}px`
      ghost.style.top = `${y + 12}px`
    }
  }

  function setDropZone(zone: DropZone | null) {
    dropZone.value = zone
  }

  /** 结束拖拽：有命中放置区时返回 { tab, zone }，否则返回 null（并清理状态） */
  function endDrag(): { tab: TabItem; zone: DropZone } | null {
    const tab = draggingTab.value
    const zone = dropZone.value
    cleanup()
    return tab && zone ? { tab, zone } : null
  }

  function cancelDrag() {
    cleanup()
  }

  function cleanup() {
    ghost?.remove()
    ghost = null
    draggingTab.value = null
    dropZone.value = null
  }

  /** TerminalView 上报本 tab 的窗格数（卸载时 clearPaneCount） */
  function setPaneCount(tabId: string, n: number) {
    paneCounts.set(tabId, n)
  }

  function clearPaneCount(tabId: string) {
    paneCounts.delete(tabId)
  }

  return {
    draggingTab,
    dropZone,
    paneCounts,
    startDrag,
    moveGhost,
    setDropZone,
    endDrag,
    cancelDrag,
    setPaneCount,
    clearPaneCount,
  }
}
