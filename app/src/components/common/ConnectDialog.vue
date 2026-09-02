<script setup lang="ts">
/**
 * ConnectDialog - 新建/编辑 SSH 会话弹窗（全局唯一入口）
 *
 * ActivityRail / 欢迎页 / ⌘T / 侧边栏 / 终端窗格等所有「新建连接」
 * 入口都打开这同一个弹窗；编辑模式由 connectProfileId 区分。
 */
import { computed } from 'vue'
import { NModal } from 'naive-ui'
import { useUiStore } from '@/stores/ui'
import { useProfilesStore } from '@/stores/profiles'
import ConnectForm from './ConnectForm.vue'

const ui = useUiStore()
const profiles = useProfilesStore()

const title = computed(() => {
  if (!ui.connectProfileId) return '新建 SSH 会话'
  const p = profiles.findById(ui.connectProfileId)
  return p ? `编辑「${p.name}」` : '编辑 SSH 会话'
})
</script>

<template>
  <NModal
    :show="ui.connectVisible"
    preset="card"
    :title="title"
    class="connect-dialog"
    :style="{ width: '700px', maxWidth: '92vw' }"
    :mask-closable="true"
    @update:show="(v: boolean) => !v && ui.closeConnectDialog()"
  >
    <ConnectForm :profile-id="ui.connectProfileId ?? undefined" @close="ui.closeConnectDialog()" />
  </NModal>
</template>
