<script setup lang="ts">
/**
 * FavoriteDirPanel - 常用记录面板「目录」Tab
 *
 * 常用目录按主机档案（profileId）隔离（favoriteDirs store）。
 * 树形下钻：点击目录节点懒加载展开（SFTP list，目录优先 + 名称排序），
 * 文件仅展示；目录行附「终端 cd 到此」（raw input，终端可见，同 FilePanel）。
 * 懒加载失败行内展示错误，点击重试。
 */
import { computed, onMounted, reactive, ref, watch } from 'vue'
import {
  NButton,
  NEmpty,
  NIcon,
  NInput,
  NModal,
  NPopconfirm,
  useMessage,
} from 'naive-ui'
import {
  ChevronRight,
  Folder,
  File,
  Pencil,
  Terminal2,
  Trash,
} from '@vicons/tabler'
import { useFavoriteDirsStore } from '@/stores/favoriteDirs'
import { useTabsStore } from '@/stores/tabs'
import * as sftpService from '@/services/sftp'
import type { SftpEntry } from '@/services/sftp'
import * as sessionService from '@/services/session'
import { shellQuote } from '@/utils/shell'
import { fmtBytes } from '@/utils/format'
import type { FavoriteDir } from '@/services/snippets'

const store = useFavoriteDirsStore()
const tabs = useTabsStore()
const message = useMessage()

const encoder = new TextEncoder()

onMounted(() => void store.load())

// 激活 Tab 的主机档案变化 → 重新加载对应列表
watch(
  () => tabs.activeTab?.profileId,
  () => void store.load(),
)

/** 当前可执行/浏览目标：激活的、未断开的 SSH 会话 */
const activeSessionId = computed(() => {
  const t = tabs.activeTab
  return t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
})

/* ---------------- 树形展开 ---------------- */

/** 已展开的目录路径 */
const expanded = reactive(new Set<string>())
/** path → 子条目缓存 */
const childrenMap = reactive(new Map<string, SftpEntry[]>())
/** 加载中的路径 */
const loadingPaths = reactive(new Set<string>())
/** 加载失败：path → 错误信息 */
const errorPaths = reactive(new Map<string, string>())

/** 拉取并缓存子目录（目录优先 + 名称排序） */
async function loadChildren(path: string) {
  const sid = activeSessionId.value
  if (!sid) {
    errorPaths.set(path, '无活跃 SSH 会话')
    return
  }
  loadingPaths.add(path)
  errorPaths.delete(path)
  try {
    const list = await sftpService.list(sid, path)
    childrenMap.set(
      path,
      [...list].sort(
        (a, b) => Number(b.is_dir) - Number(a.is_dir) || a.name.localeCompare(b.name),
      ),
    )
  } catch (e) {
    errorPaths.set(path, String(e))
  } finally {
    loadingPaths.delete(path)
  }
}

/** 点击目录节点：展开（必要时懒加载）/ 折叠 */
function toggleDir(path: string) {
  if (expanded.has(path)) {
    expanded.delete(path)
    return
  }
  expanded.add(path)
  if (!childrenMap.has(path) && !loadingPaths.has(path)) void loadChildren(path)
}

/** 树行（扁平化渲染，避免递归组件） */
interface TreeRow {
  key: string
  depth: number
  kind: 'root' | 'dir' | 'file' | 'loading' | 'error'
  name: string
  path: string
  root?: FavoriteDir
  entry?: SftpEntry
  error?: string
}

