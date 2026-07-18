/**
 * SSH 会话命令组封装
 */

import { invoke } from './invoke'
import type { SshConfig } from '@/types/session'

/** 建立 SSH 会话，返回 sessionId */
export async function connect(
  config: SshConfig,
  cols = 80,
  rows = 24,
): Promise<string> {
  return invoke<string>('session_connect', { config, cols, rows })
}

/** 终端输入 */
export async function input(sessionId: string, data: Uint8Array | number[]): Promise<void> {
  const arr = data instanceof Uint8Array ? Array.from(data) : data
  return invoke<void>('session_input', { sessionId, data: arr })
}

/** 终端 resize */
export async function resize(sessionId: string, cols: number, rows: number): Promise<void> {
  return invoke<void>('session_resize', { sessionId, cols, rows })
}

/** 断开会话 */
export async function disconnect(sessionId: string): Promise<void> {
  return invoke<void>('session_disconnect', { sessionId })
}
