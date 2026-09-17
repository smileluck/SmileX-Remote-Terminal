/**
 * updater store - 应用自升级状态
 *
 * 启动自动检查（静默，失败仅 console 记录）+ 设置页手动检查；
 * 发现新版后由 UpdateDialog 弹窗展示更新日志，引导下载安装并重启。
 */
import { computed, ref, shallowRef } from 'vue'
import { defineStore } from 'pinia'
import type { Update } from '@tauri-apps/plugin-updater'
import * as updaterService from '@/services/updater'

/** 手动检查结果（供设置页差异化提示） */
export type CheckResult = 'update' | 'latest' | 'error'

export const useUpdaterStore = defineStore('updater', () => {
  /** 正在检查更新 */
  const checking = ref(false)
  /** 检查到的新版本句柄（shallowRef：避免 reactive 代理破坏类实例内部 this） */
  const pendingUpdate = shallowRef<Update | null>(null)
  /** 更新弹窗显隐 */
  const dialogVisible = ref(false)
  /** 下载安装中 */
  const installing = ref(false)
  /** 已下载字节数 / 总字节数（null 表示未知） */
  const downloadedBytes = ref(0)
  const totalBytes = ref<number | null>(null)
  /** 安装完成待重启 */
  const readyToRestart = ref(false)

  const newVersion = computed(() => pendingUpdate.value?.version ?? '')
  const releaseDate = computed(() => pendingUpdate.value?.date ?? '')
  const releaseNotes = computed(() => pendingUpdate.value?.body ?? '')
  const progressPercent = computed(() => {
    if (!totalBytes.value) return 0
    return Math.min(100, Math.round((downloadedBytes.value / totalBytes.value) * 100))
  })

  async function runCheck(): Promise<CheckResult> {
    if (checking.value || installing.value) return 'latest'
    checking.value = true
    try {
      const update = await updaterService.checkForUpdate()
      if (update) {
        pendingUpdate.value = update
        dialogVisible.value = true
        return 'update'
      }
      return 'latest'
    } catch (e) {
      console.warn('检查更新失败:', e)
      return 'error'
    } finally {
      checking.value = false
    }
  }

  /** 启动自动检查：静默，失败不打扰用户 */
  function autoCheck() {
    void runCheck()
  }

  /** 设置页手动检查：返回结果供 UI 提示「已是最新」等 */
  async function manualCheck(): Promise<CheckResult> {
    return runCheck()
  }

  /** 下载并安装（进度驱动弹窗进度条；失败抛错由弹窗提示） */
  async function startUpdate(): Promise<void> {
    const update = pendingUpdate.value
    if (!update || installing.value || readyToRestart.value) return
    installing.value = true
    downloadedBytes.value = 0
    totalBytes.value = null
    try {
      await updaterService.downloadAndInstall(update, (downloaded, total) => {
        downloadedBytes.value = downloaded
        totalBytes.value = total
      })
      readyToRestart.value = true
    } finally {
      installing.value = false
    }
  }

  /** 重启应用使更新生效 */
  async function restart(): Promise<void> {
    await updaterService.relaunchApp()
  }

  /** 「稍后提醒」：仅关闭弹窗，保留句柄供设置页再次打开 */
  function dismiss() {
    dialogVisible.value = false
  }

  return {
    checking,
    pendingUpdate,
    dialogVisible,
    installing,
    downloadedBytes,
    totalBytes,
    readyToRestart,
    newVersion,
    releaseDate,
    releaseNotes,
    progressPercent,
    autoCheck,
    manualCheck,
    startUpdate,
    restart,
    dismiss,
  }
})
