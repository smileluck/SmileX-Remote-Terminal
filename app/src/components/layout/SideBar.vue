<script setup lang="ts">
/**
 * SideBar - 侧边栏
 *
 * 双区域布局：
 * - 上方：已保存的会话配置列表（点击发起连接；右侧按钮支持编辑/删除）
 * - 下方：已打开的 Tab 列表（切换/关闭）
 *
 * 数据来源：
 * - profiles store（持久化的 SessionProfile 列表）
 * - tabs store（运行时打开的 TabItem）
 */
import { onMounted, ref } from 'vue'
import { useProfilesStore } from '@/stores/profiles'
import { useTabsStore } from '@/stores/tabs'
import * as sessionService from '@/services/session'
import * as profileService from '@/services/profile'
import { decodeExtra } from '@/types/profile'
import type { SessionProfile } from '@/types/profile'
import type { SessionKind, TabItem } from '@/types/session'
import ConnectForm from '@/components/common/ConnectForm.vue'

const profilesStore = useProfilesStore()
const tabsStore = useTabsStore()

/** 是否正在显示编辑表单（profileId 非 null 表示进入编辑/新建） */
const editingProfileId = ref<string | null>(null)
/** 是否处于"新建"模式 */
const creating = ref(false)
/** 正在连接的 profile id（禁用按钮防抖） */
const connectingId = ref<string | null>(null)
/** 错误信息 */
const errorMsg = ref('')

/** kind 标签文案（兼容 SessionKind / ProfileKind） */
function kindLabel(kind: SessionKind | string): string {
  const map: Record<string, string> = { ssh: 'SSH', rdp: '桌面', host: 'Mac', chat: 'AI', settings: '设置' }
  return map[kind] || kind.toUpperCase()
}

/** 进入新建模式 */
function startCreate() {
  creating.value = true
  editingProfileId.value = null
}

/** 进入编辑模式 */
function startEdit(id: string) {
  creating.value = false
  editingProfileId.value = id
}

/** 关闭表单 */
function closeForm() {
  creating.value = false
  editingProfileId.value = null
}

/** 删除会话配置（二次确认） */
async function onDelete(profile: SessionProfile) {
  if (!confirm(`确定删除会话「${profile.name}」吗？`)) return
  try {
    await profilesStore.remove(profile.id)
  } catch (e) {
    errorMsg.value = String(e)
  }
}

/** 点击会话配置发起连接 */
async function onConnect(profile: SessionProfile) {
  errorMsg.value = ''
  connectingId.value = profile.id
  try {
    // 从 Keyring 取敏感字段
    const secret = await profileService.getSecret(profile.id)

    // 构造 ConnectionConfig.auth
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

    tabsStore.addTab('ssh', profile.name, sessionId)
    // 更新最近使用时间
    await profileService.touch(profile.id).catch(() => {
      /* 非关键失败：忽略 */
    })
    await profilesStore.loadAll()
  } catch (e) {
    errorMsg.value = String(e)
  } finally {
    connectingId.value = null
  }
}

onMounted(() => {
  profilesStore.loadAll()
})
</script>

<template>
  <div class="side-bar">
    <!-- 已保存的会话列表 -->
    <section class="section">
      <header class="section-header">
        <span>会话</span>
        <button class="add-btn" title="新建 SSH 会话" @click="startCreate">+</button>
      </header>

      <!-- 新建表单（不传 profileId，表示新建） -->
      <ConnectForm v-if="creating" @close="closeForm" />

      <ul v-else class="profile-list">
        <li
          v-for="p in profilesStore.profiles"
          :key="p.id"
          class="profile-item"
          :class="{ editing: editingProfileId === p.id }"
        >
          <!-- 列表行（默认显示） -->
          <div
            v-if="editingProfileId !== p.id"
            class="profile-row"
            :title="`${p.username}@${p.host}:${p.port}`"
            @click="onConnect(p)"
          >
            <span class="kind-tag" :data-kind="p.kind">{{ kindLabel(p.kind) }}</span>
            <span class="title">{{ p.name }}</span>
            <span
              class="connecting"
              v-if="connectingId === p.id"
            >…</span>
            <button
              class="row-btn"
              title="编辑"
              @click.stop="startEdit(p.id)"
            >✎</button>
            <button
              class="row-btn danger"
              title="删除"
              @click.stop="onDelete(p)"
            >✕</button>
          </div>

          <!-- 编辑表单（inline） -->
          <ConnectForm
            v-else
            :profile-id="p.id"
            @close="closeForm"
          />
        </li>
        <li v-if="profilesStore.profiles.length === 0" class="empty">
          暂无保存的会话<br />点击「+」新建
        </li>
      </ul>
    </section>

    <p v-if="errorMsg" class="error">{{ errorMsg }}</p>

    <!-- 已打开的 Tab -->
    <section class="section section-tabs">
      <header class="section-header">
        <span>已打开</span>
      </header>
      <ul class="tab-list">
        <li
          v-for="tab in tabsStore.tabs"
          :key="tab.id"
          class="tab-item"
          :class="{ active: tab.id === tabsStore.activeId }"
          @click="tabsStore.setActive(tab.id)"
        >
          <span class="kind-tag" :data-kind="tab.kind">{{ kindLabel(tab.kind) }}</span>
          <span class="title">{{ tab.title }}</span>
          <button class="close-btn" @click.stop="tabsStore.closeTab(tab.id)">×</button>
        </li>
        <li v-if="tabsStore.tabs.length === 0" class="empty">暂无打开的会话</li>
      </ul>
    </section>
  </div>
