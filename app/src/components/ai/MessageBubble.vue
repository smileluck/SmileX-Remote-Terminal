<script setup lang="ts">
/**
 * MessageBubble - 单条消息
 *
 * - user：右对齐纯文本
 * - assistant：左对齐 Markdown 渲染；```run 块渲染为可执行命令卡片（RunBlock），
 *   ```bash/```sh 等 shell 代码块渲染为可复制/可执行的 CodeBlock
 * - tool：命令执行结果，终端风格回显
 * 深色主题，代码高亮使用 github-dark。
 */
import { computed } from 'vue'
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js'
import 'highlight.js/styles/github-dark.css'
import type { ChatMessage } from '@/types/ai'
import { splitThink } from '@/utils/think'
import RunBlock from './RunBlock.vue'
import CodeBlock from './CodeBlock.vue'

const props = defineProps<{ message: ChatMessage }>()

const md = new MarkdownIt({
  html: false,
  linkify: true,
  breaks: true,
  highlight(str: string, lang: string): string {
    if (lang && hljs.getLanguage(lang)) {
      try {
        return `<pre class="hljs"><code>${
          hljs.highlight(str, { language: lang }).value
        }</code></pre>`
      } catch {
        // fallthrough
      }
    }
    return `<pre class="hljs"><code>${md.utils.escapeHtml(str)}</code></pre>`
  },
})

/** 围栏块标记：run = agent 命令执行协议；bash/sh/shell/zsh = 可复制的 shell 代码块（流式生成中只匹配已闭合的块） */
const BLOCK_RE = /```(run|bash|sh|shell|zsh)\s*\n([\s\S]*?)```/g

interface Segment {
  type: 'md' | 'run' | 'code' | 'think'
  text: string
  /** run 段在消息内的序号（与 agent store runStates key 对应） */
  index: number
  /** code 段的围栏语言 */
  lang?: string
  /** think 段是否闭合（流式生成中末段未闭合） */
  closed?: boolean
}

/** 把 assistant 消息拆成 think 折叠段、markdown 段、run 命令块与 shell 代码块（保持顺序） */
const segments = computed<Segment[]>(() => {
  if (props.message.role !== 'assistant') return []
  const content = props.message.content || ''
  const list: Segment[] = []
  let runIdx = 0
  for (const part of splitThink(content)) {
    if (part.type === 'think') {
      list.push({ type: 'think', text: part.text, index: -1, closed: part.closed })
      continue
    }
    let last = 0
    for (const m of part.text.matchAll(BLOCK_RE)) {
      const idx = m.index ?? 0
      if (idx > last) list.push({ type: 'md', text: part.text.slice(last, idx), index: -1 })
      if (m[1] === 'run') {
        list.push({ type: 'run', text: m[2].trim(), index: runIdx })
        runIdx++
      } else {
        list.push({ type: 'code', text: m[2].replace(/\n$/, ''), lang: m[1], index: -1 })
      }
      last = idx + m[0].length
    }
    if (last < part.text.length) list.push({ type: 'md', text: part.text.slice(last), index: -1 })
  }
  return list
})

/** think 折叠块标题：生成中未闭合显示「思考中…」 */
function thinkLabel(seg: Segment): string {
  return !seg.closed && props.message.pending ? '思考中…' : '思考过程'
}

function renderMd(text: string): string {
  return md.render(text)
}
</script>

<template>
  <div class="message" :class="message.role">
    <div class="bubble" :class="{ error: message.error, pending: message.pending }">
      <!-- 命令执行结果：终端风格回显 -->
      <template v-if="message.role === 'tool'">
        <pre class="tool-out">{{ message.content }}</pre>
      </template>
      <!-- assistant：markdown + run 命令卡片 -->
      <template v-else-if="message.role === 'assistant'">
        <template v-for="(seg, i) in segments" :key="i">
          <details v-if="seg.type === 'think'" class="think">
            <summary>{{ thinkLabel(seg) }}</summary>
            <div class="think-body">{{ seg.text }}</div>
          </details>
          <div v-else-if="seg.type === 'md' && seg.text.trim()" class="md" v-html="renderMd(seg.text)"></div>
          <RunBlock
            v-else-if="seg.type === 'run'"
            :message-id="message.id"
            :index="seg.index"
            :command="seg.text"
            :disabled="message.pending"
          />
          <CodeBlock
            v-else-if="seg.type === 'code'"
            :lang="seg.lang ?? ''"
            :command="seg.text"
            :disabled="message.pending"
          />
        </template>
        <span v-if="message.pending && segments.length === 0" class="cursor">▋</span>
      </template>
      <template v-else>{{ message.content }}</template>
      <span v-if="message.pending && message.role !== 'assistant'" class="cursor">▋</span>
    </div>
  </div>
</template>

<style scoped>
.message {
  margin-bottom: 12px;
  display: flex;
}
.message.user {
  justify-content: flex-end;
}
.message.assistant,
.message.tool {
  justify-content: flex-start;
}
.bubble {
  max-width: 92%;
  padding: 10px 14px;
  border-radius: var(--radius-lg);
  background: var(--bg-panel);
  color: var(--text-primary);
  word-break: break-word;
  line-height: 1.6;
  font-size: 14px;
}
.message.user .bubble {
  background: var(--primary-bg);
  border: 1px solid rgba(58, 122, 254, 0.3);
}
.message.tool .bubble {
  background: #0d1117;
  border: 1px solid var(--border-color);
  max-width: 100%;
}
.bubble.error {
  background: rgba(248, 113, 113, 0.1);
  color: var(--danger);
  border: 1px solid rgba(248, 113, 113, 0.3);
}
.bubble.pending .md {
  opacity: 0.95;
}
/* 思考过程折叠块 */
.think {
  margin: 4px 0;
  border-left: 2px solid var(--border-color);
  padding-left: 8px;
}
.think summary {
  cursor: pointer;
  font-size: 12px;
  color: var(--text-tertiary);
  user-select: none;
}
.think[open] summary {
  margin-bottom: 4px;
}
.think-body {
  font-size: 12px;
  line-height: 1.6;
  color: var(--text-tertiary);
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 220px;
  overflow-y: auto;
}
.tool-out {
  margin: 0;
  font-family: ui-monospace, Consolas, 'Courier New', monospace;
  font-size: 12px;
  color: var(--text-secondary);
  white-space: pre-wrap;
  word-break: break-all;
}
.md :deep(p) {
  margin: 4px 0;
}
.md :deep(code) {
  background: rgba(255, 255, 255, 0.08);
  padding: 1px 5px;
  border-radius: 3px;
  font-family: ui-monospace, Consolas, 'Courier New', monospace;
  font-size: 0.9em;
}
.md :deep(pre) {
  margin: 8px 0;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  overflow-x: auto;
}
/* github-dark.css 提供文字配色；显式覆盖背景，避免与 scoped 优先级打架 */
.md :deep(pre.hljs) {
  background: #0d1117;
}
.md :deep(pre code) {
  background: transparent;
  padding: 0;
}
.cursor {
  animation: blink 1s infinite;
}
@keyframes blink {
  50% {
    opacity: 0;
  }
}
</style>
