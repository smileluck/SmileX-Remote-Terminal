/**
 * cron 表达式解析与未来执行时间计算
 *
 * 支持标准 5 段（分 时 日 月 周）：*、数字、列表（a,b）、区间（a-b）、
 * 步长（*\/n、a-b\/n）、月/周英文缩写（JAN-MAR、SUN-SAT，大小写不敏感）；
 * 周字段 0 和 7 都表示周日。日/周同时受限时按 cron 惯例取「或」。
 * 另支持 @hourly / @daily(@midnight) / @weekly / @monthly / @yearly(@annually)
 * 关键字；@reboot 无固定执行时间，返回空数组。
 */

const MONTH_NAMES: Record<string, number> = {
  jan: 1, feb: 2, mar: 3, apr: 4, may: 5, jun: 6,
  jul: 7, aug: 8, sep: 9, oct: 10, nov: 11, dec: 12,
}
const DOW_NAMES: Record<string, number> = {
  sun: 0, mon: 1, tue: 2, wed: 3, thu: 4, fri: 5, sat: 6,
}

const KEYWORDS: Record<string, string> = {
  '@hourly': '0 * * * *',
  '@daily': '0 0 * * *',
  '@midnight': '0 0 * * *',
  '@weekly': '0 0 * * 0',
  '@monthly': '0 0 1 * *',
  '@yearly': '0 0 1 1 *',
  '@annually': '0 0 1 1 *',
}

/** 解析单个字段为匹配值集合；非法返回 null */
function parseField(field: string, min: number, max: number, names?: Record<string, number>): Set<number> | null {
  const values = new Set<number>()
  const toNum = (v: string): number | null => {
    const named = names?.[v.toLowerCase()]
    if (named !== undefined) return named
    if (!/^\d+$/.test(v)) return null
    const n = Number(v)
    return n >= min && n <= max ? n : null
  }
  for (const part of field.split(',')) {
    const m = part.match(/^(.+?)(?:\/(\d+))?$/)
    if (!m) return null
    const [, range, stepRaw] = m
    const step = stepRaw ? Number(stepRaw) : 1
    if (step < 1) return null
    let lo: number
    let hi: number
    if (range === '*') {
      lo = min
      hi = max
    } else {
      const rm = range.match(/^([^-]+)-([^-]+)$/)
      if (rm) {
        const a = toNum(rm[1])
        const b = toNum(rm[2])
        if (a === null || b === null || a > b) return null
        lo = a
        hi = b
      } else {
        const a = toNum(range)
        if (a === null) return null
        // 带步长的单值（如 5/10）按 a-max 区间处理；否则为单值
        lo = a
        hi = stepRaw ? max : a
      }
    }
    for (let i = lo; i <= hi; i += step) values.add(i)
  }
  return values.size ? values : null
}

interface CronFields {
  minute: Set<number>
  hour: Set<number>
  dom: Set<number>
  month: Set<number>
  dow: Set<number>
  /** 日/周是否为 *（影响 dom/dow 的 与/或 判定） */
  domAny: boolean
  dowAny: boolean
}

/** 解析 5 段表达式或 @关键字；非法返回 null（@reboot 也返回 null，调用方特判） */
export function parseCron(expr: string): CronFields | null {
  const s = expr.trim()
  const expanded = KEYWORDS[s.toLowerCase()] ?? s
  const parts = expanded.split(/\s+/)
  if (parts.length !== 5) return null
  const minute = parseField(parts[0], 0, 59)
  const hour = parseField(parts[1], 0, 23)
  const dom = parseField(parts[2], 1, 31)
  const month = parseField(parts[3], 1, 12, MONTH_NAMES)
  // 周字段允许 7（周日），归一为 0
  const dowRaw = parseField(parts[4], 0, 7, DOW_NAMES)
  if (!minute || !hour || !dom || !month || !dowRaw) return null
  const dow = new Set([...dowRaw].map((d) => (d === 7 ? 0 : d)))
  return {
    minute,
    hour,
    dom,
    month,
    dow,
    domAny: parts[2] === '*',
    dowAny: parts[4] === '*',
  }
}

/** 判断某时刻（分钟精度）是否命中 */
function matches(f: CronFields, d: Date): boolean {
  if (!f.minute.has(d.getMinutes())) return false
  if (!f.hour.has(d.getHours())) return false
  if (!f.month.has(d.getMonth() + 1)) return false
  const domHit = f.dom.has(d.getDate())
  const dowHit = f.dow.has(d.getDay())
  // cron 惯例：日/周同时受限时取或，任一受限取与
  if (!f.domAny && !f.dowAny) return domHit || dowHit
  return domHit && dowHit
}

/**
 * 计算表达式未来 count 次执行时间（本地时区，分钟精度，不含 from 本身）。
 * 返回 null = 表达式非法；@reboot 返回空数组。
 * 按分钟步进扫描，上限 4 年（覆盖 2 月 29 日等稀疏周期）。
 */
export function nextRuns(expr: string, count = 5, from = new Date()): Date[] | null {
  const s = expr.trim()
  if (s.toLowerCase() === '@reboot') return []
  const fields = parseCron(s)
  if (!fields) return null
  const out: Date[] = []
  const d = new Date(from.getTime())
  d.setSeconds(0, 0)
  d.setMinutes(d.getMinutes() + 1)
  const limit = new Date(from.getTime())
  limit.setFullYear(limit.getFullYear() + 4)
  while (out.length < count && d < limit) {
    if (matches(fields, d)) out.push(new Date(d.getTime()))
    d.setMinutes(d.getMinutes() + 1)
  }
  return out
}
