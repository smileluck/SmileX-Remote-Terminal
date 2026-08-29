/**
 * 告警规则封装
 *
 * 对接后端 `alert_rule_*` commands 与 `alert_fired` 事件。
 */

import { invoke, listen } from './invoke'

/** 告警规则 */
export interface AlertRule {
  id: string
  name: string
  metric: string
  op: string
  threshold: number
  enabled: boolean
  cooldownSec: number
  createdAt: number
}

/** alert_fired 事件 payload */
export interface AlertFired {
  rule_id: string
  name: string
  session_id: string
  metric: string
  value: number
  threshold: number
  op: string
  timestamp_ms: number
}

export const METRIC_OPTIONS = [
  { label: 'CPU 使用率 (%)', value: 'cpu_percent' },
  { label: '内存使用率 (%)', value: 'mem_percent' },
  { label: '1 分钟负载', value: 'load1' },
  { label: '下行速率 (B/s)', value: 'net_rx_bps' },
  { label: '上行速率 (B/s)', value: 'net_tx_bps' },
]

export const OP_OPTIONS = [
  { label: '超过', value: 'gt' },
  { label: '低于', value: 'lt' },
]

export function listRules(): Promise<AlertRule[]> {
  return invoke<AlertRule[]>('alert_rule_list')
}

export function saveRule(rule: AlertRule): Promise<void> {
  return invoke<void>('alert_rule_save', { rule })
}

export function deleteRule(id: string): Promise<boolean> {
  return invoke<boolean>('alert_rule_delete', { id })
}

export function onAlertFired(cb: (p: AlertFired) => void): Promise<() => void> {
  return listen<AlertFired>('alert_fired', cb)
}
