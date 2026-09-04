<script setup lang="ts">
/**
 * ChatPanel - AI Agent 助手面板（右栏页签）
 *
 * 与远程服务器绑定（跟随激活的 SSH tab，可手动切换）：
 * - 未连接：空状态引导新建连接，输入禁用
 * - 已连接：上下文（终端输出/监控/服务器身份）与命令执行均作用于绑定的服务器；
 *   AI 回复中的命令块可经确认（或自动模式）执行，结果回传继续分析
 *
 * 对话状态在 agent store（全局），切换页签/收起面板不丢对话。
 */
import { ref, computed, nextTick, watch, onMounted } from 'vue'
import { NButton, NInput, NPopconfirm, NIcon, NSelect, NTag, NSwitch, NTooltip, useMessage } from 'naive-ui'
import { Send, PlayerStop, Trash, PlugConnected, Settings } from '@vicons/tabler'
import { useAgentStore } from '@/stores/agent'
import { useTabsStore } from '@/stores/tabs'
import { useProfilesStore } from '@/stores/profiles'
import { useUiStore } from '@/stores/ui'
import { useLlmStore } from '@/stores/llm'
import MessageBubble from './MessageBubble.vue'
import ContextToggle from './ContextToggle.vue'

const agent = useAgentStore()
const tabs = useTabsStore()
const profiles = useProfilesStore()
const ui = useUiStore()
const llm = useLlmStore()
const message = useMessage()

const input = ref('')
const listRef = ref<HTMLElement | null>(null)

/** 模型下拉选项：配置名 · 模型名 */
const modelOptions = computed(() =>
  llm.profiles.map((p) => ({ label: `${p.name} · ${p.model}`, value: p.id })),
)

/** 切换 LLM 配置（后端排他激活并应用到 ChatProvider，下一轮对话生效） */
async function handleModelChange(id: string) {
  const target = llm.profiles.find((p) => p.id === id)
  try {
    await llm.switchActive(id)
    message.success(`已切换到「${target?.name ?? id}」`)
  } catch (e) {
    message.error(`切换失败：${String(e)}`)
  }
}

/** 打开设置 Tab（单例，与 ActivityRail 行为一致） */
function openSettings() {
  const existing = tabs.tabs.find((t) => t.kind === 'settings')
  if (existing) tabs.setActive(existing.id)
  else tabs.addTab('settings', '设置')
}

onMounted(() => {
  void llm.load()
})

/** 是否已绑定可用的 SSH 会话 */
const connected = computed(() => !!agent.sshSessionId)

/** 可绑定的活跃 SSH 会话（多会话时手动切换） */
const sshOptions = computed(() =>
  tabs.tabs
    .filter((t) => t.kind === 'ssh' && t.sessionId && !t.disconnected)
    .map((t) => ({ label: t.title, value: t.sessionId! })),
)

/** 绑定服务器的展示名：user@host（回退 tab 标题） */
const serverLabel = computed(() => {
  const tab = tabs.tabs.find((t) => t.sessionId === agent.sshSessionId)
  if (!tab) return ''
  const profile = tab.profileId ? profiles.findById(tab.profileId) : null
  return profile ? `${profile.username}@${profile.host}` : tab.title
})

function handleSend() {
  if (!input.value.trim() || !connected.value) return
  agent.send(input.value)
  input.value = ''
}

function handleKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !e.shiftKey) {
    e.preventDefault()
    handleSend()
  }
}

/** 新消息时滚动到底部 */
watch(
  () => agent.messages.length,
  async () => {
    await nextTick()
    if (listRef.value) listRef.value.scrollTop = listRef.value.scrollHeight
  },
)
</script>

