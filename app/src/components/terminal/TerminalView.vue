<script setup lang="ts">
/**
 * TerminalView - SSH 终端视图（支持分屏）
 *
 * - tab 未带 sessionId（从 ActivityRail 新建）：显示快速连接表单
 * - tab 已带 sessionId：分屏容器渲染 PaneTerminal（≤4 窗格，水平/垂直分割）
 * - 会话意外断开（disconnected）：显示断开遮罩 + 重新连接按钮
 * - 右上角工具按钮：SFTP 文件面板 / 向右分屏 / 向下分屏
 */
import { ref, computed, onUnmounted, watch } from 'vue'
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
import {
  Terminal2,
  Refresh,
  Folder,
  ArrowsSplit2,
  LayoutRows,
} from '@vicons/tabler'
import * as sessionService from '@/services/session'
import { connectProfile } from '@/composables/useSshConnect'
import { useTabsStore } from '@/stores/tabs'
import { useProfilesStore } from '@/stores/profiles'
import { useMonitorStore } from '@/stores/monitor'
import FilePanel from '@/components/sftp/FilePanel.vue'
import PaneTerminal from './PaneTerminal.vue'
import type { TabItem, SshConfig } from '@/types/session'

const props = defineProps<{ tab: TabItem }>()
const tabs = useTabsStore()
const profiles = useProfilesStore()
const monitor = useMonitorStore()
const message = useMessage()

const showForm = ref(!props.tab.sessionId)
/** SFTP 文件面板开关 */
const showFiles = ref(false)
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

/* ---------------- 分屏状态 ---------------- */
interface Pane {
  id: string
  sessionId: string | null
}
const panes = ref<Pane[]>([{ id: 'p0', sessionId: props.tab.sessionId ?? null }])
/** 各 pane 的 flex-grow 比例 */
const ratios = ref<number[]>([1])
/** 分割方向：row（水平并排）/ column（垂直堆叠） */
const splitDir = ref<'row' | 'column'>('row')

watch(
  () => props.tab.sessionId,
  (sid) => {
    // 重连成功：第一个 pane 绑定新会话
    if (sid && panes.value[0]) panes.value[0].sessionId = sid
  },
)

function addPane(dir: 'row' | 'column') {
  if (panes.value.length >= 4) return
  splitDir.value = dir
  panes.value.push({ id: `p-${Date.now()}`, sessionId: null })
  ratios.value.push(1)
}

function closePane(index: number) {
  const pane = panes.value[index]
  // pane 独立建立的会话（非 tab 主会话）需要断开
  if (pane.sessionId && pane.sessionId !== props.tab.sessionId) {
    monitor.stopSampling(pane.sessionId)
    sessionService.disconnect(pane.sessionId).catch(() => {})
  }
  panes.value.splice(index, 1)
  ratios.value.splice(index, 1)
}

function bindPane(index: number, sid: string) {
  panes.value[index].sessionId = sid
}

/** 分割条拖拽：调整相邻 pane 比例 */
function onDividerDown(e: MouseEvent, index: number) {
  const parent = (e.currentTarget as HTMLElement).parentElement
  if (!parent) return
  const total = splitDir.value === 'row' ? parent.clientWidth : parent.clientHeight
  const startX = splitDir.value === 'row' ? e.clientX : e.clientY
  const a = ratios.value[index]
  const b = ratios.value[index + 1]
  const onMove = (ev: MouseEvent) => {
    const delta = ((splitDir.value === 'row' ? ev.clientX - startX : ev.clientY - startX) / total) * (a + b)
    ratios.value[index] = Math.max(0.1, a + delta)
    ratios.value[index + 1] = Math.max(0.1, b - delta)
  }
  const onUp = () => {
    window.removeEventListener('mousemove', onMove)
    window.removeEventListener('mouseup', onUp)
  }
  window.addEventListener('mousemove', onMove)
  window.addEventListener('mouseup', onUp)
  e.preventDefault()
}

