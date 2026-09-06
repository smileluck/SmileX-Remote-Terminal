<script setup lang="ts">
/**
 * CodeBlock - AI 回复中的普通 shell 代码块（```bash/```sh/```shell/```zsh）
 *
 * 与 ```run 命令卡片（RunBlock，执行协议：捕获输出回传 AI）不同：
 * 普通代码块只做展示增强 —— hljs 高亮 + 「复制」+「执行」。
 * 「执行」把命令直接写入 agent 绑定的终端窗口（用户自己看结果，不回传 AI），
 * 危险命令执行前二次确认。
 */
import { computed } from 'vue'
import { NButton, NIcon, useDialog, useMessage } from 'naive-ui'
import { Copy, PlayerPlay } from '@vicons/tabler'
import hljs from 'highlight.js'
import { useAgentStore } from '@/stores/agent'
import * as sessionService from '@/services/session'

const props = defineProps<{
  /** 围栏语言（bash / sh / shell / zsh） */
  lang: string
  command: string
  /** 消息生成中不可执行 */
  disabled?: boolean
}>()

const agent = useAgentStore()
const dialog = useDialog()
const message = useMessage()

const danger = computed(() => agent.isDangerous(props.command))

/** hljs 高亮结果（无对应语言时转义原文） */
const highlighted = computed(() => {
  if (props.lang && hljs.getLanguage(props.lang)) {
    try {
      return hljs.highlight(props.command, { language: props.lang }).value
    } catch {
      // fallthrough
    }
  }
  const div = document.createElement('div')
  div.textContent = props.command
  return div.innerHTML
})

async function copyCommand() {
  try {
    await navigator.clipboard.writeText(props.command)
    message.success('命令已复制')
  } catch {
    message.error('复制失败')
  }
}

/** 仅写入绑定终端（自动补回车），不捕获输出、不回传 AI */
async function sendToTerminal() {
  const sid = agent.sshSessionId
  if (!sid) {
    message.warning('未连接 SSH 服务器，请先连接')
    return
  }
  try {
    await sessionService.input(sid, new TextEncoder().encode(props.command + '\r'))
    message.success('已发送到终端')
  } catch (e) {
    message.error(String(e))
  }
}

function execute() {
  if (danger.value) {
    dialog.warning({
      title: '危险命令确认',
      content: `该命令可能造成破坏性影响：\n\n$ ${props.command}\n\n确定要在服务器上执行吗？`,
      positiveText: '仍要执行',
      negativeText: '取消',
      onPositiveClick: () => {
        void sendToTerminal()
      },
    })
    return
  }
  void sendToTerminal()
}
</script>

<template>
  <div class="code-block" :class="{ danger }">
    <div class="code-head">
      <span class="code-lang">{{ lang }}</span>
      <div class="code-actions">
        <NButton size="tiny" quaternary title="复制命令" @click="copyCommand">
          <template #icon><NIcon :component="Copy" :size="12" /></template>
        </NButton>
        <NButton size="tiny" type="primary" secondary :disabled="disabled" @click="execute">
          <template #icon><NIcon :component="PlayerPlay" :size="12" /></template>
          执行
        </NButton>
      </div>
    </div>
    <pre class="hljs code-body"><code v-html="highlighted"></code></pre>
    <div v-if="danger" class="code-warn">⚠ 高风险命令，执行前请确认影响</div>
  </div>
</template>

<style scoped>
.code-block {
  margin: 8px 0;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  padding: 8px 10px;
  overflow: hidden;
}
.code-block.danger {
  border-left: 3px solid var(--danger);
}
.code-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-bottom: 6px;
}
.code-lang {
  font-size: 11px;
  color: var(--text-tertiary);
  letter-spacing: 0.5px;
}
.code-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.code-body {
  margin: 0;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: #0d1117;
  font-family: ui-monospace, Consolas, 'Courier New', monospace;
  font-size: 12.5px;
  white-space: pre-wrap;
  word-break: break-all;
  overflow-x: auto;
}
.code-warn {
  margin-top: 6px;
  font-size: 11px;
  color: var(--warning);
}
</style>
