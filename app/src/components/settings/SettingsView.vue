<script setup lang="ts">
/**
 * SettingsView - 设置页
 *
 * 配置 AI 助手的 LLM Provider：
 * - Provider 类型（OpenAI 兼容 / Claude / Ollama）
 * - 模型名（带常见模型下拉建议，可自定义输入）
 * - Base URL（可覆盖默认，用于代理/兼容服务）
 * - API Key（存 OS Keyring，非明文落盘）
 * - 流式输出开关
 *
 * 数据流：
 * - 加载：getConfig（SQLite 非敏感字段）+ getApiKey（Keyring）
 * - 保存：saveConfig（同时写入 SQLite + Keyring + 应用到 ChatProvider）
 */
import { computed, onMounted, ref, watch } from 'vue'
import {
  PROVIDER_OPTIONS,
  type LlmProvider,
  type LlmProviderConfig,
} from '@/types/settings'
import * as settingsService from '@/services/settings'

/** 表单状态（v-model 绑定） */
const provider = ref<LlmProvider>('openai')
const model = ref('')
const baseUrl = ref('')
const apiKey = ref('')
/** 是否流式（默认 true） */
const stream = ref(true)

/** 加载状态 */
const loading = ref(false)
/** 保存状态 */
const saving = ref(false)
/** 错误/成功提示 */
const errorMsg = ref('')
const successMsg = ref('')

/**
 * 根据当前 provider 计算选项信息
 *
 * 使用 computed 避免重复查找；切换 provider 时自动响应。
 */
const currentOption = computed(() => {
  return PROVIDER_OPTIONS.find((o) => o.value === provider.value) ?? PROVIDER_OPTIONS[0]
})

/**
 * API Key 输入框占位符
 *
 * - 已存在 Key：显示 "已配置（留空保持不变）"
 * - 未配置：显示 "请输入 API Key"
 */
const apiKeyPlaceholder = computed(() => {
  return hasExistingApiKey.value ? '已配置（留空保持不变）' : '请输入 API Key'
})

/** 是否已存在 API Key（从 Keyring 读取的标志） */
const hasExistingApiKey = ref(false)

/**
 * 切换 Provider 时：
 * - 重置 model 为该 provider 的首个默认模型
 * - 重置 baseUrl 为该 provider 的默认值（或清空）
 * - 若无需 API Key（如 ollama），清空 apiKey 输入
 */
watch(provider, (newProvider) => {
  const opt = PROVIDER_OPTIONS.find((o) => o.value === newProvider)
  if (!opt) return
  // 只在 model 为空或属于其他 provider 的模型时重置
  if (!model.value || !opt.models.includes(model.value)) {
    model.value = opt.models[0] ?? ''
  }
  baseUrl.value = opt.defaultBaseUrl ?? ''
  if (!opt.needsApiKey) {
    apiKey.value = ''
  }
})

/**
 * 加载已持久化的配置
 *
 * 同时读取：
 * - 非敏感字段（SQLite）：provider/model/baseUrl/stream
 * - API Key（Keyring）：仅标记"是否存在"，不回填明文（安全考虑）
 *
 * 失败不阻断：让用户能在空表单上重新填写。
 */
async function loadConfig() {
  loading.value = true
  errorMsg.value = ''
  try {
    const config = await settingsService.getConfig()
    if (config) {
      provider.value = config.provider
      model.value = config.model
      baseUrl.value = config.baseUrl ?? ''
      stream.value = config.stream
    }

    // 单独读取 API Key 是否存在（不回填明文到输入框）
    const existingKey = await settingsService.getApiKey()
    hasExistingApiKey.value = !!existingKey
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    loading.value = false
  }
}

/**
 * 保存配置
 *
 * API Key 语义：
 * - 输入框非空：写入新值
 * - 输入框为空 + 已有旧值：undefined（保持不变）
 * - 输入框为空 + 无旧值：undefined（保持不变）
 *
 * 保存成功后短暂显示成功提示，2 秒后清除。
 */
