<script setup lang="ts">
/**
 * SettingsView - 设置页（左侧分区导航 + 右侧内容区）
 *
 * 分区：外观（主题）/ LLM 配置 / SSH 密钥（KeyManager）。
 *
 * LLM 分区为双栏布局：
 * - 左侧：档案列表（含「+ 新建」「设为默认」「删除」「编辑」）
 * - 右侧：编辑区（名称 / Provider / Model / BaseURL / API Key / 流式）
 *
 * UI：naive-ui 控件 + useMessage 反馈。
 *
 * 数据流：
 * - 加载：listProfiles()（含 isActive 标记）
 * - 切换激活：setActiveProfile(id)（排他性）
 * - 保存：saveProfile(profile, apiKey)
 *   - API Key 三态：undefined 保持 / 空串清除 / 非空写入
 * - 测试连通性：testProfile(id)（10s 超时）
 *
 * 安全：API Key 在前端不回填明文，仅在用户主动输入时携带。
 */
import { computed, onMounted, ref, type Component } from 'vue'
import {
  NButton,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NSelect,
  NSwitch,
  NTag,
  NPopconfirm,
  NEmpty,
  NSpin,
  useMessage,
} from 'naive-ui'
import { Plus, Palette, Robot, Key, CloudDownload, Folder as FolderIcon } from '@vicons/tabler'
import { open as openFileDialog } from '@tauri-apps/plugin-dialog'
import { useThemeStore, type ThemeMode } from '@/stores/theme'
import { useTransferStore } from '@/stores/transfer'
import KeyManager from '@/components/settings/KeyManager.vue'
import {
  PROVIDER_OPTIONS,
  createDefaultProfileFields,
  findAccessMode,
  findProviderOption,
  type LlmProfile,
  type LlmProvider,
  type LlmAuthMode,
} from '@/types/settings'
import * as settingsService from '@/services/settings'

/** 简单的 UUID 生成（crypto.randomUUID 优先，回退 Math.random） */
function genId(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID()
  }
  return 'p-' + Math.random().toString(36).slice(2) + Date.now().toString(36)
}

const message = useMessage()
const themeStore = useThemeStore()
const transferStore = useTransferStore()

/** 设置分区：外观 / llm 配置 / ssh 密钥 / 文件传输 */
type Section = 'appearance' | 'llm' | 'keys' | 'transfer'
const section = ref<Section>('llm')

/** 分区元信息（左侧导航 + 内容区标题） */
const SECTIONS: { value: Section; label: string; icon: Component; title: string; desc: string }[] = [
  {
    value: 'appearance',
    label: '外观',
    icon: Palette,
    title: '外观',
    desc: '选择应用界面主题，「跟随系统」将随系统外观自动切换。',
  },
  {
    value: 'llm',
    label: 'LLM 配置',
    icon: Robot,
    title: 'LLM 配置',
    desc: '管理 AI 助手的 LLM 配置，可保存多个 Provider 档案并快速切换激活。API Key 加密存储于本地。',
  },
  {
    value: 'keys',
    label: 'SSH 密钥',
    icon: Key,
    title: 'SSH 密钥',
    desc: '生成或导入 SSH 密钥用于连接认证，私钥加密存储于本地，前端不展示明文。',
  },
  {
    value: 'transfer',
    label: '文件传输',
    icon: CloudDownload,
    title: '文件传输',
    desc: 'SFTP 上传下载的行为偏好。传输并发、进度与暂停/恢复见右下角传输管理。',
  },
]

/** 选择下载目录（原生对话框） */
async function pickDownloadDir() {
  try {
    const dir = await openFileDialog({ directory: true, title: '选择下载目录' })
    if (typeof dir === 'string') transferStore.downloadDir = dir
  } catch (e) {
    message.error(String(e))
  }
}

const currentSection = computed(() => SECTIONS.find((s) => s.value === section.value)!)

/** 主题选项（外观分区） */
const THEME_OPTIONS: { value: ThemeMode; label: string }[] = [
  { value: 'dark', label: '深色' },
  { value: 'light', label: '浅色' },
  { value: 'auto', label: '跟随系统' },
]

