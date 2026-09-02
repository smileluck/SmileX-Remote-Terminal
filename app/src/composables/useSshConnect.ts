/**
 * useSshConnect - 按 SessionProfile 建立 SSH 连接
 *
 * 从 SideBar / TerminalView（重连）复用：
 * 取 Keyring 敏感字段 → 构造 ConnectionConfig → session_connect
 */
import * as profileService from '@/services/profile'
import * as sessionService from '@/services/session'
import { getPrivateKey } from '@/services/sshKeys'
import { decodeExtra } from '@/types/profile'
import type { SessionProfile } from '@/types/profile'
import type { SshConfig } from '@/types/session'

/** 由档案构造连接配置（含 Keyring 敏感字段；secretOverride 用于未保存档案的临时连接） */
export async function buildConfig(
  profile: SessionProfile,
  secretOverride?: string | null,
): Promise<SshConfig> {
  const secret = secretOverride !== undefined ? secretOverride : await profileService.getSecret(profile.id)
  const extra = decodeExtra(profile.extra)
  let authPayload
  if (profile.auth_type === 'password') {
    // 空密码快速失败：避免静默发送空串后被服务器以"认证被拒"误导排查方向
    if (!secret) {
      throw new Error('未设置密码：请编辑会话填写密码后重试')
    }
    authPayload = { type: 'password', value: secret }
  } else if (profile.auth_type === 'private_key_mem' && extra.ssh_key_id) {
    // 密钥管理器中的密钥：从 OS Keyring 读取私钥，内存传递
    const keyData = await getPrivateKey(extra.ssh_key_id)
    if (!keyData) throw new Error('密钥不存在或已删除，请重新编辑会话认证方式')
    authPayload = { type: 'private_key_mem', value: { key_data: keyData, passphrase: secret || undefined } }
  } else {
    authPayload = {
      type: 'private_key',
      value: {
        path: extra.private_key_path || '',
        passphrase: secret || undefined,
      },
    }
  }
  return {
    host: profile.host,
    port: profile.port,
    username: profile.username,
    auth: authPayload as never,
    acceptFirstHostKey: extra.accept_first_host_key ?? false,
  }
}

/** 连接档案对应的主机，返回 sessionId */
export async function connectProfile(profile: SessionProfile): Promise<string> {
  const config = await buildConfig(profile)
  return sessionService.connect(config, 80, 24)
}
