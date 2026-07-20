<script setup lang="ts">
/**
 * ChatPanel - AI 助手面板
 *
 * 消息列表 + 输入框 + 上下文开关。
 * 深色主题，背景与全局对齐。
 */
import { ref } from 'vue'
import { NButton, NInput, NPopconfirm, NIcon } from 'naive-ui'
import { Send, PlayerStop, Trash } from '@vicons/tabler'
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
    <header class="chat-header">
      <span class="chat-title">AI 助手</span>
      <ContextToggle v-model="includeContext" />
      <NPopconfirm @positive-click="clear">
        <template #trigger>
          <NButton quaternary size="small" title="清空对话">
            <template #icon><NIcon :component="Trash" /></template>
            清空
          </NButton>
        </template>
        确定清空所有对话？
      </NPopconfirm>
    </header>

    <div class="message-list">
      <div v-if="messages.length === 0" class="empty">
        <p class="empty-title">向 AI 提问运维问题</p>
        <p class="empty-hint">可开启「附带上下文」结合当前 SSH 终端输出作答</p>
      </div>
      <MessageBubble v-for="msg in messages" :key="msg.id" :message="msg" />
      <div v-if="error" class="error">{{ error }}</div>
    </div>

    <div class="input-area">
      <NInput
        v-model:value="input"
        type="textarea"
        :autosize="{ minRows: 1, maxRows: 6 }"
        :disabled="loading"
        placeholder="输入问题（Enter 发送，Shift+Enter 换行）"
        @keydown="handleKeydown"
      />
      <NButton v-if="!loading" type="primary" @click="handleSend">
        <template #icon><NIcon :component="Send" /></template>
        发送
      </NButton>
      <NButton v-else type="error" @click="abort">
        <template #icon><NIcon :component="PlayerStop" /></template>
        停止
      </NButton>
    </div>
  </div>
</template>

<style scoped>
.chat-panel {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-app);
}
.chat-header {
  display: flex;
  align-items: center;
  padding: 8px 16px;
  border-bottom: 1px solid var(--border-color);
  gap: 12px;
  flex-shrink: 0;
}
.chat-title {
  font-weight: 600;
  flex: 1;
  color: var(--text-primary);
  font-size: 14px;
}
.message-list {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}
.empty {
  text-align: center;
  margin-top: 48px;
}
.empty-title {
  color: var(--text-secondary);
  font-size: 14px;
}
.empty-hint {
  font-size: 12px;
  margin-top: 8px;
  color: var(--text-tertiary);
}
.error {
  color: var(--danger);
  font-size: 12px;
  padding: 8px;
  background: rgba(248, 113, 113, 0.08);
  border-radius: var(--radius-sm);
  margin-top: 8px;
}
.input-area {
  display: flex;
  align-items: flex-end;
  padding: 12px;
  border-top: 1px solid var(--border-color);
  gap: 8px;
  flex-shrink: 0;
}
.input-area :deep(.n-input) {
  flex: 1;
}
</style>
