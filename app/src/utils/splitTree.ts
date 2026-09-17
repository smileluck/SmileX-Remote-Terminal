/**
 * splitTree - 分屏布局树（数据结构与操作）
 *
 * 递归 split 结构：group 为叶子（一个 tab 组，含自己的 tab 栏与激活 tab），
 * split 为分割组（带方向 row/column 与子节点比例）。每个分割条的方向与
 * 比例随所属节点独立，分割/关闭只影响目标 group 所在子树，不重排其他区域。
 *
 * 渲染采用 computeLayout 产出的扁平绝对定位（见 MainContent）：
 * 树结构变化不会导致终端组件重建，xterm 实例与历史内容得以保留。
 */
import type { TabItem } from '@/types/session'

/** 节点 id 序列（同毫秒多次分割/关闭也不冲突） */
let nodeSeq = 0
export function genNodeId(prefix: string): string {
  return `${prefix}${++nodeSeq}`
}

/** 叶子节点：一个 tab 组（Xshell 式分屏单元，拥有自己的 tab 栏） */
export interface GroupNode {
  kind: 'group'
  id: string
  tabs: TabItem[]
  activeTabId: string | null
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

export type LayoutNode = GroupNode | SplitNode

/** 统计树中的 group 数量（上限由调用方控制） */
export function countGroups(node: LayoutNode): number {
  return node.kind === 'group'
    ? 1
    : node.children.reduce((sum, child) => sum + countGroups(child), 0)
}

/** 深度优先收集所有 group 节点 */
export function collectGroups(node: LayoutNode): GroupNode[] {
  if (node.kind === 'group') return [node]
  return node.children.flatMap(collectGroups)
}

/** 按 id 查找 group 节点 */
export function findGroup(node: LayoutNode, groupId: string): GroupNode | null {
  if (node.kind === 'group') return node.id === groupId ? node : null
  for (const child of node.children) {
    const hit = findGroup(child, groupId)
    if (hit) return hit
  }
  return null
}

/**
 * 在 target group 位置原位分割：group → split(dir)[target, newGroup]。
 * 其余节点不动（不破坏已有分屏布局）。
 * before 时新 group 排在 target 之前（拖放到左/上边缘用）。
 */
export function splitAtGroup(
  node: LayoutNode,
  targetId: string,
  dir: 'row' | 'column',
  newGroup: GroupNode,
  before = false,
): LayoutNode {
  if (node.kind === 'group') {
    if (node.id !== targetId) return node
    const children: LayoutNode[] = before ? [newGroup, node] : [node, newGroup]
    return { kind: 'split', id: genNodeId('s'), dir, children, ratios: [1, 1] }
  }
  return {
    ...node,
    children: node.children.map((child) => splitAtGroup(child, targetId, dir, newGroup, before)),
  }
}

/**
 * 从树中移除 group（函数式重建）：
 * - 只剩一个子节点的 split 自动折叠为该子节点
 * - 返回新树；目标不存在时原样返回；根节点被移除时返回 null
 */
export function removeGroup(node: LayoutNode, groupId: string): LayoutNode | null {
  if (node.kind === 'group') return node.id === groupId ? null : node
  const children: LayoutNode[] = []
  const ratios: number[] = []
  node.children.forEach((child, i) => {
    const kept = removeGroup(child, groupId)
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

/** group 的渲染槽位 */
export interface GroupLayout {
  id: string
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
  groups: GroupLayout[]
  dividers: DividerLayout[]
}

/**
 * 将布局树展平为矩形集合：group → 绝对定位槽位，分割边界 → 分割条。
 * 输出 key 与 group id 稳定，树结构变化时 Vue 复用组件实例。
 */
export function computeLayout(node: LayoutNode): FlatLayout {
  const groups: GroupLayout[] = []
  const dividers: DividerLayout[] = []
  walk(node, { left: 0, top: 0, width: 1, height: 1 })
  return { groups, dividers }

  function walk(n: LayoutNode, rect: Rect) {
    if (n.kind === 'group') {
      groups.push({ id: n.id, rect })
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
