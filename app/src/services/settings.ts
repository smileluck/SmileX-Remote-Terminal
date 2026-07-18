/**
 * 设置服务（对接后端 ai_config_* / ai_secret_* commands）
 *
 * 职责：
 * - 读取/保存 LLM 配置（非敏感字段 ↔ SQLite）
 * - 读取/回填 API Key（Keyring）
 */

import { invoke } from './invoke'
import type { LlmProviderConfig } from '@/types/settings'

/**
 * 读取持久化的 LLM 配置（不含 API Key）
 *
 * 后端：`ai_config_get`
 * 返回 `null` 表示未配置过。
 */
export async function getConfig(): Promise<LlmProviderConfig | null> {
  return invoke<LlmProviderConfig | null>('ai_config_get')
}

/**
 * 持久化 LLM 配置
 *
 * 后端：`ai_config_save`
 *
 * apiKey 字段语义：
 * - `undefined`：保持现有 Keyring 不变
 * - `''`（空字符串）：清除 Keyring
 * - 非空字符串：写入 Keyring
 *
 * 保存后后端自动应用到 ChatProvider。
 */
export async function saveConfig(config: LlmProviderConfig): Promise<void> {
  return invoke<void>('ai_config_save', { config })
}

/**
 * 读取 API Key（设置页回填用）
 *
 * 后端：`ai_secret_get`
 * 返回 `null` 表示 Keyring 中无记录。
 *
 * 安全提示：仅在用户主动打开设置页时调用，避免敏感数据常驻内存。
 */
export async function getApiKey(): Promise<string | null> {
  return invoke<string | null>('ai_secret_get')
}
