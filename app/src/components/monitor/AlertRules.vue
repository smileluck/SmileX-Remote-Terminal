<script setup lang="ts">
/**
 * AlertRules - 告警规则管理（RightPanel 监控页签底部区块）
 *
 * 规则 CRUD + 最近触发记录；触发通知由 SessionEvents 全局监听。
 */
import { ref, onMounted } from 'vue'
import {
  NButton,
  NIcon,
  NInput,
  NInputNumber,
  NSelect,
  NSwitch,
  useMessage,
} from 'naive-ui'
import { Plus, Trash } from '@vicons/tabler'
import * as alertsApi from '@/services/alerts'
import { METRIC_OPTIONS, OP_OPTIONS } from '@/services/alerts'
import type { AlertRule, AlertFired } from '@/services/alerts'

const message = useMessage()
const rules = ref<AlertRule[]>([])
const fired = ref<AlertFired[]>([])
const showForm = ref(false)

const draft = ref({
  name: '',
  metric: 'cpu_percent',
  op: 'gt',
  threshold: 90,
  cooldownSec: 300,
})

async function load() {
  try {
    rules.value = await alertsApi.listRules()
  } catch {
    /* 浏览器 dev 无后端 */
  }
}

onMounted(async () => {
  void load()
  // 本面板打开期间记录触发历史
  await alertsApi.onAlertFired((p) => {
    fired.value.unshift(p)
    if (fired.value.length > 20) fired.value.pop()
  })
})

async function addRule() {
  if (!draft.value.name.trim()) {
    message.warning('请填写规则名称')
    return
  }
  const rule: AlertRule = {
    id: crypto.randomUUID(),
    name: draft.value.name.trim(),
    metric: draft.value.metric,
    op: draft.value.op,
    threshold: draft.value.threshold,
    enabled: true,
    cooldownSec: draft.value.cooldownSec,
    createdAt: Math.floor(Date.now() / 1000),
  }
  try {
    await alertsApi.saveRule(rule)
    message.success('规则已保存')
    showForm.value = false
    draft.value.name = ''
    void load()
  } catch (e) {
    message.error(String(e))
  }
}

async function toggleRule(rule: AlertRule, enabled: boolean) {
  rule.enabled = enabled
  try {
    await alertsApi.saveRule(rule)
  } catch (e) {
    message.error(String(e))
  }
}

async function removeRule(id: string) {
  try {
    await alertsApi.deleteRule(id)
    void load()
  } catch (e) {
    message.error(String(e))
  }
}

function metricLabel(metric: string): string {
  return METRIC_OPTIONS.find((m) => m.value === metric)?.label ?? metric
}

function fmtTime(ms: number): string {
  return new Date(ms).toLocaleTimeString()
}
</script>

<template>
  <section class="alert-rules">
    <div class="section-head">
      <span class="section-title">告警规则</span>
      <NButton quaternary circle size="tiny" @click="showForm = !showForm">
        <template #icon><NIcon :component="Plus" /></template>
      </NButton>
    </div>

    <div v-if="showForm" class="rule-form">
      <NInput v-model:value="draft.name" size="tiny" placeholder="规则名称（如 CPU 过高）" />
      <div class="form-row">
        <NSelect v-model:value="draft.metric" size="tiny" :options="METRIC_OPTIONS" />
        <NSelect v-model:value="draft.op" size="tiny" :options="OP_OPTIONS" class="op" />
        <NInputNumber v-model:value="draft.threshold" size="tiny" :style="{ width: '84px' }" />
      </div>
      <div class="form-row">
        <NInputNumber
          v-model:value="draft.cooldownSec"
          size="tiny"
          :min="10"
          :style="{ width: '110px' }"
        >
          <template #suffix>秒冷却</template>
        </NInputNumber>
        <NButton size="tiny" type="primary" @click="addRule">保存</NButton>
      </div>
    </div>

    <div v-if="!rules.length && !showForm" class="rule-empty">暂无规则，点 + 添加</div>
    <div v-for="rule in rules" :key="rule.id" class="rule-row">
      <div class="rule-info">
        <span class="rule-name">{{ rule.name }}</span>
        <span class="rule-expr">
          {{ metricLabel(rule.metric).split(' (')[0] }}
          {{ rule.op === 'gt' ? '>' : '<' }} {{ rule.threshold }}
        </span>
      </div>
      <NSwitch size="small" :value="rule.enabled" @update:value="(v: boolean) => toggleRule(rule, v)" />
      <button class="rule-del" @click="removeRule(rule.id)">
        <NIcon :component="Trash" :size="13" />
      </button>
    </div>

    <template v-if="fired.length">
      <div class="section-head" style="margin-top: 10px">
        <span class="section-title">最近触发</span>
      </div>
      <div v-for="(f, i) in fired" :key="i" class="fired-row">
        <span class="fired-dot" />
        <span class="fired-text">{{ f.name }}：{{ f.value.toFixed(1) }}</span>
        <span class="fired-time">{{ fmtTime(f.timestamp_ms) }}</span>
      </div>
    </template>
  </section>
</template>

<style scoped>
.alert-rules {
  margin-top: 14px;
  padding-top: 10px;
  border-top: 1px solid var(--border-color);
}
.section-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}
.section-title {
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary);
  text-transform: uppercase;
  letter-spacing: 0.8px;
}
.rule-form {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 8px;
  background: var(--bg-panel);
  border-radius: 6px;
  margin-bottom: 8px;
}
.form-row {
  display: flex;
  gap: 6px;
  align-items: center;
}
.op {
  width: 80px;
  flex-shrink: 0;
}
.rule-empty {
  font-size: 12px;
  color: var(--text-tertiary);
  padding: 4px 0;
}
.rule-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 4px;
}
.rule-row:hover {
  background: var(--bg-elevated);
  border-radius: 6px;
}
.rule-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.rule-name {
  font-size: 13px;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.rule-expr {
  font-size: 11px;
  color: var(--text-tertiary);
}
.rule-del {
  display: none;
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 2px;
}
.rule-row:hover .rule-del {
  display: block;
}
.rule-del:hover {
  color: var(--danger);
}
.fired-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 3px 4px;
  font-size: 12px;
}
.fired-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--danger);
  flex-shrink: 0;
}
.fired-text {
  flex: 1;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.fired-time {
  color: var(--text-tertiary);
  font-size: 11px;
}
</style>
