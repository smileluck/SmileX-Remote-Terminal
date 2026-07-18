/**
 * 会话/Tab 类型定义
 */

/** 会话类型 */
export type SessionKind = 'ssh' | 'rdp' | 'host' | 'chat' | 'settings'

/** 标签数据 */
export interface TabItem {
  /** 标签 ID */
  id: string
  /** 会话类型 */
  kind: SessionKind
  /** 显示标题 */
  title: string
  /** 关联的会话 ID（建立连接后填入） */
  sessionId?: string
  /** 是否正在连接 */
  connecting?: boolean
  /** 错误信息 */
  error?: string
}

/** SSH 认证方式 */
export type AuthMethod =
  | { type: 'password'; value: string }
  | { type: 'private_key'; value: { path: string; passphrase?: string } }
  | { type: 'private_key_mem'; value: { keyData: string; passphrase?: string } }

/** SSH 连接配置 */
export interface SshConfig {
  host: string
  port: number
  username: string
  auth: AuthMethod
  acceptFirstHostKey?: boolean
}

/** 远程桌面协议类型 */
export type DesktopKind = 'rdp' | 'host'

/** 远程桌面配置 */
export interface DesktopConfig {
  kind: DesktopKind
  host: string
  port: number
  username: string
  password: string
  width: number
  height: number
  colorDepth: number
}