<template>
  <div class="chat-panel">
    <header class="chat-header">
      <!-- 行 1：会话信息 + 清空 -->
      <div class="header-row">
        <NTag v-if="connected" size="small" type="success" :bordered="false" class="server-tag">
          {{ serverLabel }}
        </NTag>
        <NSelect
          v-if="connected && sshOptions.length > 1"
          v-model:value="agent.sshSessionId"
          size="tiny"
          :options="sshOptions"
          placeholder="选择会话"
          class="ctx-select"
        />
        <div class="header-spacer" />
        <NPopconfirm @positive-click="agent.clear">
          <template #trigger>
            <NButton quaternary size="small" title="清空对话">
              <template #icon><NIcon :component="Trash" /></template>
            </NButton>
          </template>
          确定清空所有对话？
        </NPopconfirm>
      </div>
      <!-- 行 2：模型切换 + 上下文/自动执行开关 -->
      <div class="header-row">
        <NTooltip placement="bottom">
          <template #trigger>
            <NSelect
              v-if="llm.profiles.length > 0"
              class="model-select"
              size="tiny"
              :value="llm.activeId"
              :options="modelOptions"
              :loading="llm.switchingId !== null"
              placeholder="选择模型"
              :consistent-menu-width="false"
              @update:value="handleModelChange"
            />
            <NButton v-else quaternary size="tiny" class="model-empty" @click="openSettings">
              <template #icon><NIcon :component="Settings" /></template>
              配置模型
            </NButton>
          </template>
          Agent 使用的 LLM 配置，切换在下一轮对话生效；档案在「设置 → LLM 配置」管理
        </NTooltip>
        <ContextToggle v-model="agent.includeContext" />
        <NTooltip placement="bottom">
          <template #trigger>
            <div class="auto-run">
              <NSwitch v-model:value="agent.autoRun" size="small" />
              <span class="auto-run-label">自动执行</span>
            </div>
          </template>
          开启后 AI 生成的命令将直接在终端窗口执行并回传输出分析（危险命令仍需手动确认）
        </NTooltip>
      </div>
    </header>

    <!-- 未连接：引导先建立 SSH 连接 -->
    <div v-if="!connected" class="message-list">
      <div class="empty">
        <NIcon :component="PlugConnected" :size="36" class="empty-icon" />
        <p class="empty-title">请先连接 SSH 服务器</p>
        <p class="empty-hint">连接后 Agent 可结合终端上下文分析问题，<br />并经确认在服务器上执行命令</p>
        <NButton type="primary" size="small" @click="ui.openConnectDialog()">新建连接</NButton>
      </div>
    </div>

    <div v-else ref="listRef" class="message-list">
      <div v-if="agent.messages.length === 0" class="empty">
        <p class="empty-title">向 Agent 提问运维问题</p>
        <p class="empty-hint">基于「{{ serverLabel }}」的终端输出与监控指标作答，<br />需要更多信息时 Agent 会给出可执行的命令</p>
      </div>
      <MessageBubble v-for="msg in agent.messages" :key="msg.id" :message="msg" />
      <div v-if="agent.error" class="error">{{ agent.error }}</div>
    </div>

    <div class="input-area">
      <NInput
        v-model:value="input"
        type="textarea"
        :autosize="{ minRows: 1, maxRows: 6 }"
        :disabled="agent.loading || !connected"
        :placeholder="connected ? '输入问题（Enter 发送，Shift+Enter 换行）' : '请先连接 SSH 服务器'"
        @keydown="handleKeydown"
      />
      <NButton v-if="!agent.loading" type="primary" :disabled="!connected" @click="handleSend">
        <template #icon><NIcon :component="Send" /></template>
        发送
      </NButton>
      <NButton v-else type="error" @click="agent.abort">
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
  min-height: 0;
}
/* 头部固定两行,避免窄面板下 flex-wrap 换行位置随内容漂移 */
.chat-header {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.header-row {
  display: flex;
  align-items: center;
  gap: 8px;
  row-gap: 4px;
  min-width: 0;
  /* 正常宽度单行;面板拖到极窄时兜底换行而非溢出裁切 */
  flex-wrap: wrap;
}
.header-spacer {
  flex: 1;
}
.server-tag {
  max-width: 150px;
  flex-shrink: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  font-family: var(--font-mono, ui-monospace, monospace);
}
.ctx-select {
  width: 120px;
  flex-shrink: 0;
}
/* 模型下拉占满行内剩余空间,给两个开关留稳定位置 */
.model-select {
  flex: 1 1 110px;
  min-width: 96px;
  max-width: 240px;
}
.model-empty {
  flex-shrink: 0;
}
.auto-run {
  display: flex;
  align-items: center;
  gap: 6px;
}
.auto-run-label {
  font-size: 12px;
  color: var(--text-secondary);
  white-space: nowrap;
}
.message-list {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
  min-height: 0;
}
.empty {
  text-align: center;
  margin-top: 48px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 6px;
}
.empty-icon {
  color: var(--text-tertiary);
  margin-bottom: 8px;
}
.empty-title {
  color: var(--text-secondary);
  font-size: 14px;
  margin: 0;
}
.empty-hint {
  font-size: 12px;
  color: var(--text-tertiary);
  margin: 0 0 10px;
  line-height: 1.7;
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
