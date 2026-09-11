/**
 * 会话配置 CRUD 服务封装
 *
 * 对接后端 `session_profile_*` commands。
 * 敏感字段单独通过 `getSecret` 拉取（仅连接时）。
 */

import { invoke } from './invoke'
import type { ScannedHost, SessionProfile } from '@/types/profile'

/** 保存或更新会话配置（含敏感字段同步到 Keyring） */
export async function save(
  profile: SessionProfile,
  secret?: string | null,
): Promise<SessionProfile> {
  return invoke<SessionProfile>('session_profile_save', { profile, secret })
}

/** 列出所有会话配置（按最近使用排序） */
export async function list(): Promise<SessionProfile[]> {
  return invoke<SessionProfile[]>('session_profile_list')
}

/** 按 id 获取会话配置（不返回敏感字段） */
export async function get(id: string): Promise<SessionProfile | null> {
  return invoke<SessionProfile | null>('session_profile_get', { id })
}

/** 按 id 获取会话配置的敏感字段（密码/私钥口令） */
export async function getSecret(id: string): Promise<string | null> {
  return invoke<string | null>('session_profile_get_secret', { id })
}

/** 删除会话配置（同时清理 SQLite + Keyring） */
export async function remove(id: string): Promise<boolean> {
  return invoke<boolean>('session_profile_delete', { id })
}

/** 标记会话为最近使用（连接成功后调用，影响侧栏排序） */
export async function touch(id: string): Promise<void> {
  return invoke<void>('session_profile_touch', { id })
}

/** 扫描本机 ~/.ssh/known_hosts，返回可导入的主机列表（哈希条目无法还原已跳过） */
export async function scanKnownHosts(): Promise<ScannedHost[]> {
  return invoke<ScannedHost[]>('known_hosts_scan')
}
