<script setup lang="ts">
/**
 * AiSecurity - 设置页「AI 命令授权」分区（只读视图）
 *
 * - 命令白名单：AI 执行闸门放行的授权条目（确认弹窗勾选「记住授权」写入），
 *   此处可逐条删除（危险命令不允许入表，由后端保证）
 * - 审计日志：最近 50 条 AI 命令执行/拒绝留痕（后端 ai_audit_list）
 */
import { onMounted, ref } from 'vue'
import { NButton, NEmpty, NIcon, NPopconfirm, NSpin, NTag, useMessage } from 'naive-ui'
import { Trash } from '@vicons/tabler'
import * as aiExecService from '@/services/aiExec'
import type { AiAllowlistEntry, AiAuditEntry } from '@/services/aiExec'

const message = useMessage()

/** 白名单与审计数据 */
const allowlist = ref<AiAllowlistEntry[]>([])
const audits = ref<AiAuditEntry[]>([])
const loadingAllowlist = ref(false)
const loadingAudit = ref(false)

const RISK_LABEL: Record<string, string> = { read_only: '只读', modify: '修改', danger: '危险' }
const SCOPE_LABEL: Record<string, string> = { chat: '单会话', profile: '单主机', global: '全局' }
const DECISION_LABEL: Record<string, string> = {
  auto: '自动放行',
  approved: '批准后执行',
  rejected: '已拒绝',
  failed: '执行失败',
}
const DECISION_TYPE: Record<string, 'success' | 'warning' | 'error' | 'default'> = {
  auto: 'success',
  approved: 'warning',
  rejected: 'error',
  failed: 'error',
}
const SOURCE_LABEL: Record<string, string> = { manual: '手动', auto: '自动', plan: '计划' }

function formatTime(ts: number): string {
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

async function loadAllowlist() {
  loadingAllowlist.value = true
  try {
    allowlist.value = await aiExecService.allowlistAll()
  } catch (e) {
    message.error(String(e))
  } finally {
    loadingAllowlist.value = false
  }
}

async function loadAudits() {
  loadingAudit.value = true
  try {
    audits.value = await aiExecService.auditList(undefined, 50)
  } catch (e) {
    message.error(String(e))
  } finally {
    loadingAudit.value = false
  }
}

async function removeEntry(id: number) {
  try {
    await aiExecService.allowlistRemove(id)
    allowlist.value = allowlist.value.filter((e) => e.id !== id)
    message.success('已删除授权')
  } catch (e) {
    message.error(String(e))
  }
}

onMounted(() => {
  void loadAllowlist()
  void loadAudits()
})
</script>

<template>
  <div class="ai-security">
    <!-- AI 命令白名单 -->
    <section class="settings-card sec-card">
      <div class="sec-head">
        <div>
          <h3 class="settings-card-title">命令授权白名单</h3>
          <p class="settings-card-desc">
            确认弹窗中勾选「记住授权」的修改类命令；命中白名单后同类命令自动放行。危险命令不可加白。
          </p>
        </div>
        <NButton size="tiny" quaternary :loading="loadingAllowlist" @click="loadAllowlist">
          刷新
        </NButton>
      </div>
      <NSpin :show="loadingAllowlist">
        <NEmpty v-if="allowlist.length === 0" size="small" description="暂无授权条目" />
        <ul v-else class="entry-list">
          <li v-for="e in allowlist" :key="e.id" class="entry">
            <code class="entry-pattern" :title="e.pattern">{{ e.pattern }}</code>
            <NTag size="tiny" :bordered="false">{{ RISK_LABEL[e.risk] ?? e.risk }}</NTag>
            <NTag size="tiny" type="info" :bordered="false">{{ SCOPE_LABEL[e.scope] ?? e.scope }}</NTag>
            <span class="entry-time">{{ formatTime(e.createdAt) }}</span>
            <NPopconfirm @positive-click="removeEntry(e.id)">
              <template #trigger>
                <NButton size="tiny" quaternary type="error" title="删除该授权">
                  <template #icon><NIcon :component="Trash" :size="12" /></template>
                </NButton>
              </template>
              删除授权「{{ e.pattern }}」？
            </NPopconfirm>
          </li>
        </ul>
      </NSpin>
    </section>

    <!-- 审计日志 -->
    <section class="settings-card sec-card">
      <div class="sec-head">
        <div>
          <h3 class="settings-card-title">审计日志</h3>
          <p class="settings-card-desc">最近 50 条 AI 命令的放行 / 拒绝 / 执行记录（只读）。</p>
        </div>
        <NButton size="tiny" quaternary :loading="loadingAudit" @click="loadAudits">刷新</NButton>
      </div>
      <NSpin :show="loadingAudit">
        <NEmpty v-if="audits.length === 0" size="small" description="暂无审计记录" />
        <ul v-else class="entry-list">
          <li v-for="a in audits" :key="a.id" class="entry audit">
            <span class="entry-time">{{ formatTime(a.ts) }}</span>
            <code class="entry-pattern" :title="a.command">{{ a.command }}</code>
            <NTag size="tiny" :bordered="false">{{ RISK_LABEL[a.risk] ?? a.risk }}</NTag>
            <NTag size="tiny" :type="DECISION_TYPE[a.decision] ?? 'default'" :bordered="false">
              {{ DECISION_LABEL[a.decision] ?? a.decision }}
            </NTag>
            <span class="entry-meta">{{ SOURCE_LABEL[a.source] ?? a.source }}</span>
            <span v-if="a.exitCode !== undefined && a.exitCode !== null" class="entry-meta">
              exit {{ a.exitCode }}
            </span>
          </li>
        </ul>
      </NSpin>
    </section>
  </div>
</template>

<style scoped>
.ai-security {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 720px;
}
.sec-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.sec-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 8px;
}
.entry-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.entry {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  background: var(--bg-app);
}
.entry-pattern {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-family: ui-monospace, Consolas, 'Courier New', monospace;
  font-size: 12px;
  color: var(--text-primary);
}
.entry-time {
  font-size: 11px;
  color: var(--text-tertiary);
  white-space: nowrap;
}
.entry-meta {
  font-size: 11px;
  color: var(--text-secondary);
  white-space: nowrap;
}
.audit .entry-pattern {
  flex: 1;
}
</style>