/** 所有档案（左侧列表展示） */
const profiles = ref<LlmProfile[]>([])
/** 当前编辑的档案 id（null 表示未选中） */
const selectedId = ref<string | null>(null)
/** 当前编辑的档案（深拷贝，避免直接修改列表） */
const editing = ref<LlmProfile | null>(null)

/** API Key 输入（不与 editing 绑定，单独管理三态） */
const apiKeyInput = ref('')
/** 是否已存在 API Key（用于占位符提示） */
const hasExistingApiKey = ref(false)

/** 加载/保存/测试状态 */
const loading = ref(false)
const saving = ref(false)
const testing = ref(false)

/** 当前编辑区 Provider 选项 */
const editingProviderOption = computed(() => {
  if (!editing.value) return PROVIDER_OPTIONS[0]
  return findProviderOption(editing.value.provider)
})

/** 当前编辑区接入方式选项（按 authMode 匹配，回退首个） */
const editingAccessMode = computed(() => {
  if (!editing.value) return PROVIDER_OPTIONS[0].accessModes[0]
  return findAccessMode(editing.value.provider, editing.value.authMode)
})

/** 该 Provider 是否区分接入方式（国内厂商：API 按量 / Coding Plan 订阅） */
const hasAccessModeSwitch = computed(
  () => editingProviderOption.value.accessModes.length > 1,
)

/** NSelect 的 Provider 选项（按「通用 / 国内厂商」分组） */
const providerOptions = computed(() => {
  const groups: Array<'通用' | '国内厂商'> = ['通用', '国内厂商']
  return groups.map((g) => ({
    type: 'group' as const,
    label: g,
    key: g,
    children: PROVIDER_OPTIONS.filter((o) => o.group === g).map((o) => ({
      label: o.label,
      value: o.value,
    })),
  }))
})

/** NSelect 的接入方式选项（仅多模式 Provider 有） */
const accessModeOptions = computed(() =>
  editingProviderOption.value.accessModes
    .filter((m) => m.value != null)
    .map((m) => ({ label: m.label, value: m.value as LlmAuthMode })),
)

/** NSelect 的 Model 选项（含自定义兼容，按当前接入方式的预设） */
const modelOptions = computed(() => {
  if (!editing.value) return []
  const presets = editingAccessMode.value.models.map((m) => ({ label: m, value: m }))
  // 当前值不在预设列表时追加（兼容旧数据 / 用户自定义）
  if (
    editing.value.model &&
    !editingAccessMode.value.models.includes(editing.value.model)
  ) {
    presets.push({ label: `${editing.value.model}（自定义）`, value: editing.value.model })
  }
  return presets
})

/** Base URL 输入框占位（部分接入方式的 URL 需用户补全信息，给模板提示） */
const baseUrlPlaceholder = computed(
  () =>
    editingAccessMode.value.baseUrlPlaceholder ??
    '留空使用官方默认；或填入代理 / 兼容服务地址',
)

/** 档案列表 meta 用：provider 值 → 显示标签 */
function providerLabel(value: string): string {
  return findProviderOption(value).label
}

/**
 * 加载档案列表
 *
 * 初次加载时若列表非空，自动选中第一个。
 */
async function loadProfiles() {
  loading.value = true
  try {
    profiles.value = await settingsService.listProfiles()
    if (profiles.value.length > 0 && !selectedId.value) {
      selectProfile(profiles.value[0].id)
    } else if (profiles.value.length === 0) {
      // 空列表：清空编辑区
      selectedId.value = null
      editing.value = null
    }
  } catch (e) {
    message.error(String(e))
  } finally {
    loading.value = false
  }
}

/**
 * 选中档案进行编辑
 *
 * 深拷贝档案到 editing，并查询 API Key 是否存在（不回填明文）。
 */
async function selectProfile(id: string) {
  selectedId.value = id
  const p = profiles.value.find((x) => x.id === id)
  if (!p) return
  // 深拷贝避免直接修改列表项
  editing.value = JSON.parse(JSON.stringify(p))
  apiKeyInput.value = ''
  hasExistingApiKey.value = false

  // 查询 API Key 是否存在（仅标记，不回填）
  try {
    const existing = await settingsService.getApiKey(id)
    hasExistingApiKey.value = !!existing
  } catch {
    // 静默失败（凭据库可能不可用）
  }
}

