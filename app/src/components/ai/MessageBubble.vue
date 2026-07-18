<script setup lang="ts">
/**
 * MessageBubble - 单条消息（Markdown 渲染）
 *
 * 用户消息右对齐，AI 消息左对齐 + Markdown 渲染
 */
import { computed } from 'vue'
import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js'
import 'highlight.js/styles/github.css'
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
  padding: 8px 12px;
  border-radius: 8px;
  background: #f0f0f0;
  word-break: break-word;
}
.message.user .bubble {
  background: var(--primary-color);
  color: #fff;
}
.bubble.error {
  background: #ffe0e0;
  color: #c00;
}
.bubble.pending .md {
  opacity: 0.95;
}
.md :deep(code) {
  background: rgba(0, 0, 0, 0.06);
  padding: 1px 4px;
  border-radius: 3px;
  font-family: Consolas, monospace;
}
.md :deep(pre) {
  margin: 8px 0;
  padding: 8px;
  background: #f6f8fa;
  border-radius: 4px;
  overflow-x: auto;
}
.cursor {
  animation: blink 1s infinite;
}
@keyframes blink {
  50% { opacity: 0; }
}
</style>
