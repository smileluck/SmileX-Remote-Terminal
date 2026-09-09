<script setup lang="ts">
/**
 * CrontabPanel - 定时任务面板（右栏页签）
 *
 * 管理远端当前用户的 crontab：
 * - 列表：执行周期 / 命令 / 启用开关；操作：编辑、删除
 * - 新增/编辑走弹窗（周期 = 5 段表达式或 @reboot 等关键字）
 * - 不轮询：挂载 / 切换会话 / 操作完成后刷新，另有手动刷新按钮
 * - 停用 = 行首标记注释（见 services/crontab DISABLE_MARK），可逆
 */
import { ref, watch, onMounted } from 'vue'
import {
  NButton,
  NIcon,
  NPopconfirm,
  NModal,
  NForm,
  NFormItem,
  NInput,
  NSwitch,
  NEmpty,
  NSpin,
  NTooltip,
  useMessage,
} from 'naive-ui'
import { Refresh, Plus, Edit, Trash, CalendarTime } from '@vicons/tabler'
import { useCrontabStore } from '@/stores/crontab'
import { useTabsStore } from '@/stores/tabs'
import type { CronLine } from '@/types/crontab'

const crontab = useCrontabStore()
const tabs = useTabsStore()
const message = useMessage()

onMounted(() => void crontab.refresh())

/** 切换活跃会话时立即刷新 */
watch(
  () => tabs.activeTab?.sessionId,
  () => void crontab.refresh(),
)

/** 环境错误提示文案 */
const errorText = () => {
  switch (crontab.errorKind) {
    case 'not-installed':
      return '远端主机未安装 cron（crontab 命令不存在）'
    case 'no-permission':
      return '当前用户无权限使用 crontab（可能被 cron.deny 限制）'
    case 'unknown':
      return crontab.errorMessage || 'crontab 查询失败'
    default:
      return ''
  }
}

