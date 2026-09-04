/**
 * SFTP 传输 store
 *
 * - 订阅 `transfer_event` 事件，按 groupId upsert 传输组快照
 * - 提供上传/下载入队与暂停/恢复/取消/重试操作
 * - `downloadDir`：下载目录偏好（localStorage 持久化，空 = 系统下载目录）
 * - `managerVisible`：全局传输管理弹窗开关（App.vue 挂载）
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import { useLocalStorage } from '@vueuse/core'

import type { TransferGroupInfo } from '@/types/transfer'
import * as sftp from '@/services/sftp'
import { listen } from '@/services/invoke'

export const useTransferStore = defineStore('transfer', () => {
  /** 全部传输组（创建时间升序） */
  const groups = ref<TransferGroupInfo[]>([])
  /** 传输管理弹窗可见 */
  const managerVisible = ref(false)
  /** 下载目录偏好（'' = 后端默认 ~/Downloads，重名自动去重） */
  const downloadDir = useLocalStorage<string>('sftp.downloadDir', '')

  let started = false

  /** App 级启动（事件订阅 + 快照初始化；幂等） */
  async function start() {
    if (started) return
    started = true
    await listen<TransferGroupInfo>('transfer_event', (p) => upsert(p))
    await refresh()
  }

  /** 按 groupId upsert */
  function upsert(g: TransferGroupInfo) {
    const idx = groups.value.findIndex((x) => x.groupId === g.groupId)
    if (idx >= 0) {
      groups.value[idx] = g
    } else {
      groups.value.push(g)
    }
  }

  /** 拉取全量快照 */
  async function refresh() {
    try {
      groups.value = await sftp.listTransfers()
    } catch (e) {
      console.warn('拉取传输列表失败:', e)
    }
  }

  /** 上传入队（文件/文件夹路径数组，来自对话框或拖拽） */
  async function startUpload(sessionId: string, localPaths: string[], remoteDir: string) {
    await sftp.transferUpload(sessionId, localPaths, remoteDir)
  }

  /** 下载入队（应用 downloadDir 偏好） */
  async function startDownload(sessionId: string, remotePaths: string[]) {
    await sftp.transferDownload(sessionId, remotePaths, downloadDir.value || null)
  }

  async function pause(groupId: string) {
    await sftp.transferPause(groupId).catch(() => {})
    refresh()
  }

  async function resume(groupId: string) {
    await sftp.transferResume(groupId).catch(() => {})
    refresh()
  }

  async function cancel(groupId: string) {
    await sftp.transferCancel(groupId).catch(() => {})
    refresh()
  }

  async function retry(groupId: string) {
    await sftp.transferRetry(groupId).catch(() => {})
    refresh()
  }

  async function clearFinished() {
    await sftp.clearTransfers().catch(() => {})
    await refresh()
  }

  /** 未完成组数（排队/传输中） */
  const activeCount = computed(
    () => groups.value.filter((g) => g.status === 'queued' || g.status === 'running').length,
  )

  /** 传输中总速度（bytes/s） */
  const totalSpeedBps = computed(() =>
    groups.value
      .filter((g) => g.status === 'running')
      .reduce((sum, g) => sum + g.speedBps, 0),
  )

  return {
    groups,
    managerVisible,
    downloadDir,
    start,
    refresh,
    startUpload,
    startDownload,
    pause,
    resume,
    cancel,
    retry,
    clearFinished,
    activeCount,
    totalSpeedBps,
  }
})
