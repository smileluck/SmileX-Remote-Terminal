<script setup lang="ts">
/**
 * CrontabPanel - 定时任务面板（右栏页签）
 *
 * 管理远端当前用户的 crontab：
 * - 列表：执行周期 / 命令 / 启用开关；操作：编辑、删除
 * - 新增/编辑走弹窗（周期支持可视化设置：每分钟/每小时/每天/每周/每月
 *   自动生成表达式，或自定义 5 段表达式、@reboot 等关键字）
 * - 不轮询：挂载 / 切换会话 / 操作完成后刷新，另有手动刷新按钮
 * - 停用 = 行首标记注释（见 services/crontab DISABLE_MARK），可逆
 */
import { ref, computed, watch, onMounted } from 'vue'
import {
  NButton,
  NIcon,
  NPopconfirm,
  NModal,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSelect,
  NSwitch,
  NEmpty,
  NSpin,
  NTooltip,
  useMessage,
} from 'naive-ui'
import { Refresh, Plus, Edit, Trash, CalendarTime, PlayerPlay } from '@vicons/tabler'
import { useCrontabStore } from '@/stores/crontab'
import { useTabsStore } from '@/stores/tabs'
import { nextRuns } from '@/utils/cron'
import * as crontabService from '@/services/crontab'
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

/* ---------------- 立即执行测试 ---------------- */

/** 正在测试的任务行下标（-1 = 无） */
const runningIndex = ref(-1)
const showRun = ref(false)
const runCmd = ref('')
const runRc = ref(0)
const runOutput = ref('')

/** 以 cron 风格最小环境执行任务命令，弹窗展示退出码与输出 */
async function runNow(job: CronLine) {
  const t = tabs.activeTab
  const sid = t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
  if (!sid || runningIndex.value >= 0 || !job.command) return
  runningIndex.value = job.index
  try {
    const r = await crontabService.runJob(sid, job.command)
    runCmd.value = job.command
    runRc.value = r.rc
    runOutput.value = r.output
    showRun.value = true
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  } finally {
    runningIndex.value = -1
  }
}

/* ---------------- 新增 / 编辑弹窗 ---------------- */

const showForm = ref(false)
/** 编辑目标行下标；-1 = 新增 */
const editingIndex = ref(-1)
const formSchedule = ref('')
const formCommand = ref('')

/* ---------------- 周期可视化设置 ---------------- */

type ScheduleMode = 'minutely' | 'hourly' | 'daily' | 'weekly' | 'monthly' | 'custom'
const scheduleMode = ref<ScheduleMode>('daily')
const sdMinute = ref(0)
const sdHour = ref(3)
const sdDom = ref(1)
const sdDow = ref(1)

const modeOptions = [
  { label: '每分钟', value: 'minutely' },
  { label: '每小时', value: 'hourly' },
  { label: '每天', value: 'daily' },
  { label: '每周', value: 'weekly' },
  { label: '每月', value: 'monthly' },
  { label: '自定义表达式', value: 'custom' },
]

const dowOptions = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'].map((label, i) => ({
  label,
  value: i,
}))

/** 由可视化字段生成 cron 表达式 */
function buildSchedule(): string {
  switch (scheduleMode.value) {
    case 'minutely':
      return '* * * * *'
    case 'hourly':
      return `${sdMinute.value} * * * *`
    case 'daily':
      return `${sdMinute.value} ${sdHour.value} * * *`
    case 'weekly':
      return `${sdMinute.value} ${sdHour.value} * * ${sdDow.value}`
    case 'monthly':
      return `${sdMinute.value} ${sdHour.value} ${sdDom.value} * *`
    default:
      return formSchedule.value
  }
}

// 可视化字段变化 → 同步生成表达式（custom 模式保留手动输入）
watch([scheduleMode, sdMinute, sdHour, sdDom, sdDow], () => {
  if (scheduleMode.value !== 'custom') formSchedule.value = buildSchedule()
})

