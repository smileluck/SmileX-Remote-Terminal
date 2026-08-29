/**
 * theme store - 深浅色主题
 *
 * mode：'dark' | 'light' | 'auto'（跟随系统），localStorage 持久化；
 * resolved 为实际生效主题。html.theme-light class 切换全局 CSS token。
 */
import { ref, computed, watchEffect } from 'vue'
import { defineStore } from 'pinia'

export type ThemeMode = 'dark' | 'light' | 'auto'

const STORAGE_KEY = 'smilex-theme-mode'

function readMode(): ThemeMode {
  const v = localStorage.getItem(STORAGE_KEY)
  return v === 'light' || v === 'dark' || v === 'auto' ? v : 'dark'
}

function systemDark(): boolean {
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

export const useThemeStore = defineStore('theme', () => {
  const mode = ref<ThemeMode>(readMode())
  const systemIsDark = ref(systemDark())

  const media = window.matchMedia('(prefers-color-scheme: dark)')
  const onChange = (e: MediaQueryListEvent) => (systemIsDark.value = e.matches)
  media.addEventListener('change', onChange)

  /** 实际生效主题 */
  const resolved = computed<'dark' | 'light'>(() =>
    mode.value === 'auto' ? (systemIsDark.value ? 'dark' : 'light') : mode.value,
  )

  /** 应用到 <html> 的 class（默认深色，浅色加 .theme-light） */
  watchEffect(() => {
    document.documentElement.classList.toggle('theme-light', resolved.value === 'light')
  })

  function setMode(m: ThemeMode) {
    mode.value = m
    localStorage.setItem(STORAGE_KEY, m)
  }

  return { mode, resolved, setMode }
})

/** xterm 深色主题（与 main.css token 对齐） */
export const xtermThemeDark = {
  background: '#0f1419',
  foreground: '#e6e9ef',
  cursor: '#4c8dff',
  cursorAccent: '#0f1419',
  selectionBackground: 'rgba(76, 141, 255, 0.3)',
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
}

/** xterm 浅色主题 */
export const xtermThemeLight = {
  background: '#ffffff',
  foreground: '#1f2733',
  cursor: '#2e6be6',
  cursorAccent: '#ffffff',
  selectionBackground: 'rgba(46, 107, 230, 0.2)',
  black: '#1f2733',
  red: '#dc2626',
  green: '#059669',
  yellow: '#d97706',
  blue: '#2e6be6',
  magenta: '#7c3aed',
  cyan: '#0891b2',
  white: '#5b6575',
  brightBlack: '#8b95a5',
  brightRed: '#b91c1c',
  brightGreen: '#047857',
  brightYellow: '#b45309',
  brightBlue: '#1d4ed8',
  brightMagenta: '#6d28d9',
  brightCyan: '#0e7490',
  brightWhite: '#1f2733',
}
