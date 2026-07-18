/**
 * 设置模块类型定义
 *
 * 阶段 6：多 LLM 配置档案管理。
 * 与后端 `desktop::storage::sqlite::LlmProfile` 对齐（camelCase）。
 */

/** LLM Provider 类型（与后端枚举小写序列化对齐） */
export type LlmProvider = 'openai' | 'claude' | 'ollama'

/** LLM 配置（LlmProfile 的核心字段，用于构造请求） */
export interface LlmProviderConfig {
  /** Provider 类型 */
  provider: LlmProvider
  /** 模型名 */
  model: string
  /** API Base URL */
  baseUrl?: string | null
  /** API Key */
  apiKey?: string | null
  /** 是否流式 */
  stream: boolean
}

/** LLM 配置档案（多档案管理）
 *
 * 与后端 SQLite `llm_profiles` 表对应。
 * 注：API Key 不在此结构内，单独通过 Keyring 接口读写。
 */
export interface LlmProfile {
  /** UUID */
  id: string
  /** 显示名称（用户可读） */
  name: string
  /** Provider 类型 */
  provider: LlmProvider
  /** 模型名 */
  model: string
  /** Base URL（留空用默认） */
  baseUrl?: string | null
  /** 是否流式输出 */
  stream: boolean
  /** 是否为激活档案 */
  isActive: boolean
  /** 创建时间戳（unix 秒） */
  createdAt: number
  /** 更新时间戳（unix 秒） */
  updatedAt: number
}

/** Provider 选项（UI 下拉用） */
export interface ProviderOption {
  /** Provider 值 */
  value: LlmProvider
  /** 显示标签 */
  label: string
  /** 默认模型列表（select 下拉用） */
  models: string[]
  /** 是否需要 API Key */
  needsApiKey: boolean
  /** 默认 BaseURL（留空表示用官方默认） */
  defaultBaseUrl?: string
  /** 帮助文案 */
  hint: string
}

/** 可选 Provider 列表（设置页下拉用）
 *
 * 包含主流 LLM 服务及国产兼容服务预设。
 */
export const PROVIDER_OPTIONS: ProviderOption[] = [
  {
    value: 'openai',
    label: 'OpenAI',
    models: ['gpt-4o', 'gpt-4o-mini', 'gpt-4-turbo', 'gpt-3.5-turbo', 'o1-preview', 'o1-mini'],
    needsApiKey: true,
    defaultBaseUrl: '',
    hint: 'OpenAI 官方 API',
  },
  {
    value: 'claude',
    label: 'Anthropic Claude',
    models: ['claude-3-5-sonnet', 'claude-3-5-haiku', 'claude-3-opus', 'claude-3-sonnet', 'claude-3-haiku'],
    needsApiKey: true,
    defaultBaseUrl: '',
    hint: 'Anthropic Claude（当前用 OpenAI 兼容占位，阶段 7 接入原生 API）',
  },
  {
    value: 'ollama',
    label: 'Ollama（本地）',
    models: ['qwen2.5:7b', 'qwen2.5:14b', 'llama3.1:8b', 'llama3.1:70b', 'deepseek-r1:7b', 'deepseek-r1:14b'],
    needsApiKey: false,
    defaultBaseUrl: 'http://localhost:11434',
    hint: '本地 Ollama 服务，无需 API Key',
  },
]

/**
 * 按 provider 值查找选项
 *
 * 找不到时回退到首个（OpenAI），保证调用方始终拿到合法值。
 */
export function findProviderOption(value: LlmProvider | string): ProviderOption {
  return (
    PROVIDER_OPTIONS.find((o) => o.value === value) ?? PROVIDER_OPTIONS[0]
  )
}

/**
 * 生成默认的新 profile（前端新建时调用）
 *
 * 返回的字段 id 由调用方填充（UUID），其它为合理默认值。
 */
export function createDefaultProfileFields(provider: LlmProvider = 'openai'): Omit<LlmProfile, 'id'> {
  const opt = findProviderOption(provider)
  return {
    name: '',
    provider,
    model: opt.models[0] ?? '',
    baseUrl: opt.defaultBaseUrl ?? '',
    stream: true,
    isActive: false,
    createdAt: 0,
    updatedAt: 0,
  }
}
