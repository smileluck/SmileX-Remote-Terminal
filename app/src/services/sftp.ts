/**
 * SFTP 命令组封装
 *
 * 对接后端 `sftp_*` / `sftp_transfer_*` commands：
 * 远端文件浏览 / 递归删除 / 增删改 + 传输队列（上传/下载/暂停/恢复/取消）。
 */

import { invoke } from './invoke'
import type { TransferGroupInfo } from '@/types/transfer'

/** 远端目录条目 */
export interface SftpEntry {
  name: string
  path: string
  is_dir: boolean
  is_symlink: boolean
  size: number
  mtime: number | null
  /** 权限位（含文件类型位；低 12 位为模式位），可能缺失 */
  permissions: number | null
}

/** 读取远端目录（path 省略时进入远端 home） */
export function list(sessionId: string, path?: string): Promise<SftpEntry[]> {
  return invoke<SftpEntry[]>('sftp_list', { sessionId, path: path ?? null })
}

/** 创建目录 */
export function mkdir(sessionId: string, path: string): Promise<void> {
  return invoke<void>('sftp_mkdir', { sessionId, path })
}

/** 递归删除文件/目录（符号链接只删链接本身） */
export function remove(sessionId: string, path: string): Promise<void> {
  return invoke<void>('sftp_remove', { sessionId, path })
}

/** 重命名 / 移动 */
export function rename(sessionId: string, oldPath: string, newPath: string): Promise<void> {
  return invoke<void>('sftp_rename', { sessionId, oldPath, newPath })
}

/** 修改文件/目录权限（mode 为八进制位如 0o755） */
export function chmod(sessionId: string, path: string, mode: number): Promise<void> {
  return invoke<void>('sftp_chmod', { sessionId, path, mode })
}

/** 上传本地文件/文件夹到远端目录（入队，返回传输组 ID） */
export function transferUpload(
  sessionId: string,
  localPaths: string[],
  remoteDir: string,
): Promise<string[]> {
  return invoke<string[]>('sftp_transfer_upload', { sessionId, localPaths, remoteDir })
}

/** 下载远端文件/文件夹（入队，返回传输组 ID；saveDir 缺省为系统下载目录） */
export function transferDownload(
  sessionId: string,
  remotePaths: string[],
  saveDir?: string | null,
): Promise<string[]> {
  return invoke<string[]>('sftp_transfer_download', {
    sessionId,
    remotePaths,
    saveDir: saveDir ?? null,
  })
}

/** 暂停传输组（保留进度） */
export function transferPause(groupId: string): Promise<void> {
  return invoke<void>('sftp_transfer_pause', { groupId })
}

/** 恢复暂停的传输组（断点续传） */
export function transferResume(groupId: string): Promise<void> {
  return invoke<void>('sftp_transfer_resume', { groupId })
}

/** 取消传输组 */
export function transferCancel(groupId: string): Promise<void> {
  return invoke<void>('sftp_transfer_cancel', { groupId })
}

/** 重试失败/已取消的传输组（断点续传） */
export function transferRetry(groupId: string): Promise<void> {
  return invoke<void>('sftp_transfer_retry', { groupId })
}

/** 传输列表快照（恢复 UI 用） */
export function listTransfers(sessionId?: string | null): Promise<TransferGroupInfo[]> {
  return invoke<TransferGroupInfo[]>('sftp_transfers', { sessionId: sessionId ?? null })
}

/** 清除已完成/失败/已取消的传输记录 */
export function clearTransfers(sessionId?: string | null): Promise<void> {
  return invoke<void>('sftp_transfers_clear', { sessionId: sessionId ?? null })
}
