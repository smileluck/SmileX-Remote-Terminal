/**
 * SSH 密钥管理封装
 *
 * 对接后端 `ssh_key_*` commands。私钥存 OS Keyring，前端仅在连接时按需读取。
 */

import { invoke } from './invoke'

/** 密钥元数据（不含私钥） */
export interface SshKeyMeta {
  id: string
  name: string
  keyType: string
  publicKey: string
  fingerprint: string
  createdAt: number
}

export function listKeys(): Promise<SshKeyMeta[]> {
  return invoke<SshKeyMeta[]>('ssh_key_list')
}

export function generateKey(name: string, keyType: 'ed25519' | 'rsa'): Promise<SshKeyMeta> {
  return invoke<SshKeyMeta>('ssh_key_generate', { name, keyType })
}

export function importKey(
  name: string,
  privatePem: string,
  passphrase?: string,
): Promise<SshKeyMeta> {
  return invoke<SshKeyMeta>('ssh_key_import', { name, privatePem, passphrase: passphrase ?? null })
}

export function deleteKey(id: string): Promise<boolean> {
  return invoke<boolean>('ssh_key_delete', { id })
}

/** 读取私钥（连接时使用，仅内存） */
export function getPrivateKey(id: string): Promise<string | null> {
  return invoke<string | null>('ssh_key_get_private', { id })
}
