<script setup lang="ts">
/**
 * ConnectForm - SSH 会话新建/编辑表单
 *
 * 功能：
 * - 名称 / 主机 / 端口 / 用户名 输入
 * - 认证方式（password / private_key）切换
 * - 密码或私钥口令（敏感字段，仅在保存时写入 Keyring）
 * - "仅连接"（不保存）与 "保存并连接" 两个动作
 * - 编辑模式下回填字段
 *
 * 对接：
 * - services/profile.save / getSecret（持久化）
 * - services/session.connect（建立 SSH）
 * - stores/tabs.addTab（打开终端 Tab）
 */
import { reactive, ref, computed, onMounted } from 'vue'
import { useProfilesStore } from '@/stores/profiles'
import { useTabsStore } from '@/stores/tabs'
import * as sessionService from '@/services/session'
import { decodeExtra, encodeExtra } from '@/types/profile'
import type { SessionProfile, AuthType } from '@/types/profile'

const props = defineProps<{
  /** 编辑模式：传入 profile id；新建模式：不传 */
  profileId?: string
}>()

const emit = defineEmits<{
  /** 保存或取消后触发，通知父组件关闭表单 */
  (e: 'close'): void
}>()

const profilesStore = useProfilesStore()
const tabsStore = useTabsStore()

/** 表单字段 */
const form = reactive({
  name: '',
  host: '',
  port: 22,
  username: 'root',
  authType: 'password' as AuthType,
  password: '',
  privateKeyPath: '',
  privateKeyPassphrase: '',
  acceptFirstHostKey: false,
})

/** 是否正在保存/连接 */
const busy = ref(false)
/** 错误信息（展示在按钮下方） */
const errorMsg = ref('')

/** 是否编辑模式 */
const isEdit = computed(() => !!props.profileId)

/** 表单校验 */
function validate(): string | null {
  if (!form.name.trim()) return '请输入会话名称'
  if (!form.host.trim()) return '请输入主机地址'
  if (!form.username.trim()) return '请输入用户名'
  if (form.port <= 0 || form.port > 65535) return '端口必须在 1-65535'
  if (form.authType === 'private_key' && !form.privateKeyPath.trim())
    return '私钥认证需要填写私钥文件路径'
  return null
}

/** 把表单组装成 SessionProfile */
function buildProfile(id?: string): SessionProfile {
  const now = Math.floor(Date.now() / 1000)
  return {
    id: id || crypto.randomUUID(),
    name: form.name.trim(),
    kind: 'ssh',
    host: form.host.trim(),
    port: form.port,
    username: form.username.trim(),
    auth_type: form.authType,
    extra: encodeExtra({
      private_key_path: form.authType === 'private_key' ? form.privateKeyPath.trim() : undefined,
      accept_first_host_key: form.acceptFirstHostKey,
    }),
    created_at: now,
    last_used_at: 0,
  }
}

/** 构造敏感字段（密码或私钥口令） */
function buildSecret(): string | null {
  if (form.authType === 'password') {
    return form.password || null
  }
  // private_key
  return form.privateKeyPassphrase || null
}

/** 拉取并建立连接（共用逻辑） */
async function doConnect(profile: SessionProfile, secret: string | null) {
  // 拼接 ConnectionConfig（后端 ssh-core 结构）
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

  const sessionId = await sessionService.connect(
    {
      host: profile.host,
      port: profile.port,
      username: profile.username,
      auth: authPayload as never,
      acceptFirstHostKey: decodeExtra(profile.extra).accept_first_host_key ?? false,
    },
    80,
    24,
  )

  // 打开终端 Tab
  tabsStore.addTab('ssh', profile.name, sessionId)
  // 标记最近使用
  await profilesStore.save(profile).catch(() => {
    /* 非关键失败：忽略 */
  })
}

/** 仅连接（不保存） */
async function onConnectOnly() {
  errorMsg.value = ''
  const err = validate()
  if (err) {
    errorMsg.value = err
    return
  }
  busy.value = true
  try {
    const profile = buildProfile()
    await doConnect(profile, buildSecret())
    emit('close')
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    busy.value = false
  }
}

/** 保存并连接 */
async function onSaveAndConnect() {
  errorMsg.value = ''
  const err = validate()
  if (err) {
    errorMsg.value = err
    return
  }
  busy.value = true
  try {
    const profile = buildProfile(props.profileId)
    const saved = await profilesStore.save(profile, buildSecret())
    await doConnect(saved, buildSecret())
    emit('close')
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    busy.value = false
  }
}

/** 仅保存（不连接） */
async function onSaveOnly() {
  errorMsg.value = ''
  const err = validate()
  if (err) {
    errorMsg.value = err
    return
  }
  busy.value = true
  try {
    const profile = buildProfile(props.profileId)
    await profilesStore.save(profile, buildSecret())
    emit('close')
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    busy.value = false
  }
}

/** 取消 */
function onCancel() {
  emit('close')
}

