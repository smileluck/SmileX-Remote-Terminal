/**
 * 监控面板通用格式化函数（MetricCard 系组件共用）
 */

/** 字节数 → 人类可读（B/KB/MB/GB） */
export function fmtBytes(n: number): string {
  if (n < 1024) return `${n.toFixed(0)} B`
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`
  return `${(n / 1024 ** 3).toFixed(2)} GB`
}

/** 速率（bytes/s）→ 人类可读 */
export function fmtBps(n: number): string {
  return `${fmtBytes(n)}/s`
}

/** 运行时长（秒）→ 中文可读 */
export function fmtUptime(s: number): string {
  if (!s) return '-'
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  if (d > 0) return `${d} 天 ${h} 小时`
  const m = Math.floor((s % 3600) / 60)
  if (h > 0) return `${h} 小时 ${m} 分`
  return `${m} 分`
}
