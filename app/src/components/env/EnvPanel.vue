<script setup lang="ts">
/**
 * EnvPanel - 环境管理面板（右栏页签）
 *
 * 对当前 SSH 会话远端主机的 10 种环境（Python / Conda / Go / Java /
 * MySQL / PostgreSQL / Redis / Nginx / OpenResty / Docker）提供：
 * - 安装（官方脚本 / 版本管理器，可选版本弹窗现场探测版本列表）
 * - 版本切换（python/go/java；仅版本管理器/官方包管理的运行时支持）
 * - Conda 环境管理（列表 / 新建 / 删除 / 终端激活）
 * - 卸载（NPopconfirm 二次确认，注明数据目录影响范围）
 * - 服务启停（仅服务类环境）
 * - 配置编辑（远程读取 → 面板文本编辑 → 带备份回写；nginx/openresty 校验失败自动回滚）
 * - 安装路径跳转（SFTP 文件面板定位 + 终端 cd 两个入口）
 * 数据来自 env store（静默 exec 通道探测，操作完成后自动刷新）。
 */
import { ref, computed, watch, onMounted, type Component } from 'vue'
import {
  NButton,
  NIcon,
  NPopconfirm,
  NModal,
  NInput,
  NSelect,
  NEmpty,
  NSpin,
  NTooltip,
  NTag,
  NDropdown,
  useMessage,
} from 'naive-ui'
import {
  Refresh,
  BrandPython,
  Hexagon,
  Coffee,
  Database,
  DatabaseExport,
  Bolt,
  Server,
  BrandOpenSource,
  BrandDocker,
  Box,
  PlayerPlay,
  PlayerStop,
  RotateClockwise,
  Trash,
  FileCode,
  Terminal2,
  Folder,
  Download,
  SwitchHorizontal,
  Versions,
} from '@vicons/tabler'
import { useEnvStore } from '@/stores/env'
import { useTabsStore } from '@/stores/tabs'
import { useLayoutStore } from '@/stores/layout'
import { ENV_DEFS, SWITCHABLE_ENVS, type EnvDef } from '@/services/env'
import * as envService from '@/services/env'
import * as sessionService from '@/services/session'
import { shellQuote } from '@/utils/shell'
import type { EnvId, EnvStatus } from '@/types/env'

const env = useEnvStore()
const tabs = useTabsStore()
const layout = useLayoutStore()
const message = useMessage()

const encoder = new TextEncoder()

onMounted(() => void env.refresh())

/** 切换活跃会话时重新探测 */
watch(
  () => tabs.activeTab?.sessionId,
  () => void env.refresh(),
)

/** 每种环境的展示图标（@vicons/tabler 近似匹配） */
const ENV_ICONS: Record<EnvId, Component> = {
  python: BrandPython,
  conda: Box,
  go: Hexagon,
  java: Coffee,
  mysql: Database,
  postgresql: DatabaseExport,
  redis: Bolt,
  nginx: Server,
  openresty: BrandOpenSource,
  docker: BrandDocker,
}

/** 当前主机提示 */
const hostLabel = computed(() => tabs.activeTab?.title ?? '')

/** 探测脚本整体失败的提示文案 */
const errorText = computed(() => {
  switch (env.errorKind) {
    case 'no-permission':
      return '当前用户权限不足，无法探测环境信息'
    default:
      return env.errorMessage || '环境探测失败'
  }
})

/** 服务状态展示（运行 / 已停止 / 异常 / 未知） */
function serviceLabel(s: EnvStatus): string {
  switch (s.serviceActive) {
    case 'active':
      return '运行中'
    case 'inactive':
      return '已停止'
    case 'failed':
      return '异常'
    default:
      return '未知'
  }
}

/** 安装来源展示（python/go/java；空串表示未探测来源） */
function sourceLabel(s: EnvStatus): string {
  switch (s.source) {
    case 'pyenv':
      return 'pyenv'
    case 'conda':
      return 'conda'
    case 'sdkman':
      return 'SDKMAN'
    case 'official':
      return '官方包'
    case 'system':
      return '系统安装'
    default:
      return ''
  }
}

