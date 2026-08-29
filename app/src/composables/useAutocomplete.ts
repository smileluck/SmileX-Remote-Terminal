/**
 * useAutocomplete - 终端命令自动补全
 *
 * - 连接后经 `session_exec("compgen -c | sort -u")` 拉取远端命令表（按 sessionId 缓存）
 * - 叠加本地命令片段/历史的首词
 * - 浮层候选列表：输入时更新，Tab/→ 补全，↑↓ 选择，Esc 关闭
 *
 * 实现为独立类（非 composable）：一个实例绑定一个 xterm + 一个会话。
 */
import type { Terminal } from '@xterm/xterm'
import * as sessionService from '@/services/session'
import * as snippetsService from '@/services/snippets'

/** 命令表缓存：sessionId → 命令集合 */
const commandCache = new Map<string, Set<string>>()

/** 拉取（或取缓存）指定会话的可用命令表 */
async function loadCommands(sessionId: string): Promise<Set<string>> {
  const cached = commandCache.get(sessionId)
  if (cached) return cached
  const set = new Set<string>()
  try {
    const out = await sessionService.exec(sessionId, 'compgen -c | sort -u')
    for (const line of out.split('\n')) {
      const cmd = line.trim()
      if (cmd && !cmd.includes('/') && cmd.length <= 40) set.add(cmd)
    }
  } catch {
    /* 非交互 exec 不可用（或非 bash）：仅用本地候选 */
  }
  // 叠加本地片段/历史首词
  try {
    const [snippets, history] = await Promise.all([
      snippetsService.snippetList(),
      snippetsService.historyList(100),
    ])
    for (const s of snippets) {
      const w = s.command.trim().split(/\s+/)[0]
      if (w) set.add(w)
    }
    for (const h of history) {
      const w = h.command.trim().split(/\s+/)[0]
      if (w) set.add(w)
    }
  } catch {
    /* 本地数据不可用则忽略 */
  }
  commandCache.set(sessionId, set)
  return set
}

export class Suggester {
  private term: Terminal
  private container: HTMLElement
  private overlay: HTMLDivElement | null = null
  private commands: Set<string> = new Set()
  /** 当前候选（已按前缀过滤） */
  private candidates: string[] = []
  private selected = 0
  /** 当前输入行（首词 = 补全目标） */
  private line = ''

  constructor(term: Terminal, container: HTMLElement) {
    this.term = term
    // 优先挂到 xterm 自身元素（position:relative），保证浮层定位基准正确
    this.container = (term as unknown as { element?: HTMLElement }).element || container
  }

  /** 绑定会话：异步加载命令表（不阻塞终端使用） */
  bindSession(sessionId: string) {
    loadCommands(sessionId).then((cmds) => {
      this.commands = cmds
    })
  }

  /** 输入数据到达时更新内部行缓冲；返回是否产生候选 */
  feed(data: string): boolean {
    if (data === '\r' || data === '\u0003' || data === '\u0015') {
      this.line = ''
      this.hide()
      return false
    }
    if (data === '\u007f') {
      this.line = this.line.slice(0, -1)
    } else if (data.length === 1 && data >= ' ') {
      this.line += data
    } else {
      // 控制序列（方向键/Tab 等）：交给 handleKey 处理，不更新
      return this.candidates.length > 0
    }
    this.update()
    return this.candidates.length > 0
  }

  /**
   * 拦截补全相关按键；返回 true 表示已消费（不再转发到远端）。
   * - Tab / →：补全选中候选
   * - ↑/↓：切换选中
   * - Esc：关闭浮层
   */
  handleKey(data: string): boolean {
    if (!this.candidates.length) return false
    if (data === '\t' || data === '\x1b[C') {
      this.complete()
      return true
    }
    if (data === '\x1b[A') {
      this.selected = (this.selected - 1 + this.candidates.length) % this.candidates.length
      this.render()
      return true
    }
    if (data === '\x1b[B') {
      this.selected = (this.selected + 1) % this.candidates.length
      this.render()
      return true
    }
    if (data === '\x1b') {
      this.hide()
      return true
    }
    return false
  }

  /** 重新计算候选并渲染 */
  private update() {
    const word = this.line.trim().split(/\s+/)[0] || ''
    // 仅在输入首个词时给出命令候选
    if (!word || /\s/.test(this.line.trim())) {
      this.hide()
      return
    }
    const lower = word.toLowerCase()
    this.candidates = [...this.commands]
      .filter((c) => c.toLowerCase().startsWith(lower) && c !== word)
      .sort()
      .slice(0, 8)
    this.selected = 0
    if (this.candidates.length) this.render()
    else this.hide()
  }

  /** 补全：把当前首词替换为选中候选（发送差量字符） */
  private complete() {
    const cand = this.candidates[this.selected]
    if (!cand) return
    const word = this.line.trim().split(/\s+/)[0] || ''
    const diff = cand.slice(word.length)
    if (!diff) return
    // 更新本地缓冲并发送差量
    this.line += diff
    const encoder = new TextEncoder()
    // 补全后补一个空格便于继续输入参数
    this.line += ' '
    const sessionId = this.currentSessionId
    if (sessionId) {
      sessionService.input(sessionId, encoder.encode(diff + ' ')).catch(() => {})
    }
    this.hide()
  }

  /** 由 useTerminal 注入当前 sessionId（发送补全差量用） */
  currentSessionId: string | null = null

  /** 渲染浮层（按光标 buffer 坐标定位） */
  private render() {
    if (!this.overlay) {
      const el = document.createElement('div')
      el.className = 'autocomplete-overlay'
      this.container.appendChild(el)
      this.overlay = el
    }
    const items = this.candidates
      .map(
        (c, i) =>
          `<div class="autocomplete-item${i === this.selected ? ' active' : ''}">${escapeHtml(c)}</div>`,
      )
      .join('')
    this.overlay.innerHTML = items
    this.overlay.style.display = 'block'

    // 定位：光标下方一行；超出终端底部则放上方
    const buf = this.term.buffer.active
    const cols = this.term.cols
    const rows = this.term.rows
    const cellW = this.container.clientWidth / cols
    const cellH = this.container.clientHeight / rows
    const x = Math.min(buf.cursorX, cols - 20) * cellW
    const below = (buf.cursorY + 1) * cellH
    const overlayH = Math.min(this.candidates.length, 8) * 22 + 8
    const flip = below + overlayH > this.container.clientHeight
    this.overlay.style.left = `${Math.max(0, x)}px`
    this.overlay.style.top = flip
      ? `${Math.max(0, buf.cursorY * cellH - overlayH)}px`
      : `${below}px`
  }

  hide() {
    this.candidates = []
    this.selected = 0
    if (this.overlay) this.overlay.style.display = 'none'
  }

  destroy() {
    this.overlay?.remove()
    this.overlay = null
  }
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c] || c,
  )
}
