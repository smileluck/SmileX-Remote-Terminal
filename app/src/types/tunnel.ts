/**
 * 端口转发隧道类型定义
 *
 * 与后端 `crates/ssh-core/src/tunnel.rs` 对齐：
 * - TunnelConfig 字段为 snake_case（Tauri 只转换顶层参数名，不递归转换嵌套字段）
 * - kind 取值为 snake_case 字符串
 *
 * 字段语义按 kind 区分（与 OpenSSH -L/-R/-D 一致）：
 * - local：local_host:local_port 本地监听 → remote_host:remote_port 远端目标
 * - remote：remote_host:remote_port 远端监听（port 0 = 服务器分配，启动后回填）
 *          → local_host:local_port 本地目标
 * - dynamic：local_host:local_port 本地 SOCKS5 监听，remote_* 不使用
 */

/** 隧道类型 */
export type TunnelKind = 'local' | 'remote' | 'dynamic'

/** 隧道配置（ProfileExtra.tunnels 与 tunnel_start 入参共用） */
export interface TunnelConfig {
  /** 隧道 ID（空串由后端生成 UUID；档案内配置在编辑时预生成） */
  id: string
  /** 隧道类型 */
  kind: TunnelKind
  /** 本地地址（local/dynamic=监听地址；remote=本地目标地址） */
  local_host: string
  /** 本地端口 */
  local_port: number
  /** 远端地址（local=远端目标地址；remote=远端监听地址；dynamic 不使用） */
  remote_host: string
  /** 远端端口 */
  remote_port: number
}

/** 隧道运行状态（后端 TunnelStatus::as_str） */
export type TunnelState = 'running' | 'stopped' | 'failed'

/** 隧道列表项（tunnel_list 返回，snake_case） */
export interface TunnelInfo {
  /** 隧道 ID */
  id: string
  /** 所属会话 ID */
  session_id: string
  /** 隧道配置（remote 端口 0 时已回填分配端口） */
  config: TunnelConfig
  /** 状态串 */
  state: TunnelState
  /** 失败原因（仅 failed 时有值） */
  error?: string | null
}

/** `tunnel_event` 事件 payload（camelCase，对应 events.rs TunnelEventPayload） */
export interface TunnelEventPayload {
  /** 隧道 ID */
  tunnelId: string
  /** 会话 ID */
  sessionId: string
  /** 状态串 */
  state: TunnelState
  /** 失败原因（仅 failed 时有值） */
  error?: string | null
}

/** 创建空隧道配置（新建表单/档案编辑的初始行） */
export function emptyTunnelConfig(kind: TunnelKind = 'local'): TunnelConfig {
  return {
    id: crypto.randomUUID(),
    kind,
    local_host: '127.0.0.1',
    local_port: kind === 'dynamic' ? 1080 : 8080,
    remote_host: '127.0.0.1',
    remote_port: 80,
  }
}
