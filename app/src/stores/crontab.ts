import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import * as crontabService from '@/services/crontab'
import { CrontabError, DISABLE_MARK } from '@/services/crontab'
import { useTabsStore } from '@/stores/tabs'
import type { CronLine, CrontabErrorKind } from '@/types/crontab'

/**
 * crontab 管理 store
 *
 * - 面向当前活跃的、未断开的 SSH 会话（activeSshSessionId）
 * - 不轮询：crontab 内容静态，仅挂载/切换会话/操作完成后刷新 + 手动刷新
 * - crontab 无单条修改接口，所有变更重组行数组后整体回写（services/crontab）
 */
export const useCrontabStore = defineStore('crontab', () => {
  /** 解析后的全部行（含注释/环境变量/空行，回写保真） */
  const lines = ref<CronLine[]>([])
  /** 首次加载中 */
  const loading = ref(false)
  /** 环境错误类别（'none' = 正常） */
  const errorKind = ref<CrontabErrorKind>('none')
  /** 错误详情（crontab 原始输出） */
  const errorMessage = ref('')
  /** 变更操作进行中（按钮 loading / 防重入） */
  const acting = ref(false)

  let refreshing = false

  /** 当前操作目标：激活的、未断开的 SSH 会话 */
  function activeSshSessionId(): string | null {
    const tabs = useTabsStore()
    const t = tabs.activeTab
    return t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
  }

  /** 是否有可用会话（面板空态依据） */
  const hasSession = computed(() => activeSshSessionId() !== null)

  /** 列表展示用：仅任务行 */
  const jobs = computed(() => lines.value.filter((l) => l.kind === 'job'))

  /** 刷新 crontab（防重入；无会话时清空） */
  async function refresh() {
    if (refreshing) return
    const sid = activeSshSessionId()
    if (!sid) {
      lines.value = []
      errorKind.value = 'none'
      errorMessage.value = ''
      loading.value = false
      return
    }
    refreshing = true
    if (!lines.value.length) loading.value = true
    try {
      const raw = await crontabService.listLines(sid)
      lines.value = crontabService.parseLines(raw)
      errorKind.value = 'none'
      errorMessage.value = ''
    } catch (e) {
      errorKind.value = e instanceof CrontabError ? e.kind : 'unknown'
      errorMessage.value = e instanceof Error ? e.message : String(e)
      // 失败时清空，避免展示上一个会话/环境的残留数据
      lines.value = []
    } finally {
      refreshing = false
      loading.value = false
    }
  }

  /** 变更操作公共入口：重组行数组整体回写后刷新；失败上抛由组件 message.error */
  async function act(mutate: (raw: string[]) => string[]) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('无活跃的 SSH 会话')
    if (acting.value) return
    acting.value = true
    try {
      const next = mutate(lines.value.map((l) => l.raw))
      await crontabService.writeLines(sid, next)
      await refresh()
    } finally {
      acting.value = false
    }
  }

  /** 新增任务 */
  function addJob(schedule: string, command: string) {
    return act((raw) => [...raw, `${schedule.trim()} ${command.trim()}`])
  }

  /** 编辑任务（保留启用/停用状态） */
  function editJob(index: number, schedule: string, command: string) {
    return act((raw) =>
      raw.map((r, i) => {
        if (i !== index) return r
        const line = `${schedule.trim()} ${command.trim()}`
        return lines.value[i]?.enabled === false ? `${DISABLE_MARK}${line}` : line
      }),
    )
  }

  /** 删除任务 */
  function removeJob(index: number) {
    return act((raw) => raw.filter((_, i) => i !== index))
  }

  /** 启用/停用（停用 = 行首加标记注释；由解析字段重组行内容） */
  function toggleJob(index: number) {
    return act((raw) =>
      raw.map((r, i) => {
        if (i !== index) return r
        const entry = lines.value[i]
        if (entry?.kind !== 'job') return r
        const line = `${entry.schedule} ${entry.command}`
        return entry.enabled ? `${DISABLE_MARK}${line}` : line
      }),
    )
  }

  return {
    lines,
    jobs,
    loading,
    errorKind,
    errorMessage,
    acting,
    hasSession,
    refresh,
    addJob,
    editJob,
    removeJob,
    toggleJob,
  }
})
