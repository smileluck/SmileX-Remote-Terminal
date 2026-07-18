<script setup lang="ts">
/**
 * SettingsView - 设置页（多 LLM 配置档案管理）
 *
 * 双栏布局：
 * - 左侧：档案列表（含「+ 新建」/「设为默认」/「删除」/「编辑」）
 * - 右侧：编辑区（名称 / Provider / Model / BaseURL / API Key / 流式）
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
import { computed, onMounted, ref } from 'vue'
import {
  PROVIDER_OPTIONS,
  createDefaultProfileFields,
  findProviderOption,
  type LlmProfile,
  type LlmProvider,
} from '@/types/settings'
import * as settingsService from '@/services/settings'

/** 简单的 UUID 生成（crypto.randomUUID 优先，回退 Math.random） */
function genId(): string {
  if (typeof crypto !== 'undefined' && crypto.randomUUID) {
    return crypto.randomUUID()
  }
  return 'p-' + Math.random().toString(36).slice(2) + Date.now().toString(36)
}

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
/** 提示信息 */
const errorMsg = ref('')
const successMsg = ref('')

/**
 * 编辑区 Provider 选项（用于级联下拉）
 */
const editingProviderOption = computed(() => {
  if (!editing.value) return PROVIDER_OPTIONS[0]
  return findProviderOption(editing.value.provider)
})

/**
 * 加载档案列表
 *
 * 初次加载时若列表非空，自动选中第一个。
 */
async function loadProfiles() {
  loading.value = true
  errorMsg.value = ''
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
    errorMsg.value = String(e)
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
  errorMsg.value = ''
  successMsg.value = ''

  // 查询 API Key 是否存在（仅标记，不回填）
  try {
    const existing = await settingsService.getApiKey(id)
    hasExistingApiKey.value = !!existing
  } catch {
    // 静默失败（Keyring 可能不可用）
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
  errorMsg.value = ''
  successMsg.value = ''
}

/**
 * 保存当前编辑的档案
 *
 * - 新建：插入到列表 + 持久化
 * - 更新：替换列表项 + 持久化
 *
 * API Key 三态：
 * - 输入框非空：写入新值
 * - 输入框为空 + 已有旧值：保持现状（传 undefined）
 * - 输入框为空 + 无旧值：保持现状（传 undefined）
 */
async function save() {
  if (!editing.value) return
  if (!editing.value.name.trim()) {
    errorMsg.value = '请填写配置名称'
    return
  }
  if (!editing.value.model.trim()) {
    errorMsg.value = '请选择模型'
    return
  }

  saving.value = true
  errorMsg.value = ''
  successMsg.value = ''
  try {
    const profile = editing.value
    // API Key 三态处理
    const apiKeyPayload =
      apiKeyInput.value.length > 0 ? apiKeyInput.value : undefined

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

    successMsg.value = '保存成功'
    setTimeout(() => {
      successMsg.value = ''
    }, 2000)
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    saving.value = false
  }
}

/**
 * 删除当前选中的档案
 *
 * 确认后调用 deleteProfile，成功后从列表移除。
 */
async function remove() {
  if (!editing.value) return
  const id = editing.value.id
  if (!confirm(`确认删除配置「${editing.value.name}」？`)) return

  errorMsg.value = ''
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
    }
  } catch (e) {
    errorMsg.value = String(e)
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
    errorMsg.value = '请先保存配置'
    return
  }

  errorMsg.value = ''
  try {
    await settingsService.setActiveProfile(editing.value.id)
    // 更新本地列表的 active 标记
    profiles.value.forEach((p) => {
      p.isActive = p.id === editing.value!.id
    })
    if (editing.value) editing.value.isActive = true
    successMsg.value = '已设为默认'
    setTimeout(() => {
      successMsg.value = ''
    }, 2000)
  } catch (e) {
    errorMsg.value = String(e)
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
    errorMsg.value = '请先保存配置再测试'
    return
  }

  testing.value = true
  errorMsg.value = ''
  successMsg.value = ''
  try {
    const firstToken = await settingsService.testProfile(editing.value.id)
    successMsg.value = `连通正常（首令牌: "${firstToken.slice(0, 20)}"）`
    setTimeout(() => {
      successMsg.value = ''
    }, 4000)
  } catch (e) {
    errorMsg.value = `测试失败：${String(e)}`
  } finally {
    testing.value = false
  }
}

/**
 * 切换 Provider 时重置 Model / BaseURL
 *
 * 自动选择该 Provider 的首个默认模型；
 * BaseURL 重置为该 Provider 的默认值（空表示官方默认）。
 */
function onProviderChange(newProvider: LlmProvider) {
  if (!editing.value) return
  const opt = findProviderOption(newProvider)
  editing.value.provider = newProvider
  editing.value.model = opt.models[0] ?? ''
  editing.value.baseUrl = opt.defaultBaseUrl ?? ''
}

onMounted(() => {
  loadProfiles()
})
</script>

