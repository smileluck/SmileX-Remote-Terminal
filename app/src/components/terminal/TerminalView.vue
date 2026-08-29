<script setup lang="ts">
/**
 * TerminalView - SSH 终端视图
 *
 * 承载 xterm.js 实例。
 * - tab 已带 sessionId（从 SideBar 会话卡片连接进入）：初始化 xterm 后 bind 到会话
 * - tab 未带 sessionId（从 ActivityRail 新建）：显示快速连接表单
 * - 会话意外断开（disconnected）：显示断开遮罩 + 重新连接按钮
 */
import { ref, watch, nextTick, computed } from 'vue'
import { useResizeObserver } from '@vueuse/core'
import {
  NButton,
  NInput,
  NInputNumber,
  NIcon,
  NForm,
  NFormItem,
  NTooltip,
  useMessage,
} from 'naive-ui'
import { Terminal2, Refresh, Folder } from '@vicons/tabler'
import { useTerminal } from '@/composables/useTerminal'
import { connectProfile } from '@/composables/useSshConnect'
import { useTabsStore } from '@/stores/tabs'
import { useProfilesStore } from '@/stores/profiles'
import { useMonitorStore } from '@/stores/monitor'
import FilePanel from '@/components/sftp/FilePanel.vue'
import type { TabItem, SshConfig } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const tabs = useTabsStore()
const profiles = useProfilesStore()
const monitor = useMonitorStore()
const message = useMessage()
const { term, sessionId, error, init, bind, connect, fit } = useTerminal()

const containerRef = ref<HTMLDivElement | null>(null)
/** SFTP 文件面板开关 */
const showFiles = ref(false)
/** tab 已带 sessionId 则不显示快速表单 */
const showForm = ref(!props.tab.sessionId)
/** 重连中 */
const reconnecting = ref(false)
const config = ref<SshConfig>({
  host: '127.0.0.1',
  port: 22,
  username: 'root',
  auth: { type: 'password', value: '' },
})

/** 重连目标档案（tab 带 profileId 时可用） */
const profile = computed(() =>
  props.tab.profileId ? profiles.profiles.find((p) => p.id === props.tab.profileId) : null,
)

/** 容器首次挂载时初始化 xterm，并绑定已存在的会话 */
watch(
  () => containerRef.value,
  async (el) => {
    if (el && !term.value) {
      init(el)
      await nextTick()
      fit()
      // SideBar 连接的 tab：xterm 后挂载，需主动绑定
      const sid = props.tab.sessionId
      if (sid && sid !== 'connected' && !props.tab.disconnected) {
        bind(sid)
      }
    }
  },
)

/** tab 的 sessionId 变化（重连成功）时重新绑定 */
watch(
  () => props.tab.sessionId,
  (sid) => {
    if (sid && sid !== 'connected' && term.value && !props.tab.disconnected) {
      bind(sid)
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
      sessionId: sessionId.value ?? undefined,
      connecting: false,
      disconnected: false,
    })
  } catch (e) {
    tabs.updateTab(props.tab.id, { error: String(e), connecting: false })
    message.error(String(e))
    showForm.value = true
  }
}

/** 重新连接（基于档案） */
async function handleReconnect() {
  if (!profile.value) {
    message.warning('该会话无关联配置，请从左侧列表重新连接')
    return
  }
  reconnecting.value = true
  try {
    const sid = await connectProfile(profile.value)
    tabs.updateTab(props.tab.id, { sessionId: sid, disconnected: false, error: undefined })
    monitor.setActive(sid)
    message.success(`已重新连接「${profile.value.name}」`)
  } catch (e) {
    message.error(`重连失败：${e}`)
  } finally {
    reconnecting.value = false
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

    <template v-else>
      <div class="terminal-main">
        <div ref="containerRef" class="xterm-container"></div>
        <!-- SFTP 文件面板 -->
        <FilePanel
          v-if="showFiles && tab.sessionId && !tab.disconnected"
          :session-id="tab.sessionId"
        />
        <!-- 文件面板开关 -->
        <NTooltip v-if="tab.sessionId" placement="left">
          <template #trigger>
            <NButton
              quaternary
              circle
              size="small"
              class="files-toggle"
              :type="showFiles ? 'primary' : 'default'"
              @click="showFiles = !showFiles"
            >
              <NIcon :component="Folder" />
            </NButton>
          </template>
          文件管理（SFTP）
        </NTooltip>
      </div>
      <!-- 断开遮罩 -->
      <div v-if="tab.disconnected" class="disconnect-overlay">
        <div class="disconnect-card">
          <p class="disconnect-text">会话已断开</p>
          <NButton
            type="primary"
            size="small"
            :loading="reconnecting"
            @click="handleReconnect"
          >
            <template #icon><NIcon :component="Refresh" /></template>
            重新连接
          </NButton>
        </div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.terminal-view {
  position: relative;
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
  min-width: 0;
}
.terminal-main {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 0;
}
.files-toggle {
  position: absolute;
  right: 10px;
  top: 8px;
  z-index: 5;
}
.disconnect-overlay {
  position: absolute;
  inset: 0;
  background: rgba(15, 20, 25, 0.72);
  display: flex;
  align-items: center;
  justify-content: center;
}
.disconnect-card {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 20px 28px;
  background: var(--bg-panel, #161c24);
  border: 1px solid var(--border-color);
  border-radius: 8px;
}
.disconnect-text {
  margin: 0;
  font-size: 13px;
  color: var(--text-secondary);
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
