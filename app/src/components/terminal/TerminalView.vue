<script setup lang="ts">
/**
 * TerminalView - SSH 终端视图
 *
 * 承载 xterm.js 实例 + 连接配置表单（首次）
 */
import { ref, watch, onMounted, nextTick, computed } from 'vue'
import { useTerminal } from '@/composables/useTerminal'
import { useTabsStore } from '@/stores/tabs'
import type { TabItem, SshConfig } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const tabs = useTabsStore()
const { term, error, init, connect, fit } = useTerminal()

const containerRef = ref<HTMLDivElement | null>(null)
const showForm = ref(true)
const config = ref<SshConfig>({
  host: '127.0.0.1',
  port: 22,
  username: 'root',
  auth: { type: 'password', value: '' },
})

/** 密码双向绑定（联合类型缩窄） */
const password = computed({
  get: () =>
    config.value.auth.type === 'password' ? config.value.auth.value : '',
  set: (v: string) => {
    config.value.auth = { type: 'password', value: v }
  },
})

onMounted(async () => {
  if (containerRef.value) {
    init(containerRef.value)
  }
})

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
  }
}
</script>

<template>
  <div class="terminal-view">
    <div v-if="showForm" class="connect-form">
      <h3>SSH 连接</h3>
      <div class="form-row">
        <label>主机</label>
        <input v-model="config.host" />
      </div>
      <div class="form-row">
        <label>端口</label>
        <input v-model.number="config.port" type="number" />
      </div>
      <div class="form-row">
        <label>用户名</label>
        <input v-model="config.username" />
      </div>
      <div class="form-row">
        <label>密码</label>
        <input
          v-model="password"
          type="password"
        />
      </div>
      <div v-if="error" class="error">{{ error }}</div>
      <button @click="handleConnect">连接</button>
    </div>
    <div ref="containerRef" class="xterm-container"></div>
  </div>
</template>

<style scoped>
.terminal-view {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
}
.xterm-container {
  flex: 1;
  background: #1e1e1e;
  padding: 4px;
}
.connect-form {
  padding: 16px;
  background: #fff;
}
.form-row {
  margin-bottom: 8px;
}
.form-row label {
  display: inline-block;
  width: 60px;
}
.form-row input {
  padding: 4px 8px;
  border: 1px solid var(--border-color);
  border-radius: 4px;
}
.error {
  color: #d33;
  font-size: 12px;
  margin: 8px 0;
}
button {
  padding: 6px 16px;
  background: var(--primary-color);
  color: #fff;
  border: none;
  border-radius: 4px;
  cursor: pointer;
}
</style>