const rows = computed<TreeRow[]>(() => {
  const out: TreeRow[] = []
  const pushChildren = (parentPath: string, depth: number) => {
    if (loadingPaths.has(parentPath) && !childrenMap.has(parentPath)) {
      out.push({ key: `${parentPath}__loading`, depth, kind: 'loading', name: '加载中…', path: parentPath })
      return
    }
    const err = errorPaths.get(parentPath)
    if (err && !childrenMap.has(parentPath)) {
      out.push({ key: `${parentPath}__error`, depth, kind: 'error', name: err, path: parentPath, error: err })
      return
    }
    for (const e of childrenMap.get(parentPath) ?? []) {
      out.push({
        key: e.path,
        depth,
        kind: e.is_dir ? 'dir' : 'file',
        name: e.name,
        path: e.path,
        entry: e,
      })
      if (e.is_dir && expanded.has(e.path)) pushChildren(e.path, depth + 1)
    }
  }
  for (const d of store.dirs) {
    out.push({ key: d.id, depth: 0, kind: 'root', name: d.name, path: d.path, root: d })
    if (expanded.has(d.path)) pushChildren(d.path, 1)
  }
  return out
})

/* ---------------- 操作 ---------------- */

/** 终端 cd 到目录（raw input，用户在终端可见命令与报错） */
async function cdTo(path: string) {
  const sid = activeSessionId.value
  if (!sid) return
  try {
    await sessionService.input(sid, encoder.encode(`cd ${shellQuote(path)}\r`))
  } catch (e) {
    message.error(`发送失败：${e}`)
  }
}

/** 删除常用目录 */
async function removeDir(d: FavoriteDir) {
  try {
    await store.remove(d.id)
    message.success('已删除')
  } catch (e) {
    message.error(String(e))
  }
}

/* ---------------- 新增 / 重命名 ---------------- */

const showForm = ref(false)
const editingId = ref<string | null>(null)
const draft = ref({ name: '', path: '' })

const formTitle = computed(() => (editingId.value ? '重命名目录' : '新增目录'))

/** 打开新增弹窗（供 SnippetPanel「新增」下拉调用） */
function openCreate() {
  editingId.value = null
  draft.value = { name: '', path: '' }
  showForm.value = true
}

function openRename(d: FavoriteDir) {
  editingId.value = d.id
  draft.value = { name: d.name, path: d.path }
  showForm.value = true
}

/** 路径末段作为默认显示名 */
function defaultName(path: string): string {
  const trimmed = path.replace(/\/+$/, '')
  return trimmed.split('/').pop() || trimmed || path
}

async function submitForm() {
  const path = draft.value.path.trim()
  if (!path) {
    message.warning('请填写目录路径')
    return
  }
  const old = editingId.value ? store.dirs.find((d) => d.id === editingId.value) : undefined
  try {
    await store.save({
      id: editingId.value ?? crypto.randomUUID(),
      profileId: old?.profileId ?? store.activeProfileId(),
      name: draft.value.name.trim() || defaultName(path),
      path,
      sortOrder: old?.sortOrder ?? store.dirs.length,
      createdAt: old?.createdAt ?? Math.floor(Date.now() / 1000),
    })
    showForm.value = false
    message.success('已保存')
  } catch (e) {
    message.error(String(e))
  }
}

defineExpose({ openCreate })
</script>