<template>
  <div class="settings-view">
    <header class="header">
      <h2>设置</h2>
      <p class="hint">
        管理 AI 助手的 LLM 配置。可保存多个 Provider 配置并快速切换激活。
        API Key 加密存储于系统凭据库（Keyring）。
      </p>
    </header>

    <div v-if="loading && profiles.length === 0" class="loading">加载中…</div>

    <div v-else class="layout">
      <!-- 左侧：档案列表 -->
      <aside class="sidebar">
        <div class="sidebar-header">
          <span class="sidebar-title">配置档案</span>
          <button class="btn-mini" title="新建" @click="newProfile">+</button>
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
              {{ p.name }}
              <span v-if="p.isActive" class="active-badge">默认</span>
            </div>
            <div class="profile-meta">
              <span class="profile-provider">{{ p.provider }}</span>
              <span class="separator">·</span>
              <span class="profile-model">{{ p.model }}</span>
            </div>
          </li>
          <li v-if="profiles.length === 0" class="empty-hint">
            暂无配置，点击 + 新建
          </li>
        </ul>
      </aside>

      <!-- 右侧：编辑区 -->
      <section class="editor">
        <div v-if="!editing" class="editor-empty">
          <p>请在左侧选择或新建配置档案</p>
        </div>

        <form v-else class="form" @submit.prevent="save">
          <!-- 名称 -->
          <div class="field">
            <label class="label">配置名称</label>
            <input
              v-model="editing.name"
              class="control"
              placeholder="如：OpenAI 工作 / DeepSeek 个人 / Ollama 本地"
            />
          </div>

          <!-- Provider -->
          <div class="field">
            <label class="label">Provider</label>
            <select
              :value="editing.provider"
              class="control"
              @change="onProviderChange(($event.target as HTMLSelectElement).value as LlmProvider)"
            >
              <option v-for="opt in PROVIDER_OPTIONS" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </option>
            </select>
            <p class="field-hint">{{ editingProviderOption.hint }}</p>
          </div>

          <!-- Model（级联下拉） -->
          <div class="field">
            <label class="label">模型</label>
            <select v-model="editing.model" class="control">
              <option value="" disabled>请选择模型</option>
              <option v-for="m in editingProviderOption.models" :key="m" :value="m">
                {{ m }}
              </option>
              <!-- 当前值不在预设列表时也显示（兼容旧数据） -->
              <option
                v-if="editing.model && !editingProviderOption.models.includes(editing.model)"
                :value="editing.model"
              >
                {{ editing.model }}（自定义）
              </option>
            </select>
          </div>

          <!-- Base URL -->
          <div class="field">
            <label class="label">Base URL（可选）</label>
            <input
              v-model="editing.baseUrl"
              class="control"
              placeholder="留空使用官方默认；或填入代理 / 兼容服务地址"
            />
          </div>

          <!-- API Key -->
          <div class="field">
            <label class="label">
              API Key
              <span v-if="editingProviderOption.needsApiKey" class="required">*</span>
              <span v-else class="optional">（此 Provider 不需要）</span>
            </label>
            <input
              v-model="apiKeyInput"
              type="password"
              class="control"
              :placeholder="hasExistingApiKey ? '已配置（留空保持不变）' : '请输入 API Key'"
              :disabled="!editingProviderOption.needsApiKey"
              autocomplete="off"
            />
            <p v-if="hasExistingApiKey" class="field-hint success">
              ✓ 已配置 API Key（留空保存将保持不变）
            </p>
          </div>

          <!-- 流式 -->
          <div class="field field-inline">
            <label class="label">流式输出</label>
            <input v-model="editing.stream" type="checkbox" class="checkbox" />
            <span class="inline-hint">逐 token 推送响应（推荐，体验更佳）</span>
          </div>

          <!-- 错误/成功提示 -->
          <p v-if="errorMsg" class="error">{{ errorMsg }}</p>
          <p v-if="successMsg" class="success">{{ successMsg }}</p>

          <!-- 动作区 -->
          <div class="actions">
            <button type="submit" class="btn primary" :disabled="saving">
              {{ saving ? '保存中…' : '保存' }}
            </button>
            <button
              type="button"
              class="btn"
              :disabled="testing"
              @click="testConnection"
            >
              {{ testing ? '测试中…' : '测试连接' }}
            </button>
            <button
              type="button"
              class="btn"
              :disabled="!editing.id || !profiles.find((p) => p.id === editing!.id) || editing.isActive"
              @click="setActive"
            >
              设为默认
            </button>
            <button type="button" class="btn danger" @click="remove">删除</button>
          </div>
        </form>
      </section>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--bg-primary, #1e1e1e);
  color: var(--text-primary, #ddd);
}
.header {
  padding: 16px 24px 12px;
  border-bottom: 1px solid var(--border-color, #3a3a3a);
}
.header h2 {
  margin: 0 0 4px 0;
  font-size: 18px;
  font-weight: 600;
  color: var(--text-primary, #ddd);
}
.hint {
  margin: 0;
  font-size: 12px;
  color: var(--text-secondary, #888);
}
.loading {
  padding: 32px;
  text-align: center;
  color: var(--text-secondary, #888);
}

/* 双栏布局 */
.layout {
  flex: 1;
  display: flex;
  overflow: hidden;
}
.sidebar {
  width: 240px;
  border-right: 1px solid var(--border-color, #3a3a3a);
  display: flex;
  flex-direction: column;
  background: var(--bg-secondary, #252526);
}
.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-color, #3a3a3a);
}
.sidebar-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary, #ccc);
  text-transform: uppercase;
  letter-spacing: 0.5px;
}
.btn-mini {
  width: 24px;
  height: 24px;
  border: 1px solid var(--border-color, #4a4a4a);
  border-radius: 4px;
  background: transparent;
  color: var(--text-primary, #ccc);
  cursor: pointer;
  font-size: 16px;
  line-height: 1;
  display: flex;
  align-items: center;
  justify-content: center;
}
.btn-mini:hover {
  background: var(--primary-color, #2e70c8);
  border-color: var(--primary-color, #2e70c8);
  color: #fff;
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
  border-radius: 4px;
  cursor: pointer;
  transition: background 0.12s;
  margin-bottom: 4px;
}
.profile-item:hover {
  background: var(--bg-hover, #2a2a2a);
}
.profile-item.active {
  background: var(--primary-color, #2e70c8);
  color: #fff;
}
.profile-item.active .profile-meta,
.profile-item.active .profile-provider,
.profile-item.active .profile-model {
  color: rgba(255, 255, 255, 0.85);
}
.profile-name {
  font-size: 13px;
  font-weight: 500;
  display: flex;
  align-items: center;
  gap: 6px;
}
.active-badge {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: #7fdc9b;
  color: #1e3a1e;
  font-weight: 600;
}
.profile-item.active .active-badge {
  background: #fff;
  color: var(--primary-color, #2e70c8);
}
.profile-meta {
  font-size: 11px;
  color: var(--text-secondary, #888);
  margin-top: 4px;
}
.separator {
  margin: 0 4px;
  opacity: 0.5;
}
.empty-hint {
  padding: 16px;
  text-align: center;
  font-size: 12px;
  color: var(--text-secondary, #666);
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
  color: var(--text-secondary, #666);
  font-size: 14px;
}
.form {
  max-width: 560px;
  display: flex;
  flex-direction: column;
  gap: 18px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field-inline {
  flex-direction: row;
  align-items: center;
  gap: 12px;
}
.label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary, #ccc);
}
.required {
  color: #ff6b6b;
  margin-left: 2px;
}
.optional {
  color: var(--text-secondary, #888);
  font-weight: normal;
  font-size: 12px;
}
.control {
  padding: 8px 12px;
  border: 1px solid var(--border-color, #3a3a3a);
  border-radius: 4px;
  background: var(--bg-secondary, #252526);
  color: var(--text-primary, #ddd);
  font-size: 14px;
  font-family: inherit;
  outline: none;
  transition: border-color 0.15s;
}
.control:focus {
  border-color: var(--primary-color, #2e70c8);
}
.control:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.checkbox {
  width: 16px;
  height: 16px;
  cursor: pointer;
}
.inline-hint {
  font-size: 12px;
  color: var(--text-secondary, #888);
}
.field-hint {
  margin: 0;
  font-size: 12px;
  color: var(--text-secondary, #888);
}
.field-hint.success {
  color: #7fdc9b;
}
.error {
  margin: 0;
  padding: 8px 12px;
  color: #ff6b6b;
  background: #3a1a1a;
  border-radius: 4px;
  font-size: 13px;
}
.success {
  margin: 0;
  padding: 8px 12px;
  color: #7fdc9b;
  background: #1a3a1f;
  border-radius: 4px;
  font-size: 13px;
}
.actions {
  display: flex;
  gap: 10px;
  margin-top: 8px;
  flex-wrap: wrap;
}
.btn {
  padding: 8px 16px;
  border: 1px solid var(--border-color, #4a4a4a);
  border-radius: 4px;
  background: var(--bg-secondary, #2d2d2d);
  color: var(--text-primary, #ddd);
  font-size: 13px;
  cursor: pointer;
  transition: background 0.15s;
}
.btn:hover:not(:disabled) {
  background: var(--bg-hover, #3a3a3a);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.btn.primary {
  background: var(--primary-color, #2e70c8);
  border-color: var(--primary-color, #2e70c8);
  color: #fff;
}
.btn.primary:hover:not(:disabled) {
  background: var(--primary-hover, #1e5fa8);
}
.btn.danger {
  border-color: #6b2a2a;
  color: #ff8a8a;
}
.btn.danger:hover:not(:disabled) {
  background: #3a1a1a;
}
</style>
