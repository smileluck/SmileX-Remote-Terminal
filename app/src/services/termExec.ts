/**
 * 终端窗口会话命令执行
 *
 * Agent 执行命令不再走独立的 exec 通道（不可见、每条命令独立 shell，
 * 不保留 cwd/env 状态），而是把命令写入绑定的 SSH 会话 PTY——即用户
 * 看到的终端窗口——再从会话输出流中捕获执行结果：
 * - 操作对用户完全可见（命令、输出都发生在真实终端里）
 * - 命令在会话当前状态下执行（保留 cd/env/交互上下文）
 *
 * 完成判定与结果截取采用「标记协议」：命令被包成
 *   echo __SRT_{id}_B__; {command}; echo __SRT_{id}_E__$?
 * 输出流中出现 END 标记即精确判定执行完成（慢启动/中途长停顿的
 * 命令不再被静默期误判截断），并按标记截取输出、带回退出码。
 * 标记未出现（交互式命令/输入被密码提示吞掉等）才回退静默期兜底，
 * 并在结果中注明可能不完整。
 *
 * 捕获后清理：剥离 ANSI 转义序列、模拟回车覆盖（\r 进度条）、
 * 去掉终端回显的命令行与标记行本身。
 */
import { bindOutput, input, unbindOutput } from './session'
import type { TerminalOutputPayload } from '@/types/session'

/** 输出停止增长多久视为命令结束（仅 END 标记未出现时的兜底） */
const QUIET_MS = 1200
/** 单条命令最长等待（交互式命令/前台长驻进程兜底） */
const MAX_WAIT_MS = 60_000
/** 输出轮询间隔 */
const POLL_MS = 120
/** 见到 END 标记后再收集一小段时间，等尾部字节到齐 */
const END_GRACE_MS = 150