async function save() {
  saving.value = true
  errorMsg.value = ''
  successMsg.value = ''
  try {
    // 构造 API Key 字段：空输入 → undefined（保持现状）
    const apiKeyPayload =
      apiKey.value.length > 0 ? apiKey.value : undefined

    const config: LlmProviderConfig = {
      provider: provider.value,
      model: model.value.trim(),
      baseUrl: baseUrl.value.trim() || null,
      apiKey: apiKeyPayload,
      stream: stream.value,
    }

    await settingsService.saveConfig(config)

    // 标记已有 API Key（若刚保存了非空值）
    if (apiKeyPayload) {
      hasExistingApiKey.value = true
      // 清空输入框（避免明文常驻）
      apiKey.value = ''
    }

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

onMounted(() => {
  loadConfig()
})
</script>

<template>
  <div class="settings-view">
    <header class="header">
      <h2>设置</h2>
      <p class="hint">配置 AI 助手的 LLM Provider。API Key 加密存储于系统凭据库。</p>
    </header>

    <div v-if="loading" class="loading">加载中…</div>

    <form v-else class="form" @submit.prevent="save">
      <!-- Provider 类型 -->
      <div class="field">
        <label class="label">Provider</label>
        <select v-model="provider" class="control">
          <option v-for="opt in PROVIDER_OPTIONS" :key="opt.value" :value="opt.value">
            {{ opt.label }}
          </option>
        </select>
        <p class="field-hint">{{ currentOption.hint }}</p>
      </div>

      <!-- 模型 -->
      <div class="field">
        <label class="label">模型</label>
        <input
          v-model="model"
          list="model-suggestions"
          class="control"
          placeholder="如 gpt-4o"
        />
        <!-- 常见模型建议（可输入自定义值） -->
        <datalist id="model-suggestions">
          <option v-for="m in currentOption.models" :key="m" :value="m" />
        </datalist>
      </div>

      <!-- Base URL -->
      <div class="field">
        <label class="label">Base URL（可选）</label>
        <input
          v-model="baseUrl"
          class="control"
          placeholder="留空使用官方默认；或填入代理/兼容服务地址"
        />
      </div>

      <!-- API Key -->
      <div class="field">
        <label class="label">
          API Key
          <span v-if="currentOption.needsApiKey" class="required">*</span>
          <span v-else class="optional">（此 Provider 不需要）</span>
        </label>
        <input
          v-model="apiKey"
          type="password"
          class="control"
          :placeholder="apiKeyPlaceholder"
          :disabled="!currentOption.needsApiKey"
          autocomplete="off"
        />
        <p v-if="hasExistingApiKey" class="field-hint success">
          ✓ 已配置 API Key（留空保存将保持不变）
        </p>
      </div>

      <!-- 流式 -->
      <div class="field field-inline">
        <label class="label">流式输出</label>
        <input v-model="stream" type="checkbox" class="checkbox" />
        <span class="inline-hint">逐 token 推送响应（推荐，体验更佳）</span>
      </div>

      <!-- 错误/成功提示 -->
      <p v-if="errorMsg" class="error">{{ errorMsg }}</p>
      <p v-if="successMsg" class="success">{{ successMsg }}</p>

      <!-- 动作区 -->
      <div class="actions">
        <button type="submit" class="btn primary" :disabled="saving">
          {{ saving ? '保存中…' : '保存配置' }}
        </button>
        <button type="button" class="btn" @click="loadConfig" :disabled="loading">
          重新加载
        </button>
      </div>
    </form>
  </div>
</template>

<style scoped>
.settings-view {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px;
  background: var(--bg-primary, #1e1e1e);
  color: var(--text-primary, #ddd);
}
.header {
  margin-bottom: 24px;
}
.header h2 {
  margin: 0 0 4px 0;
  font-size: 20px;
  font-weight: 600;
  color: var(--text-primary, #ddd);
}
.hint {
  margin: 0;
  font-size: 13px;
  color: var(--text-secondary, #888);
}
.loading {
  padding: 32px;
  text-align: center;
  color: var(--text-secondary, #888);
}
.form {
  max-width: 560px;
  display: flex;
  flex-direction: column;
  gap: 20px;
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
  gap: 12px;
  margin-top: 8px;
}
.btn {
  padding: 8px 20px;
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
</style>