/** 解析既有表达式回填可视化设置（@关键字 / 无法识别的写法 → custom） */
function parseSchedule(s: string) {
  const parts = s.trim().split(/\s+/)
  const num = (v: string, min: number, max: number) =>
    /^\d+$/.test(v) && Number(v) >= min && Number(v) <= max ? Number(v) : null
  if (parts.length !== 5) {
    scheduleMode.value = 'custom'
    return
  }
  const m = num(parts[0], 0, 59)
  const h = num(parts[1], 0, 23)
  const dom = num(parts[2], 1, 31)
  let dow = num(parts[4], 0, 7)
  if (dow === 7) dow = 0
  if (parts[0] === '*' && parts[1] === '*' && parts[2] === '*' && parts[4] === '*') {
    scheduleMode.value = 'minutely'
  } else if (m !== null && parts[1] === '*' && parts[2] === '*' && parts[4] === '*') {
    scheduleMode.value = 'hourly'
    sdMinute.value = m
  } else if (m !== null && h !== null && parts[2] === '*' && parts[4] === '*') {
    scheduleMode.value = 'daily'
    sdMinute.value = m
    sdHour.value = h
  } else if (m !== null && h !== null && parts[2] === '*' && dow !== null) {
    scheduleMode.value = 'weekly'
    sdMinute.value = m
    sdHour.value = h
    sdDow.value = dow
  } else if (m !== null && h !== null && dom !== null && parts[4] === '*') {
    scheduleMode.value = 'monthly'
    sdMinute.value = m
    sdHour.value = h
    sdDom.value = dom
  } else {
    scheduleMode.value = 'custom'
  }
}

/** 自定义表达式的未来 5 次执行时间（null=非法，[]=@reboot） */
const nextRunTimes = computed(() => {
  if (scheduleMode.value !== 'custom') return null
  const s = formSchedule.value.trim()
  if (!s) return null
  return nextRuns(s, 5)
})

function fmtDateTime(d: Date): string {
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}

function openCreate() {
  editingIndex.value = -1
  scheduleMode.value = 'daily'
  sdMinute.value = 0
  sdHour.value = 3
  formSchedule.value = buildSchedule()
  formCommand.value = ''
  showForm.value = true
}

