<script setup lang="ts">
/**
 * DesktopView - 远程桌面视图
 *
 * 承载 Canvas 渲染 + 连接配置表单（首次）。
 * rdp/host 的唯一连接入口（ConnectForm 暂仅支持 SSH，待 rdp/host 功能落地后统一）。
 */
import { ref, onUnmounted } from 'vue'
import {
  NButton,
  NInput,
  NInputNumber,
  NForm,
  NFormItem,
  NTag,
  NIcon,
  useMessage,
} from 'naive-ui'
import { DeviceDesktop } from '@vicons/tabler'
import { useDesktop } from '@/composables/useDesktop'
import { useTabsStore } from '@/stores/tabs'
import type { TabItem, DesktopConfig } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const tabs = useTabsStore()
const message = useMessage()
/** 仅 dev 环境显示帧数 HUD（import.meta 不能写在模板表达式里） */
const isDev = import.meta.env.DEV
const { canvasRef, error, frameCount, connect, disconnect } = useDesktop()

/** tab 已带 sessionId 则不显示快速表单 */
const showForm = ref(!props.tab.sessionId)
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
    message.error(String(e))
  }
}

onUnmounted(() => {
  disconnect()
})
</script>

<template>
  <div class="desktop-view">
    <div v-if="showForm" class="connect-form">
      <h3 class="form-title">
        <NIcon :component="DeviceDesktop" />
        远程桌面（{{ config.kind === 'rdp' ? 'RDP' : 'macOS' }}）
      </h3>
      <NForm label-placement="top" size="small" class="quick-form">
        <NFormItem label="主机">
          <NInput v-model:value="config.host" placeholder="192.168.1.10" />
        </NFormItem>
        <NFormItem label="端口">
          <NInputNumber v-model:value="config.port" :min="1" :max="65535" style="width: 100%" />
        </NFormItem>
        <NFormItem label="用户名">
          <NInput v-model:value="config.username" />
        </NFormItem>
        <NFormItem label="密码">
          <NInput v-model:value="config.password" type="password" show-password-on="click" />
        </NFormItem>
        <div v-if="error" class="error">{{ error }}</div>
        <NButton type="primary" @click="handleConnect">连接</NButton>
      </NForm>
    </div>
    <div v-else class="canvas-wrap">
      <canvas ref="canvasRef" class="desktop-canvas"></canvas>
      <NTag
        v-if="isDev"
        class="fps-hud"
        size="small"
        type="info"
        round
        :bordered="false"
      >
        {{ frameCount }} 帧
      </NTag>
    </div>
  </div>
</template>

<style scoped>
.desktop-view {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  background: #0d1117;
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
.fps-hud {
  position: absolute;
  top: 8px;
  right: 8px;
  font-family: ui-monospace, monospace;
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
