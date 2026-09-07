<script setup lang="ts">
/**
 * CommandPalette - ⌘K 命令面板
 *
 * 全局快速入口，模糊搜索三类条目：
 * - 命令片段（可删除/回车发送到当前终端）
 * - 历史命令（回车发送；可收藏为片段）
 * - 主机（回车连接）
 */
import { ref, computed, watch } from 'vue'
import {
  NModal,
  NInput,
  NIcon,
  NEmpty,
  useMessage,
} from 'naive-ui'
import {
  Terminal2,
  History,
  Star,
  Trash,
  Server,
  Search,
} from '@vicons/tabler'
import * as snippetsApi from '@/services/snippets'
import * as sessionService from '@/services/session'
import { useConnectFlow } from '@/composables/useConnectFlow'
import { useTabsStore } from '@/stores/tabs'
import { useProfilesStore } from '@/stores/profiles'
import type { CommandSnippet, CommandHistory } from '@/services/snippets'

const emit = defineEmits<{ (e: 'close'): void }>()

const tabs = useTabsStore()
const profiles = useProfilesStore()
const message = useMessage()
const { connect } = useConnectFlow()

const keyword = ref('')
const snippets = ref<CommandSnippet[]>([])
const history = ref<CommandHistory[]>([])

watch(keyword, (kw) => void load(kw))

async function load(kw: string) {
  try {
    const [sn, hi] = await Promise.all([
      snippetsApi.snippetList(),
      snippetsApi.historyList(50),
    ])
    const k = kw.trim().toLowerCase()
    snippets.value = k ? sn.filter((s) => match(s.name + ' ' + s.command, k)) : sn
    // 历史去重（相同命令只留最新一条）
    const seen = new Set<string>()
    history.value = (k ? hi.filter((h) => match(h.command, k)) : hi).filter((h) => {
      if (seen.has(h.command)) return false
      seen.add(h.command)
      return true
    })
  } catch {
    // 浏览器 dev 环境无后端时静默
  }
}
void load('')

function match(text: string, kw: string): boolean {
  const t = text.toLowerCase()
  // 简易子序列模糊匹配
  let i = 0
  for (const ch of t) {
    if (ch === kw[i]) i++
    if (i >= kw.length) return true
  }
  return i >= kw.length
}

/** 当前激活 SSH tab 的 sessionId（命令发送目标） */
const activeSessionId = computed(() => {
  const t = tabs.activeTab
  return t?.kind === 'ssh' && t.sessionId && !t.disconnected ? t.sessionId : null
})

/** 发送命令到当前终端（自动补回车） */
async function sendCommand(command: string) {
  const sid = activeSessionId.value
  if (!sid) {
    message.warning('当前无激活的 SSH 终端，请先连接主机')
    return
  }
  try {
    const encoder = new TextEncoder()
    await sessionService.input(sid, encoder.encode(command + '\r'))
    emit('close')
  } catch (e) {
    message.error(String(e))
  }
}

/** 发送片段到终端（服务条目发送 systemctl status 查询） */
function sendSnippet(s: CommandSnippet) {
  void sendCommand(
    s.kind === 'service' ? `systemctl status --no-pager ${s.command}` : s.command,
  )
}

/** 收藏历史为片段 */
async function saveAsSnippet(command: string) {
  const id = crypto.randomUUID()
  try {
    await snippetsApi.snippetSave({
      id,
      name: command.slice(0, 30),
      command,
      tags: '',
      groupName: '',
      sortOrder: 0,
      kind: 'command',
      checkCmd: '',
      createdAt: Math.floor(Date.now() / 1000),
    })
    message.success('已收藏为片段')
    void load(keyword.value)
  } catch (e) {
    message.error(String(e))
  }
}

async function removeSnippet(id: string) {
  try {
    await snippetsApi.snippetDelete(id)
    void load(keyword.value)
  } catch (e) {
    message.error(String(e))
  }
}

/** 连接主机（统一流程：已连接弹「切换/新开」，断线原地重连） */
async function connectHost(profileId: string) {
  const profile = profiles.profiles.find((p) => p.id === profileId)
  if (!profile) return
  emit('close')
  try {
    await connect(profile)
  } catch (e) {
    message.error(`连接失败：${e}`)
  }
}

const hostResults = computed(() => {
  const k = keyword.value.trim().toLowerCase()
  return profiles.profiles
    .filter((p) => !k || match(p.name + ' ' + p.host, k))
    .slice(0, 6)
})
</script>

