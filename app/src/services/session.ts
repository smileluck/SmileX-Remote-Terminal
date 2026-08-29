/**
 * SSH 会话命令组封装
 *
 * 终端输出走 Tauri ipc::Channel（点对点，避免全局 emit 的高频广播开销）。
 * 由于连接建立时终端组件可能尚未挂载（SideBar 先连接、TerminalView 后挂载），
 * 这里做 per-session 分发：未注册处理器前先缓冲，bindOutput 时回放。
 */

import { invoke } from './invoke'
import { Channel } from '@tauri-apps/api/core'
import type { SshConfig, TerminalOutputPayload } from '@/types/session'

/** sessionId → 终端输出处理器 */
const handlers = new Map<string, (p: TerminalOutputPayload) => void>()
/** sessionId → 待回放的缓冲（bind 前到达的输出） */
const pending = new Map<string, TerminalOutputPayload[]>()

/** 建立会话并返回 sessionId；输出经 Channel 分发 */
export async function connect(
  config: SshConfig,
  cols = 80,
  rows = 24,
): Promise<string> {
  const channel = new Channel<TerminalOutputPayload>()
  channel.onmessage = (p) => {
    const h = handlers.get(p.sessionId)
    if (h) {
      h(p)
    } else {
      const q = pending.get(p.sessionId) ?? []
      q.push(p)
      pending.set(p.sessionId, q)
    }
  }
  return invoke<string>('session_connect', {
    config,
    cols,
    rows,
    onOutput: channel,
  })
}

/** 绑定终端输出处理器（回放缓冲后转入实时分发） */
export function bindOutput(sessionId: string, cb: (p: TerminalOutputPayload) => void) {
  handlers.set(sessionId, cb)
  // 回放缓冲（有限：超出 500 条丢弃最旧）
  const q = pending.get(sessionId)
  if (q) {
    if (q.length > 500) q.splice(0, q.length - 500)
    for (const p of q) cb(p)
    pending.delete(sessionId)
  }
}

/** 解绑（组件卸载时调用） */
export function unbindOutput(sessionId: string) {
  handlers.delete(sessionId)
  pending.delete(sessionId)
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
  unbindOutput(sessionId)
  return invoke<void>('session_disconnect', { sessionId })
}
