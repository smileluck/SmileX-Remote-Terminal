/**
 * 环境管理服务
 *
 * 复用 session_exec 静默通道管理远端主机的 8 种环境
 * （Python / Go / Java / MySQL / PostgreSQL / Redis / Nginx / OpenResty）：
 * - 探测：一次 exec 执行拼接脚本（bash -lc 包装以拿到 login shell PATH），
 *   每种环境输出 SRT_ENV|<id>|<installed>|<version>|<path>|<service>|<config>
 *   分隔行，前端逐行解析
 * - 安装：官方脚本 / 版本管理器 / 厂商官方源（退出码 42 = 无权限、
 *   43 = 缺 curl/wget、44 = 发行版不支持），官方源失败自动切国内镜像
 * - 配置：读取（512KB 上限）→ 面板编辑 → base64 回写（先 cp 备份；
 *   nginx/openresty 保存后语法校验，失败自动回滚备份）
 * - 所有命令追加 SRT_RC 退出码标记（exec 通道 stdout/stderr 合并返回，
 *   无法区分流，以退出码 + 关键字判定成败与错误类别）
 */
import * as sessionService from '@/services/session'
import type { EnvId, EnvStatus, EnvErrorKind, ServiceAction } from '@/types/env'

/** 环境操作错误（kind 用于面板展示差异化提示） */
export class EnvError extends Error {
  kind: EnvErrorKind
  constructor(kind: EnvErrorKind, message: string) {
    super(message)
    this.kind = kind
  }
}

/** 单个环境的静态定义（面板展示与脚本组装共用） */
export interface EnvDef {
  id: EnvId
  name: string
  /** 是否为服务类（有启停操作） */
  service: boolean
  /** 安装弹窗是否可选版本 */
  optionalVersion: boolean
  /** 保存配置后是否做语法校验（失败自动回滚） */
  configValidate: boolean
  /** 安装方式说明（安装弹窗展示） */
  installNote: string
  /** 卸载影响范围说明（确认弹窗展示） */
  uninstallNote: string
}

/** 环境定义表（面板卡片顺序即此顺序） */
export const ENV_DEFS: EnvDef[] = [
  {
    id: 'python',
    name: 'Python',
    service: false,
    optionalVersion: true,
    configValidate: false,
    installNote: '通过 pyenv 官方 installer 安装到用户目录（无需 root）',
    uninstallNote: '将移除 ~/.pyenv 及其中安装的全部 Python 版本',
  },
  {
    id: 'go',
    name: 'Go',
    service: false,
    optionalVersion: true,
    configValidate: false,
    installNote: 'go.dev 官方二进制包，解压到 /usr/local/go（需 root 或免密 sudo）',
    uninstallNote: '将删除 /usr/local/go 目录与 /usr/local/bin/go 软链',
  },
  {
    id: 'java',
    name: 'Java',
    service: false,
    optionalVersion: true,
    configValidate: false,
    installNote: '通过 SDKMAN 安装到用户目录（无需 root）',
    uninstallNote: '将卸载 SDKMAN 当前 Java 版本（SDKMAN 本体保留）',
  },
  {
    id: 'mysql',
    name: 'MySQL',
    service: true,
    optionalVersion: false,
    configValidate: false,
    installNote: 'MySQL 官方仓库安装 mysql-server（需 root 或免密 sudo）',
    uninstallNote: '仅移除软件包，数据目录 /var/lib/mysql 保留',
  },
  {
    id: 'postgresql',
    name: 'PostgreSQL',
    service: true,
    optionalVersion: false,
    configValidate: false,
    installNote: 'PGDG 官方仓库安装（需 root 或免密 sudo）',
    uninstallNote: '仅移除软件包，数据目录保留',
  },
  {
    id: 'redis',
    name: 'Redis',
    service: true,
    optionalVersion: false,
    configValidate: false,
    installNote: 'redis.io 官方仓库安装；dnf/yum 系使用官方源码编译（需 root 或免密 sudo）',
    uninstallNote: '移除软件包或编译安装的二进制，数据文件保留',
  },
  {
    id: 'nginx',
    name: 'Nginx',
    service: true,
    optionalVersion: false,
    configValidate: true,
    installNote: 'nginx.org 官方仓库安装（需 root 或免密 sudo）',
    uninstallNote: '仅移除软件包，配置文件 /etc/nginx 保留',
  },
  {
    id: 'openresty',
    name: 'OpenResty',
    service: true,
    optionalVersion: false,
    configValidate: true,
    installNote: 'openresty.org 官方仓库安装（需 root 或免密 sudo）',
    uninstallNote: '仅移除软件包，配置文件保留',
  },
]