/** tab 卸载：清理 pane 独立建立的会话（tab 主会话由 closeTab 负责） */
onUnmounted(() => {
  for (const pane of panes.value) {
    if (pane.sessionId && pane.sessionId !== props.tab.sessionId) {
      monitor.stopSampling(pane.sessionId)
      sessionService.disconnect(pane.sessionId).catch(() => {})
    }
  }
})

/* ---------------- 快速连接（无 sessionId 时） ---------------- */
async function handleConnect() {
  showForm.value = false
  try {
    // 直接在主 pane 中快速连接
    const cols = 80
    const rows = 24
    const sid = await sessionService.connect(config.value, cols, rows)
    panes.value[0].sessionId = sid
    monitor.startSampling(sid)
    tabs.updateTab(props.tab.id, {
      title: `${config.value.host}`,
      sessionId: sid,
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
        <NButton type="primary" @click="handleConnect">连接</NButton>
      </NForm>
    </div>

    <template v-else>
      <div class="terminal-main">
        <div class="split-area" :class="splitDir">
          <template v-for="(pane, i) in panes" :key="pane.id">
            <PaneTerminal
              class="split-pane"
              :style="{ flexGrow: ratios[i], flexBasis: 0 }"
              :session-id="pane.sessionId"
              :closable="panes.length > 1"
              @bind="(sid: string) => bindPane(i, sid)"
              @close="closePane(i)"
            />
            <div
              v-if="i < panes.length - 1"
              class="split-divider"
              :class="splitDir"
              @mousedown="onDividerDown($event, i)"
            />
          </template>
        </div>
        <!-- SFTP 文件面板 -->
        <FilePanel
          v-if="showFiles && tab.sessionId && !tab.disconnected"
          :session-id="tab.sessionId"
        />
        <!-- 工具按钮 -->
        <div class="view-tools">
          <NTooltip v-if="tab.sessionId" placement="left">
            <template #trigger>
              <NButton
                quaternary
                circle
                size="small"
                :type="showFiles ? 'primary' : 'default'"
                @click="showFiles = !showFiles"
              >
                <NIcon :component="Folder" />
              </NButton>
            </template>
            文件管理（SFTP）
          </NTooltip>
          <NTooltip v-if="panes.length < 4 && tab.sessionId" placement="left">
            <template #trigger>
              <NButton quaternary circle size="small" @click="addPane('row')">
                <NIcon :component="ArrowsSplit2" style="transform: rotate(90deg)" />
              </NButton>
            </template>
            向右分屏
          </NTooltip>
          <NTooltip v-if="panes.length < 4 && tab.sessionId" placement="left">
            <template #trigger>
              <NButton quaternary circle size="small" @click="addPane('column')">
                <NIcon :component="LayoutRows" />
              </NButton>
            </template>
            向下分屏
          </NTooltip>
        </div>
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
.terminal-main {
  position: relative;
  display: flex;
  flex: 1;
  min-height: 0;
}
.split-area {
  flex: 1;
  display: flex;
  min-width: 0;
  min-height: 0;
}
.split-area.row {
  flex-direction: row;
}
.split-area.column {
  flex-direction: column;
}
.split-pane {
  min-width: 0;
  min-height: 0;
}
.split-divider {
  flex-shrink: 0;
  background: var(--border-color);
  z-index: 4;
}
.split-divider.row {
  width: 4px;
  cursor: col-resize;
  margin: 0 1px;
}
.split-divider.column {
  height: 4px;
  cursor: row-resize;
  margin: 1px 0;
}
.split-divider:hover {
  background: var(--primary);
}
.view-tools {
  position: absolute;
  right: 10px;
  top: 8px;
  z-index: 5;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.disconnect-overlay {
  position: absolute;
  inset: 0;
  background: rgba(15, 20, 25, 0.72);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10;
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
</style>
