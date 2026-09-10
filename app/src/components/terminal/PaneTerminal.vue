<script setup lang="ts">
/**
 * PaneTerminal - 分屏中的单个终端窗格
 *
 * - 已绑定 sessionId：渲染 xterm 并绑定输出
 * - 未绑定：显示「绑定现有会话 / 新建连接」选择器
 *   （新建连接打开全局连接弹窗，连接结果会开新 tab）
 */
import { ref, watch, nextTick, computed } from 'vue'
import { useResizeObserver } from '@vueuse/core'
import { NButton, NIcon, NSelect, NDropdown, NModal, NInput, useMessage } from 'naive-ui'
import { Terminal2, X, Plus, ArrowUp, ArrowDown } from '@vicons/tabler'
import { useTerminal } from '@/composables/useTerminal'
import { useTabsStore } from '@/stores/tabs'
import { useUiStore } from '@/stores/ui'
import { useSnippetsStore } from '@/stores/snippets'

const props = defineProps<{
  sessionId: string | null
  closable: boolean
  active?: boolean
  /** 绑定会话时写入的首行提示（如 `minio@nas01:/$ `，分屏窗格用，避免空白） */
  initialLine?: string
}>()
const emit = defineEmits<{
  (e: 'focus'): void
  (e: 'bind', sid: string): void
  (e: 'close'): void
}>()

const tabs = useTabsStore()
const ui = useUiStore()
const message = useMessage()
const snippets = useSnippetsStore()
const { term, searchAddon, sessionId: ownSession, init, bind, fit, getInputLine } = useTerminal()

const containerRef = ref<HTMLDivElement | null>(null)

/** 终端右键菜单（manual NDropdown，同 FilePanel 模式） */
const menuShow = ref(false)
const menuX = ref(0)
const menuY = ref(0)
/** 打开菜单时捕获的选中文本与当前输入行（菜单打开期间保持不变） */
const menuSelection = ref('')
const menuLine = ref('')

function onContextMenu(e: MouseEvent) {
  menuSelection.value = term.value?.getSelection() ?? ''
  menuLine.value = getInputLine()
  menuX.value = e.clientX
  menuY.value = e.clientY
  menuShow.value = true
}

const menuOptions = computed(() => [
  { label: '复制', key: 'copy', disabled: !menuSelection.value },
  { label: '粘贴', key: 'paste' },
  {
    label: '添加到常用记录',
    key: 'add-snippet',
    disabled: !menuSelection.value && !menuLine.value,
  },
])

function onMenuSelect(key: string) {
  menuShow.value = false
  switch (key) {
    case 'copy':
      void copySelection()
      break
    case 'paste':
      void pasteClipboard()
      break
    case 'add-snippet':
      void addToSnippets()
      break
  }
  term.value?.focus()
}

async function copySelection() {
  if (!menuSelection.value) return
  try {
    await navigator.clipboard.writeText(menuSelection.value)
  } catch {
    message.warning('复制失败：无法访问剪贴板')
  }
}

/** 粘贴：读取剪贴板后走统一粘贴入口 */
async function pasteClipboard() {
  let text = ''
  try {
    text = await navigator.clipboard.readText()
  } catch {
    message.warning('粘贴失败：无法读取剪贴板')
    return
  }
  if (text) queuePaste(text)
}

/* ---------------- 规范化粘贴 ---------------- */

const showPasteConfirm = ref(false)
const pastePending = ref('')

/**
 * 剥离剪贴板夹带的 ANSI 转义序列（CSI/OSC/两字节 ESC 序列）与 C0 控制字符
 * （保留 \t 与 \n）。从终端输出/网页复制的内容常带肉眼不可见的转义序列，
 * 粘贴进 PTY 后会直接改变终端模式（如应用光标键模式）或 readline 状态，
 * 导致之后方向键、回显错乱。用 charCode 实现，避免在源码中写转义字面量。
 */
function sanitizePaste(raw: string): { text: string; stripped: boolean } {
  let out = ''
  let i = 0
  while (i < raw.length) {
    const code = raw.charCodeAt(i)
    if (code === 0x1b) {
      const next = raw[i + 1]
      if (next === '[') {
        // CSI 序列：跳到收尾字节（0x40–0x7e）
        i += 2
        while (i < raw.length && (raw.charCodeAt(i) < 0x40 || raw.charCodeAt(i) > 0x7e)) i++
        i++
        continue
      }
      if (next === ']') {
        // OSC 序列：到 BEL 或 ESC\ 结束
        i += 2
        while (i < raw.length) {
          if (raw.charCodeAt(i) === 0x07) {
            i++
            break
          }
          if (raw.charCodeAt(i) === 0x1b && raw[i + 1] === '\\') {
            i += 2
            break
          }
          i++
        }
        continue
      }
      i += 2 // 两字节 ESC 序列
      continue
    }
    // 剔除其余 C0 控制字符（保留 0x09 \t 与 0x0a \n）与 DEL
    if ((code < 0x20 && code !== 0x09 && code !== 0x0a) || code === 0x7f) {
      i++
      continue
    }
    out += raw[i]
    i++
  }
  return { text: out, stripped: out !== raw }
}

