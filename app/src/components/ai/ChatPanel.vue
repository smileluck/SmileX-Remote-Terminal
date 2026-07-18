<script setup lang="ts">
/**
 * ChatPanel - AI 助手面板
 *
 * 消息列表 + 输入框 + 上下文开关
 */
import { ref } from 'vue'
import { useChat } from '@/composables/useChat'
import MessageBubble from './MessageBubble.vue'
import ContextToggle from './ContextToggle.vue'

const { messages, loading, error, includeContext, send, abort, clear } = useChat()

const input = ref('')

function handleSend() {
  if (!input.value.trim()) return
  send(input.value)
  input.value = ''
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    handleSend()
  }
}
</script>

<template>
  <div class="chat-panel">
    <div class="chat-header">
      <span>AI 助手</span>
      <ContextToggle v-model="includeContext" />
      <button class="clear-btn" @click="clear">清空</button>
    </div>

    <div class="message-list">
      <div v-if="messages.length === 0" class="empty">
        <p>向 AI 提问运维问题</p>
        <p class="hint">可勾选"附带上下文"结合当前 SSH 终端输出</p>
      </div>
      <MessageBubble
        v-for="msg in messages"
        :key="msg.id"
        :message="msg"
      />
      <div v-if="error" class="error">{{ error }}</div>
    </div>

    <div class="input-area">
      <textarea
        v-model="input"
        :disabled="loading"
        placeholder="输入问题（Enter 发送，Shift+Enter 换行）"
        @keydown="handleKeydown"
      />
      <button v-if="!loading" @click="handleSend">发送</button>
      <button v-else class="abort" @click="abort">停止</button>
    </div>
  </div>
</template>

<style scoped>
.chat-panel {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: #fff;
}
.chat-header {
  display: flex;
  align-items: center;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border-color);
  gap: 12px;
}
.chat-header span {
  font-weight: 600;
  flex: 1;
}
.clear-btn {
  background: none;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  padding: 2px 8px;
  cursor: pointer;
  font-size: 12px;
}
.message-list {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}
.empty {
  text-align: center;
  color: #999;
  margin-top: 40px;
}
.empty .hint {
  font-size: 12px;
  margin-top: 8px;
}
.input-area {
  display: flex;
  padding: 12px;
  border-top: 1px solid var(--border-color);
  gap: 8px;
}
textarea {
  flex: 1;
  height: 60px;
  padding: 8px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
  resize: none;
  font-family: inherit;
}
button {
  padding: 8px 16px;
  background: var(--primary-color);
  color: #fff;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}
button.abort {
  background: #d33;
}
.error {
  color: #d33;
  font-size: 12px;
  padding: 8px;
}
</style>