</template>

<style scoped>
.side-bar {
  width: 240px;
  background: var(--tab-active-bg, #252526);
  border-right: 1px solid var(--border-color, #3a3a3a);
  display: flex;
  flex-direction: column;
  color: var(--text-primary, #ddd);
  overflow: hidden;
}
.section {
  display: flex;
  flex-direction: column;
  min-height: 0;
}
.section:not(:last-child) {
  flex: 1;
  border-bottom: 1px solid var(--border-color, #3a3a3a);
}
.section-tabs {
  max-height: 40%;
}
.section-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 12px;
  font-size: 12px;
  color: var(--text-secondary, #888);
  text-transform: uppercase;
  letter-spacing: 1px;
}
.add-btn {
  background: none;
  border: 1px solid var(--border-color, #4a4a4a);
  color: var(--text-primary, #ddd);
  width: 20px;
  height: 20px;
  border-radius: 4px;
  cursor: pointer;
  font-size: 14px;
  line-height: 1;
}
.add-btn:hover {
  background: var(--bg-hover, #3a3a3a);
}
.profile-list,
.tab-list {
  list-style: none;
  margin: 0;
  padding: 0;
  overflow-y: auto;
  flex: 1;
}
.profile-item {
  border-bottom: 1px dashed var(--border-color, #333);
}
.profile-item.editing {
  padding: 0;
}
.profile-row {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 8px;
  cursor: pointer;
  font-size: 13px;
}
.profile-row:hover {
  background: var(--tab-hover-bg, #2a2d2e);
}
.title {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.row-btn {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 13px;
  color: var(--text-secondary, #888);
  padding: 2px 4px;
  opacity: 0;
  transition: opacity 0.15s;
}
.profile-row:hover .row-btn {
  opacity: 1;
}
.row-btn:hover {
  color: var(--text-primary, #ddd);
}
.row-btn.danger:hover {
  color: #ff6b6b;
}
.connecting {
  color: var(--primary-color, #2e70c8);
  font-size: 11px;
}
.tab-item {
  display: flex;
  align-items: center;
  padding: 6px 12px;
  cursor: pointer;
  font-size: 13px;
}
.tab-item:hover {
  background: var(--tab-hover-bg, #2a2d2e);
}
.tab-item.active {
  background: var(--primary-bg, #37373d);
  border-left: 3px solid var(--primary-color, #2e70c8);
}
.kind-tag {
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 11px;
  margin-right: 4px;
  background: #3a3a3a;
  color: #ccc;
}
.kind-tag[data-kind='ssh'] { background: #1b3a1f; color: #7fdc9b; }
.kind-tag[data-kind='rdp'] { background: #1a2742; color: #6ba9ff; }
.kind-tag[data-kind='host'] { background: #421a20; color: #ff8a92; }
.kind-tag[data-kind='chat'] { background: #3d2f15; color: #ffcf6b; }
.kind-tag[data-kind='settings'] { background: #2a2a2a; color: #bbb; }
.close-btn {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 14px;
  color: var(--text-secondary, #999);
}
.close-btn:hover {
  color: #ff6b6b;
}
.empty {
  padding: 16px;
  color: var(--text-secondary, #888);
  font-size: 12px;
  text-align: center;
  line-height: 1.6;
}
.error {
  padding: 8px 12px;
  color: #ff6b6b;
  font-size: 12px;
  background: #3a1a1a;
}
</style>
