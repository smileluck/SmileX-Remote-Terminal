<script setup lang="ts">
/**
 * TerminalView - SSH 终端视图
 *
 * 承载 xterm.js 实例。
 * - tab 已带 sessionId（从 SideBar 会话卡片连接进入）：直接展示终端
 * - tab 未带 sessionId（从 ActivityRail 新建）：显示快速连接表单
 */
import { ref, watch, nextTick } from 'vue'
import { useResizeObserver } from '@vueuse/core'
import { NButton, NInput, NInputNumber, NIcon, NForm, NFormItem, useMessage } from 'naive-ui'
import { Terminal2 } from '@vicons/tabler'
import { useTerminal } from '@/composables/useTerminal'
import { useTabsStore } from '@/stores/tabs'
import type { TabItem, SshConfig } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const tabs = useTabsStore()
const message = useMessage()
const { term, error, init, connect, fit } = useTerminal()

const containerRef = ref<HTMLDivElement | null>(null)
/** tab 已带 sessionId 则不显示快速表单 */
const showForm = ref(!props.tab.sessionId)
const config = ref<SshConfig>({
  host: '127.0.0.1',
  port: 22,
  username: 'root',
  auth: { type: 'password', value: '' },
})

/** 容器首次挂载时初始化 xterm */
watch(
  () => containerRef.value,
  async (el) => {
    if (el && !term.value) {
      init(el)
      await nextTick()
      fit()
    }
  },
)

/** 容器尺寸变化（窗口 resize / tab 切换）时自动 fit */
useResizeObserver(containerRef, () => {
  if (term.value) fit()
})

async function handleConnect() {
  if (!term.value) return
  showForm.value = false
  try {
    await connect(config.value)
    tabs.updateTab(props.tab.id, {
      title: `${config.value.host}`,
      sessionId: 'connected',
      connecting: false,
    })
  } catch (e) {
    tabs.updateTab(props.tab.id, { error: String(e), connecting: false })
    message.error(String(e))
  }
}
</script>

<template>
  <div class="terminal-view">
    <div v-if="showForm" class="connect-form">
      <h3 class="form-title">
        <NIcon :component="Terminal2" />
        SSH 连接
      </h3>
      <NForm label-placement="top" size="small" class="quick-form">
        <NFormItem label="主机">
          <NInput v-model:value="config.host" placeholder="192.168.1.10" />
        </NFormItem>
        <NFormItem label="端口">
          <NInputNumber v-model:value="config.port" :min="1" :max="65535" style="width: 100%" />
        </NFormItem>
        <NFormItem label="用户名">
          <NInput v-model:value="config.username" placeholder="root" />
        </NFormItem>
        <NFormItem label="密码">
          <NInput
            :value="config.auth.type === 'password' ? config.auth.value : ''"
            type="password"
            show-password-on="click"
            placeholder="输入密码"
            @update:value="(v: string) => (config.auth = { type: 'password', value: v })"
          />
        </NFormItem>
        <div v-if="error" class="error">{{ error }}</div>
        <NButton type="primary" @click="handleConnect">连接</NButton>
      </NForm>
    </div>
    <div v-else ref="containerRef" class="xterm-container"></div>
  </div>
</template>

<style scoped>
.terminal-view {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: var(--bg-app);
}
.xterm-container {
  flex: 1;
  background: #0f1419;
  padding: 4px;
}
.connect-form {
  padding: 20px 24px;
  background: var(--bg-panel);
  max-width: 420px;
}
.form-title {
  display: flex;
  align-items: center;
  gap: 6px;
  margin: 0 0 12px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.quick-form {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.error {
  color: var(--danger);
  font-size: 12px;
  margin: 4px 0;
}
</style>