/**
 * 粘贴统一入口（仅右键菜单路径；⌘/Ctrl+V 原生粘贴走 xterm 默认处理，
 * 其内部已做 CRLF 归一与 bracketed paste——勿再拦截 paste DOM 事件：
 * 拦截会绕开 xterm 内部 textarea 状态管理，导致残留文本在后续按键被重发）：
 * CRLF/孤 CR 归一为 LF → 剥离转义/控制字符；多行内容先弹确认，
 * 防止 shell 未启用 bracketed paste 时逐行立即执行造成错乱。
 */
function queuePaste(raw: string) {
  const normalized = raw.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
  const { text, stripped } = sanitizePaste(normalized)
  if (!text) return
  if (stripped) message.info('已剔除粘贴内容中的转义/控制字符')
  if (text.includes('\n')) {
    pastePending.value = text
    showPasteConfirm.value = true
    return
  }
  term.value?.paste(text)
}

const pasteLineCount = computed(() => pastePending.value.split('\n').length)
const pastePreview = computed(() => {
  const lines = pastePending.value.split('\n')
  const head = lines.slice(0, 6).join('\n')
  return lines.length > 6 ? `${head}\n…（共 ${lines.length} 行）` : head
})

function confirmPaste() {
  showPasteConfirm.value = false
  const text = pastePending.value
  pastePending.value = ''
  if (text) term.value?.paste(text)
  term.value?.focus()
}

/* ---------------- 终端内搜索（Ctrl/⌘F） ---------------- */

const searchShow = ref(false)
const searchText = ref('')
const searchInputRef = ref<InstanceType<typeof NInput> | null>(null)

/** 匹配高亮配色（琥珀色系，深/浅主题均可读） */
const SEARCH_OPTS = {
  incremental: true,
  decorations: {
    matchBackground: '#6b5300',
    matchOverviewRuler: '#e0a000',
    activeMatchBackground: '#b57614',
    activeMatchColorOverviewRuler: '#ffc400',
  },
} as const

/** ⌘/Ctrl+F 打开搜索（事件从 xterm textarea 冒泡到 .pane 容器） */
function onPaneKeydown(e: KeyboardEvent) {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') {
    e.preventDefault()
    openSearch()
  }
}

function openSearch() {
  searchShow.value = true
  void nextTick(() => searchInputRef.value?.focus())
}

function closeSearch() {
  searchShow.value = false
  searchText.value = ''
  searchAddon.value?.clearDecorations()
  term.value?.focus()
}

function searchNext() {
  if (searchText.value) searchAddon.value?.findNext(searchText.value, SEARCH_OPTS)
}

function searchPrev() {
  if (searchText.value) searchAddon.value?.findPrevious(searchText.value, SEARCH_OPTS)
}

// 输入即增量查找
watch(searchText, (t) => {
  if (t) searchAddon.value?.findNext(t, SEARCH_OPTS)
})

function onSearchKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter') {
    e.preventDefault()
    if (e.shiftKey) searchPrev()
    else searchNext()
  } else if (e.key === 'Escape') {
    closeSearch()
  }
}

/** 添加到常用记录：优先选中文本，否则当前输入行；加入「未分组」 */
async function addToSnippets() {
  const command = (menuSelection.value || menuLine.value).trim()
  if (!command) return
  try {
    await snippets.load()
    await snippets.save({
      id: crypto.randomUUID(),
      name: command.slice(0, 30),
      command,
      tags: '',
      groupName: '',
      sortOrder: 0,
      kind: 'command',
      checkCmd: '',
      createdAt: Math.floor(Date.now() / 1000),
    })
    message.success('已添加到常用记录')
  } catch (e) {
    message.error(`添加失败：${e}`)
  }
}

/** 首行提示只写一次（挂载即绑定 / 后经选择器绑定两条路径共用） */
let initialWritten = false
function writeInitialLine() {
  if (initialWritten || !props.initialLine || !term.value) return
  initialWritten = true
  term.value.writeln(props.initialLine)
}

/** 挂载时初始化 xterm 并绑定（tab 已带会话） */
watch(
  () => containerRef.value,
  async (el) => {
    if (el && !term.value) {
      init(el)
      await nextTick()
      fit()
      if (props.sessionId && props.sessionId !== ownSession.value) {
        writeInitialLine()
        bind(props.sessionId)
      }
    }
  },
)

/** 外部 sessionId 变化（pane 重绑定） */
watch(
  () => props.sessionId,
  (sid) => {
    if (sid && sid !== ownSession.value && term.value) {
      writeInitialLine()
      bind(sid)
    }
  },
)

