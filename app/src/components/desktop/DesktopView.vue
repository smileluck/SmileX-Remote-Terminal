<script setup lang="ts">
/**
 * DesktopView - 远程桌面视图
 *
 * 承载 Canvas 渲染 + 连接配置表单（回退）。
 * 新建连接统一走 DesktopConnectDialog 弹窗：tab 携带 desktopConfig，
 * 挂载时自动发起连接；失败回退到内嵌表单（回填配置可重试）。
 */
import { ref, onMounted, onUnmounted } from 'vue'
import {
  NButton,
  NInput,
  NInputNumber,
  NForm,
  NFormItem,
  NTag,
  NIcon,
  NSpin,
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

/** tab 已带 sessionId 或将自动连接（携带 desktopConfig）则不显示快速表单 */
const showForm = ref(!props.tab.sessionId && !props.tab.desktopConfig)
/** 弹窗新建后的自动连接进行中 */
const connecting = ref(false)
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

/** 弹窗新建的 tab：挂载后用携带的配置自动连接 */
onMounted(async () => {
  const cfg = props.tab.desktopConfig
  if (!cfg || props.tab.sessionId) return
  connecting.value = true
  try {
    await connect(cfg)
    tabs.updateTab(props.tab.id, { title: cfg.host, sessionId: 'connected' })
  } catch (e) {
    // 回退到内嵌表单：回填配置便于修改重试
    Object.assign(config.value, cfg)
    showForm.value = true
    tabs.updateTab(props.tab.id, { error: String(e) })
    message.error(String(e))
  } finally {
    connecting.value = false
  }
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
    <div v-if="connecting" class="auto-connecting">
      <NSpin size="large" />
      <p class="connecting-text">正在连接 {{ tab.desktopConfig?.host }}…</p>
    </div>
    <div v-else-if="showForm" class="connect-form">
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
.auto-connecting {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
}
.connecting-text {
  font-size: 13px;
  color: var(--text-secondary);
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
