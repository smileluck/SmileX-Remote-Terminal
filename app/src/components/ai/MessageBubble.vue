<script setup lang="ts">
/**
 * MessageBubble - 单条消息（Markdown 渲染）
 *
 * 用户消息右对齐，AI 消息左对齐 + Markdown 渲染。
 * 深色主题，代码高亮使用 github-dark。
 */
import { computed } from 'vue'
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js'
import 'highlight.js/styles/github-dark.css'
import type { ChatMessage } from '@/types/ai'

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

const html = computed(() => md.render(props.message.content || ''))
</script>

<template>
  <div class="message" :class="message.role">
    <div class="bubble" :class="{ error: message.error, pending: message.pending }">
      <div v-if="message.role === 'assistant'" class="md" v-html="html"></div>
      <template v-else>{{ message.content }}</template>
      <span v-if="message.pending" class="cursor">▋</span>
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
.message.assistant {
  justify-content: flex-start;
}
.bubble {
  max-width: 80%;
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
.bubble.error {
  background: rgba(248, 113, 113, 0.1);
  color: var(--danger);
  border: 1px solid rgba(248, 113, 113, 0.3);
}
.bubble.pending .md {
  opacity: 0.95;
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