/**
 * 新建档案
 *
 * 在 editing 中创建临时对象（id 已生成但未保存到列表），
 * 实际持久化发生在用户点击「保存」时。
 */
function newProfile() {
  const id = genId()
  // 注意：createDefaultProfileFields 已含 name（空串），这里覆盖为默认名
  const fields = createDefaultProfileFields('openai')
  editing.value = {
    ...fields,
    id,
    name: '新配置',
  }
  selectedId.value = id
  apiKeyInput.value = ''
  hasExistingApiKey.value = false
}

/**
 * 保存当前编辑的档案
 *
 * API Key 三态：
 * - 输入框非空：写入新值
 * - 输入框为空 + 已有/无旧值：保持现状（传 undefined）
 */
async function save() {
  if (!editing.value) return
  if (!editing.value.name.trim()) {
    message.warning('请填写配置名称')
    return
  }
  if (!editing.value.model.trim()) {
    message.warning('请选择模型')
    return
  }

  saving.value = true
  try {
    const profile = editing.value
    // API Key 三态处理
    const apiKeyPayload = apiKeyInput.value.length > 0 ? apiKeyInput.value : undefined

    const saved = await settingsService.saveProfile(profile, apiKeyPayload)

    // 若输入了新 API Key，标记为已存在并清空输入框
    if (apiKeyPayload) {
      hasExistingApiKey.value = true
      apiKeyInput.value = ''
    }

    // 更新或插入到本地列表（保持顺序）
    const idx = profiles.value.findIndex((p) => p.id === saved.id)
    if (idx >= 0) {
      // 更新：若 saved.isActive=true，取消其他 active
      if (saved.isActive) {
        profiles.value.forEach((p) => {
          if (p.id !== saved.id) p.isActive = false
        })
      }
      profiles.value[idx] = saved
    } else {
      // 新建：插入到列表（若标记 active 同样取消其他）
      if (saved.isActive) {
        profiles.value.forEach((p) => (p.isActive = false))
      }
      profiles.value.push(saved)
    }
    editing.value = JSON.parse(JSON.stringify(saved))

    message.success('保存成功')
  } catch (e) {
    message.error(String(e))
  } finally {
    saving.value = false
  }
}

/**
 * 删除当前选中的档案（由 NPopconfirm 确认后触发）
 */
async function remove() {
  if (!editing.value) return
  const id = editing.value.id
  try {
    const ok = await settingsService.deleteProfile(id)
    if (ok) {
      profiles.value = profiles.value.filter((p) => p.id !== id)
      // 选中下一个或清空
      if (profiles.value.length > 0) {
        selectProfile(profiles.value[0].id)
      } else {
        selectedId.value = null
        editing.value = null
      }
      message.success('已删除')
    }
  } catch (e) {
    message.error(String(e))
  }
}

/**
 * 设为默认（激活）档案
 *
 * 排他性：其他档案自动取消 active。
 */
async function setActive() {
  if (!editing.value) return
  // 必须先保存才能激活
  if (!profiles.value.find((p) => p.id === editing.value!.id)) {
    message.warning('请先保存配置')
    return
  }

  try {
    await settingsService.setActiveProfile(editing.value.id)
    // 更新本地列表的 active 标记
    profiles.value.forEach((p) => {
      p.isActive = p.id === editing.value!.id
    })
    if (editing.value) editing.value.isActive = true
    message.success('已设为默认')
  } catch (e) {
    message.error(String(e))
  }
}

/**
 * 测试连通性
 *
 * 先保存（确保最新配置生效），再调用 testProfile。
 * 10s 超时，返回首个 token 表示链路可用。
 */
