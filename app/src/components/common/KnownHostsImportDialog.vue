<script setup lang="ts">
/**
 * KnownHostsImportDialog - 扫描本机 known_hosts 导入会话弹窗
 *
 * 打开时调用后端 `known_hosts_scan` 解析 ~/.ssh/known_hosts，
 * 勾选确认后批量创建 SSH 会话，归入「本机known-host」分组。
 * 与现有会话 host+port 重复的项置灰标注「已存在」，不可勾选。
 *
 * 注：HashKnownHosts 产生的 `|1|...` 哈希条目无法还原主机名，后端已跳过。
 */
import { computed, ref, watch } from 'vue'
import { NButton, NCheckbox, NEmpty, NModal, NSpin, NTag, useMessage } from 'naive-ui'
import { useProfilesStore } from '@/stores/profiles'
import { scanKnownHosts } from '@/services/profile'
import { encodeExtra, type ScannedHost } from '@/types/profile'

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ 'update:show': [boolean] }>()

const profilesStore = useProfilesStore()
const message = useMessage()

/** 导入目标分组名 */
const GROUP_NAME = '本机known-host'

/** 扫描中 / 导入中状态 */
const scanning = ref(false)
const importing = ref(false)
/** 扫描结果 */
const hosts = ref<ScannedHost[]>([])
/** 勾选的主机键（host:port） */
const checked = ref<string[]>([])

function keyOf(h: ScannedHost): string {
  return `${h.host}:${h.port}`
}

/** 与现有 SSH 会话重复（host+port 相同）的主机键集合 */
const existingKeys = computed(() => {
  const set = new Set<string>()
  for (const p of profilesStore.profiles) {
    if (p.kind === 'ssh') set.add(`${p.host}:${p.port}`)
  }
  return set
})

/** 可导入（未重复）的主机 */
const importable = computed(() => hosts.value.filter((h) => !existingKeys.value.has(keyOf(h))))
/** 已勾选且未重复的主机 */
const selected = computed(() => importable.value.filter((h) => checked.value.includes(keyOf(h))))

watch(
  () => props.show,
  async (v) => {
    if (!v) return
    scanning.value = true
    hosts.value = []
    checked.value = []
    try {
      hosts.value = await scanKnownHosts()
      // 默认勾选全部未重复项
      checked.value = importable.value.map(keyOf)
    } catch {
      // invoke 层已弹出错误提示
      emit('update:show', false)
    } finally {
      scanning.value = false
    }
  },
)

/** 勾选确认后批量导入 */
async function onImport() {
  if (importing.value) return
  importing.value = true
  try {
    const group = profilesStore.normalizeGroupName(GROUP_NAME) ?? GROUP_NAME
    let count = 0
    for (const h of selected.value) {
      await profilesStore.save({
        id: crypto.randomUUID(),
        name: h.port === 22 ? h.host : `${h.host}:${h.port}`,
        kind: 'ssh',
        host: h.host,
        port: h.port,
        username: '',
        auth_type: 'password',
        extra: encodeExtra({ group }),
        created_at: 0,
        last_used_at: 0,
      })
      count++
    }
    message.success(`已导入 ${count} 个会话到「${group}」分组`)
    emit('update:show', false)
  } finally {
    importing.value = false
  }
}
</script>

<template>
  <NModal
    :show="show"
    preset="card"
    title="扫描本机 known_hosts"
    :style="{ width: '520px', maxWidth: '92vw' }"
    @update:show="(v: boolean) => emit('update:show', v)"
  >
    <NSpin :show="scanning">
      <div v-if="hosts.length > 0" class="host-list">
        <div v-for="h in hosts" :key="keyOf(h)" class="host-row">
          <NCheckbox
            :checked="checked.includes(keyOf(h))"
            :disabled="existingKeys.has(keyOf(h))"
            :label="h.port === 22 ? h.host : `${h.host}:${h.port}`"
            @update:checked="
              (v: boolean) =>
                (checked = v ? [...checked, keyOf(h)] : checked.filter((k) => k !== keyOf(h)))
            "
          />
          <NTag v-if="existingKeys.has(keyOf(h))" size="tiny" :bordered="false">已存在</NTag>
        </div>
      </div>
      <NEmpty
        v-else-if="!scanning"
        size="small"
        description="未发现可导入的主机（known_hosts 中的哈希条目无法还原，已自动跳过）"
      />
    </NSpin>

    <template #footer>
      <div class="footer">
        <NButton @click="emit('update:show', false)">取消</NButton>
        <NButton
          type="primary"
          :disabled="selected.length === 0"
          :loading="importing"
          @click="onImport"
        >
          导入 {{ selected.length }} 项到「本机known-host」
        </NButton>
      </div>
    </template>
  </NModal>
</template>

<style scoped>
.host-list {
  max-height: 320px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.host-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
