<script setup lang="ts">
/**
 * TunnelPanel - 端口转发隧道面板（右栏页签）
 *
 * 管理当前 SSH 会话的端口转发（Local 本地转发 / Remote 远端转发 /
 * Dynamic 动态 SOCKS5）：
 * - 列表：类型 tag + 监听地址 → 目标地址 + 运行状态 + 停止按钮
 *   （来自档案 extra.tunnels 自动启动的隧道带「档案」来源标注）
 * - 底部新建表单：类型选择 + 地址端口字段（按类型显隐与换标签，
 *   与 OpenSSH -L/-R/-D 语义一致）
 * 数据来自 tunnel store（会话切换 / tunnel_event 自动刷新）。
 */
import { ref, computed, watch, onMounted } from 'vue'
import {
  NButton,
  NIcon,
  NInput,
  NInputNumber,
  NSelect,
  NEmpty,
  NSpin,
  NTag,
  NTooltip,
  useMessage,
} from 'naive-ui'
import { Refresh, PlayerStop, ArrowRight } from '@vicons/tabler'
import { useTunnelStore } from '@/stores/tunnel'
import { useTabsStore } from '@/stores/tabs'
import { useProfilesStore } from '@/stores/profiles'
import { decodeExtra } from '@/types/profile'
import { emptyTunnelConfig, type TunnelConfig, type TunnelInfo, type TunnelKind } from '@/types/tunnel'

const tunnel = useTunnelStore()
const tabs = useTabsStore()
const profiles = useProfilesStore()
const message = useMessage()

onMounted(() => void tunnel.refresh())

/** 切换活跃会话时重新拉取列表 */
watch(
  () => tabs.activeTab?.sessionId,
  () => void tunnel.refresh(),
)

/** 当前主机提示 */
const hostLabel = computed(() => tabs.activeTab?.title ?? '')

/** 当前 tab 关联档案中的隧道 id 集合（标注「档案自动启动」来源用） */
const profileTunnelIds = computed<Set<string>>(() => {
  const pid = tabs.activeTab?.profileId
  const p = pid ? profiles.profiles.find((x) => x.id === pid) : null
  if (!p) return new Set()
  return new Set((decodeExtra(p.extra).tunnels ?? []).map((t) => t.id))
})

/** 类型展示 */
const KIND_LABELS: Record<TunnelKind, string> = {
  local: '本地转发',
  remote: '远端转发',
  dynamic: '动态 SOCKS5',
}
const KIND_TAG_TYPES: Record<TunnelKind, 'info' | 'warning' | 'success'> = {
  local: 'info',
  remote: 'warning',
  dynamic: 'success',
}

/** 监听侧 → 目标侧展示文本（remote 的字段是「远端视角」，与 OpenSSH -R 一致） */
function routeText(t: TunnelInfo): { listen: string; target: string } {
  const c = t.config
  switch (c.kind) {
    case 'local':
      return { listen: `${c.local_host}:${c.local_port}`, target: `${c.remote_host}:${c.remote_port}` }
    case 'remote':
      return { listen: `${c.remote_host}:${c.remote_port}`, target: `${c.local_host}:${c.local_port}` }
    case 'dynamic':
      return { listen: `${c.local_host}:${c.local_port}`, target: '按请求目标' }
  }
}

/** 状态 tag */
function stateTag(t: TunnelInfo): { label: string; type: 'success' | 'default' | 'error' } {
  switch (t.state) {
    case 'running':
      return { label: '运行中', type: 'success' }
    case 'failed':
      return { label: '已失败', type: 'error' }
    default:
      return { label: '已停止', type: 'default' }
  }
}

