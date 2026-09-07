import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import type { CommandSnippet, SnippetKind } from '@/services/snippets'
import {
  snippetList,
  snippetSave,
  snippetDelete,
  snippetReorder,
} from '@/services/snippets'
import { execInTerminalDetailed } from '@/services/termExec'
import * as sessionService from '@/services/session'
import { useTabsStore } from '@/stores/tabs'

/** 条目执行状态（整组顺序执行时逐条流转） */
export type SnippetRunState = 'pending' | 'running' | 'success' | 'failed'

/** 服务运行状态：active = 运行中，inactive = 已停止，unknown = 未知（无会话/检查异常） */
export type ServiceStatus = 'active' | 'inactive' | 'unknown'

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
 * - 服务条目（kind='service'）：command 字段存服务名；
 *   状态经静默 exec 通道轮询（startStatusPolling/stopStatusPolling 由面板组件管理生命周期），
 *   快捷操作 runServiceAction 在终端可见执行 systemctl start|stop|restart
 */
export const useSnippetsStore = defineStore('snippets', () => {
  /** 全部片段（顺序 = 展示顺序） */
  const snippets = ref<CommandSnippet[]>([])
  /** 条目执行状态（snippetId → 状态），仅运行期间/之后展示 */
  const runStates = ref<Record<string, SnippetRunState>>({})
  /** 服务运行状态（service snippetId → 状态） */
  const serviceStatus = ref<Record<string, ServiceStatus>>({})
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

  /**
   * 条目在终端实际执行的命令
   *
   * - command 条目：原样执行
   * - service 条目：执行 `systemctl status`（--no-pager 避免 pager 阻塞终端；
   *   `|| true` 兜底：服务停止时 status 退出码非零，属正常查询结果而非执行失败）
   */
  function effectiveCommand(s: CommandSnippet): string {
    return s.kind === 'service'
      ? `systemctl status --no-pager ${s.command} || true`
      : s.command
  }

  /** 在终端执行条目（供 runOne / 服务快捷操作共用） */
  async function runCommand(s: CommandSnippet, command: string) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('当前无激活的 SSH 终端，请先连接主机')
    if (running.value) throw new Error('已有命令在执行中，请等待完成')
    running.value = true
    setRunState(s.id, 'running')
    try {
      const r = await execInTerminalDetailed(sid, command)
      if (r.rc !== undefined && r.rc !== 0) {
        setRunState(s.id, 'failed')
        throw new Error(`「${s.name}」执行失败（退出码 ${r.rc}）`)
      }
      setRunState(s.id, 'success')
    } catch (e) {
      if (runStates.value[s.id] === 'running') setRunState(s.id, 'failed')
      throw e
    } finally {
      running.value = false
    }
  }

  /** 执行单条命令（等待完成；退出码非零视为失败并抛错） */
  async function runOne(snippet: CommandSnippet) {
    await runCommand(snippet, effectiveCommand(snippet))
  }

  /** 服务快捷操作：在终端可见执行 systemctl start|stop|restart，完成后延迟刷新状态 */
  async function runServiceAction(s: CommandSnippet, action: 'start' | 'stop' | 'restart') {
    await runCommand(s, `systemctl ${action} ${s.command}`)
    setTimeout(() => void refreshServiceStatus(), 1000)
  }

  /**
   * 顺序执行整组（等待每条完成再发下一条；失败即中止并抛错）
   *
   * kind 传入时只执行该类型的条目（面板按 Tab 过滤后的整组执行）；
   * 不传则执行组内全部条目。
   */
  async function runGroup(groupKey: string, kind?: SnippetKind) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('当前无激活的 SSH 终端，请先连接主机')
    if (running.value) throw new Error('已有命令在执行中，请等待完成')
    const all = groups.value.find((g) => g.key === groupKey)?.items ?? []
    const items = kind ? all.filter((s) => s.kind === kind) : all
    if (!items.length) return
    running.value = true
    for (const s of items) setRunState(s.id, 'pending')
    try {
      for (const s of items) {
        setRunState(s.id, 'running')
        const r = await execInTerminalDetailed(sid, effectiveCommand(s))
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

  /* ---------------- 服务状态监控（静默 exec 通道，不影响 PTY） ---------------- */

  let refreshingStatus = false
  let statusTimer: ReturnType<typeof setInterval> | null = null

  /** 静默检查单个服务状态（exec 抛错 = 未知） */
  async function checkService(sid: string, s: CommandSnippet): Promise<ServiceStatus> {
    const checkCmd = s.checkCmd.trim()
    if (checkCmd) {
      // 自定义检查命令：包装执行并回显退出码（输出全部丢弃，只认 SVC_RC）
      const out = await sessionService.exec(
        sid,
        `{ ${checkCmd} ; } >/dev/null 2>&1; echo "SVC_RC=$?"`,
      )
      const matches = [...out.matchAll(/SVC_RC=(\d+)/g)]
      if (!matches.length) return 'unknown'
      return Number(matches[matches.length - 1][1]) === 0 ? 'active' : 'inactive'
    }
    const out = await sessionService.exec(sid, `systemctl is-active ${s.command}`)
    return out.trim() === 'active' ? 'active' : 'inactive'
  }

  /** 刷新全部服务条目状态（并发检查；无活跃 SSH 会话时全部置 unknown；防重入） */
  async function refreshServiceStatus() {
    if (refreshingStatus) return
    const services = snippets.value.filter((s) => s.kind === 'service')
    if (!services.length) return
    const sid = activeSshSessionId()
    if (!sid) {
      const next = { ...serviceStatus.value }
      for (const s of services) next[s.id] = 'unknown'
      serviceStatus.value = next
      return
    }
    refreshingStatus = true
    try {
      const results = await Promise.allSettled(services.map((s) => checkService(sid, s)))
      const next = { ...serviceStatus.value }
      results.forEach((r, i) => {
        next[services[i].id] = r.status === 'fulfilled' ? r.value : 'unknown'
      })
      serviceStatus.value = next
    } finally {
      refreshingStatus = false
    }
  }

  /** 启动服务状态轮询（立即刷新一次 + 定时刷新；幂等，重复调用不产生多重定时器） */
  function startStatusPolling(intervalMs = 10_000) {
    if (statusTimer !== null) return
    void refreshServiceStatus()
    statusTimer = setInterval(() => void refreshServiceStatus(), intervalMs)
  }

  /** 停止服务状态轮询 */
  function stopStatusPolling() {
    if (statusTimer !== null) {
      clearInterval(statusTimer)
      statusTimer = null
    }
  }

  return {
    snippets,
    runStates,
    serviceStatus,
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
    runServiceAction,
    refreshServiceStatus,
    startStatusPolling,
    stopStatusPolling,
  }
})
