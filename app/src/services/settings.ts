/**
 * LLM 配置档案服务（对接后端 llm_profile_* commands）
 *
 * 阶段 6：多档案管理。
 * - 非敏感字段（name/provider/model/baseUrl/stream）↔ SQLite
 * - API Key ↔ OS Keyring（单独接口）
 */

import { invoke } from './invoke'
import type { LlmProfile } from '@/types/settings'

/**
 * 列出所有 LLM 配置档案（不含 API Key）
 *
 * 后端：`llm_profile_list`
 */
export async function listProfiles(): Promise<LlmProfile[]> {
  return invoke<LlmProfile[]>('llm_profile_list')
}

/**
 * 按 id 获取单个档案（不含 API Key）
 *
 * 后端：`llm_profile_get`
 */
export async function getProfile(id: string): Promise<LlmProfile | null> {
  return invoke<LlmProfile | null>('llm_profile_get', { id })
}

/**
 * 读取指定档案的 API Key（编辑时回填用）
 *
 * 后端：`llm_profile_get_api_key`
 *
 * 返回 `null` 表示未配置 API Key。
 *
 * 安全提示：仅在用户主动编辑时调用，避免敏感数据常驻内存。
 */
export async function getApiKey(id: string): Promise<string | null> {
  return invoke<string | null>('llm_profile_get_api_key', { id })
}

/**
 * 保存（新建或更新）LLM 配置档案
 *
 * 后端：`llm_profile_save`
 *
 * apiKey 参数三态语义：
 * - `undefined`：保持现状（更新时使用）
 * - `''`（空字符串）：清除 Keyring
 * - 非空字符串：写入 Keyring
 *
 * 若 `profile.isActive === true`，保存后后端会自动应用到 ChatProvider。
 */
export async function saveProfile(
  profile: LlmProfile,
  apiKey?: string,
): Promise<LlmProfile> {
  return invoke<LlmProfile>('llm_profile_save', { profile, apiKey })
}

/**
 * 删除档案（同时清理 SQLite + Keyring）
 *
 * 后端：`llm_profile_delete`
 */
export async function deleteProfile(id: string): Promise<boolean> {
  return invoke<boolean>('llm_profile_delete', { id })
}

/**
 * 设置激活档案（排他性：自动取消其他 active）
 *
 * 后端：`llm_profile_set_active`
 *
 * 同时应用到 ChatProvider，立即生效。
 */
export async function setActiveProfile(id: string): Promise<void> {
  return invoke<void>('llm_profile_set_active', { id })
}

/**
 * 获取当前激活的档案（不含 API Key）
 *
 * 后端：`llm_profile_get_active`
 */
export async function getActiveProfile(): Promise<LlmProfile | null> {
  return invoke<LlmProfile | null>('llm_profile_get_active')
}

/**
 * 测试连通性
 *
 * 后端：`llm_profile_test`
 *
 * 返回首个 token（证明链路可用）。
 * 超时（10s）或失败会抛出错误。
 */
export async function testProfile(id: string): Promise<string> {
  return invoke<string>('llm_profile_test', { id })
}