/** ANSI 转义序列（CSI/OSC 及部分双字符序列） */
// eslint-disable-next-line no-control-regex
const ANSI_RE =
  // eslint-disable-next-line no-control-regex
  /[\u001B\u009B][[\]()#;?]*(?:(?:(?:[a-zA-Z\d]*(?:;[-a-zA-Z\d/#&.:=?%@~_]*)*)?\u0007)|(?:(?:\d{1,4}(?:;\d{0,4})*)?[\dA-PR-TZcf-nq-uy=><~]))/g

/** 包裹后的命令与配套标记 */
export interface WrappedCommand {
  /** 写入 PTY 的完整命令行 */
  wrapped: string
  /** BEGIN 标记（echo 输出行，截取起点） */
  begin: string
  /** END 标记（echo 输出行，截取终点；尾部带退出码） */
  end: string
}

/** 把命令包上 BEGIN/END 标记（标记 id 随机，避免与输出内容撞车） */
export function wrapCommand(command: string): WrappedCommand {
  const id = Math.random().toString(36).slice(2, 8)
  const begin = `__SRT_${id}_B__`
  const end = `__SRT_${id}_E__`
  const body = command.trim().replace(/;+\s*$/, '')
  // 尾带 & 的命令（转后台）不能接 `;`（语法错误），用空格续接
  const joiner = body.endsWith('&') ? ' ' : '; '
  return { wrapped: `echo ${begin}; ${body}${joiner}echo ${end}$?`, begin, end }
}

/** 模拟终端的回车覆盖语义：同行 \r 后的内容覆盖前面的（进度条） */
function applyCarriageReturns(s: string): string {
  return s
    // 先归一化 CRLF 行尾（PTY 输出每行以 \r\n 结束），
    // 否则下面的按 \r 取尾会把每一行都清成空串
    .replace(/\r\n/g, '\n')
    .split('\n')
    .map((line) => {
      const parts = line.split('\r')
      return parts[parts.length - 1]
    })
    .join('\n')
}

/** 基础清理：去 ANSI、回车覆盖 */
function stripNoise(raw: string): string {
  return applyCarriageReturns(raw.replace(ANSI_RE, ''))
}

/**
 * 从清理后的输出流截取执行结果
 *
 * 流水结构：回显命令行（含标记文本）→ BEGIN 标记输出行 → 命令输出 →
 * END 标记行（尾部带退出码）→ 提示符。取最后一个 BEGIN 行之后到
 * 第一个 END 行之前的内容；没找到 BEGIN 时回退探测回显命令行。
 */
function extractOutput(
  cleaned: string,
  begin: string,
  end: string,
  command: string,
): { out: string; rc?: number } {
  if (cleaned.includes(begin)) {
    const lines = cleaned.split('\n')
    let start = -1
    for (let i = lines.length - 1; i >= 0; i--) {
      if (lines[i].includes(begin)) {
        start = i
        break
      }
    }
    let rc: number | undefined
    let stop = lines.length
    for (let i = start + 1; i < lines.length; i++) {
      const idx = lines[i].indexOf(end)
      if (idx >= 0) {
        stop = i
        const tail = lines[i].slice(idx + end.length).trim()
        if (/^\d+$/.test(tail)) rc = Number(tail)
        break
      }
    }
    return { out: lines.slice(start + 1, stop).join('\n').trim(), rc }
  }

  // 标记未出现（输入被密码提示吞掉等）：回退去掉回显命令行后原样返回
  const lines = cleaned.split('\n')
  const probe = command.trim()
  let echoLine = lines.findIndex((l) => l.includes(probe))
  if (echoLine < 0 && probe.length > 12) {
    echoLine = lines.findIndex((l) => l.includes(probe.slice(0, 12)))
  }
  const out = echoLine >= 0 ? lines.slice(echoLine + 1).join('\n') : cleaned
  return { out: out.replace(/\n{3,}/g, '\n\n').trim() }
}

/** 终端执行的结构化结果 */
export interface TermExecResult {
  /** 清理后的输出文本（同 execInTerminal 返回值） */
  text: string
  /** 退出码（END 标记正常出现时可解析；否则 undefined） */
  rc?: number
  /** 是否等到 END 标记（false = 超时/静默兜底，结果可能不完整） */
  sawEnd: boolean
}

/**
 * 在绑定的终端窗口会话中执行命令并捕获输出
 *
 * @throws 写入 PTY 失败（会话已断开等）时抛出
 */
export async function execInTerminal(
  sessionId: string,
  command: string,
): Promise<string> {
  return (await execInTerminalDetailed(sessionId, command)).text
}

/**
 * execInTerminal 的结构化版本：除文本外带回退出码与完成标记
 * （常用记录整组执行按退出码判成败、失败即中止）
 */
export async function execInTerminalDetailed(
  sessionId: string,
  command: string,
): Promise<TermExecResult> {
  const { wrapped, begin, end } = wrapCommand(command)

  let buf = ''
  // bindOutput 对首个消费者会同步回放历史缓冲（终端组件未挂载时积累的），
  // armed 之前的输出属于历史，不能混入本次捕获
  let armed = false
  const dec = new TextDecoder('utf-8')
  const handler = (p: TerminalOutputPayload) => {
    if (armed && p.sessionId === sessionId) {
      buf += dec.decode(new Uint8Array(p.data), { stream: true })
    }
  }

  bindOutput(sessionId, handler)
  armed = true

  let sawEnd = false
  let timedOut = false
  try {
    await input(sessionId, Array.from(new TextEncoder().encode(wrapped + '\r')))

    const deadline = Date.now() + MAX_WAIT_MS
    let lastLen = -1
    let lastGrowth = Date.now()
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, POLL_MS))
      // END 标记出现 = 执行完成；再收集一小段等尾部字节到齐
      if (buf.includes(end)) {
        sawEnd = true
        await new Promise((r) => setTimeout(r, END_GRACE_MS))
        break
      }
      if (buf.length !== lastLen) {
        lastLen = buf.length
        lastGrowth = Date.now()
      } else if (Date.now() - lastGrowth >= QUIET_MS) {
        break
      }
    }
    timedOut = Date.now() >= deadline
  } finally {
    buf += dec.decode()
    unbindOutput(sessionId, handler)
  }

  const { out, rc } = extractOutput(stripNoise(buf), begin, end, command)
  const parts: string[] = []
  if (out) parts.push(out)
  if (rc !== undefined) parts.push(`[exit code: ${rc}]`)
  if (!sawEnd) {
    const why = timedOut
      ? '等待超时，命令可能仍在运行或为交互式命令'
      : '输出提前停止，命令可能未执行完成（结果可能不完整）'
    parts.push(`（${why}）`)
  }
  return { text: parts.join('\n'), rc, sawEnd }
}
