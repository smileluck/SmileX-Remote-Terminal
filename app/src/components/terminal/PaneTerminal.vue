<script setup lang="ts">
/**
 * PaneTerminal - 分屏中的单个终端窗格
 *
 * - 已绑定 sessionId：渲染 xterm 并绑定输出
 * - 未绑定：显示「绑定现有会话 / 新建连接」选择器
 *   （新建连接打开全局连接弹窗，连接结果会开新 tab）
 */
import { ref, watch, nextTick, computed } from 'vue'
import { useResizeObserver } from '@vueuse/core'
import { NButton, NIcon, NSelect } from 'naive-ui'
import { Terminal2, X, Plus } from '@vicons/tabler'
import { useTerminal } from '@/composables/useTerminal'
import { useTabsStore } from '@/stores/tabs'
import { useUiStore } from '@/stores/ui'

const props = defineProps<{ sessionId: string | null; closable: boolean }>()
const emit = defineEmits<{
  (e: 'bind', sid: string): void
  (e: 'close'): void
}>()

const tabs = useTabsStore()
const ui = useUiStore()
const { term, sessionId: ownSession, init, bind, fit } = useTerminal()

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
  // tab 被隐藏（v-show 切走）时容器尺寸为 0，跳过无效 fit
  if (term.value && containerRef.value?.clientWidth) fit()
})

/** 所有 tab 的活跃 SSH 会话（供 pane 绑定） */
const sessionOptions = computed(() =>
  tabs.tabs
    .filter((t) => t.kind === 'ssh' && t.sessionId && !t.disconnected)
    .map((t) => ({ label: `${t.title}（${(t.sessionId as string).slice(0, 8)}）`, value: t.sessionId as string })),
)

const pickedSession = ref<string | null>(null)
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
        <p class="picker-label">暂无活跃会话</p>
      </template>

      <NButton size="small" dashed class="picker-btn" @click="ui.openConnectDialog()">
        <template #icon><NIcon :component="Plus" :size="14" /></template>
        新建 SSH 连接
      </NButton>
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
</style>
