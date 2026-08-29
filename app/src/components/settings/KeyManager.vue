<script setup lang="ts">
/**
 * KeyManager - SSH 密钥管理
 *
 * - 列表：名称 / 类型 / 指纹 / 创建时间 + 复制公钥 / 删除
 * - 生成：Ed25519（推荐）/ RSA 4096
 * - 导入：PEM 私钥文本（可选口令）
 *
 * 私钥存 OS Keyring，前端永不展示明文。
 */
import { onMounted, ref } from 'vue'
import {
  NButton,
  NIcon,
  NInput,
  NRadioGroup,
  NRadioButton,
  NSelect,
  NEmpty,
  NPopconfirm,
  NSpin,
  useMessage,
} from 'naive-ui'
import { Plus, Copy, Trash } from '@vicons/tabler'
import * as sshKeys from '@/services/sshKeys'
import type { SshKeyMeta } from '@/services/sshKeys'

const message = useMessage()

const keys = ref<SshKeyMeta[]>([])
const loading = ref(false)
const busy = ref(false)

/** 生成表单 */
const genName = ref('')
const genType = ref<'ed25519' | 'rsa'>('ed25519')

/** 导入表单 */
const showImport = ref(false)
const importName = ref('')
const importPem = ref('')
const importPassphrase = ref('')

async function load() {
  loading.value = true
  try {
    keys.value = await sshKeys.listKeys()
  } catch (e) {
    message.error(String(e))
  } finally {
    loading.value = false
  }
}

async function onGenerate() {
  if (!genName.value.trim()) {
    message.warning('请输入密钥名称')
    return
  }
  busy.value = true
  try {
    const key = await sshKeys.generateKey(genName.value.trim(), genType.value)
    message.success(`已生成 ${key.keyType} 密钥「${key.name}」`)
    genName.value = ''
    await load()
  } catch (e) {
    message.error(String(e))
  } finally {
    busy.value = false
  }
}

async function onImport() {
  if (!importName.value.trim() || !importPem.value.trim()) {
    message.warning('请填写名称与私钥 PEM 内容')
    return
  }
  busy.value = true
  try {
    const key = await sshKeys.importKey(
      importName.value.trim(),
      importPem.value.trim(),
      importPassphrase.value || undefined,
    )
    message.success(`已导入密钥「${key.name}」`)
    showImport.value = false
    importName.value = ''
    importPem.value = ''
    importPassphrase.value = ''
    await load()
  } catch (e) {
    message.error(`导入失败：${e}`)
  } finally {
    busy.value = false
  }
}

async function copyPublic(key: SshKeyMeta) {
  try {
    await navigator.clipboard.writeText(key.publicKey)
    message.success('公钥已复制')
  } catch {
    message.error('复制失败')
  }
}

async function onDelete(key: SshKeyMeta) {
  try {
    await sshKeys.deleteKey(key.id)
    message.success(`已删除「${key.name}」`)
    await load()
  } catch (e) {
    message.error(String(e))
  }
}

/** 密钥类型 NSelect 选项（供会话认证选择用格式一致） */
const keyOptions = () =>
  keys.value.map((k) => ({ label: `${k.name}（${k.keyType}）`, value: k.id }))

defineExpose({ keyOptions, keys })

onMounted(load)
</script>

<template>
  <div class="key-manager">
    <!-- 生成 -->
    <section class="block">
      <h4 class="block-title">生成新密钥</h4>
      <div class="gen-row">
        <NInput v-model:value="genName" placeholder="密钥名称，如 prod-deploy" style="flex: 1" />
        <NRadioGroup v-model:value="genType" size="small">
          <NRadioButton value="ed25519">Ed25519</NRadioButton>
          <NRadioButton value="rsa">RSA 4096</NRadioButton>
        </NRadioGroup>
        <NButton type="primary" size="small" :loading="busy" @click="onGenerate">
          <template #icon><NIcon :component="Plus" /></template>
          生成
        </NButton>
      </div>
    </section>

    <!-- 导入 -->
    <section class="block">
      <div class="block-title-row">
        <h4 class="block-title">导入私钥</h4>
        <NButton size="tiny" tertiary @click="showImport = !showImport">
          {{ showImport ? '收起' : '展开' }}
        </NButton>
      </div>
      <div v-if="showImport" class="import-form">
        <NInput v-model:value="importName" placeholder="密钥名称" />
        <NInput
          v-model:value="importPem"
          type="textarea"
          :rows="4"
          placeholder="-----BEGIN OPENSSH PRIVATE KEY-----&#10;...&#10;-----END OPENSSH PRIVATE KEY-----"
          style="font-family: var(--font-mono, monospace)"
        />
        <NInput
          v-model:value="importPassphrase"
          type="password"
          show-password-on="click"
          placeholder="私钥口令（可选，若已加密）"
        />
        <NButton size="small" type="primary" :loading="busy" @click="onImport">导入</NButton>
      </div>
    </section>

    <!-- 列表 -->
    <section class="block">
      <h4 class="block-title">已存密钥（{{ keys.length }}）</h4>
      <NSpin v-if="loading" size="small" style="width: 100%; padding: 16px 0" />
      <NEmpty v-else-if="keys.length === 0" size="small" description="暂无密钥" style="padding: 20px 0" />
      <div v-else class="key-list">
        <div v-for="k in keys" :key="k.id" class="key-item">
          <div class="key-main">
            <div class="key-name">
              {{ k.name }}
              <span class="key-type">{{ k.keyType }}</span>
            </div>
            <div class="key-fp" :title="k.publicKey">{{ k.fingerprint }}</div>
          </div>
          <div class="key-actions">
            <NButton text size="tiny" title="复制公钥" @click="copyPublic(k)">
              <NIcon :component="Copy" />
            </NButton>
            <NPopconfirm @positive-click="onDelete(k)">
              <template #trigger>
                <NButton text size="tiny" class="danger" title="删除">
                  <NIcon :component="Trash" />
                </NButton>
              </template>
              删除密钥「{{ k.name }}」？使用此密钥的会话将无法连接。
            </NPopconfirm>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<style scoped>
.key-manager {
  display: flex;
  flex-direction: column;
  gap: 20px;
  max-width: 640px;
}
.block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.block-title {
  margin: 0;
  font-size: 13px;
  font-weight: 600;
  color: var(--text-primary);
}
.block-title-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.gen-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.import-form {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.key-list {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--border-color);
  border-radius: var(--radius-md);
  overflow: hidden;
}
.key-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
}
.key-item + .key-item {
  border-top: 1px solid var(--border-color);
}
.key-main {
  flex: 1;
  min-width: 0;
}
.key-name {
  font-size: 13px;
  font-weight: 500;
  color: var(--text-primary);
}
.key-type {
  font-size: 11px;
  color: var(--text-tertiary);
  margin-left: 6px;
}
.key-fp {
  font-size: 11px;
  color: var(--text-secondary);
  font-family: var(--font-mono, monospace);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 2px;
}
.key-actions {
  display: flex;
  gap: 4px;
}
.key-actions .danger:hover {
  --n-text-color: var(--danger) !important;
}
</style>
