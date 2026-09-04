/**
 * 设置模块类型定义
 *
 * 阶段 6：多 LLM 配置档案管理。
 * 与后端 `desktop::storage::sqlite::LlmProfile` 对齐（camelCase）。
 *
 * 厂商预设（provider）与接入方式（authMode）分离：
 * 国内厂商的「按量 API」走 OpenAI 兼容端点，「Coding Plan 订阅」走
 * Anthropic 兼容端点，两者 Base URL / 协议 / API Key 常不通用。
 */

/** LLM 厂商预设（与后端 LlmProvider 枚举小写序列化对齐） */
export type LlmProvider =
  | 'openai'
  | 'claude'
  | 'ollama'
  | 'zhipu'
  | 'deepseek'
  | 'moonshot'
  | 'qwen'
  | 'minimax'

/** 接入方式（与后端 LlmAuthMode 枚举 snake_case 序列化对齐） */
export type LlmAuthMode = 'api' | 'coding_plan'

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
  /** 接入方式（国内厂商预设使用；null 表示不区分） */
  authMode?: LlmAuthMode | null
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
  /** 接入方式：api（按量）/ coding_plan（订阅）；null 表示不区分（旧数据或通用预设） */
  authMode?: LlmAuthMode | null
  /** 是否流式输出 */
  stream: boolean
  /** 是否为激活档案 */
  isActive: boolean
  /** 创建时间戳（unix 秒） */
  createdAt: number
  /** 更新时间戳（unix 秒） */
  updatedAt: number
}

/** 接入方式选项（Provider 内的一种接入方式） */
export interface AccessModeOption {
  /** 接入方式值（null 表示该预设不区分接入方式，仅一种） */
  value: LlmAuthMode | null
  /** 显示标签（如「标准 API（按量计费）」） */
  label: string
  /** 默认 Base URL（空串表示留空用官方默认） */
  defaultBaseUrl: string
  /** Base URL 输入框占位提示（defaultBaseUrl 为空模板时使用） */
  baseUrlPlaceholder?: string
  /** 预设模型列表（select 下拉用，可自定义输入） */
  models: string[]
  /** 帮助文案（说明计费方式与 Key 签发平台） */
  hint: string
}

/** Provider 选项（UI 下拉用） */
export interface ProviderOption {
  /** Provider 值 */
  value: LlmProvider
  /** 显示标签 */
  label: string
  /** 下拉分组 */
  group: '通用' | '国内厂商'
  /** 是否需要 API Key */
  needsApiKey: boolean
  /** 可选接入方式（多项时 UI 显示切换；单项即固定） */
  accessModes: AccessModeOption[]
}

/** 可选 Provider 列表（设置页下拉用）
 *
 * 通用预设（OpenAI/Claude/Ollama）单一接入方式；
 * 国内厂商预设区分「标准 API（按量）」与「Coding Plan（订阅）」，
 * 两者端点与协议不同，切换时自动填充 Base URL 与模型列表。
 */