async function testConnection() {
  if (!editing.value) return
  // 必须先保存才能测试（testProfile 按 id 查）
  if (!profiles.value.find((p) => p.id === editing.value!.id)) {
    message.warning('请先保存配置再测试')
    return
  }

  testing.value = true
  try {
    const firstToken = await settingsService.testProfile(editing.value.id)
    message.success(`连通正常（首令牌: "${firstToken.slice(0, 20)}"）`, {
      duration: 4000,
    })
  } catch (e) {
    message.error(`测试失败：${String(e)}`)
  } finally {
    testing.value = false
  }
}

/**
 * 切换 Provider 时重置接入方式 / Model / BaseURL
 *
 * 国内厂商默认取首个接入方式（标准 API）；
 * BaseURL / 模型列表重置为该方式默认值。
 */
function onProviderChange(newProvider: LlmProvider) {
  if (!editing.value) return
  const opt = findProviderOption(newProvider)
  const mode = opt.accessModes[0]
  editing.value.provider = newProvider
  editing.value.authMode = opt.accessModes.length > 1 ? mode.value : null
  editing.value.model = mode.models[0] ?? ''
  editing.value.baseUrl = mode.defaultBaseUrl
}

/**
 * 切换接入方式（API 按量 / Coding Plan 订阅）
 *
 * 两种方式端点与协议不同：重置 BaseURL 与模型为该方式默认值。
 */
function onAccessModeChange(newMode: LlmAuthMode) {
  if (!editing.value) return
  const mode = findAccessMode(editing.value.provider, newMode)
  editing.value.authMode = newMode
  editing.value.model = mode.models[0] ?? ''
  editing.value.baseUrl = mode.defaultBaseUrl
}

onMounted(() => {
  loadProfiles()
})
</script>

