/**
 * 会话配置（SessionProfile）类型定义
 *
 * 对应后端 `crates/desktop/src/storage/sqlite.rs::SessionProfile`。
 * 敏感字段（密码/私钥口令）不在此结构中，单独通过 Keyring 存取。
 */

import type { TunnelConfig } from './tunnel'

/** 会话种类 */
export type ProfileKind = 'ssh' | 'rdp' | 'host'

/** 认证方式 */
export type AuthType = 'password' | 'private_key' | 'private_key_mem'

/** extra 字段（JSON 字符串解析后的结构） */
export interface ProfileExtra {
  /** 私钥文件路径（仅 auth_type=private_key 时有意义） */
  private_key_path?: string
  /** 引用密钥管理器中的 SSH 密钥 id（auth_type=private_key_mem 时使用） */
  ssh_key_id?: string
  /** 会话分组名（SideBar 分组展示） */
  group?: string
  /** 是否自动接受 host key（仅 SSH） */
  accept_first_host_key?: boolean
  /** 远程桌面分辨率宽（仅 rdp/host） */
  width?: number
  /** 远程桌面分辨率高（仅 rdp/host） */
  height?: number
  /** 远程桌面色深（仅 rdp/host） */
  color_depth?: number
  /** 端口转发隧道配置（仅 ssh；连接成功后自动启动） */
  tunnels?: TunnelConfig[]
}

/** 会话配置（前端可读可写） */
export interface SessionProfile {
  /** 客户端生成的 UUID */
  id: string
  /** 显示名称 */
  name: string
  /** 会话种类 */
  kind: ProfileKind
  /** 目标主机 */
  host: string
  /** 目标端口 */
  port: number
  /** 登录用户名 */
  username: string
  /** 认证方式 */
  auth_type: AuthType
  /** 附加配置 JSON 字符串（解析为 ProfileExtra） */
  extra: string
  /** 创建时间戳（unix 秒） */
  created_at: number
  /** 最近连接时间戳（unix 秒，0 表示从未连接） */
  last_used_at: number
}

/** 保存会话配置时的请求 payload */
export interface SaveProfileRequest {
  /** 会话配置 */
  profile: SessionProfile
  /** 敏感字段（密码/私钥口令）。null/undefined/空字符串表示不更新 */
  secret?: string | null
}

/**
 * 把 ProfileExtra 序列化为 extra 字段所需的 JSON 字符串
 */
export function encodeExtra(extra: ProfileExtra): string {
  return JSON.stringify(extra)
}

/**
 * 把 extra 字段的 JSON 字符串解析为 ProfileExtra（容错：空串返回空对象）
 */
export function decodeExtra(extra: string): ProfileExtra {
  if (!extra) return {}
  try {
    return JSON.parse(extra) as ProfileExtra
  } catch {
    return {}
  }
}
