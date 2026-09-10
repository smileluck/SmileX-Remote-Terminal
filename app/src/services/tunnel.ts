/**
 * 端口转发隧道命令封装
 *
 * 后端命令组见 `crates/desktop/src/commands/tunnel.rs`。
 * 状态变更另通过全局事件 `tunnel_event` 推送（启动/停止/运行期失败）。
 */
import { invoke, listen } from './invoke'
import type { TunnelConfig, TunnelInfo, TunnelEventPayload } from '@/types/tunnel'

/** 启动隧道，返回 tunnel_id */
export async function start(sessionId: string, config: TunnelConfig): Promise<string> {
  return invoke<string>('tunnel_start', { sessionId, config })
}

/** 停止隧道（幂等） */
export async function stop(tunnelId: string): Promise<void> {
  return invoke<void>('tunnel_stop', { tunnelId })
}

/** 列出指定会话的隧道（含运行状态） */
export async function list(sessionId: string): Promise<TunnelInfo[]> {
  return invoke<TunnelInfo[]>('tunnel_list', { sessionId })
}

/** 监听隧道状态变更事件（返回 unlisten 函数） */
export function onTunnelEvent(
  handler: (payload: TunnelEventPayload) => void,
): Promise<() => void> {
  return listen<TunnelEventPayload>('tunnel_event', handler)
}
