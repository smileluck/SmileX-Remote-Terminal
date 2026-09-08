<script setup lang="ts">
/**
 * DockerPanel - Docker 管理面板（右栏页签）
 *
 * - 页签：容器 / 镜像（NTabs segment，骨架同 MonitorDashboard）
 * - 容器：名称 / 关联镜像 / 状态 / 端口；操作：启动、停止、重启
 * - 镜像：repo:tag / ID / 大小 / 创建时间；操作：运行（参数对话框）、删除
 * - 数据来自 docker store（静默 exec 通道轮询，面板挂载期间 10s 一轮）
 * - 变更类操作静默执行，完成后自动刷新
 */
import { ref, computed, watch, onMounted, onUnmounted } from 'vue'
import {
  NTabs,
  NTabPane,
  NButton,
  NIcon,
  NPopconfirm,
  NModal,
  NForm,
  NFormItem,
  NInput,
  NSwitch,
  NSelect,
  NEmpty,
  NSpin,
  NTooltip,
  useMessage,
} from 'naive-ui'
import {
  Refresh,
  PlayerPlay,
  PlayerStop,
  RotateClockwise,
  Trash,
  Plus,
  X,
  BrandDocker,
} from '@vicons/tabler'
import { useDockerStore } from '@/stores/docker'
import { useTabsStore } from '@/stores/tabs'
import { segmentTabThemeOverrides } from '@/components/common/segmentTabTheme'
import type { DockerContainer, DockerImage, RunContainerOptions } from '@/types/docker'

const docker = useDockerStore()
const tabs = useTabsStore()
const message = useMessage()

onMounted(() => docker.startPolling())
onUnmounted(() => docker.stopPolling())

/** 切换活跃会话时立即刷新（不等下一轮轮询） */
watch(
  () => tabs.activeTab?.sessionId,
  () => void docker.refresh(),
)

/** 当前页签 */
const activeTab = ref<'containers' | 'images'>('containers')

/** 压缩 segment 页签（共享主题变量，见 common/segmentTabTheme） */
const tabsThemeOverrides = segmentTabThemeOverrides

/** 环境错误提示文案 */
const errorText = computed(() => {
  switch (docker.errorKind) {
    case 'not-installed':
      return '远端主机未安装 Docker'
    case 'no-permission':
      return '当前用户无权限访问 Docker（可将用户加入 docker 用户组，或使用有权限的账号连接）'
    case 'daemon-down':
      return 'Docker 守护进程未运行（可尝试 systemctl start docker）'
    case 'unknown':
      return docker.errorMessage || 'Docker 查询失败'
    default:
      return ''
  }
})

/* ---------------- 容器操作 ---------------- */

function isRunning(c: DockerContainer) {
  return c.state === 'running'
}

