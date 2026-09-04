/**
 * splitTree - 分屏布局树（数据结构与操作）
 *
 * 递归 split 结构：pane 为叶子（一个终端窗格），split 为 flex 容器
 * （带方向 row/column 与子节点比例）。每个分割条的方向与比例随所属
 * 节点独立，分割/关闭只影响目标 pane 所在子树，不重排其他区域。
 */

/** 叶子节点：一个终端窗格 */
export interface PaneNode {
  kind: 'pane'
  id: string
  sessionId: string | null
}

/** 容器节点：一个 flex 分割组 */
export interface SplitNode {
  kind: 'split'
  /** 分割方向：row（水平并排）/ column（垂直堆叠） */
  dir: 'row' | 'column'
  children: LayoutNode[]
  /** 各子节点 flex-grow 比例（与 children 一一对应） */
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
    return { kind: 'split', dir, children: [node, newPane], ratios: [1, 1] }
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