useResizeObserver(containerRef, () => {
  // tab 被隐藏（v-show 切走）时容器尺寸为 0，跳过无效 fit
  if (term.value && containerRef.value?.clientWidth) fit()
})

/** 所有 tab 的活跃 SSH 会话（供 pane 绑定） */
const sessionOptions = computed(() =>
  tabs.tabs
    .filter((t) => t.kind === 'ssh' && t.sessionId && !t.disconnected)
    .map((t) => ({ label: `${t.title}（${(t.sessionId as string).slice(0, 8)}）`, value: t.sessionId as string })),
)

const pickedSession = ref<string | null>(null)
</script>

<template>
  <div class="pane" :class="{ active }" @mousedown="emit('focus')" @keydown="onPaneKeydown">
    <div class="pane-head">
      <span class="pane-title">
        <NIcon :component="Terminal2" :size="12" />
        {{ sessionId ? '会话 ' + sessionId.slice(0, 8) : '未绑定' }}
      </span>
      <NButton v-if="closable" quaternary circle size="tiny" @click="emit('close')">
        <NIcon :component="X" :size="12" />
      </NButton>
    </div>

    <div
      v-if="sessionId"
      ref="containerRef"
      class="pane-term"
      @contextmenu.prevent="onContextMenu"
    ></div>

    <!-- 终端内搜索框（Ctrl/⌘F 唤起，Esc 关闭） -->
    <div v-if="sessionId && searchShow" class="term-search" @mousedown.stop @keydown.stop>
      <NInput
        ref="searchInputRef"
        v-model:value="searchText"
        size="tiny"
        placeholder="搜索终端内容"
        class="term-search-input"
        @keydown="onSearchKeydown"
      />
      <NButton quaternary circle size="tiny" title="上一个（Shift+Enter）" @click="searchPrev">
        <NIcon :component="ArrowUp" :size="13" />
      </NButton>
      <NButton quaternary circle size="tiny" title="下一个（Enter）" @click="searchNext">
        <NIcon :component="ArrowDown" :size="13" />
      </NButton>
      <NButton quaternary circle size="tiny" title="关闭（Esc）" @click="closeSearch">
        <NIcon :component="X" :size="13" />
      </NButton>
    </div>

    <div v-else class="pane-picker">
      <template v-if="sessionOptions.length">
        <p class="picker-label">绑定现有会话</p>
        <NSelect
          v-model:value="pickedSession"
          size="small"
          :options="sessionOptions"
          placeholder="选择活跃 SSH 会话"
          @update:value="(v: string) => emit('bind', v)"
        />
      </template>
      <template v-else>
        <p class="picker-label">暂无活跃会话</p>
      </template>

      <NButton size="small" dashed class="picker-btn" @click="ui.openConnectDialog()">
        <template #icon><NIcon :component="Plus" :size="14" /></template>
        新建 SSH 连接
      </NButton>
    </div>

    <NDropdown
      trigger="manual"
      :show="menuShow"
      :x="menuX"
      :y="menuY"
      placement="bottom-start"
      :options="menuOptions"
      @select="onMenuSelect"
      @clickoutside="menuShow = false"
    />

    <!-- 多行粘贴确认 -->
    <NModal
      v-model:show="showPasteConfirm"
      preset="card"
      title="粘贴多行内容"
      style="width: 480px"
      :bordered="false"
    >
      <p class="paste-warn">
        粘贴内容包含 {{ pasteLineCount }} 行，若远端 shell 未启用 bracketed paste 将逐行立即执行。请确认内容：
      </p>
      <pre class="paste-preview">{{ pastePreview }}</pre>
      <template #footer>
        <div class="dialog-footer">
          <NButton size="small" @click="showPasteConfirm = false">取消</NButton>
          <NButton size="small" type="primary" @click="confirmPaste">粘贴</NButton>
        </div>
      </template>
    </NModal>
  </div>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  position: relative;
}
.pane-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 8px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
/* 选中窗格高亮（分屏作用目标） */
.pane.active .pane-head {
  border-bottom-color: var(--primary);
}
.pane.active .pane-title {
  color: var(--primary);
}
.pane-title {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.pane-term {
  flex: 1;
  min-height: 0;
  background: var(--bg-app);
  padding: 2px;
}
.pane-picker {
  flex: 1;
  overflow-y: auto;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.picker-label {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0;
}
.picker-btn {
  align-self: flex-start;
}
.term-search {
  position: absolute;
  top: 34px;
  right: 10px;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 4px;
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  box-shadow: 0 4px 16px rgba(0, 0, 0, 0.35);
}
.term-search-input {
  width: 180px;
}
.paste-warn {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0 0 8px;
}
.paste-preview {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-primary);
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 8px 10px;
  margin: 0;
  max-height: 40vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
