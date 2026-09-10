/**
 * useTerminal - 终端 composable
 *
 * 封装 xterm.js 实例 + SSH 会话绑定 + 输入/输出转发。
 */
import { ref, onUnmounted, watch, type Ref } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'

import * as sessionService from '@/services/session'
import * as snippetsService from '@/services/snippets'
import { useThemeStore, xtermThemeDark, xtermThemeLight } from '@/stores/theme'
import type { SshConfig, TerminalOutputPayload } from '@/types/session'

export function useTerminal() {
  const themeStore = useThemeStore()
  const term = ref<Terminal | null>(null)
  const fitAddon = ref<FitAddon | null>(null)
  const sessionId = ref<string | null>(null)
  const error = ref<string | null>(null)

  /** 初始化 xterm 实例 */
  function init(container: HTMLElement) {
    const t = new Terminal({
      fontFamily: 'Consolas, "Courier New", monospace',
      fontSize: 14,
      cursorBlink: true,
      theme: themeStore.resolved === 'dark' ? xtermThemeDark : xtermThemeLight,
    })
    const fit = new FitAddon()
    t.loadAddon(fit)
    t.open(container)
    fit.fit()
    term.value = t
    fitAddon.value = fit
  }

  /** 主题切换时更新已创建终端的配色 */
  watch(
    () => themeStore.resolved,
    (r) => {
      if (term.value) term.value.options.theme = r === 'dark' ? xtermThemeDark : xtermThemeLight
    },
  )

  /** 绑定到已建立的会话（SideBar 连接的 tab：xterm 后挂载场景）
   *
   * 幂等：重复调用同一 sessionId 直接返回。
   */
  async function bind(id: string) {
    if (!term.value || sessionId.value === id) return
    const t = term.value

    attachOutput(id, t)
    attachInput(t)
    sessionId.value = id
  }

  /** 建立 SSH 连接并绑定数据双向转发 */
  async function connect(config: SshConfig) {
    if (!term.value) throw new Error('终端未初始化')

    const t = term.value
    const cols = t.cols
    const rows = t.rows

    attachInput(t)

    // 建立连接（输出经 ipc::Channel 由 service 层分发）
    try {
      sessionId.value = await sessionService.connect(config, cols, rows)
      attachOutput(sessionId.value, t)
    } catch (e) {
      error.value = String(e)
      throw e
    }
  }

  /** 当前注册的输出回调（id + 引用，按引用解绑，不影响绑定同一会话的其他终端） */
  let boundOutput: { id: string; cb: (p: TerminalOutputPayload) => void } | null = null

  /** 解绑当前输出回调 */
  function detachOutput() {
    if (boundOutput) {
      sessionService.unbindOutput(boundOutput.id, boundOutput.cb)
      boundOutput = null
    }
  }

  /** 注册输出分发（Channel 回调 → xterm.write）；先解绑旧会话，防止其输出继续写入本终端（串台） */
  function attachOutput(id: string, t: Terminal) {
    detachOutput()
    const cb = (payload: TerminalOutputPayload) => {
      if (payload.sessionId === id) {
        t.write(new Uint8Array(payload.data))
      }
    }
    sessionService.bindOutput(id, cb)
    boundOutput = { id, cb }
  }

  /** 输入监听是否已挂载（xterm onData 是累加订阅，重复挂载会导致击键多次发送） */
  let inputAttached = false

  /** 注册输入/resize 转发（幂等：仅首次调用生效） */
  function attachInput(t: Terminal) {
    if (inputAttached) return
    inputAttached = true
    // 用户输入 → 后端
    t.onData((data) => {
      if (sessionId.value) {
        const encoder = new TextEncoder()
        sessionService.input(sessionId.value, encoder.encode(data)).catch((e) => {
          error.value = String(e)
        })
        recordHistory(sessionId.value, data)
      }
    })

    // resize → 后端
    t.onResize(({ cols, rows }) => {
      if (sessionId.value) {
        sessionService.resize(sessionId.value, cols, rows).catch(() => {})
      }
    })
  }

  /** 命令历史采集：缓冲当前输入行，回车时落库（不记录纯控制序列） */
  let inputLine = ''
  function recordHistory(sid: string, data: string) {
    if (data === '\r') {
      const cmd = inputLine.trim()
      inputLine = ''
      if (cmd && !cmd.startsWith(' ') && cmd.length <= 500) {
        snippetsService.historyAdd(sid, cmd).catch(() => {})
      }
      return
    }
    if (data === '\u007f') {
      // 退格
      inputLine = inputLine.slice(0, -1)
      return
    }
    if (data === '\u0003' || data === '\u0015') {
      // Ctrl+C / Ctrl+U：清空当前行
      inputLine = ''
      return
    }
    // 仅累积可打印字符（忽略方向键/Tab 等）
    if (data.length === 1 && data >= ' ') inputLine += data
  }

  /** 当前正在编辑的命令行（右键「添加到常用记录」无选区时的回退来源） */
  function getInputLine(): string {
    return inputLine.trim()
  }

  /** 调整终端尺寸（容器变化时调用） */
  function fit() {
    fitAddon.value?.fit()
  }

  /** 断开连接 */
  async function disconnect() {
    if (sessionId.value) {
      detachOutput()
      await sessionService.disconnect(sessionId.value).catch(() => {})
      sessionId.value = null
    }
  }

  onUnmounted(() => {
    detachOutput()
    term.value?.dispose()
  })

  return {
    term: term as Ref<Terminal | null>,
    sessionId,
    error,
    init,
    bind,
    connect,
    fit,
    disconnect,
    getInputLine,
  }
}