<template>
  <NModal
    :show="true"
    :auto-focus="false"
    transform-origin="center"
    @update:show="(v: boolean) => !v && emit('close')"
  >
    <div class="palette" @keydown.esc="emit('close')">
      <div class="palette-search">
        <NIcon :component="Search" class="search-icon" />
        <NInput
          v-model:value="keyword"
          placeholder="搜索命令片段 / 历史 / 主机…"
          :bordered="false"
          autofocus
        />
      </div>
      <div class="palette-body">
        <div v-if="!snippets.length && !history.length && !hostResults.length" class="palette-empty">
          <NEmpty size="small" description="没有匹配结果" />
        </div>

        <template v-if="hostResults.length">
          <div class="palette-group">主机</div>
          <div
            v-for="p in hostResults"
            :key="p.id"
            class="palette-row"
            @click="connectHost(p.id)"
          >
            <NIcon :component="Server" class="row-icon host" />
            <span class="row-main">{{ p.name }}</span>
            <span class="row-sub">{{ p.host }}</span>
          </div>
        </template>

        <template v-if="snippets.length">
          <div class="palette-group">命令片段</div>
          <div
            v-for="s in snippets.slice(0, 8)"
            :key="s.id"
            class="palette-row"
            @click="sendSnippet(s)"
          >
            <NIcon :component="Terminal2" class="row-icon" />
            <span class="row-main">{{ s.name }}</span>
            <span class="row-sub mono">{{ s.command }}</span>
            <button class="row-op" title="删除片段" @click.stop="removeSnippet(s.id)">
              <NIcon :component="Trash" />
            </button>
          </div>
        </template>

        <template v-if="history.length">
          <div class="palette-group">历史命令</div>
          <div
            v-for="h in history.slice(0, 10)"
            :key="h.id"
            class="palette-row"
            @click="sendCommand(h.command)"
          >
            <NIcon :component="History" class="row-icon dim" />
            <span class="row-main mono">{{ h.command }}</span>
            <button class="row-op" title="收藏为片段" @click.stop="saveAsSnippet(h.command)">
              <NIcon :component="Star" />
            </button>
          </div>
        </template>
      </div>
      <div class="palette-foot">回车执行 · ⌘K 关闭</div>
    </div>
  </NModal>
</template>

<style scoped>
.palette {
  width: 560px;
  max-width: 90vw;
  margin-top: 12vh;
  background: var(--bg-panel);
  border: 1px solid var(--border-strong);
  border-radius: 10px;
  box-shadow: var(--shadow-pop);
  overflow: hidden;
  display: flex;
  flex-direction: column;
}
.palette-search {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 14px;
  border-bottom: 1px solid var(--border-color);
}
.search-icon {
  color: var(--text-tertiary);
  font-size: 16px;
}
.palette-body {
  max-height: 420px;
  overflow-y: auto;
  padding: 6px 0;
}
.palette-empty {
  padding: 28px 0;
}
.palette-group {
  padding: 8px 14px 4px;
  font-size: 11px;
  font-weight: 600;
  color: var(--text-tertiary);
  letter-spacing: 0.5px;
  text-transform: uppercase;
}
.palette-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 7px 14px;
  cursor: pointer;
  font-size: 13px;
}
.palette-row:hover {
  background: var(--bg-elevated);
}
.row-icon {
  font-size: 15px;
  color: var(--primary);
  flex-shrink: 0;
}
.row-icon.host {
  color: var(--kind-rdp-fg);
}
.row-icon.dim {
  color: var(--text-tertiary);
}
.row-main {
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 200px;
}
.row-main.mono,
.row-sub.mono {
  font-family: Consolas, Menlo, monospace;
  font-size: 12px;
}
.row-sub {
  flex: 1;
  color: var(--text-tertiary);
  font-size: 12px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  text-align: right;
}
.row-op {
  display: none;
  background: none;
  border: none;
  color: var(--text-tertiary);
  cursor: pointer;
  padding: 2px;
  font-size: 14px;
  flex-shrink: 0;
}
.palette-row:hover .row-op {
  display: block;
}
.row-op:hover {
  color: var(--danger);
}
.palette-foot {
  padding: 6px 14px;
  border-top: 1px solid var(--border-color);
  font-size: 11px;
  color: var(--text-tertiary);
}
</style>