/**
 * 各环境支持一键管理的安装来源（卸载/切换仅对这些来源开放；
 * 系统包安装的一律禁用，避免误删系统依赖）
 */
const MANAGED_SOURCES: Partial<Record<EnvId, string[]>> = {
  python: ['pyenv'],
  java: ['sdkman'],
  go: ['official'],
}

/** 切换版本额外允许 conda 管理的 python（conda install python=<v>） */
const SWITCH_SOURCES: Partial<Record<EnvId, string[]>> = {
  python: ['pyenv', 'conda'],
  java: ['sdkman'],
  go: ['official'],
}

function sourceOf(id: EnvId): string {
  return env.statuses[id]?.source ?? ''
}

/** 非受管来源时禁用一键卸载 */
function uninstallBlocked(id: EnvId): boolean {
  const allowed = MANAGED_SOURCES[id]
  return !!allowed && !allowed.includes(sourceOf(id))
}

/** 非受管来源时禁用版本切换 */
function switchBlocked(id: EnvId): boolean {
  const allowed = SWITCH_SOURCES[id]
  return !!allowed && !allowed.includes(sourceOf(id))
}

/** 卸载禁用原因（按来源差异化提示） */
function uninstallBlockedReason(id: EnvId): string {
  if (id === 'python' && sourceOf(id) === 'conda') {
    return 'Conda 管理的 Python，请在 Conda 卡片中卸载'
  }
  return '系统包安装，为避免破坏系统请用包管理器手动卸载'
}

