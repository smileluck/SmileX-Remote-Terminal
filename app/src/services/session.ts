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

/** sessionId → 终端输出处理器集合（多消费者场景：如多个分屏 pane 绑定同一会话） */
const handlers = new Map<string, Set<(p: TerminalOutputPayload) => void>>()
/** sessionId → 待回放的缓冲（bind 前到达的输出） */
const pending = new Map<string, TerminalOutputPayload[]>()
/** sessionId → 连接配置（分屏克隆用：以原配置新建独立连接；随断开清理） */
const configBySession = new Map<string, SshConfig>()

/** 建立会话并返回 sessionId；输出经 Channel 分发 */
export async function connect(
  config: SshConfig,
  cols = 80,
  rows = 24,
): Promise<string> {
  const channel = new Channel<TerminalOutputPayload>()
  channel.onmessage = (p) => {
    const set = handlers.get(p.sessionId)
    if (set && set.size > 0) {
      for (const h of set) h(p)
    } else {
      const q = pending.get(p.sessionId) ?? []
      q.push(p)
      pending.set(p.sessionId, q)
    }
  }
  const sessionId = await invoke<string>('session_connect', {
    config,
    cols,
    rows,
    onOutput: channel,
  })
  configBySession.set(sessionId, config)
  return sessionId
}

/** 克隆已有会话：用其原始配置新建一条全新独立 SSH 连接（独立 PTY） */
export async function clone(
  sessionId: string,
  cols = 80,
  rows = 24,
): Promise<string> {
  const config = configBySession.get(sessionId)
  if (!config) throw new Error(`会话 ${sessionId.slice(0, 8)} 的连接配置不可用，无法克隆`)
  return connect(config, cols, rows)
}

/** 绑定终端输出处理器（回放缓冲后转入实时分发；后绑定的消费者不重复回放历史） */
export function bindOutput(sessionId: string, cb: (p: TerminalOutputPayload) => void) {
  let set = handlers.get(sessionId)
  if (!set) {
    set = new Set()
    handlers.set(sessionId, set)
  }
  set.add(cb)
  // 回放缓冲（有限：超出 500 条丢弃最旧）
  const q = pending.get(sessionId)
  if (q) {
    if (q.length > 500) q.splice(0, q.length - 500)
    for (const p of q) cb(p)
    pending.delete(sessionId)
  }
}

/** 解绑指定处理器（该会话无剩余消费者时一并清理缓冲） */
export function unbindOutput(sessionId: string, cb: (p: TerminalOutputPayload) => void) {
  const set = handlers.get(sessionId)
  if (!set) return
  set.delete(cb)
  if (set.size === 0) {
    handlers.delete(sessionId)
    pending.delete(sessionId)
  }
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
  handlers.delete(sessionId)
  pending.delete(sessionId)
  configBySession.delete(sessionId)
  return invoke<void>('session_disconnect', { sessionId })
}

/** 测试连接（连接 → 认证 → 断开），成功返回耗时毫秒 */
export async function test(config: SshConfig): Promise<number> {
  return invoke<number>('session_test', { config })
}

/** 在独立通道执行一次性命令（非交互，用于命令补全等） */
export async function exec(sessionId: string, command: string): Promise<string> {
  return invoke<string>('session_exec', { sessionId, command })
}
