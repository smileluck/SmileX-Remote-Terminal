/**
 * 应用自升级服务
 *
 * 基于 tauri-plugin-updater：从 GitHub Releases 的 latest.json 静态清单
 * 检查新版本（minisign 签名校验，端点与公钥配置在 tauri.conf.json 的
 * plugins.updater）。安装完成后由 tauri-plugin-process 的 relaunch 重启生效。
 * vite 预览（非 Tauri 环境）下全部 noop。
 */
import type { Update } from '@tauri-apps/plugin-updater'

// 检测 Tauri 环境（与 services/invoke.ts 一致：web 预览时回退 noop）
const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** 检查更新：有新版返回 Update 句柄，已是最新返回 null */
export async function checkForUpdate(): Promise<Update | null> {
  if (!isTauri) return null
  const { check } = await import('@tauri-apps/plugin-updater')
  return check()
}

/** 下载进度回调（total 为 null 表示服务端未给 contentLength） */
export type DownloadProgress = (downloaded: number, total: number | null) => void

/** 下载并安装更新（安装完成不自动重启，重启时机交给调用方） */
export async function downloadAndInstall(
  update: Update,
  onProgress?: DownloadProgress,
): Promise<void> {
  let downloaded = 0
  let total: number | null = null
  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case 'Started':
        total = event.data.contentLength ?? null
        break
      case 'Progress':
        downloaded += event.data.chunkLength
        break
      case 'Finished':
        break
    }
    onProgress?.(downloaded, total)
  })
}

/** 重启应用使更新生效 */
export async function relaunchApp(): Promise<void> {
  if (!isTauri) return
  const { relaunch } = await import('@tauri-apps/plugin-process')
  await relaunch()
}
