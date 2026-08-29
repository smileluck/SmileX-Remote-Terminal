/**
 * useTerminal - 终端 composable
 *
 * 封装 xterm.js 实例 + SSH 会话绑定 + 输入/输出转发。
 */
import { ref, onUnmounted, type Ref } from 'vue'
import { Terminal } from '@xterm/xterm'
import { FitAddon } from '@xterm/addon-fit'
import '@xterm/xterm/css/xterm.css'

import * as sessionService from '@/services/session'
import type { SshConfig } from '@/types/session'

export function useTerminal() {
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
      theme: {
        background: '#0f1419',
        foreground: '#e6e9ef',
        cursor: '#4c8dff',
        cursorAccent: '#0f1419',
        selectionBackground: 'rgba(58, 122, 254, 0.3)',
        black: '#0f1419',
        red: '#f87171',
        green: '#34d399',
        yellow: '#fbbf24',
        blue: '#6ba9ff',
        magenta: '#a87cff',
        cyan: '#22d3ee',
        white: '#e6e9ef',
        brightBlack: '#6b7280',
        brightRed: '#fca5a5',
        brightGreen: '#6ee7b7',
        brightYellow: '#fcd34d',
        brightBlue: '#93c5fd',
        brightMagenta: '#c4b5fd',
        brightCyan: '#67e8f9',
        brightWhite: '#f3f4f6',
      },
    })
    const fit = new FitAddon()
    t.loadAddon(fit)
    t.open(container)
    fit.fit()
    term.value = t
    fitAddon.value = fit
  }

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

  /** 注册输出分发（Channel 回调 → xterm.write） */
  function attachOutput(id: string, t: Terminal) {
    sessionService.bindOutput(id, (payload) => {
      if (payload.sessionId === id) {
        t.write(new Uint8Array(payload.data))
      }
    })
  }

  /** 注册输入/resize 转发（幂等：xterm 事件可重复注册但建议仅一次） */
  function attachInput(t: Terminal) {
    // 用户输入 → 后端
    t.onData((data) => {
      if (sessionId.value) {
        const encoder = new TextEncoder()
        sessionService.input(sessionId.value, encoder.encode(data)).catch((e) => {
          error.value = String(e)
        })
      }
    })

    // resize → 后端
    t.onResize(({ cols, rows }) => {
      if (sessionId.value) {
        sessionService.resize(sessionId.value, cols, rows).catch(() => {})
      }
    })
  }

  /** 调整终端尺寸（容器变化时调用） */
  function fit() {
    fitAddon.value?.fit()
  }

  /** 断开连接 */
  async function disconnect() {
    if (sessionId.value) {
      await sessionService.disconnect(sessionId.value).catch(() => {})
      sessionId.value = null
    }
  }

  onUnmounted(() => {
    if (sessionId.value) sessionService.unbindOutput(sessionId.value)
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
  }
}