<template>
  <div class="settings-view">
    <!-- 左侧：分区导航 -->
    <nav class="settings-nav">
      <div class="nav-header">设置</div>
      <button
        v-for="s in SECTIONS"
        :key="s.value"
        type="button"
        class="nav-item"
        :class="{ active: section === s.value }"
        @click="section = s.value"
      >
        <NIcon :component="s.icon" class="nav-icon" />
        <span>{{ s.label }}</span>
      </button>
    </nav>

    <!-- 右侧：分区内容 -->
    <div class="settings-main">
      <header class="section-header">
        <h2>{{ currentSection.title }}</h2>
        <p class="section-desc">{{ currentSection.desc }}</p>
      </header>

      <!-- 外观：主题选择 -->
      <div v-if="section === 'appearance'" class="section-body">
        <section class="settings-card theme-card">
          <div>
            <h3 class="settings-card-title">主题</h3>
            <p class="settings-card-desc">选择应用界面主题；「跟随系统」将随系统外观自动切换。</p>
          </div>
          <div class="theme-options">
            <button
              v-for="t in THEME_OPTIONS"
              :key="t.value"
              type="button"
              class="theme-option"
              :class="{ active: themeStore.mode === t.value }"
              @click="themeStore.setMode(t.value)"
            >
              <span class="theme-preview" :class="t.value">
                <span class="preview-rail"></span>
                <span class="preview-body">
                  <span class="preview-line"></span>
                  <span class="preview-line short"></span>
                </span>
              </span>
              <span class="theme-option-label">{{ t.label }}</span>
            </button>
          </div>
        </section>
      </div>

      <!-- SSH 密钥管理 -->
      <div v-else-if="section === 'keys'" class="section-body">
        <KeyManager />
      </div>

      <!-- 文件传输：下载目录偏好 -->
      <div v-else-if="section === 'transfer'" class="section-body">
        <section class="settings-card transfer-card">
          <div>
            <h3 class="settings-card-title">下载目录</h3>
            <p class="settings-card-desc">
              SFTP 下载文件的保存位置；同名文件自动加「(1)」后缀，不会覆盖。
            </p>
          </div>
          <div class="dir-picker">
            <NInput
              :value="transferStore.downloadDir || '系统下载目录（~/Downloads）'"
              readonly
              size="small"
            >
              <template #prefix><NIcon :component="FolderIcon" /></template>
            </NInput>
            <NButton size="small" @click="pickDownloadDir">选择目录</NButton>
            <NButton
              v-if="transferStore.downloadDir"
              size="small"
              quaternary
              @click="transferStore.downloadDir = ''"
            >
              恢复默认
            </NButton>
          </div>
        </section>
      </div>

      <!-- LLM 配置 -->
      <template v-else>
        <NSpin v-if="loading && profiles.length === 0" class="loading" />

        <div v-else class="layout">
          <!-- 左侧：档案列表 -->
          <aside class="sidebar">
            <div class="sidebar-header">
              <span class="sidebar-title">配置档案</span>
              <NButton quaternary size="tiny" circle title="新建" @click="newProfile">
                <template #icon><NIcon :component="Plus" /></template>
              </NButton>
            </div>
            <ul class="profile-list">
              <li
                v-for="p in profiles"
                :key="p.id"
                class="profile-item"
                :class="{ active: p.id === selectedId }"
                @click="selectProfile(p.id)"
              >
                <div class="profile-name">
                  <span class="name-text">{{ p.name }}</span>
                  <NTag v-if="p.isActive" size="tiny" type="success" :bordered="false">默认</NTag>
                </div>
                <div class="profile-meta">
                  <span>{{ providerLabel(p.provider) }}</span>
                  <span
                    v-if="p.authMode"
                    class="auth-mode-tag"
                    :class="{ plan: p.authMode === 'coding_plan' }"
                  >
                    {{ p.authMode === 'coding_plan' ? 'Coding Plan' : 'API' }}
                  </span>
                  <span class="separator">·</span>
                  <span>{{ p.model }}</span>
                </div>
              </li>
              <li v-if="profiles.length === 0" class="empty-hint">
                <NEmpty size="small" description="暂无配置">
                  <template #extra>
                    <span class="empty-tip">点击上方 + 新建</span>
                  </template>
                </NEmpty>
              </li>
            </ul>
          </aside>

          <!-- 右侧：编辑区 -->
          <section class="editor">
            <div v-if="!editing" class="editor-empty">
              <NEmpty description="请在左侧选择或新建配置档案" />
            </div>

            <template v-else>
              <NForm
                :model="editing"
                label-placement="top"
                size="small"
                class="form"
                @submit.prevent="save"
              >
                <!-- 名称 -->
                <NFormItem>
                  <template #label>配置名称<span class="required">*</span></template>
                  <NInput
                    v-model:value="editing.name"
                    placeholder="如：OpenAI 工作 / DeepSeek 个人 / Ollama 本地"
                  />
                </NFormItem>

                <!-- Provider -->
                <NFormItem label="Provider">
                  <NSelect
                    :value="editing.provider"
                    :options="providerOptions"
                    @update:value="onProviderChange"
                  />
                  <p v-if="!hasAccessModeSwitch" class="field-hint">
                    {{ editingAccessMode.hint }}
                  </p>
                </NFormItem>

                <!-- 接入方式（国内厂商：API 按量 / Coding Plan 订阅） -->
                <NFormItem v-if="hasAccessModeSwitch" label="接入方式">
                  <NSelect
                    :value="editing.authMode ?? undefined"
                    :options="accessModeOptions"
                    @update:value="onAccessModeChange"
                  />
                  <p class="field-hint">{{ editingAccessMode.hint }}</p>
                </NFormItem>

                <!-- Model（级联 + 可自定义输入） -->
                <NFormItem>
                  <template #label>模型<span class="required">*</span></template>
                  <NSelect
                    v-model:value="editing.model"
                    :options="modelOptions"
                    filterable
                    tag
                    :consistent-menu-width="false"
                    placeholder="选择或输入模型名"
                  />
                </NFormItem>

                <!-- Base URL -->
                <NFormItem label="Base URL（可选）">
                  <NInput
                    v-model:value="editing.baseUrl"
                    :placeholder="baseUrlPlaceholder"
                  />
                </NFormItem>

                <!-- API Key -->
                <NFormItem>
                  <template #label>
                    API Key
                    <span v-if="editingProviderOption.needsApiKey" class="required">*</span>
                    <span v-else class="optional">（此 Provider 不需要）</span>
                  </template>
                  <NInput
                    v-model:value="apiKeyInput"
                    type="password"
                    show-password-on="click"
                    :placeholder="hasExistingApiKey ? '已配置（留空保持不变）' : '请输入 API Key'"
                    :disabled="!editingProviderOption.needsApiKey"
                  />
                  <p v-if="hasExistingApiKey" class="field-hint success">
                    ✓ 已配置 API Key（留空保存将保持不变）
                  </p>
                </NFormItem>

                <!-- 流式 -->
                <NFormItem label="流式输出">
                  <div class="inline-switch">
                    <NSwitch v-model:value="editing.stream" />
                    <span class="inline-hint">逐 token 推送响应（推荐，体验更佳）</span>
                  </div>
                </NFormItem>
              </NForm>

              <!-- 动作区：左侧主操作，右侧默认/危险操作 -->
              <div class="actions">
                <div class="actions-group">
                  <NButton type="primary" :loading="saving" @click="save">保存</NButton>
                  <NButton :loading="testing" @click="testConnection">测试连接</NButton>
                </div>
                <div class="actions-group">
                  <NButton
                    tertiary
                    :disabled="
                      !editing.id ||
                      !profiles.find((p) => p.id === editing!.id) ||
                      editing.isActive
                    "
                    @click="setActive"
                  >
                    设为默认
                  </NButton>
                  <NPopconfirm @positive-click="remove">
                    <template #trigger>
                      <NButton tertiary type="error">删除</NButton>
                    </template>
                    确认删除配置「{{ editing.name }}」？
                  </NPopconfirm>
                </div>
              </div>
            </template>
          </section>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  flex: 1;
  display: flex;
  overflow: hidden;
  background: var(--bg-app);
}