/** shell 单引号转义（' → '\''），用户输入一律经此包装防注入 */
function sq(s: string): string {
  return `'${s.replace(/'/g, `'\\''`)}'`
}

/** 按 stderr/stdout 关键字归类环境操作错误 */
function classifyError(out: string): EnvErrorKind {
  const s = out.toLowerCase()
  if (s.includes('command not found') || s.includes('not recognized')) return 'not-installed'
  if (s.includes('permission denied') || s.includes('eacces')) return 'no-permission'
  return 'unknown'
}

/** 远端脚本退出码 + 输出 → 归类后的 EnvError（42/43/44 为脚本约定码） */
function mapError(rc: number, body: string): EnvError {
  if (rc === 42) {
    return new EnvError('no-permission', body || '需要 root 或免密 sudo 权限，请在终端中手动操作')
  }
  if (rc === 43) return new EnvError('unknown', '远端主机缺少 curl/wget，无法下载安装文件，请手动安装')
  if (rc === 44) {
    return new EnvError('unsupported-os', body || '当前系统发行版暂不支持一键安装，请在终端手动安装')
  }
  return new EnvError(classifyError(body), body || `命令执行失败（退出码 ${rc}）`)
}

/** 执行命令并带回退出码与原始输出（不裁剪，供配置文件读取用） */
async function runWithRc(sid: string, cmd: string): Promise<{ rc: number; body: string }> {
  const out = await sessionService.exec(sid, `{ ${cmd} ; } 2>&1; echo "SRT_RC=$?"`)
  const m = out.match(/SRT_RC=(\d+)\s*$/)
  const rc = m ? Number(m[1]) : -1
  const body = m ? out.slice(0, m.index) : out
  return { rc, body }
}

/**
 * 执行命令并带回退出码（输出末尾追加 SRT_RC=<rc> 标记后剥离）。
 * rc 非零时抛出归类后的 EnvError。
 */
async function runChecked(sid: string, cmd: string): Promise<string> {
  const { rc, body: raw } = await runWithRc(sid, cmd)
  const body = raw.trim()
  if (rc !== 0) throw mapError(rc, body)
  return body
}

/* ---------------- 探测 ---------------- */

/**
 * 一次性探测脚本：每种环境输出一行 SRT_ENV|<id>|<installed>|<version>|
 * <path>|<unit:state 或 ->|<config>。emit 各字段不允许含 '|'。
 */
const DETECT_SCRIPT = [
  'emit() { echo "SRT_ENV|$1|$2|$3|$4|$5|$6"; }',
  'first_file() { for f in "$@"; do if [ -f "$f" ]; then echo "$f"; return; fi; done; }',
  // 服务状态：优先 systemctl（输出 unit:state），无 systemd 时 pgrep 兜底
  'svc_state() { if command -v systemctl >/dev/null 2>&1; then for u in "$@"; do if systemctl list-unit-files "${u}.service" 2>/dev/null | grep -q "^${u}"; then st=$(systemctl is-active "$u" 2>/dev/null); echo "$u:${st:-unknown}"; return; fi; done; fi; for u in "$@"; do if pgrep -x "$u" >/dev/null 2>&1; then echo "$u:active"; return; fi; done; echo "$1:inactive"; }',
  // python：pyenv 安装优先展示 ~/.pyenv
  'pp=$(command -v python3 2>/dev/null); if [ -n "$pp" ]; then pv=$(python3 -V 2>&1 | awk "{print \\$2}"); if [ -d "$HOME/.pyenv" ] && [ "${pp#$HOME/.pyenv}" != "$pp" ]; then pdir="$HOME/.pyenv"; else rpy=$(readlink -f "$pp" 2>/dev/null || echo "$pp"); pdir=$(dirname "$(dirname "$rpy")"); fi; pcfg=""; [ -f "$HOME/.pyenv/version" ] && pcfg="$HOME/.pyenv/version"; emit python 1 "$pv" "$pdir" "-" "$pcfg"; else emit python 0 "" "" "-" ""; fi',
  // go：GOROOT 为安装目录，GOENV 为 go env -w 持久化文件
  'gp=$(command -v go 2>/dev/null); if [ -n "$gp" ]; then gv=$(go version 2>/dev/null | awk "{print \\$3}"); gv=${gv#go}; gdir=$(go env GOROOT 2>/dev/null); [ -n "$gdir" ] || gdir="/usr/local/go"; gcfg=$(go env GOENV 2>/dev/null); { [ -n "$gcfg" ] && [ -f "$gcfg" ]; } || gcfg=""; emit go 1 "$gv" "$gdir" "-" "$gcfg"; else emit go 0 "" "" "-" ""; fi',
  // java
  'jp=$(command -v java 2>/dev/null); if [ -n "$jp" ]; then jv=$(java -version 2>&1 | head -1 | sed -E "s/.*\\"([^\\"]+)\\".*/\\1/"); rj=$(readlink -f "$jp" 2>/dev/null || echo "$jp"); jdir=$(dirname "$(dirname "$rj")"); jcfg=""; [ -f "$HOME/.sdkman/etc/config" ] && jcfg="$HOME/.sdkman/etc/config"; emit java 1 "$jv" "$jdir" "-" "$jcfg"; else emit java 0 "" "" "-" ""; fi',
  // mysql：路径展示数据目录（跳转更有意义）
  'mp=$(command -v mysqld 2>/dev/null || command -v mysql 2>/dev/null); if [ -n "$mp" ]; then mv=$(mysql --version 2>/dev/null | sed -E "s/.*Ver ([0-9.]+).*/\\1/"); [ -n "$mv" ] || mv=$(mysqld --version 2>/dev/null | sed -E "s/.*Ver ([0-9.]+).*/\\1/"); mdir="/var/lib/mysql"; [ -d "$mdir" ] || mdir=$(dirname "$mp"); mcfg=$(first_file /etc/mysql/my.cnf /etc/my.cnf /etc/mysql/mysql.conf.d/mysqld.cnf); emit mysql 1 "$mv" "$mdir" "$(svc_state mysqld mysql)" "$mcfg"; else emit mysql 0 "" "" "-" ""; fi',
  // postgresql：配置优先 psql show config_file，fallback 常见路径
  'qp=$(command -v psql 2>/dev/null); if [ -n "$qp" ]; then qv=$(psql --version 2>/dev/null | awk "{print \\$3}"); qcfg=$(psql -Atqc "show config_file" 2>/dev/null); { [ -n "$qcfg" ] && [ -f "$qcfg" ]; } || qcfg=$(ls /etc/postgresql/*/*/postgresql.conf /var/lib/pgsql/*/data/postgresql.conf /var/lib/pgsql/data/postgresql.conf 2>/dev/null | head -1); qdir=$(ls -d /usr/lib/postgresql/* /usr/pgsql-* 2>/dev/null | head -1); [ -n "$qdir" ] || qdir="/var/lib/postgresql"; emit postgresql 1 "$qv" "$qdir" "$(svc_state postgresql postgres)" "$qcfg"; else emit postgresql 0 "" "" "-" ""; fi',
  // redis
  'rp=$(command -v redis-server 2>/dev/null); if [ -n "$rp" ]; then rv=$(redis-server --version 2>/dev/null | sed -E "s/.*v=([0-9.]+).*/\\1/"); rdir=$(dirname "$rp"); rcfg=$(first_file /etc/redis/redis.conf /etc/redis.conf /usr/local/etc/redis.conf); emit redis 1 "$rv" "$rdir" "$(svc_state redis-server redis)" "$rcfg"; else emit redis 0 "" "" "-" ""; fi',
  // nginx：从 nginx -V 解析 --conf-path；路径指到配置文件所在目录，无配置时 fallback --prefix
  'np=$(command -v nginx 2>/dev/null); if [ -n "$np" ]; then nv=$(nginx -v 2>&1 | sed -E "s#.*/([0-9.]+)#\\1#"); nvv=$(nginx -V 2>&1); ncfg=$(echo "$nvv" | sed -nE "s/.*--conf-path=([^ ]+).*/\\1/p"); { [ -n "$ncfg" ] && [ -f "$ncfg" ]; } || ncfg=$(first_file /etc/nginx/nginx.conf); if [ -n "$ncfg" ]; then ndir=$(dirname "$ncfg"); else ndir=$(echo "$nvv" | sed -nE "s/.*--prefix=([^ ]+).*/\\1/p"); [ -n "$ndir" ] || ndir=$(dirname "$np"); fi; emit nginx 1 "$nv" "$ndir" "$(svc_state nginx)" "$ncfg"; else emit nginx 0 "" "" "-" ""; fi',
  // openresty：路径指到 nginx 配置文件所在目录，无配置时 fallback 安装前缀
  'op=$(command -v openresty 2>/dev/null); if [ -n "$op" ]; then ov=$(openresty -v 2>&1 | sed -E "s#.*/([0-9.]+)#\\1#"); ovv=$(openresty -V 2>&1); ocfg=$(echo "$ovv" | sed -nE "s/.*--conf-path=([^ ]+).*/\\1/p"); { [ -n "$ocfg" ] && [ -f "$ocfg" ]; } || ocfg=$(first_file /usr/local/openresty/nginx/conf/nginx.conf /etc/openresty/nginx.conf); if [ -n "$ocfg" ]; then odir=$(dirname "$ocfg"); else odir="/usr/local/openresty"; [ -d "$odir" ] || odir=$(dirname "$(dirname "$op")"); fi; emit openresty 1 "$ov" "$odir" "$(svc_state openresty)" "$ocfg"; else emit openresty 0 "" "" "-" ""; fi',
].join('\n')

/** 探测全部环境（一次 exec，bash -lc 包装确保拿到 login shell 的 PATH） */
export async function detectAll(sid: string): Promise<EnvStatus[]> {
  const out = await runChecked(sid, `bash -lc ${sq(DETECT_SCRIPT)}`)
  const list: EnvStatus[] = []
  for (const line of out.split('\n')) {
    const t = line.trim()
    if (!t.startsWith('SRT_ENV|')) continue
    const p = t.split('|')
    const svc = p[5] && p[5] !== '-' ? p[5].split(':') : null
    list.push({
      id: p[1] as EnvId,
      installed: p[2] === '1',
      version: p[3] ?? '',
      installPath: p[4] ?? '',
      serviceName: svc?.[0],
      serviceActive: svc ? ((svc[1] || 'unknown') as EnvStatus['serviceActive']) : undefined,
      configPath: p[6] || undefined,
    })
  }
  return list
}

/* ---------------- 安装 / 卸载 ---------------- */

// 脚本公共前导：权限（42）、下载器（43）、发行版分发（44）
// DLF <url> <output>：curl/wget 下载到文件（wget 输出参数是 -O，与 curl -o 不通用，故包成函数）
const P_SUDO =
  'if [ "$(id -u)" -eq 0 ]; then SUDO=""; elif sudo -n true 2>/dev/null; then SUDO="sudo -n"; else echo "需要 root 或免密 sudo 权限"; exit 42; fi'
const P_DL =
  'if command -v curl >/dev/null 2>&1; then DLF() { curl -fsSL --connect-timeout 8 --max-time 300 -o "$2" "$1"; }; elif command -v wget >/dev/null 2>&1; then DLF() { wget -q -T 300 -O "$2" "$1"; }; else echo "缺少 curl/wget"; exit 43; fi'
const P_PM =
  'if command -v apt-get >/dev/null 2>&1; then PM=apt; elif command -v dnf >/dev/null 2>&1; then PM=dnf; elif command -v yum >/dev/null 2>&1; then PM=yum; else echo "当前系统发行版暂不支持一键安装（仅支持 apt/dnf/yum 系）"; exit 44; fi'

/** 组装每种环境的安装脚本（version 仅 python/go/java 有意义） */
function installScript(id: EnvId, version: string): string {
  switch (id) {
    // pyenv 官方 installer，失败切 gitee 镜像；装完可选 pyenv install + global
    case 'python':
      return [
        P_DL,
        'export PYENV_ROOT="$HOME/.pyenv"',
        'export PATH="$PYENV_ROOT/bin:$PATH"',
        'if ! command -v pyenv >/dev/null 2>&1; then DLF https://pyenv.run /tmp/srt-pyenv.sh && bash /tmp/srt-pyenv.sh; rc=$?; if [ $rc -ne 0 ]; then echo "官方源失败，切换 gitee 镜像重试..."; git clone https://gitee.com/mirrors/pyenv.git "$PYENV_ROOT"; rc=$?; fi; rm -f /tmp/srt-pyenv.sh; [ $rc -eq 0 ] || exit $rc; fi',
        'hash -r; command -v pyenv >/dev/null 2>&1 || { echo "pyenv 安装失败"; exit 1; }',
        version ? `pyenv install -s ${sq(version)} && pyenv global ${sq(version)}` : 'true',
      ].join('; ')
    // go.dev 官方 tar 包，失败切 golang.google.cn 镜像
    case 'go':
      return [
        P_SUDO,
        P_DL,
        'ARCH=$(uname -m); case "$ARCH" in x86_64) ARCH=amd64;; aarch64|arm64) ARCH=arm64;; *) echo "暂不支持的 CPU 架构：$ARCH"; exit 44;; esac',
        `DLF "https://go.dev/dl/go${version}.linux-$ARCH.tar.gz" /tmp/srt-go.tgz || DLF "https://golang.google.cn/dl/go${version}.linux-$ARCH.tar.gz" /tmp/srt-go.tgz`,
        '$SUDO rm -rf /usr/local/go && $SUDO tar -C /usr/local -xzf /tmp/srt-go.tgz',
        '$SUDO ln -sf /usr/local/go/bin/go /usr/local/bin/go',
        'rm -f /tmp/srt-go.tgz',
      ].join('; ')
    // SDKMAN 官方脚本；装完可选 sdk install java <version>
    case 'java':
      return [
        P_DL,
        'if [ ! -d "$HOME/.sdkman" ]; then DLF "https://get.sdkman.io" /tmp/srt-sdkman.sh && bash /tmp/srt-sdkman.sh; rc=$?; rm -f /tmp/srt-sdkman.sh; [ $rc -eq 0 ] || exit $rc; fi',
        'source "$HOME/.sdkman/bin/sdkman-init.sh"',
        version ? `yes | sdk install java ${sq(version)}` : 'true',
      ].join('; ')
    // MySQL 官方仓库配置包（apt: mysql-apt-config deb；dnf/yum: release rpm）
    case 'mysql':
      return [
        P_SUDO,
        P_DL,
        P_PM,
        'if [ "$PM" = "apt" ]; then DLF https://repo.mysql.com/mysql-apt-config_0.8.33-1_all.deb /tmp/srt-mysql.deb && $SUDO env DEBIAN_FRONTEND=noninteractive dpkg -i /tmp/srt-mysql.deb && $SUDO apt-get update -y && $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y mysql-server; rc=$?; rm -f /tmp/srt-mysql.deb; else REL="el$(. /etc/os-release; echo ${VERSION_ID%%.*})"; $SUDO $PM install -y "https://repo.mysql.com/mysql80-community-release-$REL-1.noarch.rpm" && $SUDO $PM install -y mysql-community-server; rc=$?; fi',
        '[ $rc -eq 0 ] || exit $rc',
        '$SUDO systemctl enable --now mysqld 2>/dev/null || $SUDO systemctl enable --now mysql 2>/dev/null || true',
      ].join('; ')
    // PGDG 官方仓库（apt: pgdg.list + 官方 key；dnf/yum: pgdg-redhat-repo rpm）
    case 'postgresql':
      return [
        P_SUDO,
        P_DL,
        P_PM,
        'if [ "$PM" = "apt" ]; then . /etc/os-release; echo "deb http://apt.postgresql.org/pub/repos/apt ${VERSION_CODENAME}-pgdg main" | $SUDO tee /etc/apt/sources.list.d/pgdg.list >/dev/null && DLF https://www.postgresql.org/media/keys/ACCC4CF8.asc /tmp/srt-pgdg.asc && $SUDO cp /tmp/srt-pgdg.asc /etc/apt/trusted.gpg.d/pgdg.asc && $SUDO apt-get update -y && $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y postgresql; rc=$?; rm -f /tmp/srt-pgdg.asc; else REL=$(rpm -E %{rhel}); $SUDO $PM install -y "https://download.postgresql.org/pub/repos/yum/reporpms/EL-$REL-x86_64/pgdg-redhat-repo-latest.noarch.rpm" && $SUDO $PM -qy module disable postgresql && $SUDO $PM install -y postgresql-server && $SUDO postgresql-setup --initdb; rc=$?; fi',
        '[ $rc -eq 0 ] || exit $rc',
        '$SUDO systemctl enable --now postgresql 2>/dev/null || true',
      ].join('; ')
    // redis.io 官方 apt 仓库；dnf/yum 系 fallback 官方源码编译
    case 'redis':
      return [
        P_SUDO,
        P_DL,
        P_PM,
        'if [ "$PM" = "apt" ]; then $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y gpg lsb-release && DLF https://packages.redis.io/gpg /tmp/srt-redis.gpg && gpg --dearmor < /tmp/srt-redis.gpg | $SUDO tee /usr/share/keyrings/redis-archive-keyring.gpg >/dev/null && echo "deb [signed-by=/usr/share/keyrings/redis-archive-keyring.gpg] https://packages.redis.io/deb $(lsb_release -cs) main" | $SUDO tee /etc/apt/sources.list.d/redis.list >/dev/null && $SUDO apt-get update -y && $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y redis; rc=$?; rm -f /tmp/srt-redis.gpg; else echo "dnf/yum 系使用官方源码编译安装（需要 gcc/make）"; DLF https://download.redis.io/redis-stable.tar.gz /tmp/srt-redis.tgz && tar -xzf /tmp/srt-redis.tgz -C /tmp && cd /tmp/redis-stable && make -j2 && $SUDO make install; rc=$?; cd /; rm -rf /tmp/srt-redis.tgz /tmp/redis-stable; fi',
        '[ $rc -eq 0 ] || exit $rc',
        '$SUDO systemctl enable --now redis 2>/dev/null || $SUDO systemctl enable --now redis-server 2>/dev/null || true',
      ].join('; ')
    // nginx.org 官方仓库（按发行版写源 + 导入官方签名 key）
    case 'nginx':
      return [
        P_SUDO,
        P_DL,
        P_PM,
        'if [ "$PM" = "apt" ]; then . /etc/os-release; DIST=debian; [ "$ID" = "ubuntu" ] && DIST=ubuntu; $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y curl gnupg2 ca-certificates lsb-release; DLF https://nginx.org/keys/nginx_signing.key /tmp/srt-nginx.key && $SUDO cp /tmp/srt-nginx.key /etc/apt/trusted.gpg.d/nginx.asc && echo "deb http://nginx.org/packages/$DIST $(lsb_release -cs) nginx" | $SUDO tee /etc/apt/sources.list.d/nginx.list >/dev/null && $SUDO apt-get update -y && $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y nginx; rc=$?; rm -f /tmp/srt-nginx.key; else printf "[nginx-stable]\nname=nginx stable repo\nbaseurl=http://nginx.org/packages/centos/\\$releasever/\\$basearch/\ngpgcheck=1\nenabled=1\ngpgkey=https://nginx.org/keys/nginx_signing.key\nmodule_hotfixes=true\n" | $SUDO tee /etc/yum.repos.d/nginx.repo >/dev/null && $SUDO $PM install -y nginx; rc=$?; fi',
        '[ $rc -eq 0 ] || exit $rc',
        '$SUDO systemctl enable --now nginx 2>/dev/null || true',
      ].join('; ')
    // openresty.org 官方仓库
    case 'openresty':
      return [
        P_SUDO,
        P_DL,
        P_PM,
        'if [ "$PM" = "apt" ]; then . /etc/os-release; DIST=debian; [ "$ID" = "ubuntu" ] && DIST=ubuntu; $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y curl gnupg2 ca-certificates lsb-release && DLF https://openresty.org/package/pubkey.gpg /tmp/srt-openresty.gpg && gpg --dearmor < /tmp/srt-openresty.gpg | $SUDO tee /usr/share/keyrings/openresty.gpg >/dev/null && echo "deb [signed-by=/usr/share/keyrings/openresty.gpg] http://openresty.org/package/$DIST $(lsb_release -cs) openresty" | $SUDO tee /etc/apt/sources.list.d/openresty.list >/dev/null && $SUDO apt-get update -y && $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y openresty; rc=$?; rm -f /tmp/srt-openresty.gpg; else DLF https://openresty.org/package/centos/openresty.repo /tmp/srt-openresty.repo && $SUDO cp /tmp/srt-openresty.repo /etc/yum.repos.d/openresty.repo && $SUDO $PM install -y openresty; rc=$?; rm -f /tmp/srt-openresty.repo; fi',
        '[ $rc -eq 0 ] || exit $rc',
        '$SUDO systemctl enable --now openresty 2>/dev/null || true',
      ].join('; ')
  }
}

/**
 * 安装环境（官方脚本 / 版本管理器 / 厂商官方源）。
 * 安装过程可能耗时数分钟（exec 通道无超时），调用方需维护 loading 态。
 */
export async function install(sid: string, id: EnvId, version?: string): Promise<void> {
  await runChecked(sid, installScript(id, version?.trim() ?? ''))
}

/** 组装每种环境的卸载脚本（数据库类仅移除软件包，数据目录保留） */
function uninstallScript(id: EnvId): string {
  switch (id) {
    case 'python':
      return [
        'export PATH="$HOME/.pyenv/bin:$PATH"',
        'if command -v pyenv >/dev/null 2>&1; then CUR=$(cat "$HOME/.pyenv/version" 2>/dev/null); [ -n "$CUR" ] && yes | pyenv uninstall -f "$CUR"; fi',
        'rm -rf "$HOME/.pyenv"',
      ].join('; ')
    case 'go':
      return [P_SUDO, '$SUDO rm -rf /usr/local/go', '$SUDO rm -f /usr/local/bin/go /usr/local/bin/gofmt'].join(
        '; ',
      )
    case 'java':
      return [
        'source "$HOME/.sdkman/bin/sdkman-init.sh" 2>/dev/null || { echo "未检测到 SDKMAN"; exit 1; }',
        'CUR=$(sdk current java 2>/dev/null | awk "{print \\$NF}")',
        '[ -n "$CUR" ] && yes | sdk uninstall java "$CUR" || true',
      ].join('; ')
    case 'mysql':
      return [
        P_SUDO,
        P_PM,
        'if [ "$PM" = "apt" ]; then $SUDO env DEBIAN_FRONTEND=noninteractive apt-get remove -y mysql-server; else $SUDO $PM remove -y mysql-community-server; fi',
      ].join('; ')
    case 'postgresql':
      return [
        P_SUDO,
        P_PM,
        'if [ "$PM" = "apt" ]; then $SUDO env DEBIAN_FRONTEND=noninteractive apt-get remove -y postgresql; else $SUDO $PM remove -y postgresql-server; fi',
      ].join('; ')
    case 'redis':
      return [
        P_SUDO,
        P_PM,
        'if [ "$PM" = "apt" ]; then $SUDO env DEBIAN_FRONTEND=noninteractive apt-get remove -y redis redis-server; else $SUDO $PM remove -y redis 2>/dev/null; $SUDO rm -f /usr/local/bin/redis-*; fi',
      ].join('; ')
    case 'nginx':
      return [
        P_SUDO,
        P_PM,
        'if [ "$PM" = "apt" ]; then $SUDO env DEBIAN_FRONTEND=noninteractive apt-get remove -y nginx; else $SUDO $PM remove -y nginx; fi',
      ].join('; ')
    case 'openresty':
      return [
        P_SUDO,
        P_PM,
        'if [ "$PM" = "apt" ]; then $SUDO env DEBIAN_FRONTEND=noninteractive apt-get remove -y openresty; else $SUDO $PM remove -y openresty; fi',
      ].join('; ')
  }
}

/** 卸载环境（UI 层 NPopconfirm 二次确认，数据库类默认保留数据目录） */
export async function uninstall(sid: string, id: EnvId): Promise<void> {
  await runChecked(sid, uninstallScript(id))
}

/* ---------------- 服务启停 ---------------- */

/** 服务启停（systemctl 优先，无 systemd 时 service 命令兜底） */
export async function serviceAction(sid: string, serviceName: string, action: ServiceAction): Promise<void> {
  await runChecked(
    sid,
    [
      P_SUDO,
      `if command -v systemctl >/dev/null 2>&1 && systemctl list-units >/dev/null 2>&1; then $SUDO systemctl ${action} ${sq(serviceName)}; else $SUDO service ${sq(serviceName)} ${action}; fi`,
    ].join('; '),
  )
}

/* ---------------- 配置读写 ---------------- */

/** 配置文件大小上限（exec 输出有缓冲上限风险） */
const CONFIG_MAX_BYTES = 512 * 1024

/** 读取远端配置文件内容（优先 sudo -n；超过 512KB 拒绝） */
export async function readConfig(sid: string, path: string): Promise<string> {
  const { rc, body } = await runWithRc(
    sid,
    [
      `SIZE=$(stat -c %s ${sq(path)} 2>/dev/null || stat -f %z ${sq(path)} 2>/dev/null || echo 0)`,
      `if [ "$SIZE" -gt ${CONFIG_MAX_BYTES} ]; then echo "配置文件超过 512KB，请在终端手动编辑"; exit 45; fi`,
      `if [ "$(id -u)" -eq 0 ]; then cat ${sq(path)}; elif sudo -n true 2>/dev/null; then sudo -n cat ${sq(path)}; else cat ${sq(path)}; fi`,
    ].join('; '),
  )
  if (rc === 45) throw new EnvError('unknown', '配置文件超过 512KB，请在终端手动编辑')
  if (rc !== 0) throw mapError(rc, body.trim())
  // 剥离 SRT_RC 标记行遗留的换行，不改动正文其他内容
  return body.replace(/\n$/, '')
}

/** UTF-8 安全的 base64 编码（远端 base64 -d 解码回写，规避引号问题） */
function toBase64(s: string): string {
  const bytes = new TextEncoder().encode(s)
  let bin = ''
  for (const b of bytes) bin += String.fromCharCode(b)
  return btoa(bin)
}

/**
 * 回写配置文件：先 cp 备份（<path>.srt-bak-<timestamp>），再 base64 解码写入。
 * nginx/openresty 写入后做语法校验（nginx -t / openresty -t），失败自动回滚备份。
 */
export async function writeConfig(sid: string, id: EnvId, path: string, content: string): Promise<void> {
  const def = ENV_DEFS.find((d) => d.id === id)
  const b64 = toBase64(content)
  const parts = [
    P_SUDO,
    `BAK="${path.replace(/"/g, '')}.srt-bak-$(date +%Y%m%d%H%M%S)"`,
    `$SUDO cp ${sq(path)} "$BAK" || exit 1`,
    `echo ${sq(b64)} | base64 -d | $SUDO tee ${sq(path)} >/dev/null || { $SUDO cp "$BAK" ${sq(path)}; echo "写入失败，已回滚备份"; exit 1; }`,
  ]
  if (def?.configValidate) {
    const check = id === 'openresty' ? 'openresty -t' : 'nginx -t'
    parts.push(
      `if ! $SUDO ${check} 2>/tmp/srt-cfg-check.err; then $SUDO cp "$BAK" ${sq(path)}; cat /tmp/srt-cfg-check.err; echo "配置校验失败，已自动回滚"; exit 46; fi; rm -f /tmp/srt-cfg-check.err`,
    )
  }
  await runChecked(sid, parts.join('; '))
}