async function onAction(fn: () => Promise<unknown>, ok: string) {
  try {
    await fn()
    message.success(ok)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/* ---------------- 安装弹窗 ---------------- */

const showInstall = ref(false)
const installDef = ref<EnvDef | null>(null)
const installVersion = ref<string | null>(null)
const versionOptions = ref<Array<{ label: string; value: string }>>([])
const versionsLoading = ref(false)

async function openInstall(def: EnvDef) {
  installDef.value = def
  installVersion.value = null
  versionOptions.value = []
  showInstall.value = true
  if (def.optionalVersion) {
    const sid = env.activeSshSessionId()
    if (!sid) return
    versionsLoading.value = true
    try {
      const list = await envService.listVersions(sid, def.id)
      versionOptions.value = list.map((v) => ({ label: v, value: v }))
      installVersion.value = list[0] ?? null
    } catch {
      // 版本列表探测失败不阻塞安装（可手动输入版本号）
    } finally {
      versionsLoading.value = false
    }
  }
}

async function submitInstall() {
  const def = installDef.value
  if (!def) return
  try {
    await env.install(def.id, installVersion.value ?? undefined)
    message.success(`${def.name} 安装完成`)
    showInstall.value = false
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/* ---------------- 版本切换弹窗 ---------------- */

const showSwitch = ref(false)
const switchDef = ref<EnvDef | null>(null)
const switchTarget = ref<string | null>(null)
const switchOptions = ref<Array<{ label: string; value: string }>>([])
const switchLoading = ref(false)

/** 切换弹窗说明（按环境区分数据来源与切换方式） */
const switchNote = computed(() => {
  switch (switchDef.value?.id) {
    case 'python':
      return 'pyenv 管理：选择本地已装版本直接切换，输入新版本号将先安装再切换（pyenv global 生效）；conda 管理：输入目标版本号经 conda install python=<版本> 切换（作用于 base 环境）'
    case 'java':
      return '选择本地已安装的 SDKMAN 版本直接切换；输入新版本号将先安装再切换（sdk default 生效，不影响系统自带 JDK）'
    case 'go':
      return '选择版本后下载 go.dev 官方包替换 /usr/local/go（需 root 或免密 sudo）'
    default:
      return ''
  }
})

async function openSwitch(def: EnvDef) {
  switchDef.value = def
  switchTarget.value = null
  switchOptions.value = []
  showSwitch.value = true
  const sid = env.activeSshSessionId()
  if (!sid) return
  switchLoading.value = true
  try {
    // go 列可下载的官方版本；python/java 列本地已装版本（均可手动输入）
    const list =
      def.id === 'go'
        ? await envService.listVersions(sid, 'go')
        : await envService.listLocalVersions(sid, def.id)
    switchOptions.value = list.map((v) => ({ label: v, value: v }))
  } catch {
    // 版本列表探测失败不阻塞切换（可手动输入版本号）
  } finally {
    switchLoading.value = false
  }
}

async function submitSwitch() {
  const def = switchDef.value
  if (!def || !switchTarget.value?.trim()) {
    message.warning('请选择或输入目标版本')
    return
  }
  try {
    await env.switchVersion(def.id, switchTarget.value)
    message.success(`${def.name} 已切换到 ${switchTarget.value}`)
    showSwitch.value = false
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/* ---------------- Conda 环境管理弹窗 ---------------- */

const showCondaEnvs = ref(false)
const condaEnvs = ref<envService.CondaEnv[]>([])
const condaEnvsLoading = ref(false)
/** 新建环境表单 */
const newEnvName = ref('')
const newEnvPy = ref('')
const condaEnvOperating = ref(false)

async function openCondaEnvs() {
  showCondaEnvs.value = true
  newEnvName.value = ''
  newEnvPy.value = ''
  await loadCondaEnvs()
}

async function loadCondaEnvs() {
  const sid = env.activeSshSessionId()
  if (!sid) return
  condaEnvsLoading.value = true
  try {
    condaEnvs.value = await envService.listCondaEnvs(sid)
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  } finally {
    condaEnvsLoading.value = false
  }
}

async function createCondaEnv() {
  const sid = env.activeSshSessionId()
  if (!sid || condaEnvOperating.value) return
  if (!newEnvName.value.trim()) {
    message.warning('请输入环境名')
    return
  }
  condaEnvOperating.value = true
  try {
    await envService.createCondaEnv(sid, newEnvName.value, newEnvPy.value || undefined)
    message.success(`环境 ${newEnvName.value} 已创建`)
    newEnvName.value = ''
    newEnvPy.value = ''
    await loadCondaEnvs()
    void env.refresh()
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  } finally {
    condaEnvOperating.value = false
  }
}

async function removeCondaEnv(name: string) {
  const sid = env.activeSshSessionId()
  if (!sid || condaEnvOperating.value) return
  condaEnvOperating.value = true
  try {
    await envService.removeCondaEnv(sid, name)
    message.success(`环境 ${name} 已删除`)
    await loadCondaEnvs()
    void env.refresh()
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  } finally {
    condaEnvOperating.value = false
  }
}

/** 切换环境：向当前终端发送 conda activate（用户在终端可见） */
function activateCondaEnv(name: string) {
  const sid = env.activeSshSessionId()
  if (!sid) return
  void sessionService.input(sid, encoder.encode(`conda activate ${shellQuote(name)}\r`)).catch((e) => {
    message.error(`发送失败：${e}`)
  })
}

/* ---------------- 配置编辑弹窗 ---------------- */

const showConfig = ref(false)
const configDraft = ref('')

async function openConfig(def: EnvDef) {
  const s = env.statuses[def.id]
  if (!s?.configPath) return
  try {
    await env.loadConfig(def.id, s.configPath)
    configDraft.value = env.configContent
    showConfig.value = true
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

async function reloadConfig() {
  if (!env.configEnvId || !env.configPath) return
  try {
    await env.loadConfig(env.configEnvId, env.configPath)
    configDraft.value = env.configContent
    message.success('已重新加载')
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

async function saveConfig() {
  try {
    await env.saveConfig(configDraft.value)
    message.success('配置已保存（原文件已备份）')
    showConfig.value = false
  } catch (e) {
    message.error(e instanceof Error ? e.message : String(e))
  }
}

/* ---------------- 路径跳转 ---------------- */

/** 终端跳转：向当前会话发送 cd（用户在终端可见命令与报错） */
function jumpTerminal(path: string) {
  const sid = env.activeSshSessionId()
  if (!sid) return
  void sessionService.input(sid, encoder.encode(`cd ${shellQuote(path)}\r`)).catch((e) => {
    message.error(`发送失败：${e}`)
  })
}

/** 文件跳转：打开 SFTP 文件面板并定位到安装目录 */
function jumpFiles(path: string) {
  layout.openFilesAt(path)
}

/**
 * 跳转目标选项：默认只有安装路径一项；nginx 等路径已指到配置目录、
 * 且探测到二进制目录的环境，提供「配置目录 / 二进制目录」选择。
 */
function jumpTargets(s: EnvStatus): Array<{ label: string; key: string }> {
  const list: Array<{ label: string; key: string }> = []
  if (s.installPath) list.push({ label: s.binPath ? '配置目录' : '安装路径', key: s.installPath })
  if (s.binPath && s.binPath !== s.installPath) {
    list.push({ label: '二进制目录', key: s.binPath })
  }
  return list
}
</script>

<template>
  <div class="env-panel">
    <div class="toolbar">
      <span class="host" :title="hostLabel">{{ hostLabel }}</span>
      <NTooltip>
        <template #trigger>
          <NButton quaternary circle size="small" :loading="env.detecting" @click="env.refresh()">
            <NIcon :component="Refresh" :size="14" />
          </NButton>
        </template>
        刷新
      </NTooltip>
    </div>

    <NSpin :show="env.detecting" size="small">
      <div class="panel-content">
        <!-- 无活跃会话 -->
        <NEmpty v-if="!env.hasSession" description="连接 SSH 会话后管理远端环境" class="empty" />

        <!-- 探测脚本整体失败 -->
        <div v-else-if="env.errorKind !== 'none'" class="error-state">
          <p class="error-text">{{ errorText }}</p>
        </div>

        <!-- 环境卡片 -->
        <template v-else>
          <div v-for="def in ENV_DEFS" :key="def.id" class="card">
            <div class="card-head">
              <NIcon :component="ENV_ICONS[def.id]" :size="18" class="card-icon" />
              <span class="card-name">{{ def.name }}</span>
              <NTag
                v-if="env.statuses[def.id]"
                size="tiny"
                :type="env.statuses[def.id]!.installed ? 'success' : 'default'"
                :bordered="false"
              >
                {{
                  env.statuses[def.id]!.installed
                    ? `已安装 ${env.statuses[def.id]!.version}`
                    : '未安装'
                }}
              </NTag>
              <span v-else class="card-pending">…</span>
              <NTag
                v-if="env.statuses[def.id]?.installed && sourceLabel(env.statuses[def.id]!)"
                size="tiny"
                type="info"
                :bordered="false"
              >
                {{ sourceLabel(env.statuses[def.id]!) }}
              </NTag>
              <span
                v-if="def.service && env.statuses[def.id]?.installed"
                class="svc"
                :class="env.statuses[def.id]!.serviceActive"
                :title="`服务：${env.statuses[def.id]!.serviceName ?? ''}`"
              >
                <span class="svc-dot" />{{ serviceLabel(env.statuses[def.id]!) }}
              </span>
            </div>
            <div v-if="env.statuses[def.id]?.installPath" class="card-path" :title="env.statuses[def.id]!.installPath">
              {{ env.statuses[def.id]!.installPath }}
            </div>
            <div class="card-actions">
              <template v-if="!env.statuses[def.id]?.installed">
                <NButton
                  size="tiny"
                  type="primary"
                  :loading="env.operating.has(def.id)"
                  @click="openInstall(def)"
                >
                  <template #icon><NIcon :component="Download" /></template>
                  安装
                </NButton>
              </template>
              <template v-else>
                <template v-if="def.service && env.statuses[def.id]!.serviceName">
                  <NTooltip v-if="env.statuses[def.id]!.serviceActive !== 'active'">
                    <template #trigger>
                      <NButton
                        quaternary
                        circle
                        size="tiny"
                        :disabled="env.operating.has(def.id)"
                        @click="onAction(() => env.serviceAction(def.id, 'start'), `${def.name} 已启动`)"
                      >
                        <NIcon :component="PlayerPlay" :size="14" />
                      </NButton>
                    </template>
                    启动
                  </NTooltip>
                  <NTooltip v-else>
                    <template #trigger>
                      <NButton
                        quaternary
                        circle
                        size="tiny"
                        :disabled="env.operating.has(def.id)"
                        @click="onAction(() => env.serviceAction(def.id, 'stop'), `${def.name} 已停止`)"
                      >
                        <NIcon :component="PlayerStop" :size="14" />
                      </NButton>
                    </template>
                    停止
                  </NTooltip>
                  <NTooltip>
                    <template #trigger>
                      <NButton
                        quaternary
                        circle
                        size="tiny"
                        :disabled="env.operating.has(def.id)"
                        @click="onAction(() => env.serviceAction(def.id, 'restart'), `${def.name} 已重启`)"
                      >
                        <NIcon :component="RotateClockwise" :size="14" />
                      </NButton>
                    </template>
                    重启
                  </NTooltip>
                </template>
                <NTooltip v-if="def.id === 'conda'">
                  <template #trigger>
                    <NButton
                      quaternary
                      circle
                      size="tiny"
                      :disabled="env.operating.has(def.id)"
                      @click="openCondaEnvs"
                    >
                      <NIcon :component="Versions" :size="14" />
                    </NButton>
                  </template>
                  管理 conda 环境
                </NTooltip>
                <NTooltip v-if="SWITCHABLE_ENVS.includes(def.id)">
                  <template #trigger>
                    <NButton
                      quaternary
                      circle
                      size="tiny"
                      :disabled="env.operating.has(def.id) || switchBlocked(def.id)"
                      @click="openSwitch(def)"
                    >
                      <NIcon :component="SwitchHorizontal" :size="14" />
                    </NButton>
                  </template>
                  {{ switchBlocked(def.id) ? '系统包安装，不支持一键切换版本' : '切换版本' }}
                </NTooltip>
                <NTooltip v-if="env.statuses[def.id]!.configPath">
                  <template #trigger>
                    <NButton
                      quaternary
                      circle
                      size="tiny"
                      :disabled="env.operating.has(def.id)"
                      @click="openConfig(def)"
                    >
                      <NIcon :component="FileCode" :size="14" />
                    </NButton>
                  </template>
                  编辑配置
                </NTooltip>
                <template v-if="env.statuses[def.id]!.installPath">
                  <NDropdown
                    v-if="env.statuses[def.id]!.binPath"
                    trigger="click"
                    :options="jumpTargets(env.statuses[def.id]!)"
                    @select="(p: string) => jumpFiles(p)"
                  >
                    <NButton quaternary circle size="tiny" title="文件面板定位（可选目录）">
                      <NIcon :component="Folder" :size="14" />
                    </NButton>
                  </NDropdown>
                  <NTooltip v-else>
                    <template #trigger>
                      <NButton
                        quaternary
                        circle
                        size="tiny"
                        @click="jumpFiles(env.statuses[def.id]!.installPath)"
                      >
                        <NIcon :component="Folder" :size="14" />
                      </NButton>
                    </template>
                    文件面板定位
                  </NTooltip>
                </template>
                <template v-if="env.statuses[def.id]!.installPath">
                  <NDropdown
                    v-if="env.statuses[def.id]!.binPath"
                    trigger="click"
                    :options="jumpTargets(env.statuses[def.id]!)"
                    @select="(p: string) => jumpTerminal(p)"
                  >
                    <NButton quaternary circle size="tiny" title="终端 cd 跳转（可选目录）">
                      <NIcon :component="Terminal2" :size="14" />
                    </NButton>
                  </NDropdown>
                  <NTooltip v-else>
                    <template #trigger>
                      <NButton
                        quaternary
                        circle
                        size="tiny"
                        @click="jumpTerminal(env.statuses[def.id]!.installPath)"
                      >
                        <NIcon :component="Terminal2" :size="14" />
                      </NButton>
                    </template>
                    终端 cd 跳转
                  </NTooltip>
                </template>
                <NTooltip v-if="uninstallBlocked(def.id)">
                  <template #trigger>
                    <NButton quaternary circle size="tiny" disabled>
                      <NIcon :component="Trash" :size="14" />
                    </NButton>
                  </template>
                  {{ uninstallBlockedReason(def.id) }}
                </NTooltip>
                <NPopconfirm
                  v-else
                  @positive-click="onAction(() => env.uninstall(def.id), `${def.name} 已卸载`)"
                >
                  <template #trigger>
                    <NButton
                      quaternary
                      circle
                      size="tiny"
                      :loading="env.operating.has(def.id)"
                    >
                      <NIcon :component="Trash" :size="14" />
                    </NButton>
                  </template>
                  确认卸载 {{ def.name }}？{{ def.uninstallNote }}。
                </NPopconfirm>
              </template>
            </div>
          </div>
        </template>
      </div>
    </NSpin>

    <!-- 安装弹窗 -->
    <NModal
      v-model:show="showInstall"
      preset="card"
      :title="`安装 ${installDef?.name ?? ''}`"
      style="width: 440px"
      :bordered="false"
    >
      <p class="install-note">{{ installDef?.installNote }}</p>
      <div v-if="installDef?.optionalVersion" class="install-version">
        <span class="install-version-label">版本</span>
        <NSelect
          v-model:value="installVersion"
          :options="versionOptions"
          :loading="versionsLoading"
          filterable
          tag
          size="small"
          placeholder="选择或输入版本号"
        />
      </div>
      <template #footer>
        <div class="dialog-footer">
          <NButton size="small" @click="showInstall = false">取消</NButton>
          <NButton
            size="small"
            type="primary"
            :loading="installDef ? env.operating.has(installDef.id) : false"
            @click="submitInstall"
          >
            安装
          </NButton>
        </div>
      </template>
    </NModal>

    <!-- 版本切换弹窗 -->
    <NModal
      v-model:show="showSwitch"
      preset="card"
      :title="`切换 ${switchDef?.name ?? ''} 版本`"
      style="width: 440px"
      :bordered="false"
    >
      <p class="install-note">
        当前版本：{{ env.statuses[switchDef!.id]?.version || '未知' }}。{{ switchNote }}
      </p>
      <div class="install-version">
        <span class="install-version-label">目标版本</span>
        <NSelect
          v-model:value="switchTarget"
          :options="switchOptions"
          :loading="switchLoading"
          filterable
          tag
          size="small"
          placeholder="选择或输入版本号"
        />
      </div>
      <template #footer>
        <div class="dialog-footer">
          <NButton size="small" @click="showSwitch = false">取消</NButton>
          <NButton
            size="small"
            type="primary"
            :loading="switchDef ? env.operating.has(switchDef.id) : false"
            @click="submitSwitch"
          >
            切换
          </NButton>
        </div>
      </template>
    </NModal>

    <!-- Conda 环境管理弹窗 -->
    <NModal
      v-model:show="showCondaEnvs"
      preset="card"
      title="Conda 环境管理"
      style="width: 480px"
      :bordered="false"
    >
      <NSpin :show="condaEnvsLoading" size="small">
        <div class="conda-envs">
          <NEmpty v-if="!condaEnvs.length" description="暂无 conda 环境" class="empty" />
          <div v-for="ce in condaEnvs" :key="ce.path" class="conda-env-row">
            <span class="conda-env-name">
              {{ ce.name }}
              <NTag v-if="ce.isBase" size="tiny" type="info" :bordered="false">base</NTag>
            </span>
            <span class="conda-env-path" :title="ce.path">{{ ce.path }}</span>
            <span class="conda-env-actions">
              <NTooltip>
                <template #trigger>
                  <NButton
                    quaternary
                    circle
                    size="tiny"
                    @click="activateCondaEnv(ce.name)"
                  >
                    <NIcon :component="Terminal2" :size="14" />
                  </NButton>
                </template>
                在终端激活
              </NTooltip>
              <NPopconfirm
                :disabled="ce.isBase"
                @positive-click="removeCondaEnv(ce.name)"
              >
                <template #trigger>
                  <NButton quaternary circle size="tiny" :disabled="ce.isBase || condaEnvOperating">
                    <NIcon :component="Trash" :size="14" />
                  </NButton>
                </template>
                确认删除环境 {{ ce.name }}？该环境下的全部包将被移除。
              </NPopconfirm>
            </span>
          </div>
        </div>
      </NSpin>
      <div class="conda-create">
        <NInput v-model:value="newEnvName" size="small" placeholder="环境名" class="conda-create-name" />
        <NInput v-model:value="newEnvPy" size="small" placeholder="Python 版本（可选，如 3.12）" />
        <NButton size="small" type="primary" :loading="condaEnvOperating" @click="createCondaEnv">
          新建
        </NButton>
      </div>
    </NModal>

    <!-- 配置编辑弹窗 -->
    <NModal
      v-model:show="showConfig"
      preset="card"
      title="编辑配置"
      style="width: 80%"
      :bordered="false"
    >
      <div class="config-path">{{ env.configPath }}</div>
      <p v-if="env.configEnvId === 'nginx' || env.configEnvId === 'openresty'" class="config-hint">
        保存时将先校验语法，失败自动回滚到备份
      </p>
      <NInput
        v-model:value="configDraft"
        type="textarea"
        class="config-editor"
        :autosize="{ minRows: 18, maxRows: 30 }"
        spellcheck="false"
      />
      <template #footer>
        <div class="dialog-footer">
          <NButton size="small" :loading="env.configLoading" @click="reloadConfig">重新加载</NButton>
          <NButton size="small" type="primary" :loading="env.configSaving" @click="saveConfig">
            保存
          </NButton>
        </div>
      </template>
    </NModal>
  </div>
</template>

<style scoped>
.env-panel {
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
.error-state {
  padding: 32px 16px;
  text-align: center;
}
.error-text {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0;
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
.card-icon {
  color: var(--text-secondary);
  flex-shrink: 0;
}
.card-name {
  font-size: 12px;
  font-weight: 500;
  color: var(--text-primary);
}
.card-pending {
  font-size: 11px;
  color: var(--text-tertiary);
}
.svc {
  display: flex;
  align-items: center;
  gap: 4px;
  margin-left: auto;
  font-size: 11px;
  color: var(--text-tertiary);
  flex-shrink: 0;
}
.svc-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--text-tertiary);
}
.svc.active {
  color: var(--success);
}
.svc.active .svc-dot {
  background: var(--success);
}
.svc.failed {
  color: var(--danger);
}
.svc.failed .svc-dot {
  background: var(--danger);
}
.card-path {
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.card-actions {
  display: flex;
  align-items: center;
  gap: 2px;
}
.install-note {
  font-size: 12px;
  color: var(--text-secondary);
  margin: 0 0 10px;
}
.install-version {
  display: flex;
  align-items: center;
  gap: 8px;
}
.install-version-label {
  font-size: 12px;
  color: var(--text-secondary);
  flex-shrink: 0;
}
.config-path {
  font-size: 12px;
  color: var(--text-secondary);
  font-family: var(--font-mono);
  word-break: break-all;
  margin-bottom: 8px;
}
.config-hint {
  font-size: 11px;
  color: var(--warning);
  margin: 0 0 8px;
}
.config-editor :deep(textarea) {
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.5;
}
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
.conda-envs {
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 40vh;
  overflow-y: auto;
}
.conda-env-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 6px;
  border-radius: 4px;
}
.conda-env-row:hover {
  background: var(--bg-elevated);
}
.conda-env-name {
  font-size: 12px;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.conda-env-path {
  flex: 1;
  min-width: 0;
  font-size: 11px;
  color: var(--text-tertiary);
  font-family: var(--font-mono);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.conda-env-actions {
  display: flex;
  align-items: center;
  gap: 2px;
  flex-shrink: 0;
}
.conda-create {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--border-color);
}
.conda-create-name {
  width: 140px;
  flex-shrink: 0;
}
</style>
