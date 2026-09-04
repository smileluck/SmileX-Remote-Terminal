<script setup lang="ts">
/**
 * SplitLayout - 分屏布局树递归渲染组件
 *
 * - pane 节点：渲染 PaneTerminal（选中高亮 / 关闭 / 绑定会话）
 * - split 节点：flex 容器渲染子节点 + 分割条（拖拽只调整本节点内
 *   相邻子节点的比例，方向随节点自身，互不影响）
 * 交互事件逐层上抛，由 TerminalView 统一处理。
 */
import PaneTerminal from './PaneTerminal.vue'
import type { LayoutNode } from './splitTree'

const props = defineProps<{
  node: LayoutNode
  /** 当前选中 pane id（高亮与分屏目标） */
  activeId: string
  /** pane 总数（>1 才允许关闭） */
  paneCount: number
}>()

const emit = defineEmits<{
  (e: 'pane-focus', paneId: string): void
  (e: 'pane-close', paneId: string): void
  (e: 'pane-bind', paneId: string, sid: string): void
}>()

/** 分割条拖拽：只调整本 split 节点内相邻两个子节点的比例 */
function onDividerDown(e: MouseEvent, index: number) {
  const node = props.node
  if (node.kind !== 'split') return
  const container = (e.currentTarget as HTMLElement).parentElement
  if (!container) return
  const horizontal = node.dir === 'row'
  const total = horizontal ? container.clientWidth : container.clientHeight
  if (!total) return
  const startX = horizontal ? e.clientX : e.clientY
  const a = node.ratios[index]
  const b = node.ratios[index + 1]
  const onMove = (ev: MouseEvent) => {
    const delta =
      (((horizontal ? ev.clientX : ev.clientY) - startX) / total) * (a + b)
    node.ratios[index] = Math.max(0.1, a + delta)
    node.ratios[index + 1] = Math.max(0.1, b - delta)
  }
  const onUp = () => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
  e.preventDefault()
}

/** split 子节点 key：pane 用自身 id；split 无自身状态，用位置索引即可 */
function childKey(child: LayoutNode, i: number): string {
  return child.kind === 'pane' ? child.id : `split-${i}`
}

/** pane 事件上抛（事件回调里不做 v-if 类型收窄，运行时再校验） */
function emitPaneFocus() {
  if (props.node.kind === 'pane') emit('pane-focus', props.node.id)
}
function emitPaneClose() {
  if (props.node.kind === 'pane') emit('pane-close', props.node.id)
}
function emitPaneBind(sid: string) {
  if (props.node.kind === 'pane') emit('pane-bind', props.node.id, sid)
}
</script>

<template>
  <!-- 叶子：终端窗格 -->
  <PaneTerminal
    v-if="node.kind === 'pane'"
    :session-id="node.sessionId"
    :active="node.id === activeId"
    :closable="paneCount > 1"
    @focus="emitPaneFocus"
    @close="emitPaneClose"
    @bind="emitPaneBind"
  />
  <!-- 容器：按节点方向分割 -->
  <div v-else class="split-node" :class="node.dir">
    <template v-for="(child, i) in node.children" :key="childKey(child, i)">
      <SplitLayout
        class="split-child"
        :style="{ flexGrow: node.ratios[i], flexBasis: 0 }"
        :node="child"
        :active-id="activeId"
        :pane-count="paneCount"
        @pane-focus="(id: string) => emit('pane-focus', id)"
        @pane-close="(id: string) => emit('pane-close', id)"
        @pane-bind="(id: string, sid: string) => emit('pane-bind', id, sid)"
      />
      <div
        v-if="i < node.children.length - 1"
        class="split-divider"
        :class="node.dir"
        @mousedown="onDividerDown($event, i)"
      />
    </template>
  </div>
</template>

<style scoped>
.split-node {
  display: flex;
  min-width: 0;
  min-height: 0;
}
.split-node.row {
  flex-direction: row;
}
.split-node.column {
  flex-direction: column;
}
.split-child {
  min-width: 0;
  min-height: 0;
}
.split-divider {
  flex-shrink: 0;
  background: var(--border-color);
  z-index: 4;
}
.split-divider.row {
  width: 4px;
  cursor: col-resize;
  margin: 0 1px;
}
.split-divider.column {
  height: 4px;
  cursor: row-resize;
  margin: 1px 0;
}
.split-divider:hover {
  background: var(--primary);
}
</style>
