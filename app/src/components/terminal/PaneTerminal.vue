<script setup lang="ts">
/**
 * PaneTerminal - 分屏中的单个终端窗格
 *
 * - 已绑定 sessionId：渲染 xterm 并绑定输出
 * - 未绑定：显示「选择会话 / 快速连接」选择器
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
  NSelect,
} from 'naive-ui'
import { Terminal2, X } from '@vicons/tabler'
import { useTerminal } from '@/composables/useTerminal'
import { useTabsStore } from '@/stores/tabs'
import { useMonitorStore } from '@/stores/monitor'
import type { SshConfig } from '@/types/session'

const props = defineProps<{ sessionId: string | null; closable: boolean }>()
const emit = defineEmits<{
  (e: 'bind', sid: string): void
  (e: 'close'): void
}>()

const tabs = useTabsStore()
const monitor = useMonitorStore()
const { term, sessionId: ownSession, init, bind, connect, fit } = useTerminal()

const containerRef = ref<HTMLDivElement | null>(null)

/** 挂载时初始化 xterm 并绑定（tab 已带会话） */
watch(
  () => containerRef.value,
  async (el) => {
    if (el && !term.value) {
      init(el)
      await nextTick()
      fit()
      if (props.sessionId && props.sessionId !== ownSession.value) {
        bind(props.sessionId)
      }
    }
  },
)

/** 外部 sessionId 变化（pane 重绑定） */
watch(
  () => props.sessionId,
  (sid) => {
    if (sid && sid !== ownSession.value && term.value) bind(sid)
  },
)

useResizeObserver(containerRef, () => {
  if (term.value) fit()
})

/** 其他 tab 的活跃 SSH 会话（供 pane 绑定） */
const sessionOptions = computed(() =>
  tabs.tabs
    .filter((t) => t.kind === 'ssh' && t.sessionId && !t.disconnected)
    .map((t) => ({ label: `${t.title}（${(t.sessionId as string).slice(0, 8)}）`, value: t.sessionId as string })),
)

const pickedSession = ref<string | null>(null)

/** 快速连接（pane 内独立建立新会话） */
const showQuick = ref(false)
const connecting = ref(false)
const config = ref<SshConfig>({
  host: '127.0.0.1',
  port: 22,
  username: 'root',
  auth: { type: 'password', value: '' },
})

async function quickConnect() {
  connecting.value = true
  try {
    await connect(config.value)
    if (ownSession.value) emit('bind', ownSession.value)
    monitor.startSampling(ownSession.value!)
  } finally {
    connecting.value = false
  }
}
</script>

<template>
  <div class="pane">
    <div class="pane-head">
      <span class="pane-title">
        <NIcon :component="Terminal2" :size="12" />
        {{ sessionId ? '会话 ' + sessionId.slice(0, 8) : '未绑定' }}
      </span>
      <NButton v-if="closable" quaternary circle size="tiny" @click="emit('close')">
        <NIcon :component="X" :size="12" />
      </NButton>
    </div>

    <div v-if="sessionId" ref="containerRef" class="pane-term"></div>

    <div v-else class="pane-picker">
      <template v-if="sessionOptions.length">
        <p class="picker-label">绑定现有会话</p>
        <NSelect
          v-model:value="pickedSession"
          size="small"
          :options="sessionOptions"
          placeholder="选择活跃 SSH 会话"
          @update:value="(v: string) => emit('bind', v)"
        />
      </template>
      <template v-else>
        <p class="picker-label">暂无其他活跃会话</p>
      </template>

      <template v-if="!showQuick">
        <NButton size="small" dashed class="picker-btn" @click="showQuick = true">
          快速连接新主机
        </NButton>
      </template>
      <NForm v-else label-placement="top" size="small" class="picker-form">
        <NFormItem label="主机">
          <NInput v-model:value="config.host" placeholder="192.168.1.10" />
        </NFormItem>
        <div class="picker-row">
          <NFormItem label="端口" class="grow">
            <NInputNumber v-model:value="config.port" :min="1" :max="65535" style="width: 100%" />
          </NFormItem>
          <NFormItem label="用户名" class="grow">
            <NInput v-model:value="config.username" />
          </NFormItem>
        </div>
        <NFormItem label="密码">
          <NInput
            :value="config.auth.type === 'password' ? config.auth.value : ''"
            type="password"
            show-password-on="click"
            placeholder="输入密码"
            @update:value="(v: string) => (config.auth = { type: 'password', value: v })"
          />
        </NFormItem>
        <NButton type="primary" size="small" :loading="connecting" @click="quickConnect">
          连接
        </NButton>
      </NForm>
    </div>
  </div>
</template>

<style scoped>
.pane {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  overflow: hidden;
  position: relative;
}
.pane-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 8px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}
.pane-title {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.pane-term {
  flex: 1;
  min-height: 0;
  background: var(--bg-app);
  padding: 2px;
}
.pane-picker {
  flex: 1;
  overflow-y: auto;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.picker-label {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0;
}
.picker-btn {
  align-self: flex-start;
}
.picker-form {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.picker-row {
  display: flex;
  gap: 10px;
}
.grow {
  flex: 1;
}
</style>