async function onAction(fn: () => Promise<unknown>, ok: string) {
  try {
    await fn()
    message.success(ok)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/* ---------------- 新增 / 编辑弹窗 ---------------- */

const showForm = ref(false)
/** 编辑目标行下标；-1 = 新增 */
const editingIndex = ref(-1)
const formSchedule = ref('')
const formCommand = ref('')

function openCreate() {
  editingIndex.value = -1
  formSchedule.value = ''
  formCommand.value = ''
  showForm.value = true
}

function openEdit(job: CronLine) {
  editingIndex.value = job.index
  formSchedule.value = job.schedule ?? ''
  formCommand.value = job.command ?? ''
  showForm.value = true
}

/** 周期校验：5 段表达式或 @关键字 */
function validSchedule(s: string): boolean {
  return /^@\w+$/.test(s) || /^\S+(\s+\S+){4}$/.test(s)
}

async function submitForm() {
  const schedule = formSchedule.value.trim()
  const command = formCommand.value.trim()
  if (!validSchedule(schedule)) {
    message.warning('执行周期格式不正确：5 段表达式（分 时 日 月 周）或 @reboot 等关键字')
    return
  }
  if (!command) {
    message.warning('命令不能为空')
    return
  }
  try {
    if (editingIndex.value < 0) {
      await crontab.addJob(schedule, command)
      message.success('已新增定时任务')
    } else {
      await crontab.editJob(editingIndex.value, schedule, command)
      message.success('已更新定时任务')
    }
    showForm.value = false
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}
</script>

<template>
  <div class="crontab-panel">
    <div class="toolbar">
      <span class="summary">共 {{ crontab.jobs.length }} 项任务</span>
      <NTooltip>
        <template #trigger>
          <NButton quaternary circle size="small" :loading="crontab.loading" @click="crontab.refresh()">
            <NIcon :component="Refresh" :size="14" />
          </NButton>
        </template>
        刷新
      </NTooltip>
      <NButton size="small" type="primary" :disabled="!crontab.hasSession" @click="openCreate">
        <template #icon><NIcon :component="Plus" :size="14" /></template>
        新增任务
      </NButton>
    </div>

    <NSpin :show="crontab.loading" size="small">
      <div class="panel-content">
        <!-- 无活跃会话 -->
        <NEmpty v-if="!crontab.hasSession" description="连接 SSH 会话后管理定时任务" class="empty" />

        <!-- cron 环境异常 -->
        <div v-else-if="crontab.errorKind !== 'none'" class="error-state">
          <NIcon :component="CalendarTime" :size="32" class="error-icon" />
          <p class="error-text">{{ errorText() }}</p>
        </div>

        <!-- 任务列表 -->
        <template v-else>
          <NEmpty v-if="!crontab.jobs.length" description="暂无定时任务" class="empty" />
          <div v-for="job in crontab.jobs" :key="job.index" class="item" :class="{ disabled: !job.enabled }">
            <div class="item-main">
              <div class="item-title">
                <span class="schedule" :title="job.schedule">{{ job.schedule }}</span>
              </div>
              <div class="item-sub">
                <span :title="job.command">{{ job.command }}</span>
              </div>
            </div>
            <div class="item-actions">
              <NTooltip>
                <template #trigger>
                  <NSwitch
                    :value="job.enabled"
                    size="small"
                    :disabled="crontab.acting"
                    @update:value="onAction(() => crontab.toggleJob(job.index), job.enabled ? '已停用' : '已启用')"
                  />
                </template>
                {{ job.enabled ? '停用' : '启用' }}
              </NTooltip>
              <NTooltip>
                <template #trigger>
                  <NButton quaternary circle size="tiny" :disabled="crontab.acting" @click="openEdit(job)">
                    <NIcon :component="Edit" :size="14" />
                  </NButton>
                </template>
                编辑
              </NTooltip>
              <NPopconfirm
                @positive-click="onAction(() => crontab.removeJob(job.index), '已删除定时任务')"
              >
                <template #trigger>
                  <NButton quaternary circle size="tiny" title="删除" :disabled="crontab.acting">
                    <NIcon :component="Trash" :size="14" />
                  </NButton>
                </template>
                确认删除任务 {{ job.schedule }} {{ job.command }}？
              </NPopconfirm>
            </div>
          </div>
        </template>
      </div>
    </NSpin>

    <!-- 新增 / 编辑弹窗 -->
    <NModal
      v-model:show="showForm"
      preset="card"
      :title="editingIndex < 0 ? '新增定时任务' : '编辑定时任务'"
      style="width: 420px"
      :bordered="false"
    >
      <NForm label-placement="top" size="small">
        <NFormItem label="执行周期">
          <NInput v-model:value="formSchedule" placeholder="* * * * * 或 @reboot" />
          <template #feedback>
            <span class="hint">分 时 日 月 周（如 0 3 * * * = 每天 03:00）；@reboot / @daily 等关键字亦可</span>
          </template>
        </NFormItem>
        <NFormItem label="命令">
          <NInput
            v-model:value="formCommand"
            type="textarea"
            :autosize="{ minRows: 2, maxRows: 6 }"
            placeholder="要执行的 shell 命令"
          />
        </NFormItem>
      </NForm>
      <template #footer>
        <div class="dialog-footer">
          <NButton size="small" @click="showForm = false">取消</NButton>
          <NButton size="small" type="primary" :loading="crontab.acting" @click="submitForm">
            {{ editingIndex < 0 ? '新增' : '保存' }}
          </NButton>
        </div>
      </template>
    </NModal>
  </div>
</template>

<style scoped>
.crontab-panel {
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
.summary {
  flex: 1;
  min-width: 0;
  font-size: 12px;
  color: var(--text-secondary);
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
.item.disabled .item-main {
  opacity: 0.5;
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
.schedule {
  font-family: monospace;
  font-size: 11px;
  color: var(--primary);
  background: color-mix(in srgb, var(--primary) 12%, transparent);
  border-radius: 4px;
  padding: 1px 6px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
  gap: 2px;
  flex-shrink: 0;
}
.hint {
  font-size: 11px;
  color: var(--text-tertiary);
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