<template>
  <div class="fav-dir-panel">
    <NEmpty
      v-if="!store.dirs.length"
      size="small"
      description="暂无常用目录，点上方「新增」添加"
      class="fd-empty"
    />

    <div
      v-for="r in rows"
      :key="r.key"
      class="fd-row"
      :class="[`fd-${r.kind}`]"
      :style="{ paddingLeft: `${8 + r.depth * 16}px` }"
      @click="r.kind === 'root' || r.kind === 'dir' ? toggleDir(r.path) : r.kind === 'error' ? loadChildren(r.path) : undefined"
    >
      <template v-if="r.kind === 'loading'">
        <span class="fd-loading-text">{{ r.name }}</span>
      </template>
      <template v-else-if="r.kind === 'error'">
        <span class="fd-error-text" :title="`${r.error}（点击重试）`">加载失败：{{ r.name }}</span>
      </template>
      <template v-else>
        <NIcon
          v-if="r.kind !== 'file'"
          class="fd-arrow"
          :class="{ open: expanded.has(r.path) }"
          :component="ChevronRight"
          :size="12"
        />
        <NIcon
          class="fd-icon"
          :class="{ 'fd-icon-dir': r.kind !== 'file' }"
          :component="r.kind === 'file' ? File : Folder"
          :size="14"
        />
        <span class="fd-name" :title="r.path">{{ r.name }}</span>
        <span v-if="r.kind === 'file'" class="fd-size">{{ fmtBytes(r.entry?.size ?? 0) }}</span>

        <span class="fd-ops" @click.stop>
          <button
            v-if="r.kind !== 'file'"
            class="fd-op"
            title="终端 cd 到此"
            :disabled="!activeSessionId"
            @click="cdTo(r.path)"
          >
            <NIcon :component="Terminal2" :size="13" />
          </button>
          <template v-if="r.kind === 'root' && r.root">
            <button class="fd-op" title="重命名" @click="openRename(r.root)">
              <NIcon :component="Pencil" :size="13" />
            </button>
            <NPopconfirm @positive-click="removeDir(r.root)">
              <template #trigger>
                <button class="fd-op danger" title="删除">
                  <NIcon :component="Trash" :size="13" />
                </button>
              </template>
              删除常用目录「{{ r.name }}」？
            </NPopconfirm>
          </template>
        </span>
      </template>
    </div>

    <!-- 新增 / 重命名目录 -->
    <NModal
      :show="showForm"
      preset="card"
      :title="formTitle"
      style="width: 420px"
      @update:show="(v: boolean) => (showForm = v)"
    >
      <div class="fd-form">
        <NInput
          v-model:value="draft.path"
          placeholder="目录路径，如 /var/log"
          :disabled="!!editingId"
          @keydown.enter="submitForm"
        />
        <NInput
          v-model:value="draft.name"
          placeholder="显示名（可选，默认路径末段）"
          @keydown.enter="submitForm"
        />
        <div class="fd-form-ops">
          <NButton @click="showForm = false">取消</NButton>
          <NButton type="primary" @click="submitForm">保存</NButton>
        </div>
      </div>
    </NModal>
  </div>
</template>

<style scoped>
.fav-dir-panel {
  display: flex;
  flex-direction: column;
}

.fd-empty {
  padding: 32px 12px;
}

.fd-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 5px 8px;
  border-radius: var(--radius-sm, 4px);
  font-size: 12px;
  color: var(--text-primary);
  user-select: none;
}

.fd-root,
.fd-dir,
.fd-error {
  cursor: pointer;
}

.fd-root:hover,
.fd-dir:hover,
.fd-error:hover {
  background: var(--bg-elevated);
}

.fd-arrow {
  flex-shrink: 0;
  transition: transform 0.12s;
  color: var(--text-tertiary);
}

.fd-arrow.open {
  transform: rotate(90deg);
}

.fd-icon {
  flex-shrink: 0;
  color: var(--text-tertiary);
}

.fd-icon-dir {
  color: var(--warning, #fbbf24);
}

.fd-name {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fd-root .fd-name {
  font-weight: 500;
}

.fd-size {
  flex-shrink: 0;
  font-size: 11px;
  color: var(--text-tertiary);
}

.fd-loading-text,
.fd-error-text {
  font-size: 11px;
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.fd-error-text {
  color: var(--danger);
}

.fd-ops {
  display: flex;
  gap: 2px;
  opacity: 0;
  transition: opacity 0.12s;
  flex-shrink: 0;
}

.fd-row:hover .fd-ops {
  opacity: 1;
}

.fd-op {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border: none;
  background: transparent;
  color: var(--text-tertiary);
  cursor: pointer;
  border-radius: 4px;
  padding: 0;
}

.fd-op:hover:not(:disabled) {
  color: var(--text-primary);
  background: var(--bg-panel);
}

.fd-op.danger:hover:not(:disabled) {
  color: var(--danger);
}

.fd-op:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.fd-form {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.fd-form-ops {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
