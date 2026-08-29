/**
 * useSshConnect - 按 SessionProfile 建立 SSH 连接
 *
 * 从 SideBar / TerminalView（重连）复用：
 * 取 Keyring 敏感字段 → 构造 ConnectionConfig → session_connect
 */
import * as profileService from '@/services/profile'
import * as sessionService from '@/services/session'
import { decodeExtra } from '@/types/profile'
import type { SessionProfile } from '@/types/profile'
import type { SshConfig } from '@/types/session'

/** 由档案构造连接配置（含 Keyring 敏感字段） */
export async function buildConfig(profile: SessionProfile): Promise<SshConfig> {
  const secret = await profileService.getSecret(profile.id)
  const authPayload =
    profile.auth_type === 'password'
      ? { type: 'password', value: secret || '' }
      : {
          type: 'private_key',
          value: {
            path: decodeExtra(profile.extra).private_key_path || '',
            passphrase: secret || undefined,
          },
        }
  return {
    host: profile.host,
    port: profile.port,
    username: profile.username,
    auth: authPayload as never,
    acceptFirstHostKey: decodeExtra(profile.extra).accept_first_host_key ?? false,
  }
}

/** 连接档案对应的主机，返回 sessionId */
export async function connectProfile(profile: SessionProfile): Promise<string> {
  const config = await buildConfig(profile)
  return sessionService.connect(config, 80, 24)
}
