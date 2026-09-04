/**
 * SFTP 传输相关类型定义（对应后端 events.rs 的 transfer_event payload）
 */

/** 传输方向 */
export type TransferKind = 'upload' | 'download'

/** 传输组状态 */
export type TransferStatus =
  | 'queued'
  | 'running'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'canceled'

/** 一次传输组快照（transfer_event payload / sftp_transfers 返回项） */
export interface TransferGroupInfo {
  sessionId: string
  /** 服务器身份（"user@host:port"） */
  sessionLabel: string | null
  /** 传输组 ID（一个顶层文件/文件夹一组） */
  groupId: string
  kind: TransferKind
  /** 顶层名称（文件名/文件夹名） */
  name: string
  isDir: boolean
  status: TransferStatus
  bytesDone: number
  sizeTotal: number
  /** 当前速度（bytes/s，仅 running 有值） */
  speedBps: number
  filesDone: number
  fileCount: number
  error: string | null
  /** 下载对应的本地目标路径 */
  localPath: string | null
  createdAtMs: number
}
