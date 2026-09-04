/**
 * 监控相关类型定义（对应后端 events.rs / commands/session.rs）
 */

/** 单挂载点磁盘信息 */
export interface MonitorDisk {
  mount: string
  total_kb: number
  used_kb: number
  used_percent: number
}

/** 单 GPU 信息（显存单位 MiB） */
export interface MonitorGpu {
  index: number
  name: string
  util_percent: number
  mem_used_mb: number
  mem_total_mb: number
  temp_c: number | null
}

/** monitor_metrics 事件 payload（一次采样） */
export interface MonitorSample {
  session_id: string
  timestamp_ms: number
  latency_ms: number | null
  cpu_percent: number | null
  mem_total_bytes: number
  mem_used_bytes: number
  mem_percent: number
  swap_total_bytes: number
  swap_used_bytes: number
  load1: number
  load5: number
  load15: number
  uptime_s: number
  net_rx_bps: number
  net_tx_bps: number
  disks: MonitorDisk[]
  gpus: MonitorGpu[]
  error: string | null
}

/** 会话健康信息（session_stats_all 返回项） */
export interface SessionHealth {
  session_id: string
  connected: boolean
  connected_at_ms: number
  rx_bytes: number
  tx_bytes: number
}
