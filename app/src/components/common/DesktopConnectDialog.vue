<script setup lang="ts">
/**
 * DesktopConnectDialog - 新建/编辑远程桌面服务器弹窗（全局唯一入口）
 *
 * 会话列表「+」下拉 / 欢迎页「远程桌面」按钮 / 桌面 tab 卡片编辑打开：
 * - 仅保存：落库为 rdp/host profile（密码进 Keyring），出现在侧栏「远程桌面」tab
 * - 保存并连接：保存后走统一连接流程（tab 复用策略同 SSH）
 * - 编辑模式：回填字段（密码不回填，留空表示沿用已存密码）
 *
 * 分辨率/色深不进表单，保存时写默认值（1920×1080/32）到 extra。
 */
import { computed, ref, watch } from 'vue'
import {
  NModal,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NRadioGroup,
  NRadio,
  NButton,
  useMessage,
} from 'naive-ui'
import { useUiStore } from '@/stores/ui'
import { useProfilesStore } from '@/stores/profiles'
import { useConnectFlow } from '@/composables/useConnectFlow'
import { decodeExtra, encodeExtra } from '@/types/profile'
import type { SessionProfile } from '@/types/profile'
import type { DesktopKind } from '@/types/session'

const ui = useUiStore()
const profilesStore = useProfilesStore()
const message = useMessage()
const { connect } = useConnectFlow()

const kind = ref<DesktopKind>('rdp')
const name = ref('')
const host = ref('')
const port = ref(3389)
const username = ref('')
const password = ref('')
const group = ref('')
const busy = ref(false)

/** 新建模式下缓存首次生成的 id，保存失败重试时复用，避免重复插入 */
const createdId = ref<string>()

/** 是否编辑模式 */
const isEdit = computed(() => !!ui.desktopProfileId)

/** 每次打开：编辑模式回填（密码不回填），新建模式重置 */
watch(
  () => ui.desktopConnectVisible,
  (visible) => {
    if (!visible) return
    createdId.value = undefined
    password.value = ''
    const p = ui.desktopProfileId ? profilesStore.findById(ui.desktopProfileId) : undefined
    if (p) {
      const extra = decodeExtra(p.extra)
      kind.value = p.kind === 'host' ? 'host' : 'rdp'
      name.value = p.name
      host.value = p.host
      port.value = p.port
      username.value = p.username
      group.value = extra.group || ''
    } else {
      kind.value = 'rdp'
      name.value = ''
      host.value = ''
      port.value = 3389
      username.value = ''
      group.value = ''
    }
    // 分组名归一化依赖已加载的列表（侧栏通常已加载，这里兜底）
    profilesStore.loadAll()
  },
)

/** 把表单组装成 SessionProfile（rdp/host） */
function buildProfile(id?: string): SessionProfile {
  return {
    id: id || (createdId.value ??= crypto.randomUUID()),
    name: name.value.trim(),
    kind: kind.value,
    host: host.value.trim(),
    port: port.value || 3389,
    username: username.value.trim(),
    auth_type: 'password',
    extra: encodeExtra({
      group: profilesStore.normalizeGroupName(group.value),
      width: 1920,
      height: 1080,
      color_depth: 32,
    }),
    created_at: Math.floor(Date.now() / 1000),
    last_used_at: 0,
  }
}

function validate(): boolean {
  if (!name.value.trim()) {
    message.warning('请填写名称')
    return false
  }
  if (!host.value.trim()) {
    message.warning('请填写主机地址')
    return false
  }
  if (!isEdit.value && !password.value) {
    message.warning('请填写密码')
    return false
  }
  return true
}

/** 统一提交入口：校验 → 执行 → 成功后关闭弹窗 */
async function submit(action: () => Promise<unknown>) {
  if (!validate()) return
  busy.value = true
  try {
    await action()
    ui.closeDesktopConnectDialog()
  } catch (e) {
    message.error(String(e))
  } finally {
    busy.value = false
  }
}

/** 保存（密码留空 = 不更新 Keyring 已存密码） */
const saveProfile = () =>
  profilesStore.save(buildProfile(ui.desktopProfileId ?? undefined), password.value || null)

/** 仅保存 */
const onSaveOnly = () => submit(saveProfile)

/** 保存并连接（走统一连接流程：tab 复用策略同 SSH） */
const onSaveAndConnect = () =>
  submit(async () => {
    const saved = await saveProfile()
    await connect(saved)
  })
</script>

<template>
  <NModal
    :show="ui.desktopConnectVisible"
    preset="card"
    :title="isEdit ? '编辑远程桌面' : '新建远程桌面连接'"
    class="desktop-connect-dialog"
    :style="{ width: '440px', maxWidth: '92vw' }"
    :mask-closable="true"
    @update:show="(v: boolean) => !v && ui.closeDesktopConnectDialog()"
  >
    <NForm label-placement="top" size="small">
      <NFormItem label="类型">
        <NRadioGroup v-model:value="kind" :disabled="busy">
          <NRadio value="rdp">RDP 远程桌面</NRadio>
          <NRadio value="host">macOS 主机</NRadio>
        </NRadioGroup>
      </NFormItem>
      <NFormItem label="名称">
        <NInput v-model:value="name" placeholder="my-windows-server" :disabled="busy" />
      </NFormItem>
      <NFormItem label="主机">
        <NInput v-model:value="host" placeholder="192.168.1.10" :disabled="busy" />
      </NFormItem>
      <NFormItem label="端口">
        <NInputNumber v-model:value="port" :min="1" :max="65535" style="width: 100%" :disabled="busy" />
      </NFormItem>
      <NFormItem label="用户名">
        <NInput v-model:value="username" :disabled="busy" />
      </NFormItem>
      <NFormItem label="密码">
        <NInput
          v-model:value="password"
          type="password"
          show-password-on="click"
          :placeholder="isEdit ? '留空不更新' : ''"
          :disabled="busy"
        />
      </NFormItem>
      <NFormItem label="分组（可选）">
        <NInput v-model:value="group" placeholder="如：生产环境" :disabled="busy" />
      </NFormItem>
    </NForm>
    <template #footer>
      <div class="dialog-footer">
        <NButton size="small" :disabled="busy" @click="ui.closeDesktopConnectDialog()">取消</NButton>
        <NButton size="small" tertiary :loading="busy" @click="onSaveOnly">仅保存</NButton>
        <NButton type="primary" size="small" :loading="busy" @click="onSaveAndConnect">保存并连接</NButton>
      </div>
    </template>
  </NModal>
</template>

<style scoped>
.dialog-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
