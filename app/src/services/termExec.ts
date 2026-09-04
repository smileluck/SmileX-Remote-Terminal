/**
 * 终端窗口会话命令执行
 *
 * Agent 执行命令不再走独立的 exec 通道（不可见、每条命令独立 shell，
 * 不保留 cwd/env 状态），而是把命令写入绑定的 SSH 会话 PTY——即用户
 * 看到的终端窗口——再从会话输出流中捕获回显作为执行结果：
 * - 操作对用户完全可见（命令、输出都发生在真实终端里）
 * - 命令在会话当前状态下执行（保留 cd/env/交互上下文）
 * - 以「静默期」判定命令结束：输出停止增长 QUIET_MS 视为完成，
 *   超过 MAX_WAIT_MS 强制截断（交互式命令兜底）
 *
 * 捕获后做清理：剥离 ANSI 转义序列、模拟回车覆盖（\r 进度条）、
 * 去掉终端回显的命令行本身。
 */
import { bindOutput, input, unbindOutput } from './session'
import type { TerminalOutputPayload } from '@/types/session'

/** 输出停止增长多久视为命令结束 */
const QUIET_MS = 1200
/** 单条命令最长等待（交互式命令/前台长驻进程兜底） */
const MAX_WAIT_MS = 30_000
/** 输出轮询间隔 */
const POLL_MS = 120

/** ANSI 转义序列（CSI/OSC 及部分双字符序列） */
// eslint-disable-next-line no-control-regex
const ANSI_RE =
  // eslint-disable-next-line no-control-regex
  /[\u001B\u009B][[\]()#;?]*(?:(?:(?:[a-zA-Z\d]*(?:;[-a-zA-Z\d/#&.:=?%@~_]*)*)?\u0007)|(?:(?:\d{1,4}(?:;\d{0,4})*)?[\dA-PR-TZcf-nq-uy=><~]))/g

/** 模拟终端的回车覆盖语义：同行 \r 后的内容覆盖前面的（进度条） */
function applyCarriageReturns(s: string): string {
  return s
    .split('\n')
    .map((line) => {
      const parts = line.split('\r')
      return parts[parts.length - 1]
    })
    .join('\n')
}

/** 清理捕获的原始输出：去 ANSI、回车覆盖、去回显命令行 */
function cleanOutput(raw: string, command: string): string {
  let s = raw.replace(ANSI_RE, '')
  s = applyCarriageReturns(s)

  // 去掉回显的命令行：找到包含命令文本的行，从其下一行起保留。
  // 长命令可能被终端折行导致整串匹配不到，回退用前 12 字符探测
  const lines = s.split('\n')
  const probe = command.trim()
  let echoLine = lines.findIndex((l) => l.includes(probe))
  if (echoLine < 0 && probe.length > 12) {
    echoLine = lines.findIndex((l) => l.includes(probe.slice(0, 12)))
  }
  if (echoLine >= 0) s = lines.slice(echoLine + 1).join('\n')

  return s.replace(/\n{3,}/g, '\n\n').trim()
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

  let timedOut = false
  try {
    await input(sessionId, Array.from(new TextEncoder().encode(command + '\r')))

    const deadline = Date.now() + MAX_WAIT_MS
    let lastLen = -1
    let lastGrowth = Date.now()
    while (Date.now() < deadline) {
      await new Promise((r) => setTimeout(r, POLL_MS))
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

  const out = cleanOutput(buf, command)
  if (timedOut) {
    // 交互式命令/长驻进程：带回显的部分内容 + 截断说明
    return (out ? out + '\n' : '') + '（等待输出超时，命令可能仍在运行或为交互式命令）'
  }
  return out
}
