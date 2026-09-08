/**
 * Docker 管理服务
 *
 * 复用 session_exec 静默通道在远端主机执行 docker CLI：
 * - 列表查询走 `--format '{{json .}}'` per-line JSON，前端逐行解析
 * - 变更类操作（start/stop/restart/rmi/run）静默执行，调用方负责刷新
 * - 所有命令追加 SRT_RC 退出码标记（exec 通道 stdout/stderr 合并返回，
 *   无法区分流，以退出码 + 关键字判定成败与错误类别）
 */
import * as sessionService from '@/services/session'
import type {
  DockerContainer,
  DockerImage,
  DockerErrorKind,
  RunContainerOptions,
} from '@/types/docker'

/** Docker 操作错误（kind 用于面板展示差异化提示） */
export class DockerError extends Error {
  kind: DockerErrorKind
  constructor(kind: DockerErrorKind, message: string) {
    super(message)
    this.kind = kind
  }
}

/** shell 单引号转义（' → '\''），用户输入一律经此包装防注入 */
function sq(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`
}

/** 按 stderr/stdout 关键字归类 docker 错误 */
function classifyError(out: string): DockerErrorKind {
  const s = out.toLowerCase()
  if (s.includes('command not found') || s.includes('not recognized')) return 'not-installed'
  if (s.includes('permission denied')) return 'no-permission'
  if (s.includes('cannot connect to the docker daemon') || s.includes('is the docker daemon running'))
    return 'daemon-down'
  return 'unknown'
}

/**
 * 执行命令并带回退出码（输出末尾追加 SRT_RC=<rc> 标记后剥离）。
 * rc 非零时抛出归类后的 DockerError。
 */
async function runChecked(sid: string, cmd: string): Promise<string> {
  const out = await sessionService.exec(sid, `{ ${cmd} ; } 2>&1; echo "SRT_RC=$?"`)
  const m = out.match(/SRT_RC=(\d+)\s*$/)
  const rc = m ? Number(m[1]) : -1
  const body = m ? out.slice(0, m.index).trim() : out.trim()
  if (rc !== 0) {
    throw new DockerError(classifyError(body), body || `命令执行失败（退出码 ${rc}）`)
  }
  return body
}

/** 解析 per-line JSON 输出（跳过非 JSON 行） */
function parseJsonLines<T>(out: string, map: (raw: Record<string, string>) => T): T[] {
  const items: T[] = []
  for (const line of out.split('\n')) {
    const t = line.trim()
    if (!t.startsWith('{')) continue
    try {
      items.push(map(JSON.parse(t) as Record<string, string>))
    } catch {
      /* 跳过无法解析的行 */
    }
  }
  return items
}

/** 一键安装脚本：官方 get.docker.com，失败自动切阿里云镜像重试 */
const INSTALL_SCRIPT = [
  // 权限：root 直接执行；否则要求免密 sudo
  'if [ "$(id -u)" -eq 0 ]; then SUDO=""; elif sudo -n true 2>/dev/null; then SUDO="sudo -n"; else echo "需要 root 或免密 sudo 权限"; exit 42; fi',
  // 下载器：优先 curl，其次 wget
  'if command -v curl >/dev/null 2>&1; then DL="curl -fsSL --connect-timeout 8 --max-time 120 -o /tmp/srt-get-docker.sh"; elif command -v wget >/dev/null 2>&1; then DL="wget -q -T 120 -O /tmp/srt-get-docker.sh"; else echo "缺少 curl/wget"; exit 43; fi',
  // 尝试 1：官方源
  '$DL https://get.docker.com && $SUDO sh /tmp/srt-get-docker.sh',
  'rc=$?',
  // 尝试 2：阿里云镜像
  'if [ $rc -ne 0 ]; then echo "官方源安装失败，切换阿里云镜像重试..."; $DL https://get.docker.com && $SUDO sh /tmp/srt-get-docker.sh --mirror Aliyun; rc=$?; fi',
  'rm -f /tmp/srt-get-docker.sh',
  'exit $rc',
].join('; ')

/**
 * 一键安装 Docker（get.docker.com 官方脚本，失败自动切阿里云镜像）
 *
 * 远端脚本约定退出码：42 = 无 root/免密 sudo；43 = 缺少 curl/wget。
 * 安装过程可能耗时数分钟（exec 通道无超时），调用方需维护 loading 态。
 */
export async function installDocker(sid: string): Promise<void> {
  const out = await sessionService.exec(sid, `{ ${INSTALL_SCRIPT} ; } 2>&1; echo "SRT_RC=$?"`)
  const m = out.match(/SRT_RC=(\d+)\s*$/)
  const rc = m ? Number(m[1]) : -1
  if (rc === 0) return
  const body = (m ? out.slice(0, m.index) : out).trim()
  if (rc === 42) {
    throw new DockerError('no-permission', '一键安装需要 root 或免密 sudo 权限，请在终端中手动安装 Docker')
  }
  if (rc === 43) {
    throw new DockerError('unknown', '远端主机缺少 curl/wget，无法下载安装脚本，请手动安装')
  }
  throw new DockerError('unknown', body || `Docker 安装失败（退出码 ${rc}）`)
}

/** 列出全部容器（含已停止） */
export async function listContainers(sid: string): Promise<DockerContainer[]> {
  const out = await runChecked(sid, `docker ps -a --format '{{json .}}'`)
  return parseJsonLines(out, (r) => ({
    id: r.ID ?? '',
    name: r.Names ?? '',
    image: r.Image ?? '',
    status: r.Status ?? '',
    state: (r.State ?? '').toLowerCase(),
    ports: r.Ports ?? '',
    createdAt: r.CreatedAt ?? '',
  }))
}

/** 列出镜像 */
export async function listImages(sid: string): Promise<DockerImage[]> {
  const out = await runChecked(sid, `docker images --format '{{json .}}'`)
  return parseJsonLines(out, (r) => ({
    repository: r.Repository ?? '',
    tag: r.Tag ?? '',
    id: r.ID ?? '',
    size: r.Size ?? '',
    createdSince: r.CreatedSince ?? '',
  }))
}

/** 启动容器 */
export async function startContainer(sid: string, id: string): Promise<void> {
  await runChecked(sid, `docker start ${sq(id)}`)
}

/** 停止容器 */
export async function stopContainer(sid: string, id: string): Promise<void> {
  await runChecked(sid, `docker stop ${sq(id)}`)
}

/** 重启容器 */
export async function restartContainer(sid: string, id: string): Promise<void> {
  await runChecked(sid, `docker restart ${sq(id)}`)
}

/** 删除镜像 */
export async function removeImage(sid: string, id: string): Promise<void> {
  await runChecked(sid, `docker rmi ${sq(id)}`)
}

/** 从镜像启动容器（常用选项：名称 / 端口映射 / 环境变量 / -d / 重启策略） */
export async function runContainer(
  sid: string,
  image: string,
  opts: RunContainerOptions,
): Promise<void> {
  const parts: string[] = ['docker run']
  if (opts.detach) parts.push('-d')
  if (opts.name.trim()) parts.push(`--name ${sq(opts.name.trim())}`)
  for (const p of opts.ports) {
    const host = p.host.trim()
    const container = p.container.trim()
    if (host && container) parts.push(`-p ${sq(`${host}:${container}`)}`)
  }
  for (const e of opts.env) {
    const key = e.key.trim()
    if (key) parts.push(`-e ${sq(`${key}=${e.value}`)}`)
  }
  if (opts.restart) parts.push(`--restart ${opts.restart}`)
  parts.push(sq(image))
  await runChecked(sid, parts.join(' '))
}
