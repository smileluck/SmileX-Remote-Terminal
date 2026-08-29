<script setup lang="ts">
/**
 * ConnectForm - SSH 会话新建/编辑表单
 *
 * 功能：
 * - 名称 / 主机 / 端口 / 用户名 输入
 * - 认证方式（password / private_key）切换
 * - 密码或私钥口令（敏感字段，仅在保存时写入 Keyring）
 * - 仅连接（不保存）/ 保存并连接 / 仅保存 三个动作
 * - 编辑模式下回填字段（密码/口令不回填）
 *
 * UI：naive-ui NForm 声明式校验 + useMessage 反馈。
 *
 * 对接：
 * - services/profile.save / getSecret（持久化）
 * - services/session.connect（建立 SSH）
 * - stores/tabs.addTab（打开终端 Tab）
 */
import { reactive, ref, computed, onMounted } from 'vue'
import {
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NRadioGroup,
  NRadio,
  NCheckbox,
  NButton,
  NSelect,
  useMessage,
  type FormInst,
  type FormRules,
} from 'naive-ui'
import { useProfilesStore } from '@/stores/profiles'
import { useTabsStore } from '@/stores/tabs'
import * as sessionService from '@/services/session'
import * as sshKeys from '@/services/sshKeys'
import { decodeExtra, encodeExtra } from '@/types/profile'
import type { SessionProfile, AuthType } from '@/types/profile'
import { buildConfig } from '@/composables/useSshConnect'

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
const message = useMessage()

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
  sshKeyId: '',
  group: '',
  acceptFirstHostKey: false,
})

/** 密钥管理器中的可选密钥 */
const keyOptions = ref<{ label: string; value: string }[]>([])

async function loadKeys() {
  try {
    const keys = await sshKeys.listKeys()
    keyOptions.value = keys.map((k) => ({ label: `${k.name}（${k.keyType}）`, value: k.id }))
  } catch {
    /* 浏览器 dev 静默 */
  }
}

/** 是否正在保存/连接 */
const busy = ref(false)
/** NForm 实例引用 */
const formRef = ref<FormInst | null>(null)
/** 是否编辑模式 */
const isEdit = computed(() => !!props.profileId)

/** 声明式校验规则（privateKeyPath 为条件必填，用 validator） */
const rules: FormRules = {
  name: { required: true, message: '请输入会话名称', trigger: ['blur', 'input'] },
  host: { required: true, message: '请输入主机地址', trigger: ['blur', 'input'] },
  username: { required: true, message: '请输入用户名', trigger: ['blur', 'input'] },
  port: {
    type: 'number',
    required: true,
    min: 1,
    max: 65535,
    message: '端口必须在 1-65535',
    trigger: ['blur', 'input'],
  },
  privateKeyPath: {
    trigger: ['blur', 'input'],
    validator: (_rule, value) => {
      if (form.authType === 'private_key' && !value) {
        return new Error('私钥认证需要填写私钥文件路径')
      }
      return true
    },
  },
  sshKeyId: {
    trigger: ['blur', 'change'],
    validator: (_rule, value) => {
      if (form.authType === 'private_key_mem' && !value) {
        return new Error('请选择已保存的密钥（可在设置 → SSH 密钥中生成/导入）')
      }
      return true
    },
  },
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
      ssh_key_id: form.authType === 'private_key_mem' ? form.sshKeyId : undefined,
      group: form.group.trim() || undefined,
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
  // private_key / private_key_mem：均为私钥口令
  return form.privateKeyPassphrase || null
}

/** 拉取并建立连接（共用逻辑，认证细节统一走 buildConfig） */
async function doConnect(profile: SessionProfile, secret: string | null) {
  const config = await buildConfig(profile, secret)

  const sessionId = await sessionService.connect(config, 80, 24)

  // 打开终端 Tab
  tabsStore.addTab('ssh', profile.name, sessionId)
  // 标记最近使用
  await profilesStore.save(profile).catch(() => {
    /* 非关键失败：忽略 */
  })
}

/** 统一提交入口：先校验，通过则执行 action，成功后关闭表单 */
async function submit(action: () => Promise<unknown>) {
  try {
    await formRef.value?.validate()
  } catch {
    message.warning('请检查表单填写')
    return
  }
  busy.value = true
  try {
    await action()
    emit('close')
  } catch (e) {
    message.error(String(e))
  } finally {
    busy.value = false
  }
}

/** 仅连接（不保存） */
const onConnectOnly = () => submit(() => doConnect(buildProfile(), buildSecret()))

/** 保存并连接 */
const onSaveAndConnect = () =>
  submit(async () => {
    const profile = buildProfile(props.profileId)
    const saved = await profilesStore.save(profile, buildSecret())
    await doConnect(saved, buildSecret())
  })

