/**
 * splitTree - 分屏布局树（数据结构与操作）
 *
 * 递归 split 结构：pane 为叶子（一个终端窗格），split 为分割组
 * （带方向 row/column 与子节点比例）。每个分割条的方向与比例随所属
 * 节点独立，分割/关闭只影响目标 pane 所在子树，不重排其他区域。
 *
 * 渲染采用 computeLayout 产出的扁平绝对定位（见 TerminalView）：
 * 树结构变化不会导致窗格组件重建，xterm 实例与历史内容得以保留。
 */

/** 节点 id 序列（同毫秒多次分割/关闭也不冲突） */
let nodeSeq = 0
export function genNodeId(prefix: string): string {
  return `${prefix}${++nodeSeq}`
}

/** 叶子节点：一个终端窗格 */
export interface PaneNode {
  kind: 'pane'
  id: string
  sessionId: string | null
}

/** 容器节点：一个分割组 */
export interface SplitNode {
  kind: 'split'
  /** 稳定 id（分割条 key 用；树结构变化时保持不变） */
  id: string
  /** 分割方向：row（水平并排）/ column（垂直堆叠） */
  dir: 'row' | 'column'
  children: LayoutNode[]
  /** 各子节点主轴占比比例（与 children 一一对应） */
  ratios: number[]
}

export type LayoutNode = PaneNode | SplitNode

/** 统计树中的 pane 数量（上限 4 由调用方控制） */
export function countPanes(node: LayoutNode): number {
  return node.kind === 'pane'
    ? 1
    : node.children.reduce((sum, child) => sum + countPanes(child), 0)
}

/** 深度优先收集所有 pane 节点 */
export function collectPanes(node: LayoutNode): PaneNode[] {
  if (node.kind === 'pane') return [node]
  return node.children.flatMap(collectPanes)
}

/** 按 id 查找 pane 节点 */
export function findPane(node: LayoutNode, paneId: string): PaneNode | null {
  if (node.kind === 'pane') return node.id === paneId ? node : null
  for (const child of node.children) {
    const hit = findPane(child, paneId)
    if (hit) return hit
  }
  return null
}

/**
 * 在 target pane 位置原位分割：pane → split(dir)[target, newPane]。
 * 其余节点不动（不破坏已有分屏布局）。
 */
export function splitAtPane(
  node: LayoutNode,
  targetId: string,
  dir: 'row' | 'column',
  newPane: PaneNode,
): LayoutNode {
  if (node.kind === 'pane') {
    if (node.id !== targetId) return node
    return { kind: 'split', id: genNodeId('s'), dir, children: [node, newPane], ratios: [1, 1] }
  }
  return {
    ...node,
    children: node.children.map((child) => splitAtPane(child, targetId, dir, newPane)),
  }
}

/**
 * 从树中移除 pane（函数式重建）：
 * - 只剩一个子节点的 split 自动折叠为该子节点
 * - 返回新树；目标不存在时原样返回；根节点被移除时返回 null
 *   （调用方保证 pane 总数 > 1 才允许关闭，根不会为 null）
 */
export function removePane(node: LayoutNode, paneId: string): LayoutNode | null {
  if (node.kind === 'pane') return node.id === paneId ? null : node
  const children: LayoutNode[] = []
  const ratios: number[] = []
  node.children.forEach((child, i) => {
    const kept = removePane(child, paneId)
    if (kept) {
      children.push(kept)
      ratios.push(node.ratios[i] ?? 1)
    }
  })
  if (children.length === 0) return null
  if (children.length === 1) return children[0]
  return { ...node, children, ratios }
}

/* ---------------- 扁平布局计算（绝对定位渲染用） ---------------- */

/** 相对整个分屏区的矩形（各边均为 0..1 占比） */
export interface Rect {
  left: number
  top: number
  width: number
  height: number
}

/** 窗格的渲染槽位 */
export interface PaneLayout {
  id: string
  sessionId: string | null
  rect: Rect
}

/** 分割条（覆盖在相邻子节点边界上） */
export interface DividerLayout {
  /** 稳定 key：所属 split id + 子节点序号 */
  key: string
  dir: 'row' | 'column'
  /** 边界在主轴上的位置（0..1；row 为 x，column 为 y） */
  center: number
  /** 交叉轴起点与长度（0..1） */
  crossStart: number
  crossLength: number
  /** 所属 split 的主轴总长（0..1，拖拽换算比例用） */
  axisLength: number
  /** 所属 split 节点（拖拽时直接改其 ratios） */
  node: SplitNode
  index: number
}

export interface FlatLayout {
  panes: PaneLayout[]
  dividers: DividerLayout[]
}

/**
 * 将布局树展平为矩形集合：pane → 绝对定位槽位，分割边界 → 分割条。
 * 输出 key 与窗格 id 稳定，树结构变化时 Vue 复用组件实例，
 * 终端不会因关闭/分割其他窗格而重建清空。
 */
export function computeLayout(node: LayoutNode): FlatLayout {
  const panes: PaneLayout[] = []
  const dividers: DividerLayout[] = []
  walk(node, { left: 0, top: 0, width: 1, height: 1 })
  return { panes, dividers }

  function walk(n: LayoutNode, rect: Rect) {
    if (n.kind === 'pane') {
      panes.push({ id: n.id, sessionId: n.sessionId, rect })
      return
    }
    const total = n.ratios.reduce((s, r) => s + r, 0) || 1
    const horizontal = n.dir === 'row'
    let acc = 0
    n.children.forEach((child, i) => {
      const share = (n.ratios[i] ?? 1) / total
      const start = acc
      acc += share
      walk(
        child,
        horizontal
          ? { left: rect.left + rect.width * start, top: rect.top, width: rect.width * share, height: rect.height }
          : { left: rect.left, top: rect.top + rect.height * start, width: rect.width, height: rect.height * share },
      )
      if (i < n.children.length - 1) {
        dividers.push({
          key: `${n.id}:${i}`,
          dir: n.dir,
          center: horizontal ? rect.left + rect.width * acc : rect.top + rect.height * acc,
          crossStart: horizontal ? rect.top : rect.left,
          crossLength: horizontal ? rect.height : rect.width,
          axisLength: horizontal ? rect.width : rect.height,
          node: n,
          index: i,
        })
      }
    })
  }
}