async function onAction(fn: () => Promise<unknown>, ok: string) {
  try {
    await fn()
    message.success(ok)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/* ---------------- 运行对话框 ---------------- */

const showRun = ref(false)
const runImage = ref('')
const runForm = ref<RunContainerOptions>({
  name: '',
  ports: [],
  env: [],
  detach: true,
  restart: '',
})

const restartOptions = [
  { label: '不设置', value: '' },
  { label: 'always', value: 'always' },
  { label: 'unless-stopped', value: 'unless-stopped' },
  { label: 'on-failure', value: 'on-failure' },
]

function openRun(img: DockerImage) {
  runImage.value = `${img.repository}:${img.tag}`
  runForm.value = { name: '', ports: [], env: [], detach: true, restart: '' }
  showRun.value = true
}

async function submitRun() {
  try {
    await docker.runContainer(runImage.value, runForm.value)
    message.success(`容器已从镜像 ${runImage.value} 启动`)
    showRun.value = false
    activeTab.value = 'containers'
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}
</script>

<template>
  <div class="docker-panel">
    <div class="toolbar">
      <NTabs
        v-model:value="activeTab"
        type="segment"
        size="small"
        class="tabs"
        :theme-overrides="tabsThemeOverrides"
      >
        <NTabPane name="containers" :tab="`容器（${docker.containers.length}）`" />
        <NTabPane name="images" :tab="`镜像（${docker.images.length}）`" />
      </NTabs>
      <NTooltip>
        <template #trigger>
          <NButton quaternary circle size="small" :loading="docker.loading" @click="docker.refresh()">
            <NIcon :component="Refresh" :size="14" />
          </NButton>
        </template>
        刷新
      </NTooltip>
    </div>

    <NSpin :show="docker.loading" size="small">
      <div class="panel-content">
        <!-- 无活跃会话 -->
        <NEmpty v-if="!docker.hasSession" description="连接 SSH 会话后管理远端 Docker" class="empty" />

        <!-- Docker 环境异常 -->
        <div v-else-if="docker.errorKind !== 'none'" class="error-state">
          <NIcon :component="BrandDocker" :size="32" class="error-icon" />
          <p class="error-text">{{ errorText }}</p>
        </div>

        <!-- 容器列表 -->
        <template v-else-if="activeTab === 'containers'">
          <NEmpty v-if="!docker.containers.length" description="暂无容器" class="empty" />
          <div v-for="c in docker.containers" :key="c.id" class="item">
            <div class="item-main">
              <div class="item-title">
                <span class="dot" :class="{ running: isRunning(c) }" />
                <span class="name" :title="c.name">{{ c.name }}</span>
                <span class="status" :class="{ running: isRunning(c) }">{{ c.status }}</span>
              </div>
              <div class="item-sub">
                <span :title="c.image">镜像：{{ c.image }}</span>
              </div>
              <div v-if="c.ports" class="item-sub">
                <span :title="c.ports">端口：{{ c.ports }}</span>
              </div>
            </div>
            <div class="item-actions">
              <template v-if="isRunning(c)">
                <NPopconfirm @positive-click="onAction(() => docker.stopContainer(c.id), `已停止 ${c.name}`)">
                  <template #trigger>
                    <NButton quaternary circle size="tiny" title="停止" :disabled="docker.acting">
                      <NIcon :component="PlayerStop" :size="14" />
                    </NButton>
                  </template>
                  确认停止容器 {{ c.name }}？
                </NPopconfirm>
                <NTooltip>
                  <template #trigger>
                    <NButton
                      quaternary
                      circle
                      size="tiny"
                      :disabled="docker.acting"
                      @click="onAction(() => docker.restartContainer(c.id), `已重启 ${c.name}`)"
                    >
                      <NIcon :component="RotateClockwise" :size="14" />
                    </NButton>
                  </template>
                  重启
                </NTooltip>
              </template>
              <NTooltip v-else>
                <template #trigger>
                  <NButton
                    quaternary
                    circle
                    size="tiny"
                    :disabled="docker.acting"
                    @click="onAction(() => docker.startContainer(c.id), `已启动 ${c.name}`)"
                  >
                    <NIcon :component="PlayerPlay" :size="14" />
                  </NButton>
                </template>
                启动
              </NTooltip>
            </div>
          </div>
        </template>

        <!-- 镜像列表 -->
        <template v-else>
          <NEmpty v-if="!docker.images.length" description="暂无镜像" class="empty" />
          <div v-for="img in docker.images" :key="img.id + img.repository + img.tag" class="item">
            <div class="item-main">
              <div class="item-title">
                <span class="name" :title="`${img.repository}:${img.tag}`">
                  {{ img.repository }}:{{ img.tag }}
                </span>
              </div>
              <div class="item-sub">
                <span>{{ img.id }} · {{ img.size }} · {{ img.createdSince }}</span>
              </div>
            </div>
            <div class="item-actions">
              <NTooltip>
                <template #trigger>
                  <NButton quaternary circle size="tiny" title="运行" :disabled="docker.acting" @click="openRun(img)">
                    <NIcon :component="PlayerPlay" :size="14" />
                  </NButton>
                </template>
                运行容器
              </NTooltip>
              <NPopconfirm
                @positive-click="onAction(() => docker.removeImage(img.id), `已删除镜像 ${img.repository}:${img.tag}`)"
              >
                <template #trigger>
                  <NButton quaternary circle size="tiny" title="删除" :disabled="docker.acting">
                    <NIcon :component="Trash" :size="14" />
                  </NButton>
                </template>
                确认删除镜像 {{ img.repository }}:{{ img.tag }}？
              </NPopconfirm>
            </div>
          </div>
        </template>
      </div>
    </NSpin>

    <!-- 从镜像运行容器对话框 -->
    <NModal
      v-model:show="showRun"
      preset="card"
      title="运行容器"
      style="width: 460px"
      :bordered="false"
    >
      <div class="run-image">镜像：{{ runImage }}</div>
      <NForm label-placement="top" size="small">
        <NFormItem label="容器名（留空自动生成）">
          <NInput v-model:value="runForm.name" placeholder="例如 my-app" />
        </NFormItem>
        <NFormItem label="端口映射（主机端口 : 容器端口）">
          <div class="kv-list">
            <div v-for="(p, i) in runForm.ports" :key="i" class="kv-row">
              <NInput v-model:value="p.host" placeholder="主机端口" />
              <span class="kv-sep">:</span>
              <NInput v-model:value="p.container" placeholder="容器端口" />
              <NButton quaternary circle size="tiny" @click="runForm.ports.splice(i, 1)">
                <NIcon :component="X" :size="14" />
              </NButton>
            </div>
            <NButton size="tiny" dashed @click="runForm.ports.push({ host: '', container: '' })">
              <template #icon><NIcon :component="Plus" /></template>
              添加映射
            </NButton>
          </div>
        </NFormItem>
        <NFormItem label="环境变量（KEY = VALUE）">
          <div class="kv-list">
            <div v-for="(e, i) in runForm.env" :key="i" class="kv-row">
              <NInput v-model:value="e.key" placeholder="KEY" />
              <span class="kv-sep">=</span>
              <NInput v-model:value="e.value" placeholder="VALUE" />
              <NButton quaternary circle size="tiny" @click="runForm.env.splice(i, 1)">
                <NIcon :component="X" :size="14" />
              </NButton>
            </div>
            <NButton size="tiny" dashed @click="runForm.env.push({ key: '', value: '' })">
              <template #icon><NIcon :component="Plus" /></template>
              添加变量
            </NButton>
          </div>
        </NFormItem>
        <div class="run-inline">
          <NFormItem label="后台运行（-d）" class="inline-item">
            <NSwitch v-model:value="runForm.detach" />
          </NFormItem>
          <NFormItem label="重启策略" class="inline-item">
            <NSelect v-model:value="runForm.restart" :options="restartOptions" />
          </NFormItem>
        </div>
      </NForm>
      <template #footer>
        <div class="dialog-footer">
          <NButton size="small" @click="showRun = false">取消</NButton>
          <NButton size="small" type="primary" :loading="docker.acting" @click="submitRun">运行</NButton>
        </div>
      </template>
    </NModal>
  </div>
</template>

<style scoped>
.docker-panel {
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
.tabs {
  flex: 1;
  min-width: 0;
}
/* 压缩 segment 页签轨道，与刷新按钮高度对齐 */
.tabs :deep(.n-tabs-rail) {
  padding: 2px;
}
/* 面板内容在 NTabs 外自渲染：隐藏空 pane 区域（其 8px 上 padding 会把页签轨顶偏） */
.tabs :deep(.n-tabs-pane-wrapper),
.tabs :deep(.n-tab-pane) {
  display: none;
}
.panel-content {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-height: 120px;
}
.empty {
  padding: 32px 0;
}
.error-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 10px;
  padding: 32px 16px;
  text-align: center;
}
.error-icon {
  color: var(--text-tertiary);
}
.error-text {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0;
}
.item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  background: var(--bg-elevated);
}
.item-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.item-title {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
}
.dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-tertiary);
  flex-shrink: 0;
}
.dot.running {
  background: #18a058;
}
.name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.status {
  font-size: 11px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.status.running {
  color: #18a058;
}
.item-sub {
  font-size: 11px;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.item-actions {
  display: flex;
  align-items: center;
  flex-shrink: 0;
}
.run-image {
  font-size: 12px;
  color: var(--text-secondary);
  margin-bottom: 10px;
  word-break: break-all;
}
.kv-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}
.kv-row {
  display: flex;
  align-items: center;
  gap: 6px;
}
.kv-sep {
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.run-inline {
  display: flex;
  gap: 16px;
}
.inline-item {
  flex: 1;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
