<script setup lang="ts">
/**
 * DesktopView - 远程桌面视图
 *
 * 承载 Canvas 渲染 + 连接配置表单（首次）
 */
import { ref, onUnmounted } from 'vue'
import { useDesktop } from '@/composables/useDesktop'
import { useTabsStore } from '@/stores/tabs'
import type { TabItem, DesktopConfig } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const tabs = useTabsStore()
const { canvasRef, error, frameCount, connect, disconnect } = useDesktop()

const showForm = ref(true)
const config = ref<DesktopConfig>({
  kind: props.tab.kind === 'host' ? 'host' : 'rdp',
  host: '127.0.0.1',
  port: 3389,
  username: '',
  password: '',
  width: 1920,
  height: 1080,
  colorDepth: 32,
})

async function handleConnect() {
  showForm.value = false
  try {
    await connect(config.value)
    tabs.updateTab(props.tab.id, { title: `${config.value.host}`, sessionId: 'connected' })
  } catch (e) {
    tabs.updateTab(props.tab.id, { error: String(e) })
  }
}

onUnmounted(() => {
  disconnect()
})
</script>

<template>
  <div class="desktop-view">
    <div v-if="showForm" class="connect-form">
      <h3>远程桌面（{{ config.kind === 'rdp' ? 'RDP' : 'macOS' }}）</h3>
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
        <input v-model="config.password" type="password" />
      </div>
      <div v-if="error" class="error">{{ error }}</div>
      <button @click="handleConnect">连接</button>
    </div>
    <div v-else class="canvas-wrap">
      <canvas ref="canvasRef" class="desktop-canvas"></canvas>
      <div class="status">帧数: {{ frameCount }}</div>
    </div>
  </div>
</template>

<style scoped>
.desktop-view {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: #000;
}
.canvas-wrap {
  flex: 1;
  position: relative;
  overflow: hidden;
}
.desktop-canvas {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.status {
  position: absolute;
  top: 8px;
  right: 8px;
  color: #0f0;
  font-size: 12px;
  background: rgba(0, 0, 0, 0.5);
  padding: 2px 8px;
  border-radius: 3px;
}
.connect-form {
  padding: 16px;
  background: #fff;
  color: #333;
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
