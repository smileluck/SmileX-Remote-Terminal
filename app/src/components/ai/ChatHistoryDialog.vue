<script setup lang="ts">
/**
 * ChatHistoryDialog - Agent 历史会话弹窗
 *
 * 展示全部历史会话（最近更新在前）：点击切换到该会话继续对话，
 * 支持删除单个会话（级联删除全部消息，删激活会话自动切最近一个）。
 */
import { NModal, NIcon, NPopconfirm, NTag } from 'naive-ui'
import { Trash } from '@vicons/tabler'
import { useAgentStore } from '@/stores/agent'
import type { AgentChat } from '@/types/ai'

defineProps<{ show: boolean }>()
const emit = defineEmits<{ (e: 'update:show', v: boolean): void }>()

const agent = useAgentStore()

/** 会话更新时间：今天 HH:mm，今年 MM-DD HH:mm，更早 YYYY-MM-DD */
function fmtTime(sec: number): string {
  const d = new Date(sec * 1000)
  const now = new Date()
  const pad = (n: number) => String(n).padStart(2, '0')
  const hm = `${pad(d.getHours())}:${pad(d.getMinutes())}`
  if (d.toDateString() === now.toDateString()) return hm
  const md = `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`
  if (d.getFullYear() === now.getFullYear()) return `${md} ${hm}`
  return `${d.getFullYear()}-${md}`
}

/** 切换到选中会话并关闭弹窗（切回历史会话后端上下文自动恢复，可直接续聊） */
async function pick(chat: AgentChat) {
  await agent.switchChat(chat.id)
  emit('update:show', false)
}
</script>

<template>
  <NModal
    :show="show"
    preset="card"
    title="历史会话"
    class="chat-history-dialog"
    :style="{ width: '440px', maxWidth: '92vw' }"
    :mask-closable="true"
    @update:show="(v: boolean) => emit('update:show', v)"
  >
    <div v-if="agent.chats.length === 0" class="empty">暂无历史会话</div>
    <div v-else class="chat-list">
      <div
        v-for="c in agent.chats"
        :key="c.id"
        class="chat-item"
        :class="{ active: c.id === agent.activeChatId }"
        @click="pick(c)"
      >
        <div class="chat-item-main">
          <span class="chat-item-title">{{ c.title || '新会话' }}</span>
          <NTag v-if="c.id === agent.activeChatId" size="small" type="primary" :bordered="false">
            当前
          </NTag>
        </div>
        <div class="chat-item-side">
          <span class="chat-item-time">{{ fmtTime(c.updatedAt) }}</span>
          <NPopconfirm @positive-click="agent.deleteChat(c.id)">
            <template #trigger>
              <NIcon :component="Trash" :size="14" class="chat-item-delete" @click.stop />
            </template>
            删除该历史会话及全部消息？
          </NPopconfirm>
        </div>
      </div>
    </div>
  </NModal>
</template>

<style scoped>
.empty {
  text-align: center;
  color: var(--text-tertiary);
  font-size: 13px;
  padding: 24px 0;
}
.chat-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  max-height: 420px;
  overflow-y: auto;
}
.chat-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  border: 1px solid transparent;
  cursor: pointer;
  transition: background-color 0.15s ease;
}
.chat-item:hover {
  background: var(--bg-elevated);
}
.chat-item.active {
  background: var(--primary-bg);
  border-color: var(--primary-border);
}
.chat-item-main {
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 1;
  min-width: 0;
}
.chat-item-title {
  font-size: 13px;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  min-width: 0;
}
.chat-item-side {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}
.chat-item-time {
  font-size: 12px;
  color: var(--text-tertiary);
}
.chat-item-delete {
  color: var(--text-tertiary);
  opacity: 0.6;
  border-radius: 3px;
}
.chat-item-delete:hover {
  opacity: 1;
  color: var(--danger);
}
</style>
