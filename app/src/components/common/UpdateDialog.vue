<script setup lang="ts">
/**
 * UpdateDialog - 版本更新弹窗
 *
 * 发现新版本后展示版本号 / 更新日志（markdown 渲染），
 * 引导用户下载安装（进度条）并重启生效；「稍后提醒」仅关闭弹窗。
 */
import { computed } from 'vue'
import { NModal, NButton, NProgress, useMessage } from 'naive-ui'
import MarkdownIt from 'markdown-it'
import { useUpdaterStore } from '@/stores/updater'

const updater = useUpdaterStore()
const message = useMessage()

const md = new MarkdownIt({ linkify: true, breaks: true })
const notesHtml = computed(() =>
  updater.releaseNotes ? md.render(updater.releaseNotes) : '<p>该版本未提供更新日志。</p>',
)

/** 已下载大小的可读展示（未知总大小时只显示已下载量） */
const downloadedText = computed(() => {
  const fmt = (n: number) => (n >= 1024 * 1024 ? `${(n / 1024 / 1024).toFixed(1)} MB` : `${Math.round(n / 1024)} KB`)
  const done = fmt(updater.downloadedBytes)
  return updater.totalBytes ? `${done} / ${fmt(updater.totalBytes)}` : done
})

async function onUpdate() {
  try {
    await updater.startUpdate()
  } catch (e) {
    message.error(`更新失败：${e instanceof Error ? e.message : String(e)}`)
  }
}

async function onRestart() {
  try {
    await updater.restart()
  } catch (e) {
    message.error(`重启失败：${e instanceof Error ? e.message : String(e)}`)
  }
}
</script>

<template>
  <NModal
    :show="updater.dialogVisible"
    preset="card"
    :title="`发现新版本 v${updater.newVersion}`"
    style="width: 520px"
    :closable="!updater.installing"
    :mask-closable="!updater.installing"
    @update:show="(v: boolean) => !v && !updater.installing && updater.dismiss()"
  >
    <div class="update-body">
      <p v-if="updater.releaseDate" class="update-date">发布于 {{ updater.releaseDate }}</p>
      <!-- eslint-disable-next-line vue/no-v-html -->
      <div class="update-notes markdown-body" v-html="notesHtml"></div>

      <div v-if="updater.installing" class="update-progress">
        <NProgress
          type="line"
          :percentage="updater.progressPercent"
          :indicator-placement="'inside'"
          processing
        />
        <p class="update-progress-text">正在下载更新… {{ downloadedText }}</p>
      </div>
      <p v-else-if="updater.readyToRestart" class="update-ready">更新已安装完成，重启应用后生效。</p>
    </div>

    <template #footer>
      <div class="update-footer">
        <NButton :disabled="updater.installing" @click="updater.dismiss()">稍后提醒</NButton>
        <NButton v-if="!updater.readyToRestart" type="primary" :loading="updater.installing" @click="onUpdate">
          {{ updater.installing ? '正在更新…' : '立即更新' }}
        </NButton>
        <NButton v-else type="primary" @click="onRestart">重启生效</NButton>
      </div>
    </template>
  </NModal>
</template>

<style scoped>
.update-date {
  margin: 0 0 8px;
  font-size: 12px;
  color: var(--n-text-color-3, #7d8798);
}
.update-notes {
  max-height: 300px;
  overflow-y: auto;
  font-size: 13px;
  line-height: 1.6;
}
.update-notes :deep(h1),
.update-notes :deep(h2),
.update-notes :deep(h3) {
  font-size: 14px;
  margin: 12px 0 6px;
}
.update-notes :deep(ul) {
  padding-left: 20px;
  margin: 6px 0;
}
.update-progress {
  margin-top: 16px;
}
.update-progress-text {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--n-text-color-3, #7d8798);
}
.update-ready {
  margin: 16px 0 0;
  font-size: 13px;
  color: #34d399;
}
.update-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
