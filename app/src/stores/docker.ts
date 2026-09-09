import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

import * as dockerService from '@/services/docker'
import { DockerError } from '@/services/docker'
import { useTabsStore } from '@/stores/tabs'
import type { DockerContainer, DockerImage, DockerErrorKind, RunContainerOptions } from '@/types/docker'

/**
 * Docker 管理 store
 *
 * - 面向当前活跃的、未断开的 SSH 会话（activeSshSessionId）
 * - 列表数据经静默 exec 通道轮询（startPolling/stopPolling 由面板组件管理生命周期），
 *   模式同 snippets 的服务状态轮询
 * - 变更类操作（start/stop/restart/rm/rmi/run）静默执行后自动刷新
 */
export const useDockerStore = defineStore('docker', () => {
  /** 容器列表 */
  const containers = ref<DockerContainer[]>([])
  /** 镜像列表 */
  const images = ref<DockerImage[]>([])
  /** 首次加载中（后续轮询静默进行，不闪烁） */
  const loading = ref(false)
  /** 环境错误类别（'none' = 正常） */
  const errorKind = ref<DockerErrorKind | 'none'>('none')
  /** 错误详情（docker 原始输出） */
  const errorMessage = ref('')
  /** 变更操作进行中（按钮 loading / 防重入） */
  const acting = ref(false)

  let refreshing = false
  let timer: ReturnType<typeof setInterval> | null = null

  /** 当前操作目标：激活的、未断开的 SSH 会话 */
  function activeSshSessionId(): string | null {
    const tabs = useTabsStore()
    const t = tabs.activeTab
    return t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
  }

  /** 是否有可用会话（面板空态依据） */
  const hasSession = computed(() => activeSshSessionId() !== null)

  /** 刷新容器 + 镜像列表（并行；防重入；无会话时清空并置未安装态以外由面板处理） */
  async function refresh() {
    if (refreshing) return
    const sid = activeSshSessionId()
    if (!sid) {
      containers.value = []
      images.value = []
      errorKind.value = 'none'
      errorMessage.value = ''
      loading.value = false
      return
    }
    refreshing = true
    if (!containers.value.length && !images.value.length) loading.value = true
    try {
      const results = await Promise.allSettled([
        dockerService.listContainers(sid),
        dockerService.listImages(sid),
      ])
      const failed = results.find((r) => r.status === 'rejected') as
        | PromiseRejectedResult
        | undefined
      if (failed) {
        const e = failed.reason
        errorKind.value = e instanceof DockerError ? e.kind : 'unknown'
        errorMessage.value = e instanceof Error ? e.message : String(e)
        // 失败时清空列表，避免展示上一个会话/环境的残留数据
        containers.value = []
        images.value = []
      } else {
        errorKind.value = 'none'
        errorMessage.value = ''
      }
      if (results[0].status === 'fulfilled') containers.value = results[0].value
      if (results[1].status === 'fulfilled') images.value = results[1].value
    } finally {
      refreshing = false
      loading.value = false
    }
  }

  /** 启动轮询（立即刷新一次 + 定时刷新；幂等） */
  function startPolling(intervalMs = 10_000) {
    if (timer !== null) return
    void refresh()
    timer = setInterval(() => void refresh(), intervalMs)
  }

  /** 停止轮询 */
  function stopPolling() {
    if (timer !== null) {
      clearInterval(timer)
      timer = null
    }
  }

  /** 变更操作公共入口：执行后刷新；失败向上抛出由组件 message.error 提示 */
  async function act(fn: (sid: string) => Promise<unknown>) {
    const sid = activeSshSessionId()
    if (!sid) throw new Error('无活跃的 SSH 会话')
    if (acting.value) return
    acting.value = true
    try {
      await fn(sid)
      await refresh()
    } finally {
      acting.value = false
    }
  }

  /** 启动容器 */
  function startContainer(id: string) {
    return act((sid) => dockerService.startContainer(sid, id))
  }

  /** 停止容器 */
  function stopContainer(id: string) {
    return act((sid) => dockerService.stopContainer(sid, id))
  }

  /** 重启容器 */
  function restartContainer(id: string) {
    return act((sid) => dockerService.restartContainer(sid, id))
  }

  /** 删除容器（运行中的容器传 force=true 强制删除） */
  function removeContainer(id: string, force = false) {
    return act((sid) => dockerService.removeContainer(sid, id, force))
  }

  /** 删除镜像 */
  function removeImage(id: string) {
    return act((sid) => dockerService.removeImage(sid, id))
  }

  /** 从镜像启动容器 */
  function runContainer(image: string, opts: RunContainerOptions) {
    return act((sid) => dockerService.runContainer(sid, image, opts))
  }

  return {
    containers,
    images,
    loading,
    errorKind,
    errorMessage,
    acting,
    hasSession,
    refresh,
    startPolling,
    stopPolling,
    startContainer,
    stopContainer,
    restartContainer,
    removeContainer,
    removeImage,
    runContainer,
  }
})
