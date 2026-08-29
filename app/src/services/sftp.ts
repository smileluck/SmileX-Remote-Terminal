/**
 * SFTP 命令组封装
 *
 * 对接后端 `sftp_*` commands（远端文件浏览 / 上传 / 下载 / 增删改）。
 */

import { invoke } from './invoke'

/** 远端目录条目 */
export interface SftpEntry {
  name: string
  path: string
  is_dir: boolean
  is_symlink: boolean
  size: number
  mtime: number | null
}

/** 读取远端目录（path 省略时进入远端 home） */
export function list(sessionId: string, path?: string): Promise<SftpEntry[]> {
  return invoke<SftpEntry[]>('sftp_list', { sessionId, path: path ?? null })
}

/** 上传文件（data 为 Uint8Array/ArrayBuffer） */
export function upload(
  sessionId: string,
  remotePath: string,
  data: Uint8Array,
): Promise<void> {
  // Tauri 2 将 ArrayBuffer 参数自动转为 Vec<u8>
  return invoke<void>('sftp_upload', {
    sessionId,
    remotePath,
    data: data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength),
  })
}

/** 下载文件到本地 ~/Downloads（返回保存路径） */
export function download(sessionId: string, remotePath: string): Promise<string> {
  return invoke<string>('sftp_download', { sessionId, remotePath })
}

/** 创建目录 */
export function mkdir(sessionId: string, path: string): Promise<void> {
  return invoke<void>('sftp_mkdir', { sessionId, path })
}

/** 删除文件或空目录 */
export function remove(sessionId: string, path: string, isDir: boolean): Promise<void> {
  return invoke<void>('sftp_remove', { sessionId, path, isDir })
}

/** 重命名 / 移动 */
export function rename(sessionId: string, oldPath: string, newPath: string): Promise<void> {
  return invoke<void>('sftp_rename', { sessionId, oldPath, newPath })
}
