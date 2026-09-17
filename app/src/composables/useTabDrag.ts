/**
 * useTabDrag - 会话 Tab 拖拽分屏（共享状态）
 *
 * GroupTabBar 发起拖拽（pointer 事件 + ghost，与 SnippetPanel 同模式），
 * MainContent 依据本模块状态为每个 group 渲染放置区；放置结果由
 * GroupTabBar 在 pointerup 时消费：命中 tab 栏 / 中央区 = moveTabToGroup
 * 并入该 group；命中四边缘 = splitWithTab 分裂出新 group。
 */
import { ref } from 'vue'
import type { TabItem } from '@/types/session'

/** 放置区：四边缘分裂 + merge（tab 栏 / 中央区并入该 group） */
export type DropZoneKind = 'left' | 'right' | 'top' | 'bottom' | 'merge'

export interface DropTarget {
  groupId: string
  zone: DropZoneKind
}

/** 正在拖拽的 tab（null = 未拖拽） */
const draggingTab = ref<TabItem | null>(null)
/** 当前命中的放置目标（由拖拽方按指针位置计算） */
const dropTarget = ref<DropTarget | null>(null)

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
    dropTarget.value = null
    ghost = createGhost(tab.title)
  }

  function moveGhost(x: number, y: number) {
    if (ghost) {
      ghost.style.left = `${x + 12}px`
      ghost.style.top = `${y + 12}px`
    }
  }

  function setDropTarget(target: DropTarget | null) {
    dropTarget.value = target
  }

  /** 结束拖拽：有命中放置目标时返回 { tab, target }，否则返回 null（并清理状态） */
  function endDrag(): { tab: TabItem; target: DropTarget } | null {
    const tab = draggingTab.value
    const target = dropTarget.value
    cleanup()
    return tab && target ? { tab, target } : null
  }

  function cancelDrag() {
    cleanup()
  }

  function cleanup() {
    ghost?.remove()
    ghost = null
    draggingTab.value = null
    dropTarget.value = null
  }

  return {
    draggingTab,
    dropTarget,
    startDrag,
    moveGhost,
    setDropTarget,
    endDrag,
    cancelDrag,
  }
}