/** 编辑模式：回填表单 */
async function loadForEdit() {
  if (!props.profileId) return
  const p = profilesStore.findById(props.profileId)
  if (!p) return
  form.name = p.name
  form.host = p.host
  form.port = p.port
  form.username = p.username
  form.authType = p.auth_type
  const extra = decodeExtra(p.extra)
  form.privateKeyPath = extra.private_key_path || ''
  form.acceptFirstHostKey = extra.accept_first_host_key ?? false
  // 密码/口令不回填（Keyring 保护），用户重新输入才更新
}

onMounted(loadForEdit)
</script>

<template>
  <div class="connect-form-wrap">
    <h2>{{ isEdit ? '编辑 SSH 会话' : '新建 SSH 会话' }}</h2>

    <div class="form-grid">
      <label class="field">
        <span class="label">名称</span>
        <input v-model="form.name" placeholder="my-server" :disabled="busy" />
      </label>

      <label class="field field-host">
        <span class="label">主机</span>
        <input v-model="form.host" placeholder="192.168.1.10" :disabled="busy" />
      </label>

      <label class="field field-port">
        <span class="label">端口</span>
        <input
          v-model.number="form.port"
          type="number"
          min="1"
          max="65535"
          :disabled="busy"
        />
      </label>

      <label class="field">
        <span class="label">用户名</span>
        <input v-model="form.username" placeholder="root" :disabled="busy" />
      </label>

      <div class="field">
        <span class="label">认证方式</span>
        <div class="radio-group">
          <label>
            <input
              v-model="form.authType"
              type="radio"
              value="password"
              :disabled="busy"
            />
            密码
          </label>
          <label>
            <input
              v-model="form.authType"
              type="radio"
              value="private_key"
              :disabled="busy"
            />
            私钥
          </label>
        </div>
      </div>

      <!-- 密码认证 -->
      <label v-if="form.authType === 'password'" class="field">
        <span class="label">密码</span>
        <input
          v-model="form.password"
          type="password"
          :placeholder="isEdit ? '留空不更新' : '输入密码'"
          :disabled="busy"
        />
      </label>

      <!-- 私钥认证 -->
      <template v-if="form.authType === 'private_key'">
        <label class="field">
          <span class="label">私钥路径</span>
          <input
            v-model="form.privateKeyPath"
            placeholder="~/.ssh/id_rsa"
            :disabled="busy"
          />
        </label>
        <label class="field">
          <span class="label">私钥口令（可选）</span>
          <input
            v-model="form.privateKeyPassphrase"
            type="password"
            :placeholder="isEdit ? '留空不更新' : '若私钥已加密'"
            :disabled="busy"
          />
        </label>
      </template>

      <label class="field-checkbox">
        <input
          v-model="form.acceptFirstHostKey"
          type="checkbox"
          :disabled="busy"
        />
        自动接受首次 host key（开发模式，跳过指纹校验）
      </label>
    </div>

    <p v-if="errorMsg" class="error">{{ errorMsg }}</p>

    <div class="actions">
      <button class="btn btn-secondary" @click="onCancel" :disabled="busy">取消</button>
      <button class="btn" @click="onConnectOnly" :disabled="busy">仅连接</button>
      <button class="btn btn-primary" @click="onSaveAndConnect" :disabled="busy">
        保存并连接
      </button>
      <button class="btn btn-secondary" @click="onSaveOnly" :disabled="busy">
        仅保存
      </button>
    </div>
  </div>
</template>

<style scoped>
.connect-form-wrap {
  flex: 1;
  overflow: auto;
  padding: 24px 32px;
  background: var(--bg-panel, #1e1e1e);
  color: var(--text-primary, #ddd);
}
.connect-form-wrap h2 {
  margin: 0 0 20px;
  color: var(--primary-color, #2e70c8);
}
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 16px;
  max-width: 720px;
}
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.field-host {
  grid-column: 1;
}
.field-port {
  grid-column: 2;
}
.field-checkbox {
  grid-column: 1 / 3;
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text-secondary, #888);
}
.label {
  font-size: 12px;
  color: var(--text-secondary, #888);
}
input[type='text'],
input[type='password'],
input[type='number'],
input:not([type]) {
  padding: 8px 10px;
  background: var(--bg-input, #2a2a2a);
  border: 1px solid var(--border-color, #3a3a3a);
  border-radius: 4px;
  color: inherit;
  font-size: 14px;
}
input:disabled {
  opacity: 0.6;
}
.radio-group {
  display: flex;
  gap: 16px;
}
.radio-group label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 14px;
}
.error {
  color: #ff6b6b;
  margin: 12px 0;
  font-size: 13px;
}
.actions {
  display: flex;
  gap: 8px;
  margin-top: 24px;
  flex-wrap: wrap;
}
.btn {
  padding: 8px 16px;
  background: var(--bg-input, #3a3a3a);
  border: 1px solid var(--border-color, #4a4a4a);
  border-radius: 4px;
  color: var(--text-primary, #ddd);
  cursor: pointer;
  font-size: 14px;
}
.btn:hover:not(:disabled) {
  background: var(--bg-hover, #4a4a4a);
}
.btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.btn-primary {
  background: var(--primary-color, #2e70c8);
  border-color: var(--primary-color, #2e70c8);
  color: #fff;
}
.btn-primary:hover:not(:disabled) {
  filter: brightness(1.1);
}
.btn-secondary {
  background: transparent;
}
</style>
