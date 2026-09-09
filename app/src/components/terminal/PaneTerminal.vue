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
import { NButton, NIcon, NSelect, NDropdown, NModal, useMessage } from 'naive-ui'
import { Terminal2, X, Plus } from '@vicons/tabler'
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
const { term, sessionId: ownSession, init, bind, fit, getInputLine } = useTerminal()

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

/**
 * 拦截 webview 原生粘贴（Cmd/Ctrl+V 触发 textarea 的 paste DOM 事件）：
 * capture 阶段在容器上截获，阻止 xterm 默认粘贴处理，统一走规范化流程。
 */
function onNativePaste(e: ClipboardEvent) {
  e.preventDefault()
  e.stopPropagation()
  const text = e.clipboardData?.getData('text') ?? ''
  if (text) queuePaste(text)
}

/* ---------------- 规范化粘贴 ---------------- */

const showPasteConfirm = ref(false)
const pastePending = ref('')

/**
 * 粘贴统一入口：CRLF/孤 CR 一律归一为 LF（CRLF 进 PTY 会变成双重换行、
 * 孤 CR 在 bracketed paste 下显示为 ^M 乱行）；多行内容先弹确认，
 * 防止 shell 未启用 bracketed paste 时逐行立即执行造成错乱。
 */
function queuePaste(raw: string) {
  const text = raw.replace(/\r\n/g, '\n').replace(/\r/g, '\n')
  if (!text) return
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
      el.addEventListener('paste', onNativePaste, true)
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
  <div class="pane" :class="{ active }" @mousedown="emit('focus')">
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
