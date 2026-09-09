/**
 * crontab 管理服务
 *
 * 复用 session_exec 静默通道管理远端当前用户的 crontab：
 * - 读取：crontab -l（"no crontab for" 视为空表正常态）
 * - 写入：crontab 无单条修改接口，所有变更重组行数组后 printf | crontab - 整体回写
 * - 停用 = 行首加 DISABLE_MARK 标记注释（与用户自有注释区分，可逆）
 * - 所有命令追加 SRT_RC 退出码标记（exec 通道 stdout/stderr 合并返回，
 *   无法区分流，以退出码 + 关键字判定成败与错误类别）
 */
import * as sessionService from '@/services/session'
import type { CronLine, CrontabErrorKind } from '@/types/crontab'

/** 停用任务的注释标记前缀 */
export const DISABLE_MARK = '#SRT_DISABLED# '

/** crontab 操作错误（kind 用于面板展示差异化提示） */
export class CrontabError extends Error {
  kind: CrontabErrorKind
  constructor(kind: CrontabErrorKind, message: string) {
    super(message)
    this.kind = kind
  }
}

/** shell 单引号转义（' → '\''），用户输入一律经此包装防注入 */
function sq(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`
}

/** 按输出关键字归类 crontab 错误 */
function classifyError(out: string): CrontabErrorKind {
  const s = out.toLowerCase()
  if (s.includes('command not found') || s.includes('not recognized')) return 'not-installed'
  if (s.includes('permission denied') || s.includes('not allowed')) return 'no-permission'
  return 'unknown'
}

/**
 * 执行命令并带回退出码（输出末尾追加 SRT_RC=<rc> 标记后剥离）。
 * rc 非零时抛出归类后的 CrontabError。
 */
async function runChecked(sid: string, cmd: string): Promise<string> {
  const out = await sessionService.exec(sid, `{ ${cmd} ; } 2>&1; echo "SRT_RC=$?"`)
  const m = out.match(/SRT_RC=(\d+)\s*$/)
  const rc = m ? Number(m[1]) : -1
  const body = m ? out.slice(0, m.index).trim() : out.trim()
  if (rc !== 0) {
    throw new CrontabError(classifyError(body), body || `命令执行失败（退出码 ${rc}）`)
  }
  return body
}

/** 读取当前用户的 crontab 原始行（空表返回 []） */
export async function listLines(sid: string): Promise<string[]> {
  try {
    const out = await runChecked(sid, 'crontab -l')
    return out ? out.split('\n') : []
  } catch (e) {
    // 无 crontab 是正常态（不同发行版文案均含 "no crontab for"）
    if (e instanceof CrontabError && e.message.toLowerCase().includes('no crontab for')) return []
    throw e
  }
}

/** 整体回写 crontab（空数组 = 清空为空白表） */
export async function writeLines(sid: string, lines: string[]): Promise<void> {
  if (!lines.length) {
    await runChecked(sid, `printf '' | crontab -`)
    return
  }
  await runChecked(sid, `printf '%s\\n' ${lines.map(sq).join(' ')} | crontab -`)
}

/** 解析 crontab 行为结构化条目（无法识别的行按 comment 原样保留，不丢数据） */
export function parseLines(lines: string[]): CronLine[] {
  return lines.map((raw, index) => {
    const trimmed = raw.trim()
    if (!trimmed) return { index, kind: 'blank', raw }

    if (trimmed.startsWith(DISABLE_MARK)) {
      const inner = trimmed.slice(DISABLE_MARK.length)
      const job = parseJob(inner)
      return { index, kind: 'job', raw, schedule: job?.schedule ?? '', command: job?.command ?? inner, enabled: false }
    }
    if (trimmed.startsWith('#')) return { index, kind: 'comment', raw }

    if (trimmed.startsWith('@')) {
      const m = trimmed.match(/^(@\w+)\s+(.*)$/)
      if (m) return { index, kind: 'job', raw, schedule: m[1], command: m[2], enabled: true }
      return { index, kind: 'comment', raw }
    }

    if (/^\w+=/.test(trimmed)) return { index, kind: 'env', raw }

    const job = parseJob(trimmed)
    if (job) return { index, kind: 'job', raw, schedule: job.schedule, command: job.command, enabled: true }
    // 不足 6 段的行容错保留
    return { index, kind: 'comment', raw }
  })
}

/** 解析 5 段 cron 表达式 + 命令 */
function parseJob(s: string): { schedule: string; command: string } | null {
  const parts = s.split(/\s+/)
  if (parts.length < 6) return null
  return { schedule: parts.slice(0, 5).join(' '), command: parts.slice(5).join(' ') }
}

/* ---------------- 立即执行测试 ---------------- */

/** 测试执行结果（rc 为命令退出码，非 0 不算服务层错误） */
export interface CronRunResult {
  rc: number
  output: string
}

/**
 * 立即执行测试：以 cron 风格最小环境（env -i，/bin/sh，PATH=/usr/bin:/bin）
 * 运行任务命令并捕获输出（尾部 8KB 截断），便于提前暴露 cron 下的 PATH 类问题。
 * 退出码经 SRT_RUN_RC 标记回传（非 0 是正常结果，不抛错）。
 */
export async function runJob(sid: string, command: string): Promise<CronRunResult> {
  const script = [
    `OUT=$(env -i HOME="$HOME" LOGNAME="$(id -un 2>/dev/null || echo "$USER")" SHELL=/bin/sh PATH=/usr/bin:/bin /bin/sh -c ${sq(command)} 2>&1)`,
    'RC=$?',
    'printf \'%s\' "$OUT" | tail -c 8192',
    'echo "SRT_RUN_RC=$RC"',
  ].join('; ')
  const out = await sessionService.exec(sid, script)
  const m = out.match(/SRT_RUN_RC=(\d+)\s*$/)
  if (!m) throw new CrontabError('unknown', '测试执行失败：未取到退出码')
  return { rc: Number(m[1]), output: out.slice(0, m.index).replace(/\n$/, '') }
}