/* 左侧分区导航 */
.settings-nav {
  width: 180px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding: 12px 10px;
  background: var(--bg-sidebar);
  border-right: 1px solid var(--border-color);
}
.nav-header {
  padding: 4px 10px 10px;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 1px;
  text-transform: uppercase;
  color: var(--text-tertiary);
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 10px;
  border: none;
  border-radius: var(--radius-md);
  background: transparent;
  font-family: inherit;
  font-size: 13px;
  color: var(--text-secondary);
  text-align: left;
  cursor: pointer;
  transition: background 0.12s, color 0.12s;
}
.nav-item:hover {
  background: var(--bg-elevated);
  color: var(--text-primary);
}
.nav-item.active {
  background: var(--primary-bg);
  color: var(--primary);
  font-weight: 500;
}
.nav-icon {
  font-size: 16px;
  flex-shrink: 0;
}

/* 右侧内容区 */
.settings-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.section-header {
  flex-shrink: 0;
  padding: 18px 28px 14px;
  border-bottom: 1px solid var(--border-color);
}
.section-header h2 {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
  color: var(--text-primary);
}
.section-desc {
  margin: 4px 0 0;
  font-size: 13px;
  line-height: 1.5;
  color: var(--text-secondary);
}
.section-body {
  flex: 1;
  overflow-y: auto;
  padding: 24px 28px;
}
.loading {
  flex: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}

/* 外观：主题预览卡片 */
.theme-card {
  max-width: 520px;
}

