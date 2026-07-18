/**
 * 远程桌面命令组封装
 */

import { invoke } from './invoke'
import type { DesktopConfig } from '@/types/session'
import type { InputEvent } from '@/types/desktop'

/** 建立远程桌面会话，返回 sessionId */
export async function connect(config: DesktopConfig): Promise<string> {
  // 转 Rust 端 SessionConfig 字段名
  const cfg = {
    kind: config.kind,
    host: config.host,
    port: config.port,
    username: config.username,
    password: config.password,
    width: config.width,
    height: config.height,
    color_depth: config.colorDepth,
  }
  return invoke<string>('desktop_connect', { config: cfg })
}

/** 发送输入事件 */
export async function sendInput(sessionId: string, event: InputEvent): Promise<void> {
  return invoke<void>('desktop_input', { sessionId, event })
}

/** 断开会话 */
export async function disconnect(sessionId: string): Promise<void> {
  return invoke<void>('desktop_disconnect', { sessionId })
}