/* ---------------- 可选版本列表 ---------------- */

/** 安装弹窗的可选版本（现场探测，失败/无来源时返回静态兜底列表） */
export async function listVersions(sid: string, id: EnvId): Promise<string[]> {
  if (id === 'python') {
    const out = await runChecked(
      sid,
      `bash -lc ${sq('export PATH="$HOME/.pyenv/bin:$PATH"; if command -v pyenv >/dev/null 2>&1; then pyenv install -l 2>/dev/null | grep -E "^ *3\\.[0-9]+\\.[0-9]+$" | tail -12; fi; true')}`,
    )
    const versions = out
      .split('\n')
      .map((s) => s.trim())
      .filter(Boolean)
    return versions.length ? versions : ['3.13.7', '3.12.11', '3.11.13', '3.10.18', '3.9.23']
  }
  if (id === 'go') {
    const out = await runChecked(
      sid,
      'if command -v curl >/dev/null 2>&1; then DL="curl -fsSL --connect-timeout 8 --max-time 30"; elif command -v wget >/dev/null 2>&1; then DL="wget -q -T 30 -O -"; else echo "缺少 curl/wget"; exit 43; fi; $DL "https://go.dev/dl/?mode=json" 2>/dev/null || $DL "https://golang.google.cn/dl/?mode=json" 2>/dev/null; true',
    )
    const seen = new Set<string>()
    for (const m of out.matchAll(/"version":\s*"go(\d+\.\d+(?:\.\d+)?)"/g)) {
      seen.add(m[1])
      if (seen.size >= 8) break
    }
    return seen.size ? [...seen] : ['1.25.1', '1.24.7', '1.23.12']
  }
  if (id === 'java') {
    const out = await runChecked(
      sid,
      `bash -lc ${sq('if [ -s "$HOME/.sdkman/bin/sdkman-init.sh" ]; then source "$HOME/.sdkman/bin/sdkman-init.sh"; sdk ls java 2>/dev/null | grep -oE "[0-9]+\\.[0-9]+\\.[0-9]+[^ ]*-tem" | sort -u | head -8; fi; true')}`,
    )
    const versions = out
      .split('\n')
      .map((s) => s.trim())
      .filter(Boolean)
    return versions.length ? versions : ['21.0.5-tem', '17.0.13-tem', '11.0.25-tem']
  }
  return []
}
