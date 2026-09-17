<script setup lang="ts">
/**
 * PaneTerminal - 单会话终端组件（纯 xterm 载体）
 *
 * - 已绑定 sessionId：渲染 xterm 并绑定输出（sessionId 变化时自动重绑，断线重连走此路径）
 * - 右键菜单：复制 / 粘贴 / 发送给 AI / 解释此错误 / 添加到常用记录
 * - ⌘/Ctrl+F 终端内搜索
 */
import { ref, watch, nextTick, computed } from 'vue'
import { useResizeObserver } from '@vueuse/core'
import { NButton, NIcon, NDropdown, NModal, NInput, useMessage } from 'naive-ui'
import { X, ArrowUp, ArrowDown } from '@vicons/tabler'
import { useTerminal } from '@/composables/useTerminal'
import { useLayoutStore } from '@/stores/layout'
import { useAgentStore } from '@/stores/agent'
import { useSnippetsStore } from '@/stores/snippets'

const props = defineProps<{
  sessionId: string | null
}>()

const layout = useLayoutStore()
const agent = useAgentStore()
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
  ...(menuSelection.value
    ? [
        { label: '发送给 AI', key: 'send-to-ai' },
        { label: '解释此错误', key: 'explain-error' },
      ]
    : []),
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
    case 'send-to-ai':
      void sendSelectionToAi()
      break
    case 'explain-error':
      void explainSelection()
      break
    case 'add-snippet':
      void addToSnippets()
      break
  }
  term.value?.focus()
}

/** 选中文本送入 AI 的长度上限（超出截断尾部并标注） */
const AI_SELECTION_MAX = 4 * 1024

function clipSelection(text: string): string {
  const t = text.trim()
  if (t.length <= AI_SELECTION_MAX) return t
  return `${t.slice(0, AI_SELECTION_MAX)}\n…（内容过长，已截断）`
}

/** 发送给 AI：选中文本以代码块预填进输入框，并打开绑定该服务器的 AI 面板 */
async function sendSelectionToAi() {
  const text = clipSelection(menuSelection.value)
  if (!text || !props.sessionId) return
  if (!(await agent.ensureChatForSession(props.sessionId))) return
  layout.openRightPanel('agent')
  agent.prefillInput(`\`\`\`\n${text}\n\`\`\`\n`)
}

/** 解释此错误：选中文本作为报错直接向该服务器的聊天会话提问 */
async function explainSelection() {
  const text = clipSelection(menuSelection.value)
  if (!text || !props.sessionId) return
  if (agent.busy) {
    message.warning('AI 正在生成中，请稍后重试')
    return
  }
  if (!(await agent.ensureChatForSession(props.sessionId))) return
  layout.openRightPanel('agent')
  await agent.send(`请解释以下终端输出/报错的原因和解决办法：\n\`\`\`\n${text}\n\`\`\``)
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
      startCmd: '',
      stopCmd: '',
      restartCmd: '',
      statusCmd: '',
      workDir: '',
      profileId: '',
      scope: 'host',
      builtin: false,
      createdAt: Math.floor(Date.now() / 1000),
    })
    message.success('已添加到常用记录')
  } catch (e) {
    message.error(`添加失败：${e}`)
  }
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
        bind(props.sessionId)
      }
    }
  },
)

/** 外部 sessionId 变化（断线重连后换绑新会话） */
watch(
  () => props.sessionId,
  (sid) => {
    if (sid && sid !== ownSession.value) {
      bind(sid)
    }
  },
)

useResizeObserver(containerRef, () => {
  // tab 被隐藏（v-show 切走）时容器尺寸为 0，跳过无效 fit
  if (term.value && containerRef.value?.clientWidth) fit()
})
</script>

<template>
  <div class="pane" @keydown="onPaneKeydown">
    <div
      v-if="sessionId"
      ref="containerRef"
      class="pane-term"
      @contextmenu.prevent="onContextMenu"
    ></div>
    <div v-else class="pane-empty">未绑定会话</div>

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
.pane-term {
  flex: 1;
  min-height: 0;
  background: var(--bg-app);
  padding: 4px;
}
.pane-empty {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  color: var(--text-tertiary);
}
.term-search {
  position: absolute;
  top: 10px;
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