/** 仅保存（不连接） */
const onSaveOnly = () =>
  submit(() => profilesStore.save(buildProfile(props.profileId), buildSecret()))

/** 取消 */
function onCancel() {
  emit('close')
}

/** 编辑模式：回填表单（密码/口令不回填，重新输入才更新） */
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
  form.sshKeyId = extra.ssh_key_id || ''
  form.group = extra.group || ''
  form.acceptFirstHostKey = extra.accept_first_host_key ?? false
}

onMounted(() => {
  loadForEdit()
  loadKeys()
})
</script>

<template>
  <div class="connect-form-wrap">
    <h3 class="form-title">{{ isEdit ? '编辑 SSH 会话' : '新建 SSH 会话' }}</h3>

    <NForm
      ref="formRef"
      :model="form"
      :rules="rules"
      label-placement="top"
      :disabled="busy"
      class="form-grid"
      size="small"
    >
      <NFormItem label="名称" path="name">
        <NInput v-model:value="form.name" placeholder="my-server" />
      </NFormItem>

      <NFormItem label="主机" path="host">
        <NInput v-model:value="form.host" placeholder="192.168.1.10" />
      </NFormItem>

      <NFormItem label="端口" path="port">
        <NInputNumber v-model:value="form.port" :min="1" :max="65535" style="width: 100%" />
      </NFormItem>

      <NFormItem label="用户名" path="username">
        <NInput v-model:value="form.username" placeholder="root" />
      </NFormItem>

      <NFormItem label="分组（可选）" path="group">
        <NInput v-model:value="form.group" placeholder="如：生产环境" />
      </NFormItem>

      <NFormItem label="认证方式" path="authType" class="col-span-2">
        <NRadioGroup v-model:value="form.authType">
          <NRadio value="password">密码</NRadio>
          <NRadio value="private_key">私钥</NRadio>
          <NRadio value="private_key_mem">已存密钥</NRadio>
        </NRadioGroup>
      </NFormItem>

      <!-- 密码认证 -->
      <NFormItem
        v-if="form.authType === 'password'"
        label="密码"
        path="password"
        class="col-span-2"
      >
        <NInput
          v-model:value="form.password"
          type="password"
          show-password-on="click"
          :placeholder="isEdit ? '留空不更新' : '输入密码'"
        />
      </NFormItem>

      <!-- 私钥认证 -->
      <template v-if="form.authType === 'private_key'">
        <NFormItem label="私钥路径" path="privateKeyPath" class="col-span-2">
          <NInput v-model:value="form.privateKeyPath" placeholder="~/.ssh/id_rsa" />
        </NFormItem>
        <NFormItem label="私钥口令（可选）" path="privateKeyPassphrase" class="col-span-2">
          <NInput
            v-model:value="form.privateKeyPassphrase"
            type="password"
            show-password-on="click"
            :placeholder="isEdit ? '留空不更新' : '若私钥已加密'"
          />
        </NFormItem>
      </template>

      <!-- 已存密钥认证 -->
      <template v-if="form.authType === 'private_key_mem'">
        <NFormItem label="选择密钥" path="sshKeyId" class="col-span-2">
          <NSelect
            v-model:value="form.sshKeyId"
            :options="keyOptions"
            placeholder="选择密钥管理器中的密钥"
          />
        </NFormItem>
        <NFormItem label="私钥口令（可选）" path="privateKeyPassphrase" class="col-span-2">
          <NInput
            v-model:value="form.privateKeyPassphrase"
            type="password"
            show-password-on="click"
            placeholder="若生成/导入时已加密"
          />
        </NFormItem>
      </template>

      <NFormItem class="col-span-2" :show-label="false">
        <NCheckbox v-model:checked="form.acceptFirstHostKey">
          自动接受首次 host key（开发模式，跳过指纹校验）
        </NCheckbox>
      </NFormItem>
    </NForm>

    <div class="actions">
      <NButton :disabled="busy" @click="onCancel">取消</NButton>
      <NButton tertiary :disabled="busy" @click="onConnectOnly">仅连接</NButton>
      <NButton type="primary" :loading="busy" @click="onSaveAndConnect">保存并连接</NButton>
      <NButton tertiary :disabled="busy" @click="onSaveOnly">仅保存</NButton>
    </div>
  </div>
</template>

<style scoped>
.connect-form-wrap {
  padding: 16px 18px;
  background: var(--bg-panel);
  border-radius: var(--radius-md);
}
.form-title {
  margin: 0 0 12px;
  font-size: 14px;
  font-weight: 600;
  color: var(--text-primary);
}
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 0 14px;
  max-width: 640px;
}
.col-span-2 {
  grid-column: 1 / 3;
}
.actions {
  display: flex;
  gap: 8px;
  margin-top: 8px;
  flex-wrap: wrap;
}
</style>
