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
import { listen } from '@/services/invoke'
import type { SshConfig } from '@/types/session'

/** 终端输出事件 payload */
interface TerminalOutputPayload {
  sessionId: string
  data: number[]
}

export function useTerminal() {
  const term = ref<Terminal | null>(null)
  const fitAddon = ref<FitAddon | null>(null)
  const sessionId = ref<string | null>(null)
  const error = ref<string | null>(null)
  let unlisten: (() => void) | null = null

  /** 初始化 xterm 实例 */
  function init(container: HTMLElement) {
    const t = new Terminal({
      fontFamily: 'Consolas, "Courier New", monospace',
      fontSize: 14,
      cursorBlink: true,
      theme: {
        background: '#0f1419',
        foreground: '#e6e9ef',
        cursor: '#3a7afe',
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

  /** 建立 SSH 连接并绑定数据双向转发 */
  async function connect(config: SshConfig) {
    if (!term.value) throw new Error('终端未初始化')

    const t = term.value
    const cols = t.cols
    const rows = t.rows

    // 监听输出
    unlisten = await listen<TerminalOutputPayload>('terminal_output', (payload) => {
      if (payload.sessionId === sessionId.value) {
        t.write(new Uint8Array(payload.data))
      }
    })

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

    // 建立连接
    try {
      sessionId.value = await sessionService.connect(config, cols, rows)
    } catch (e) {
      error.value = String(e)
      throw e
    }
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
    unlisten?.()
    unlisten = null
  }

  onUnmounted(() => {
    disconnect()
    term.value?.dispose()
  })

  return {
    term: term as Ref<Terminal | null>,
    sessionId,
    error,
    init,
    connect,
    fit,
    disconnect,
  }
}
