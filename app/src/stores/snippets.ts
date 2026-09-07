import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { CommandSnippet } from '@/services/snippets'
import {
  snippetList,
  snippetSave,
  snippetDelete,
  snippetReorder,
} from '@/services/snippets'
import { execInTerminalDetailed } from '@/services/termExec'
import { useTabsStore } from '@/stores/tabs'

/** 条目执行状态（整组顺序执行时逐条流转） */
export type SnippetRunState = 'pending' | 'running' | 'success' | 'failed'

export interface SnippetGroup {
  /** 分组名（'' = 未分组） */
  key: string
  /** 显示名（未分组显示「未分组」） */
  name: string
  items: CommandSnippet[]
}

/**
 * 命令片段（常用记录）store
 *
 * - 与 ⌘K 命令面板的片段共用同一份数据（command_snippets 表）
 * - snippets 数组顺序即展示顺序：分组连续排列，组内按 sortOrder
 * - 任何排序/分组变动后 persistOrder() 统一重编 sortOrder 并整体落库
 * - runOne/runGroup 走终端标记协议（execInTerminalDetailed），
 *   退出码非零即判定失败，整组执行失败中止
 */
export const useSnippetsStore = defineStore('snippets', () => {
  /** 全部片段（顺序 = 展示顺序） */
  const snippets = ref<CommandSnippet[]>([])
  /** 条目执行状态（snippetId → 状态），仅运行期间/之后展示 */
  const runStates = ref<Record<string, SnippetRunState>>({})
  /** 是否有执行进行中（防重入） */
  const running = ref(false)

  let loaded = false

  /** 分组视图（首次出现顺序 = 组顺序；snippets 已按展示顺序维护） */
  const groups = computed<SnippetGroup[]>(() => {
    const map = new Map<string, CommandSnippet[]>()
    for (const s of snippets.value) {
      const list = map.get(s.groupName)
      if (list) list.push(s)
      else map.set(s.groupName, [s])
    }
    return [...map.entries()].map(([key, items]) => ({
      key,
      name: key || '未分组',
      items,
    }))
  })

  /** 当前可执行目标：激活的、未断开的 SSH 会话 */
  function activeSshSessionId(): string | null {
    const tabs = useTabsStore()
    const t = tabs.activeTab
    return t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
  }

  /** 是否有可执行的 SSH 会话（执行按钮置灰依据） */
  const canRun = computed(() => activeSshSessionId() !== null)

  /** 加载（幂等；force 强制刷新） */
  async function load(force = false) {
    if (loaded && !force) return
    try {
      const list = await snippetList()
      snippets.value = [...list].sort(
        (a, b) => a.sortOrder - b.sortOrder || a.createdAt - b.createdAt,
      )
      loaded = true
    } catch {
      /* 浏览器 dev 环境无后端时静默 */
    }
  }

  /** 按内存顺序重编 sortOrder 并整体落库 */
  async function persistOrder() {
    let i = 0
    const renumbered = snippets.value.map((s) => ({ ...s, sortOrder: i++ }))
    snippets.value = renumbered
    await snippetReorder(renumbered)
  }

  /** 组内末尾的插入下标（组不存在则排到列表末尾） */
  function groupEndIndex(arr: CommandSnippet[], group: string): number {
    for (let i = arr.length - 1; i >= 0; i--) {
      if (arr[i].groupName === group) return i + 1
    }
    return arr.length
  }

  /** 保存/更新片段（换组时移动到新组末尾） */
  async function save(input: CommandSnippet) {
    const arr = [...snippets.value]
    const idx = arr.findIndex((s) => s.id === input.id)
    if (idx >= 0 && arr[idx].groupName === input.groupName) {
      arr[idx] = input
    } else {
      if (idx >= 0) arr.splice(idx, 1)
      arr.splice(groupEndIndex(arr, input.groupName), 0, input)
    }
    snippets.value = arr
    await persistOrder()
    const final = snippets.value.find((s) => s.id === input.id)
    if (final) await snippetSave(final)
  }

  /** 删除片段 */
  async function remove(id: string) {
    snippets.value = snippets.value.filter((s) => s.id !== id)
    await snippetDelete(id)
  }

  /** 重命名分组（目标名已存在则合并：来源组条目挪到目标组末尾） */
  async function renameGroup(from: string, to: string) {
    if (from === to) return
    if (snippets.value.some((s) => s.groupName === to)) {
      const items = snippets.value
        .filter((s) => s.groupName === from)
        .map((s) => ({ ...s, groupName: to }))
      const rest = snippets.value.filter((s) => s.groupName !== from)
      rest.splice(groupEndIndex(rest, to), 0, ...items)
      snippets.value = rest
    } else {
      snippets.value = snippets.value.map((s) =>
        s.groupName === from ? { ...s, groupName: to } : s,
      )
    }
    await persistOrder()
  }

  /** 删除分组（组内条目移入未分组，排在其末尾） */
  async function deleteGroup(key: string) {
    const items = snippets.value
      .filter((s) => s.groupName === key)
      .map((s) => ({ ...s, groupName: '' }))
    const rest = snippets.value.filter((s) => s.groupName !== key)
    rest.splice(groupEndIndex(rest, ''), 0, ...items)
    snippets.value = rest
    await persistOrder()
  }

  /** 拖拽落点：移动条目到目标组 beforeId 之前（null = 组末尾） */
  async function moveItem(id: string, targetGroup: string, beforeId: string | null) {
    const arr = [...snippets.value]
    const idx = arr.findIndex((s) => s.id === id)
    if (idx < 0) return
    const [item] = arr.splice(idx, 1)
    item.groupName = targetGroup
    let insertAt = beforeId ? arr.findIndex((s) => s.id === beforeId) : -1
    if (insertAt < 0) insertAt = groupEndIndex(arr, targetGroup)
    arr.splice(insertAt, 0, item)
    snippets.value = arr
    await persistOrder()
  }

  /** 拖拽落点：整组移动到 beforeKey 组之前（null = 末尾） */
  async function moveGroup(key: string, beforeKey: string | null) {
    const items = snippets.value.filter((s) => s.groupName === key)
    if (!items.length) return
    const rest = snippets.value.filter((s) => s.groupName !== key)
    let insertAt = beforeKey ? rest.findIndex((s) => s.groupName === beforeKey) : -1
    if (insertAt < 0) insertAt = rest.length
    rest.splice(insertAt, 0, ...items)
    snippets.value = rest
    await persistOrder()
  }

  function setRunState(id: string, state: SnippetRunState) {
    runStates.value = { ...runStates.value, [id]: state }
  }

  /** 执行单条命令（等待完成；退出码非零视为失败并抛错） */
  async function runOne(snippet: CommandSnippet) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('当前无激活的 SSH 终端，请先连接主机')
    if (running.value) throw new Error('已有命令在执行中，请等待完成')
    running.value = true
    setRunState(snippet.id, 'running')
    try {
      const r = await execInTerminalDetailed(sid, snippet.command)
      if (r.rc !== undefined && r.rc !== 0) {
        setRunState(snippet.id, 'failed')
        throw new Error(`「${snippet.name}」执行失败（退出码 ${r.rc}）`)
      }
      setRunState(snippet.id, 'success')
    } catch (e) {
      if (runStates.value[snippet.id] === 'running') setRunState(snippet.id, 'failed')
      throw e
    } finally {
      running.value = false
    }
  }

  /** 顺序执行整组（等待每条完成再发下一条；失败即中止并抛错） */
  async function runGroup(groupKey: string) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('当前无激活的 SSH 终端，请先连接主机')
    if (running.value) throw new Error('已有命令在执行中，请等待完成')
    const items = groups.value.find((g) => g.key === groupKey)?.items ?? []
    if (!items.length) return
    running.value = true
    for (const s of items) setRunState(s.id, 'pending')
    try {
      for (const s of items) {
        setRunState(s.id, 'running')
        const r = await execInTerminalDetailed(sid, s.command)
        if (r.rc !== undefined && r.rc !== 0) {
          setRunState(s.id, 'failed')
          throw new Error(`组执行中止：「${s.name}」失败（退出码 ${r.rc}）`)
        }
        setRunState(s.id, 'success')
      }
    } catch (e) {
      // PTY 写入失败等异常：当前执行中条目标记为失败
      const cur = items.find((s) => runStates.value[s.id] === 'running')
      if (cur) setRunState(cur.id, 'failed')
      throw e
    } finally {
      running.value = false
    }
  }

  return {
    snippets,
    runStates,
    running,
    groups,
    canRun,
    load,
    persistOrder,
    save,
    remove,
    renameGroup,
    deleteGroup,
    moveItem,
    moveGroup,
    runOne,
    runGroup,
  }
})