async function onStop(t: TunnelInfo) {
  try {
    await tunnel.stop(t.id)
    message.success('隧道已停止')
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/* ---------------- 新建表单 ---------------- */

const form = ref<TunnelConfig>(emptyTunnelConfig('local'))

const kindOptions = [
  { label: '本地转发（本地监听 → 远端目标）', value: 'local' },
  { label: '远端转发（远端监听 → 本地目标）', value: 'remote' },
  { label: '动态 SOCKS5（本地代理）', value: 'dynamic' },
]

/** 切换类型时重置为对应默认配置（端口默认值不同） */
function onKindChange(kind: TunnelKind) {
  form.value = emptyTunnelConfig(kind)
}

/** 表单字段标签按类型区分（字段含义不同，见 types/tunnel.ts 注释） */
const fieldLabels = computed(() => {
  switch (form.value.kind) {
    case 'local':
      return { listenHost: '本地监听地址', listenPort: '本地监听端口', targetHost: '远端目标地址', targetPort: '远端目标端口' }
    case 'remote':
      return { listenHost: '远端监听地址', listenPort: '远端监听端口', targetHost: '本地目标地址', targetPort: '本地目标端口' }
    case 'dynamic':
      return { listenHost: '本地监听地址', listenPort: '本地监听端口', targetHost: '', targetPort: '' }
  }
})

/** local/dynamic 的监听侧是 local_*；remote 的监听侧是 remote_*（后端约定） */
const isRemote = computed(() => form.value.kind === 'remote')
const listenHostModel = computed({
  get: () => (isRemote.value ? form.value.remote_host : form.value.local_host),
  set: (v: string) => {
    if (isRemote.value) form.value.remote_host = v
    else form.value.local_host = v
  },
})
const listenPortModel = computed({
  get: () => (isRemote.value ? form.value.remote_port : form.value.local_port),
  set: (v: number | null) => {
    if (isRemote.value) form.value.remote_port = v ?? 0
    else form.value.local_port = v ?? 0
  },
})
const targetHostModel = computed({
  get: () => (isRemote.value ? form.value.local_host : form.value.remote_host),
  set: (v: string) => {
    if (isRemote.value) form.value.local_host = v
    else form.value.remote_host = v
  },
})
const targetPortModel = computed({
  get: () => (isRemote.value ? form.value.local_port : form.value.remote_port),
  set: (v: number | null) => {
    if (isRemote.value) form.value.local_port = v ?? 0
    else form.value.remote_port = v ?? 0
  },
})

async function onCreate() {
  const c = form.value
  if (c.local_port < 0 || c.local_port > 65535 || c.remote_port < 0 || c.remote_port > 65535) {
    message.warning('端口必须在 0-65535')
    return
  }
  if (c.kind !== 'dynamic' && c.local_port === 0) {
    message.warning('本地端口不能为 0')
    return
  }
  try {
    await tunnel.start({ ...c })
    message.success('隧道已启动')
    form.value = emptyTunnelConfig(c.kind)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}
</script>

<template>
  <div class="tunnel-panel">
    <div class="toolbar">
      <span class="host" :title="hostLabel">{{ hostLabel }}</span>
      <NTooltip>
        <template #trigger>
          <NButton quaternary circle size="small" :loading="tunnel.loading" @click="tunnel.refresh()">
            <NIcon :component="Refresh" :size="14" />
          </NButton>
        </template>
        刷新
      </NTooltip>
    </div>

    <NSpin :show="tunnel.loading" size="small">
      <div class="panel-content">
        <!-- 无活跃会话 -->
        <NEmpty v-if="!tunnel.hasSession" description="连接 SSH 会话后管理端口转发" class="empty" />

        <template v-else>
          <NEmpty v-if="!tunnel.tunnels.length" description="暂无运行中的隧道" class="empty" />
          <!-- 隧道卡片 -->
          <div v-for="t in tunnel.tunnels" :key="t.id" class="card">
            <div class="card-head">
              <NTag size="tiny" :type="KIND_TAG_TYPES[t.config.kind]" :bordered="false">
                {{ KIND_LABELS[t.config.kind] }}
              </NTag>
              <NTag v-if="profileTunnelIds.has(t.id)" size="tiny" :bordered="false">档案</NTag>
              <NTag size="tiny" :type="stateTag(t).type" :bordered="false" class="state-tag">
                {{ stateTag(t).label }}
              </NTag>
              <NTooltip>
                <template #trigger>
                  <NButton
                    quaternary
                    circle
                    size="tiny"
                    :loading="tunnel.operating.has(t.id)"
                    @click="onStop(t)"
                  >
                    <NIcon :component="PlayerStop" :size="14" />
                  </NButton>
                </template>
                停止
              </NTooltip>
            </div>
            <div class="card-route">
              <span class="route-addr" :title="`监听：${routeText(t).listen}`">{{ routeText(t).listen }}</span>
              <NIcon :component="ArrowRight" :size="12" class="route-arrow" />
              <span class="route-addr" :title="`目标：${routeText(t).target}`">{{ routeText(t).target }}</span>
            </div>
            <div v-if="t.state === 'failed' && t.error" class="card-error">{{ t.error }}</div>
          </div>

          <!-- 新建隧道表单 -->
          <div class="create-form">
            <div class="create-title">新建隧道</div>
            <NSelect
              :value="form.kind"
              :options="kindOptions"
              size="small"
              @update:value="onKindChange"
            />
            <div class="form-row">
              <NInput v-model:value="listenHostModel" size="small" :placeholder="fieldLabels.listenHost" class="host-input" />
              <NInputNumber v-model:value="listenPortModel" size="small" :min="0" :max="65535" :placeholder="fieldLabels.listenPort" class="port-input" />
            </div>
            <template v-if="form.kind !== 'dynamic'">
              <div class="form-arrow">
                <NIcon :component="ArrowRight" :size="12" />
              </div>
              <div class="form-row">
                <NInput v-model:value="targetHostModel" size="small" :placeholder="fieldLabels.targetHost" class="host-input" />
                <NInputNumber v-model:value="targetPortModel" size="small" :min="0" :max="65535" :placeholder="fieldLabels.targetPort" class="port-input" />
              </div>
            </template>
            <NButton
              size="small"
              type="primary"
              block
              :loading="tunnel.operating.has('new')"
              @click="onCreate"
            >
              启动隧道
            </NButton>
          </div>
        </template>
      </div>
    </NSpin>
  </div>
</template>

<style scoped>
.tunnel-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  height: 100%;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 6px;
}
.host {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.panel-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 120px;
}
.empty {
  padding: 32px 0;
}
.card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-elevated);
}
.card-head {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.state-tag {
  margin-left: auto;
}
.card-route {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.route-addr {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.route-arrow {
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.card-error {
  font-size: 11px;
  color: var(--danger);
  word-break: break-all;
}
.create-form {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 10px;
  border: 1px dashed var(--border-color);
  border-radius: 6px;
}
.create-title {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
}
.form-row {
  display: flex;
  gap: 6px;
}
.host-input {
  flex: 1;
  min-width: 0;
}
.port-input {
  width: 110px;
  flex-shrink: 0;
}
.form-arrow {
  display: flex;
  justify-content: center;
  color: var(--text-tertiary);
}
</style>
