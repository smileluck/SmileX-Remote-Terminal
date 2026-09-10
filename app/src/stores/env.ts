import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import * as envService from '@/services/env'
import { EnvError } from '@/services/env'
import { useTabsStore } from '@/stores/tabs'
import type { EnvId, EnvStatus, EnvErrorKind } from '@/types/env'

/**
 * 环境管理 store
 *
 * - 面向当前活跃的、未断开的 SSH 会话（activeSshSessionId）
 * - 探测走一次 exec 的拼接脚本（detectAll），面板挂载/会话切换/操作完成后刷新
 * - 变更类操作（install/uninstall/serviceAction）按环境维度维护 operating 态，完成后自动刷新
 */
export const useEnvStore = defineStore('env', () => {
  /** 每种环境的探测状态（未探测为 null） */
  const statuses = ref<Record<EnvId, EnvStatus | null>>({
    python: null,
    conda: null,
    uv: null,
    go: null,
    node: null,
    java: null,
    mysql: null,
    postgresql: null,
    redis: null,
    nginx: null,
    openresty: null,
    docker: null,
  })
  /** 探测进行中 */
  const detecting = ref(false)
  /** 探测失败的错误类别（'none' = 正常；探测脚本整体失败时才有值） */
  const errorKind = ref<EnvErrorKind | 'none'>('none')
  /** 错误详情 */
  const errorMessage = ref('')
  /** 按环境维度的操作 loading（安装/卸载/启停，防重入） */
  const operating = ref<Set<EnvId>>(new Set())
  /** 配置编辑态：当前编辑的文件路径 / 内容 / 加载与保存态 */
  const configEnvId = ref<EnvId | null>(null)
  const configPath = ref('')
  const configContent = ref('')
  const configLoading = ref(false)
  const configSaving = ref(false)

  /** 当前操作目标：激活的、未断开的 SSH 会话 */
  function activeSshSessionId(): string | null {
    const tabs = useTabsStore()
    const t = tabs.activeTab
    return t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
  }

  /** 是否有可用会话（面板空态依据） */
  const hasSession = computed(() => activeSshSessionId() !== null)

  /** 刷新全部环境状态（防重入；无会话时清空） */
  let refreshing = false
  async function refresh() {
    if (refreshing) return
    const sid = activeSshSessionId()
    if (!sid) {
      for (const k of Object.keys(statuses.value) as EnvId[]) statuses.value[k] = null
      errorKind.value = 'none'
      errorMessage.value = ''
      detecting.value = false
      return
    }
    refreshing = true
    detecting.value = true
    try {
      const list = await envService.detectAll(sid)
      for (const s of list) statuses.value[s.id] = s
      errorKind.value = 'none'
      errorMessage.value = ''
    } catch (e) {
      errorKind.value = e instanceof EnvError ? e.kind : 'unknown'
      errorMessage.value = e instanceof Error ? e.message : String(e)
    } finally {
      refreshing = false
      detecting.value = false
    }
  }

  /** 变更操作公共入口：按环境防重入，执行后自动刷新；失败向上抛出由组件提示 */
  async function act(id: EnvId, fn: (sid: string) => Promise<unknown>) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('无活跃的 SSH 会话')
    if (operating.value.has(id)) return
    operating.value = new Set(operating.value).add(id)
    try {
      await fn(sid)
      await refresh()
    } finally {
      const next = new Set(operating.value)
      next.delete(id)
      operating.value = next
    }
  }

  /** 安装（可选版本；耗时长，loading 由 operating 承载） */
  function install(id: EnvId, version?: string) {
    return act(id, (sid) => envService.install(sid, id, version))
  }

  /** 卸载 */
  function uninstall(id: EnvId) {
    return act(id, (sid) => envService.uninstall(sid, id))
  }

  /** 切换版本（仅 python/go/java；完成后自动刷新探测） */
  function switchVersion(id: EnvId, version: string) {
    return act(id, (sid) => envService.switchVersion(sid, id, version))
  }

  /** 服务启停 */
  function serviceAction(id: EnvId, action: 'start' | 'stop' | 'restart') {
    const svc = statuses.value[id]?.serviceName
    if (!svc) throw new Error('未探测到服务名')
    return act(id, (sid) => envService.serviceAction(sid, svc, action))
  }

  /** 读取配置文件到编辑态 */
  async function loadConfig(id: EnvId, path: string) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('无活跃的 SSH 会话')
    configLoading.value = true
    try {
      configContent.value = await envService.readConfig(sid, path)
      configEnvId.value = id
      configPath.value = path
    } finally {
      configLoading.value = false
    }
  }

  /** 保存配置（含备份；nginx/openresty 校验失败自动回滚） */
  async function saveConfig(content: string) {
    const sid = activeSshSessionId()
    const id = configEnvId.value
    if (!sid || !id || !configPath.value) throw new Error('无活跃的 SSH 会话')
    configSaving.value = true
    try {
      await envService.writeConfig(sid, id, configPath.value, content)
      configContent.value = content
    } finally {
      configSaving.value = false
    }
  }

  return {
    statuses,
    detecting,
    errorKind,
    errorMessage,
    operating,
    configEnvId,
    configPath,
    configContent,
    configLoading,
    configSaving,
    hasSession,
    activeSshSessionId,
    refresh,
    install,
    uninstall,
    switchVersion,
    serviceAction,
    loadConfig,
    saveConfig,
  }
})
