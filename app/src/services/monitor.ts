/**
 * 监控命令组封装
 *
 * 对接后端 `monitor_*` commands 与 `monitor_metrics` 事件。
 */

import { invoke, listen } from './invoke'
import type { MonitorSample, SessionHealth } from '@/types/monitor'

/** 启动某会话的监控采样（intervalMs 默认 3000） */
export function start(sessionId: string, intervalMs = 3000): Promise<void> {
  return invoke<void>('monitor_start', { sessionId, intervalMs })
}

/** 停止某会话的监控采样 */
export function stop(sessionId: string): Promise<void> {
  return invoke<void>('monitor_stop', { sessionId })
}

/** 当前采样中的会话列表 */
export function list(): Promise<string[]> {
  return invoke<string[]>('monitor_list')
}

/** 所有会话的健康/流量信息（会话健康面板轮询） */
export function sessionStatsAll(): Promise<SessionHealth[]> {
  return invoke<SessionHealth[]>('session_stats_all')
}

/** 订阅 monitor_metrics 事件（返回 unlisten） */
export function onMetrics(cb: (sample: MonitorSample) => void): Promise<() => void> {
  return listen<MonitorSample>('monitor_metrics', cb)
}