function openEdit(job: CronLine) {
  editingIndex.value = job.index
  formSchedule.value = job.schedule ?? ''
  formCommand.value = job.command ?? ''
  parseSchedule(formSchedule.value)
  if (scheduleMode.value !== 'custom') formSchedule.value = buildSchedule()
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
                  <NButton
                    quaternary
                    circle
                    size="tiny"
                    :loading="runningIndex === job.index"
                    :disabled="crontab.acting || runningIndex >= 0"
                    @click="runNow(job)"
                  >
                    <NIcon :component="PlayerPlay" :size="14" />
                  </NButton>
                </template>
                立即执行测试（模拟 cron 最小环境）
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
      style="width: 480px"
      :bordered="false"
    >
      <NForm label-placement="top" size="small">
        <NFormItem label="执行周期">
          <div class="schedule-builder">
            <NSelect v-model:value="scheduleMode" :options="modeOptions" size="small" class="schedule-mode" />
            <div v-if="scheduleMode === 'hourly'" class="schedule-fields">
              <span>每小时的第</span>
              <NInputNumber v-model:value="sdMinute" :min="0" :max="59" size="small" class="schedule-num" />
              <span>分</span>
            </div>
            <div v-else-if="scheduleMode === 'daily'" class="schedule-fields">
              <span>每天</span>
              <NInputNumber v-model:value="sdHour" :min="0" :max="23" size="small" class="schedule-num" />
              <span>时</span>
              <NInputNumber v-model:value="sdMinute" :min="0" :max="59" size="small" class="schedule-num" />
              <span>分</span>
            </div>
            <div v-else-if="scheduleMode === 'weekly'" class="schedule-fields">
              <span>每周</span>
              <NSelect v-model:value="sdDow" :options="dowOptions" size="small" class="schedule-dow" />
              <NInputNumber v-model:value="sdHour" :min="0" :max="23" size="small" class="schedule-num" />
              <span>时</span>
              <NInputNumber v-model:value="sdMinute" :min="0" :max="59" size="small" class="schedule-num" />
              <span>分</span>
            </div>
            <div v-else-if="scheduleMode === 'monthly'" class="schedule-fields">
              <span>每月</span>
              <NInputNumber v-model:value="sdDom" :min="1" :max="31" size="small" class="schedule-num" />
              <span>号</span>
              <NInputNumber v-model:value="sdHour" :min="0" :max="23" size="small" class="schedule-num" />
              <span>时</span>
              <NInputNumber v-model:value="sdMinute" :min="0" :max="59" size="small" class="schedule-num" />
              <span>分</span>
            </div>
            <div v-else-if="scheduleMode === 'minutely'" class="schedule-fields">
              <span>每分钟执行一次</span>
            </div>
            <NInput
              v-model:value="formSchedule"
              :readonly="scheduleMode !== 'custom'"
              placeholder="* * * * * 或 @reboot"
              size="small"
              class="schedule-raw"
            />
            <div v-if="scheduleMode === 'custom' && formSchedule.trim()" class="next-runs">
              <span v-if="nextRunTimes === null">表达式无法解析，请检查格式</span>
              <span v-else-if="nextRunTimes.length === 0">@reboot：随系统启动时执行一次</span>
              <template v-else>
                <div class="next-runs-title">未来 5 次执行时间（本地时区）：</div>
                <div v-for="(t, i) in nextRunTimes" :key="i" class="next-run">{{ fmtDateTime(t) }}</div>
              </template>
            </div>
          </div>
          <template #feedback>
            <span v-if="scheduleMode === 'custom'" class="hint">
              分 时 日 月 周（如 0 3 * * * = 每天 03:00）；@reboot / @daily 等关键字亦可
            </span>
            <span v-else class="hint">上方可视化设置自动生成表达式，切换到「自定义表达式」可手动编辑</span>
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

    <!-- 测试执行结果弹窗 -->
    <NModal
      v-model:show="showRun"
      preset="card"
      title="测试执行结果"
      style="width: 560px"
      :bordered="false"
    >
      <div class="run-cmd">{{ runCmd }}</div>
      <p class="run-meta">
        退出码：
        <span :class="runRc === 0 ? 'run-ok' : 'run-fail'">{{ runRc }}</span>
        <span class="run-note">以 cron 风格最小环境执行（/bin/sh，PATH=/usr/bin:/bin）</span>
      </p>
      <pre class="run-output">{{ runOutput || '（无输出）' }}</pre>
      <template #footer>
        <div class="dialog-footer">
          <NButton size="small" @click="showRun = false">关闭</NButton>
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
.schedule-builder {
  display: flex;
  flex-direction: column;
  gap: 8px;
  width: 100%;
}
.schedule-mode {
  width: 160px;
}
.schedule-fields {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12px;
  color: var(--text-secondary);
}
.schedule-num {
  width: 72px;
}
.schedule-dow {
  width: 84px;
}
.schedule-raw {
  font-family: var(--font-mono);
}
.next-runs {
  font-size: 11px;
  color: var(--text-tertiary);
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.next-run {
  font-family: var(--font-mono);
}
.run-cmd {
  font-size: 12px;
  font-family: var(--font-mono);
  color: var(--text-primary);
  word-break: break-all;
  margin-bottom: 8px;
}
.run-meta {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0 0 8px;
}
.run-ok {
  color: var(--success);
  font-weight: 500;
}
.run-fail {
  color: var(--danger);
  font-weight: 500;
}
.run-note {
  margin-left: 8px;
  font-size: 11px;
  color: var(--text-tertiary);
}
.run-output {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
  color: var(--text-primary);
  background: var(--bg-elevated);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 8px 10px;
  margin: 0;
  max-height: 40vh;
  overflow: auto;
  white-space: pre-wrap;
  word-break: break-all;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
