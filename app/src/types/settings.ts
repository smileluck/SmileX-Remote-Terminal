/**
 * 设置模块类型定义
 *
 * 与后端 `ai_core::LlmProviderConfig`（`#[serde(rename_all = "camelCase")]`）对齐。
 * - provider 序列化为小写（见后端 `#[serde(rename_all = "lowercase")]`）
 * - 字段名 camelCase
 *
 * 注：此模块替代 `types/ai.ts` 中的 `LlmProviderConfig`，统一设置/Chat 模块的类型。
 */

/** LLM Provider 类型（与后端枚举小写序列化对齐） */
export type LlmProvider = 'openai' | 'claude' | 'ollama'

/** LLM 配置（前端 ↔ 后端传输用） */
export interface LlmProviderConfig {
  /** Provider 类型 */
  provider: LlmProvider
  /** 模型名（如 "gpt-4o" / "claude-3-5-sonnet" / "qwen2.5:7b"） */
  model: string
  /** API Base URL（留空用 Provider 默认） */
  baseUrl?: string | null
  /** API Key（保存时携带；读取时为 null） */
  apiKey?: string | null
  /** 是否流式 */
  stream: boolean
}

/** Provider 选项（UI 下拉用） */
export interface ProviderOption {
  /** Provider 值 */
  value: LlmProvider
  /** 显示标签 */
  label: string
  /** 默认模型列表（供用户选择或占位） */
  models: string[]
  /** 是否需要 API Key */
  needsApiKey: boolean
  /** 默认 BaseURL（留空表示用官方默认） */
  defaultBaseUrl?: string
  /** 帮助文案 */
  hint: string
}

/** 可选 Provider 列表（设置页下拉用） */
export const PROVIDER_OPTIONS: ProviderOption[] = [
  {
    value: 'openai',
    label: 'OpenAI 兼容',
    models: ['gpt-4o', 'gpt-4o-mini', 'gpt-4-turbo', 'gpt-3.5-turbo'],
    needsApiKey: true,
    hint: '支持 OpenAI 官方及兼容协议（DeepSeek / 智谱 / 通义千问等）',
  },
  {
    value: 'claude',
    label: 'Anthropic Claude',
    models: ['claude-3-5-sonnet', 'claude-3-5-haiku', 'claude-3-opus'],
    needsApiKey: true,
    hint: 'Anthropic Claude（当前用 OpenAI 兼容占位，阶段 6 接入原生 API）',
  },
  {
    value: 'ollama',
    label: 'Ollama（本地）',
    models: ['qwen2.5:7b', 'llama3.1:8b', 'deepseek-r1:7b'],
    needsApiKey: false,
    defaultBaseUrl: 'http://localhost:11434',
    hint: '本地 Ollama 服务，无需 API Key',
  },
]