/* 文件传输：下载目录 */
.transfer-card {
  max-width: 560px;
}
.dir-picker {
  display: flex;
  gap: 8px;
  align-items: center;
}
.dir-picker .n-input {
  flex: 1;
}
.theme-options {
  display: flex;
  gap: 12px;
}
.theme-option {
  flex: 1;
  max-width: 148px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  padding: 10px;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  background: var(--bg-app);
  font-family: inherit;
  cursor: pointer;
  transition: background 0.12s, border-color 0.12s;
}
.theme-option:hover {
  background: var(--bg-elevated);
}
.theme-option.active {
  border-color: var(--primary-border);
  background: var(--primary-bg);
}
.theme-option-label {
  font-size: 12px;
  color: var(--text-secondary);
}
.theme-option.active .theme-option-label {
  color: var(--primary);
  font-weight: 500;
}
/* 预览配色为固定值：展示目标主题的样子，不随当前主题变化 */
.theme-preview {
  width: 100%;
  height: 58px;
  display: flex;
  border-radius: 4px;
  overflow: hidden;
  border: 1px solid var(--border-strong);
}
.preview-rail {
  width: 18px;
  flex-shrink: 0;
}
.preview-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  justify-content: center;
  gap: 6px;
  padding: 0 8px;
}
.preview-line {
  height: 5px;
  border-radius: 2px;
}
.preview-line.short {
  width: 60%;
}
.theme-preview.dark {
  background: #0f1419;
}
.theme-preview.dark .preview-rail {
  background: #0b0f14;
}
.theme-preview.dark .preview-line {
  background: #2a3340;
}
.theme-preview.light {
  background: #f5f7fa;
}
.theme-preview.light .preview-rail {
  background: #eceff4;
}
.theme-preview.light .preview-line {
  background: #d3dae4;
}
.theme-preview.auto {
  background: linear-gradient(90deg, #0f1419 50%, #f5f7fa 50%);
}
.theme-preview.auto .preview-rail {
  background: linear-gradient(90deg, #0b0f14 50%, #eceff4 50%);
}
.theme-preview.auto .preview-line {
  background: linear-gradient(90deg, #2a3340 50%, #d3dae4 50%);
}

/* LLM 双栏布局 */
.layout {
  flex: 1;
  display: flex;
  overflow: hidden;
}
.sidebar {
  width: 240px;
  border-right: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  background: var(--bg-sidebar);
  flex-shrink: 0;
}
.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border-color);
}
.sidebar-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-secondary);
  text-transform: uppercase;
  letter-spacing: 1px;
}
.profile-list {
  list-style: none;
  margin: 0;
  padding: 8px;
  flex: 1;
  overflow-y: auto;
}
.profile-item {
  padding: 10px 12px;
  border-radius: var(--radius-md);
  cursor: pointer;
  transition: background 0.12s;
  margin-bottom: 4px;
  border: 1px solid transparent;
}
.profile-item:hover {
  background: var(--bg-elevated);
}
.profile-item.active {
  background: var(--primary-bg);
  border-color: var(--primary-border);
}
.profile-item.active .name-text {
  color: var(--primary);
}
.profile-name {
  font-size: 13px;
  font-weight: 500;
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-primary);
}
.profile-meta {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-top: 4px;
  display: flex;
  align-items: center;
  gap: 2px;
  min-width: 0;
}
.separator {
  margin: 0 4px;
  opacity: 0.6;
}
/* 接入方式小标签：API（按量）/ Coding Plan（订阅） */
.auth-mode-tag {
  flex-shrink: 0;
  padding: 0 5px;
  border-radius: var(--radius-sm, 3px);
  font-size: 10px;
  line-height: 16px;
  border: 1px solid var(--border-color);
  color: var(--text-tertiary);
}
.auth-mode-tag.plan {
  color: var(--primary);
  border-color: var(--primary-bg);
  background: var(--primary-bg);
}
.empty-hint {
  display: flex;
  justify-content: center;
  padding: 24px 8px;
}
.empty-tip {
  font-size: 12px;
  color: var(--text-tertiary);
}

/* 编辑区 */
.editor {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px;
}
.editor-empty {
  display: flex;
  align-items: center;
  justify-content: center;
  height: 100%;
}
.form {
  max-width: 560px;
}
/* n-form-item-blank 默认为横向 flex，会把 field-hint 挤到控件右侧 */
.form :deep(.n-form-item-blank) {
  flex-direction: column;
  align-items: stretch;
}
.inline-switch {
  display: flex;
  align-items: center;
  gap: 10px;
}
.inline-hint {
  font-size: 12px;
  color: var(--text-secondary);
}
.field-hint {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--text-secondary);
}
.field-hint.success {
  color: var(--success);
}
.required {
  color: var(--danger);
  margin-left: 2px;
}
.optional {
  color: var(--text-tertiary);
  font-weight: normal;
  font-size: 12px;
}

/* 动作区：主操作与危险操作左右分离 */
.actions {
  max-width: 560px;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
  margin-top: 4px;
  padding-top: 16px;
  border-top: 1px solid var(--border-color);
}
.actions-group {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}
</style>
