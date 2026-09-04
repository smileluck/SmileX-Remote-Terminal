<script setup lang="ts">
/**
 * DesktopConnectDialog - 新建远程桌面连接弹窗（全局唯一入口）
 *
 * 会话列表「+」下拉 / 欢迎页「远程桌面」按钮打开：
 * 填写连接信息后新建 rdp/host tab（携带 desktopConfig），
 * 由 DesktopView 挂载时自动发起连接（会话生命周期归属 DesktopView）。
 */
import { ref, watch } from 'vue'
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
import { useTabsStore } from '@/stores/tabs'
import type { DesktopConfig, DesktopKind } from '@/types/session'

const ui = useUiStore()
const tabs = useTabsStore()
const message = useMessage()

const kind = ref<DesktopKind>('rdp')
const host = ref('')
const port = ref(3389)
const username = ref('')
const password = ref('')

/** 每次打开重置表单 */
watch(
  () => ui.desktopConnectVisible,
  (visible) => {
    if (visible) {
      kind.value = 'rdp'
      host.value = ''
      port.value = 3389
      username.value = ''
      password.value = ''
    }
  },
)

function handleConnect() {
  if (!host.value.trim()) {
    message.warning('请填写主机地址')
    return
  }
  const config: DesktopConfig = {
    kind: kind.value,
    host: host.value.trim(),
    port: port.value || 3389,
    username: username.value.trim(),
    password: password.value,
    width: 1920,
    height: 1080,
    colorDepth: 32,
  }
  const tab = tabs.addTab(kind.value, config.host)
  tabs.updateTab(tab.id, { desktopConfig: config })
  ui.closeDesktopConnectDialog()
}
</script>

<template>
  <NModal
    :show="ui.desktopConnectVisible"
    preset="card"
    title="新建远程桌面连接"
    class="desktop-connect-dialog"
    :style="{ width: '440px', maxWidth: '92vw' }"
    :mask-closable="true"
    @update:show="(v: boolean) => !v && ui.closeDesktopConnectDialog()"
  >
    <NForm label-placement="top" size="small">
      <NFormItem label="类型">
        <NRadioGroup v-model:value="kind">
          <NRadio value="rdp">RDP 远程桌面</NRadio>
          <NRadio value="host">macOS 主机</NRadio>
        </NRadioGroup>
      </NFormItem>
      <NFormItem label="主机">
        <NInput v-model:value="host" placeholder="192.168.1.10" />
      </NFormItem>
      <NFormItem label="端口">
        <NInputNumber v-model:value="port" :min="1" :max="65535" style="width: 100%" />
      </NFormItem>
      <NFormItem label="用户名">
        <NInput v-model:value="username" />
      </NFormItem>
      <NFormItem label="密码">
        <NInput v-model:value="password" type="password" show-password-on="click" />
      </NFormItem>
    </NForm>
    <template #footer>
      <div class="dialog-footer">
        <NButton size="small" @click="ui.closeDesktopConnectDialog()">取消</NButton>
        <NButton type="primary" size="small" @click="handleConnect">连接</NButton>
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
