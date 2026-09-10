import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import * as tunnelService from '@/services/tunnel'
import { useTabsStore } from '@/stores/tabs'
import type { TunnelConfig, TunnelInfo } from '@/types/tunnel'

/**
 * 端口转发隧道 store
 *
 * - 面向当前活跃的、未断开的 SSH 会话（activeSshSessionId，同 env store 模式）
 * - 面板挂载/会话切换/操作完成后 refresh；另监听 `tunnel_event`
 *   （启动/停止/运行期失败）自动刷新，覆盖后台失败等非命令路径
 * - start/stop 按隧道维度维护 operating 态（防重入），失败向上抛出由组件提示
 */
export const useTunnelStore = defineStore('tunnel', () => {
  /** 当前会话的隧道列表 */
  const tunnels = ref<TunnelInfo[]>([])
  /** 列表加载中 */
  const loading = ref(false)
  /** 按隧道维度的操作 loading（启动/停止，防重入；key 为 tunnel_id 或 'new'） */
  const operating = ref<Set<string>>(new Set())

  /** 当前操作目标：激活的、未断开的 SSH 会话 */
  function activeSshSessionId(): string | null {
    const tabs = useTabsStore()
    const t = tabs.activeTab
    return t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
  }

  /** 是否有可用会话（面板空态依据） */
  const hasSession = computed(() => activeSshSessionId() !== null)

  /** 刷新隧道列表（防重入；无会话时清空） */
  let refreshing = false
  async function refresh() {
    if (refreshing) return
    const sid = activeSshSessionId()
    if (!sid) {
      tunnels.value = []
      return
    }
    refreshing = true
    loading.value = true
    try {
      tunnels.value = await tunnelService.list(sid)
    } catch {
      // 列表失败不清空已有数据（避免抖动），仅静默（下次事件/手动刷新恢复）
    } finally {
      refreshing = false
      loading.value = false
    }
  }

  /** 启动隧道（config.id 为空时后端生成；成功后自动刷新） */
  async function start(config: TunnelConfig) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('无活跃的 SSH 会话')
    if (operating.value.has('new')) return
    operating.value = new Set(operating.value).add('new')
    try {
      await tunnelService.start(sid, config)
      await refresh()
    } finally {
      const next = new Set(operating.value)
      next.delete('new')
      operating.value = next
    }
  }

  /** 停止隧道（幂等；完成后自动刷新） */
  async function stop(tunnelId: string) {
    if (operating.value.has(tunnelId)) return
    operating.value = new Set(operating.value).add(tunnelId)
    try {
      await tunnelService.stop(tunnelId)
      await refresh()
    } finally {
      const next = new Set(operating.value)
      next.delete(tunnelId)
      operating.value = next
    }
  }

  // 全局事件监听：仅注册一次（store 单例）；事件属于当前会话时刷新列表
  let listening = false
  function ensureListening() {
    if (listening) return
    listening = true
    void tunnelService.onTunnelEvent((p) => {
      if (p.sessionId === activeSshSessionId()) void refresh()
    })
  }
  ensureListening()

  return {
    tunnels,
    loading,
    operating,
    hasSession,
    activeSshSessionId,
    refresh,
    start,
    stop,
  }
})