export const PROVIDER_OPTIONS: ProviderOption[] = [
  {
    value: 'openai',
    label: 'OpenAI',
    group: '通用',
    needsApiKey: true,
    accessModes: [
      {
        value: null,
        label: '',
        defaultBaseUrl: '',
        models: ['gpt-4o', 'gpt-4o-mini', 'gpt-4-turbo', 'gpt-3.5-turbo', 'o1-preview', 'o1-mini'],
        hint: 'OpenAI 官方 API；也可填 Base URL 接入任意 OpenAI 兼容服务',
      },
    ],
  },
  {
    value: 'claude',
    label: 'Anthropic Claude',
    group: '通用',
    needsApiKey: true,
    accessModes: [
      {
        value: null,
        label: '',
        defaultBaseUrl: '',
        models: ['claude-sonnet-4-5', 'claude-3-5-sonnet', 'claude-3-5-haiku', 'claude-3-opus', 'claude-3-haiku'],
        hint: 'Anthropic 原生 Messages API（官方 API Key）',
      },
    ],
  },
  {
    value: 'ollama',
    label: 'Ollama（本地）',
    group: '通用',
    needsApiKey: false,
    accessModes: [
      {
        value: null,
        label: '',
        defaultBaseUrl: 'http://localhost:11434',
        models: ['qwen2.5:7b', 'qwen2.5:14b', 'llama3.1:8b', 'llama3.1:70b', 'deepseek-r1:7b', 'deepseek-r1:14b'],
        hint: '本地 Ollama 服务，无需 API Key',
      },
    ],
  },
  {
    value: 'zhipu',
    label: '智谱 GLM',
    group: '国内厂商',
    needsApiKey: true,
    accessModes: [
      {
        value: 'api',
        label: '标准 API（按量计费）',
        defaultBaseUrl: 'https://open.bigmodel.cn/api/paas/v4',
        models: ['glm-4.6', 'glm-4.5-air', 'glm-4-flash'],
        hint: '按量计费。在 open.bigmodel.cn 开放平台申请 API Key',
      },
      {
        value: 'coding_plan',
        label: 'GLM Coding Plan（订阅）',
        defaultBaseUrl: 'https://open.bigmodel.cn/api/anthropic',
        models: ['glm-4.6', 'glm-4.5-air'],
        hint: '编程订阅套餐（Lite/Pro/Max）。请在「编程套餐」页面新建专用 Key；走 Anthropic 兼容端点',
      },
    ],
  },
  {
    value: 'deepseek',
    label: 'DeepSeek',
    group: '国内厂商',
    needsApiKey: true,
    accessModes: [
      {
        value: 'api',
        label: '标准 API（按量计费）',
        defaultBaseUrl: 'https://api.deepseek.com/v1',
        models: ['deepseek-chat', 'deepseek-reasoner'],
        hint: '按量计费。在 platform.deepseek.com 申请 API Key',
      },
      {
        value: 'coding_plan',
        label: 'Anthropic 兼容端点（按量）',
        defaultBaseUrl: 'https://api.deepseek.com/anthropic',
        models: ['deepseek-chat', 'deepseek-reasoner'],
        hint: 'Anthropic 兼容端点，仍按量计费，与标准 API 使用同一 Key（无独立订阅套餐）',
      },
    ],
  },
  {
    value: 'moonshot',
    label: 'Kimi（月之暗面）',
    group: '国内厂商',
    needsApiKey: true,
    accessModes: [
      {
        value: 'api',
        label: '标准 API（按量计费）',
        defaultBaseUrl: 'https://api.moonshot.cn/v1',
        models: ['kimi-k2-0905-preview', 'moonshot-v1-128k', 'moonshot-v1-32k', 'moonshot-v1-8k'],
        hint: '按量计费。在开放平台（platform.moonshot.cn）申请 API Key',
      },
      {
        value: 'coding_plan',
        label: 'Kimi Coding Plan（订阅）',
        defaultBaseUrl: 'https://api.moonshot.ai/anthropic',
        models: ['kimi-k3', 'kimi-k2-turbo-preview', 'kimi-k2-0905-preview'],
        hint: '独立订阅套餐，Key 在 platform.kimi.ai 获取——与按量 API 的 Key 不通用！走 Anthropic 兼容端点',
      },
    ],
  },
  {
    value: 'qwen',
    label: '通义千问（百炼）',
    group: '国内厂商',
    needsApiKey: true,
    accessModes: [
      {
        value: 'api',
        label: '标准 API（按量计费）',
        defaultBaseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
        models: ['qwen3-coder-plus', 'qwen-max', 'qwen-plus', 'qwen-turbo'],
        hint: '按量计费。在阿里云百炼控制台申请 API Key',
      },
      {
        value: 'coding_plan',
        label: '百炼 Coding Plan（订阅）',
        defaultBaseUrl: '',
        baseUrlPlaceholder: 'https://{业务空间ID}.cn-beijing.maas.aliyuncs.com/apps/anthropic',
        models: ['qwen3-coder-plus', 'qwen-max', 'qwen-plus'],
        hint: '订阅套餐，走 Anthropic 兼容端点。Base URL 中的业务空间 ID 需替换为你的百炼工作空间 ID（见输入框提示格式）',
      },
    ],
  },
  {
    value: 'minimax',
    label: 'MiniMax',
    group: '国内厂商',
    needsApiKey: true,
    accessModes: [
      {
        value: 'api',
        label: '标准 API（按量计费）',
        defaultBaseUrl: 'https://api.minimaxi.com/v1',
        models: ['MiniMax-M3', 'MiniMax-M2.7', 'MiniMax-M2'],
        hint: '按量计费。在 MiniMax 开放平台（国内站）申请 API Key',
      },
      {
        value: 'coding_plan',
        label: 'MiniMax Coding Plan（订阅）',
        defaultBaseUrl: 'https://api.minimaxi.com/anthropic',
        models: ['MiniMax-M3', 'MiniMax-M2.7', 'MiniMax-M2'],
        hint: '订阅套餐，走国内站 Anthropic 兼容端点；国际站为 api.minimax.io/anthropic',
      },
    ],
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
 * 查找 provider 的接入方式选项
 *
 * authMode 为 null / 不匹配时回退首个（国内厂商默认「标准 API」）。
 */
export function findAccessMode(provider: LlmProvider | string, authMode?: LlmAuthMode | null): AccessModeOption {
  const opt = findProviderOption(provider)
  return (
    opt.accessModes.find((m) => m.value != null && m.value === authMode) ?? opt.accessModes[0]
  )
}

/** 该 provider 是否需要「接入方式」切换（多于一种模式） */
export function hasAccessModeSwitch(provider: LlmProvider | string): boolean {
  return findProviderOption(provider).accessModes.length > 1
}

/**
 * 生成默认的新 profile（前端新建时调用）
 *
 * 返回的字段 id 由调用方填充（UUID），其它为合理默认值。
 * 国内厂商默认取首个接入方式（标准 API）。
 */
export function createDefaultProfileFields(provider: LlmProvider = 'openai'): Omit<LlmProfile, 'id'> {
  const opt = findProviderOption(provider)
  const mode = opt.accessModes[0]
  return {
    name: '',
    provider,
    model: mode.models[0] ?? '',
    baseUrl: mode.defaultBaseUrl,
    authMode: opt.accessModes.length > 1 ? mode.value : null,
    stream: true,
    isActive: false,
    createdAt: 0,
    updatedAt: 0,
  }
}
