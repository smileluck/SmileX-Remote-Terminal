# SmileX-Remote-Terminal 开发文档

| 项目 | 说明 |
|------|------|
| 文档版本 | v1.0 |
| 创建日期 | 2026-07-17 |
| 文档状态 | 评审中 |
| 关联文档 | [architecture.md](../../.trae/documents/smilex-remote-terminal-architecture.md)、[requirements.md](./requirements.md) |

> 本文档面向开发者，描述项目结构、技术栈、模块设计、核心接口、数据流、构建发布、测试与规范。
> 业务需求请参考 [requirements.md](./requirements.md)，整体架构请参考 [architecture.md](../../.trae/documents/smilex-remote-terminal-architecture.md)。

---

## 一、项目结构总览

### 1.1 Monorepo 布局

项目采用 **Cargo workspace（Rust）+ pnpm workspace（前端）** 的 monorepo 结构。

```
SmileX-Remote-Terminal/
├── Cargo.toml                    # Rust workspace 根
├── package.json                  # 前端 workspace 根
├── pnpm-workspace.yaml
├── README.md
├── .github/workflows/            # CI (build/release)
├── docs/                         # 文档
│   ├── design/                   # 设计文档（本文件所在）
│   └── ...
│
├── crates/                       # Rust 工作空间（领域核心 + 应用）
│   ├── ssh-core/                 # SSH 领域核心
│   ├── remote-desktop-core/      # 远程桌面领域核心
│   ├── ai-core/                  # AI Agent 领域核心
│   └── desktop/                  # Tauri 桌面应用（应用层）
│
├── host-service/                 # macOS 被控服务（独立项目，部署在被控端）
│
└── app/                          # 前端（Vue3，运行于 Tauri WebView）
```

### 1.2 分层职责

| 层 | 组件 | 语言 | 职责 |
|----|------|------|------|
| 表现层 | `app/` | TypeScript/Vue3 | UI 渲染、用户交互、xterm.js/Canvas/Chat 面板 |
| 应用层 | `crates/desktop/` | Rust | Tauri commands/events、会话注册表、存储封装 |
| 领域核心 | `crates/ssh-core`、`remote-desktop-core`、`ai-core` | Rust | 协议/业务核心，无 UI 依赖 |
| 被控端 | `host-service/` | Rust + Swift FFI | macOS 屏幕捕获/编码/输入注入 |
| 基础设施 | desktop/storage | Rust | SQLite + OS Keyring |

**依赖方向**：`app → (Tauri IPC) → desktop → (ssh-core | remote-desktop-core | ai-core)`

---

## 二、技术栈与版本

### 2.1 核心技术栈

| 类别 | 技术 | 版本建议 | 用途 |
|------|------|---------|------|
| 桌面框架 | **Tauri** | 2.x | 跨平台桌面壳 + Rust 后端 + WebView |
| 后端语言 | **Rust** | 1.75+ | 领域核心与应用层 |
| 异步运行时 | **Tokio** | 1.x | 异步 IO、任务调度 |
| 前端框架 | **Vue 3** | 3.4+ | UI（Composition API） |
| 前端语言 | **TypeScript** | 5.x | 类型安全 |
| 状态管理 | **Pinia** | 2.x | 前端状态 |
| 构建工具 | **Vite** | 5.x | 前端构建 |
| UI 库 | **Naive UI** | 2.x | 组件库 |
| 包管理 | **pnpm** | 8+ | 前端 workspace |

### 2.2 核心依赖库

#### Rust（后端）

| 库 | 用途 |
|----|------|
| **russh** / russh-keys | SSH 协议、密钥解析（纯 Rust、Tokio 异步） |
| **ironrdp** | RDP 协议客户端（连 Win/Linux） |
| **reqwest** + eventsource-stream | LLM HTTP 调用 + SSE 流式解析 |
| **rustls** | TLS 1.3（被控服务加密） |
| **rusqlite** | SQLite 配置存储 |
| **keyring** | OS Keyring 凭据存储 |
| **objc2** / Swift FFI | macOS ScreenCaptureKit/VideoToolbox/CGEvent |
| **tracing** | 结构化日志 |

#### 前端

| 库 | 用途 |
|----|------|
| **@xterm/xterm** | 终端渲染 |
| **markdown-it** + highlight.js | AI 回答 Markdown 渲染 + 代码高亮 |
| **@tauri-apps/api** | Tauri IPC（invoke/event） |

### 2.3 macOS 被控服务技术栈

| 能力 | 技术 |
|------|------|
| 屏幕捕获 | ScreenCaptureKit（macOS 12.3+） |
| 视频编码 | VideoToolbox（VP9 硬编） |
| 输入注入 | CGEvent API |
| 部署 | launchd 守护进程 |

---

## 三、核心模块设计

### 3.1 ssh-core（SSH 领域核心）

```
ssh-core/src/
├── connection/       # SSH 连接管理（russh Session 生命周期）
├── terminal/         # PTY 会话（channel + 读循环 + 写接口）
├── sftp/             # SFTP 客户端
├── tunnel/           # 端口转发引擎（Local/Remote/Dynamic）
├── keys/             # 密钥管理（生成/导入/Agent Forwarding）
└── error.rs          # 统一错误类型
```

**关键设计**：

- **连接管理**：`SessionManager` 持有 `HashMap<sessionId, SshSession>`，每个 Session 封装 russh 的 `client::Handle`。
- **PTY 终端**：每个终端 channel 独立读循环，写入 **有界 mpsc channel**（容量 64）实现背压（详见架构 §2.6）。
- **算法白名单**：默认禁用过时算法，优先 curve25519/chacha20-poly1305/ed25519。

### 3.2 remote-desktop-core（远程桌面领域核心）

```
remote-desktop-core/src/
├── session/          # RemoteDesktopSession trait（统一抽象）
│   ├── rdp/          # RDP 实现（ironrdp，连 Win/Linux）
│   └── host/         # macOS 被控服务协议客户端（自研协议）
├── frame/            # 帧解码（协议格式 → RGBA）
├── input/            # 输入事件映射
├── adaptive.rs       # 自适应码率/画质
├── clipboard.rs      # 剪贴板同步
└── error.rs
```

**关键设计**：

- **统一抽象**：`RemoteDesktopSession` trait，`RdpSession` 与 `HostSession` 各自实现。
- **按平台路由**：应用层根据会话配置的 `platform` 字段自动选择实现。
- **对前端透明**：前端只接收 RGBA 帧，不感知协议差异。

### 3.3 ai-core（AI Agent 领域核心）

```
ai-core/src/
├── provider/             # AgentProvider trait（Chat/Work 统一抽象）
│   ├── chat/             # Chat 模式（已实现）
│   │   ├── llm/          # LlmClient trait + 各 Provider 适配器
│   │   ├── context.rs    # ContextProvider（采集终端上下文）
│   │   └── history.rs    # 对话历史（多轮上下文）
│   └── work/             # Work 模式（trait 预留，未实现）
└── error.rs
```

**关键设计**：

- **双模式抽象**：`AgentProvider` trait 统一 Chat/Work，Work 未来实现不改 trait。
- **多 Provider**：`LlmClient` trait 屏蔽 OpenAI/Claude/Ollama 差异。
- **上下文采集**：`ContextProvider` 通过应用层 SessionRegistry 读 ssh-core 终端 buffer 尾部。

### 3.4 desktop（Tauri 应用层）

```
desktop/src/
├── main.rs             # Tauri 入口
├── commands/           # Tauri invoke 处理器
│   ├── registry.rs     # 统一会话注册表（SSH+RDP+Host+Chat）
│   ├── session.rs      # SSH 命令
│   ├── desktop.rs      # 远程桌面命令（按 platform 路由）
│   ├── ai.rs           # AI 命令
│   ├── sftp.rs / tunnel.rs / key.rs
├── events.rs           # 事件推送封装
└── storage/            # SQLite + Keyring 封装
```

**关键设计**：

- **统一会话注册表**：`SessionRegistry` 用 `HashMap<sessionId, Box<dyn Any>>` 管理所有类型会话。
- **长任务持有**：长连接持有在全局 `AppState`，避免被请求作用域回收。

### 3.5 host-service（macOS 被控服务）

```
host-service/src/
├── capture/            # ScreenCaptureKit 封装（Swift FFI）
├── encoder/            # VideoToolbox VP9 硬编
├── protocol/           # 自定义流协议服务端（TCP+TLS）
├── input/              # CGEvent 输入注入
├── auth/               # 预共享密钥 + 设备指纹
└── launcher/           # launchd 守护进程
```

**关键设计**：独立项目，部署在被控 macOS，通过自研协议与控制端 `remote-desktop-core::session::host` 通信。

### 3.6 app（前端）

```
app/src/
├── services/           # Tauri IPC 封装（invoke + event）
├── stores/             # Pinia（tabs/session/config/transfer）
├── views/              # 主视图（Terminal/RemoteDesktop/AiAssistant/Sftp/...）
├── components/
│   ├── terminal/       # xterm.js 封装
│   ├── desktop/        # Canvas 渲染
│   ├── ai/             # ChatPanel/MessageBubble/...
│   ├── sftp/ tunnel/ viewer/ common/
├── composables/        # useTerminal/useDesktop/useChat/...
├── types/              # TS 类型（session/desktop/ai/sftp/tunnel）
└── styles/
```

---

## 四、核心接口与数据结构

### 4.1 协议/会话统一抽象（Rust）

```rust
/// 协议提供者 trait —— 不同协议(SSH/RDP/Host)实现此接口
pub trait ProtocolProvider: Send + Sync {
    /// 建立连接
    async fn connect(&self, config: &ConnectionConfig) -> Result<Box<dyn Connection>>;
    /// 协议类型标识
    fn protocol_type(&self) -> ProtocolType;
}

/// 连接抽象 —— SSH/远程桌面会话的统一接口
pub trait Connection: Send {
    /// 创建交互通道
    async fn open_channel(&self, kind: ChannelKind) -> Result<Box<dyn Channel>>;
    async fn close(&self) -> Result<()>;
}

/// 通道种类
pub enum ChannelKind {
    Terminal,       // SSH PTY
    Sftp,
    Tunnel,
    RemoteDesktop,  // RDP(Win/Linux) 或 Host(macOS)
}
```

### 4.2 AI Agent 抽象（Rust）

```rust
/// AI Agent 提供者 trait —— Chat/Work 统一抽象
/// 已实现 ChatProvider；WorkProvider 未来实现，无需改此 trait
pub trait AgentProvider: Send + Sync {
    /// 模式标识
    fn mode(&self) -> AgentMode;
    /// 发送消息（流式响应通过回调推送 token）
    async fn send(&self, msg: &str, ctx: &Context, on_token: Box<dyn Fn(String)>) -> Result<()>;
    /// 中断当前生成
    async fn abort(&self) -> Result<()>;
    /// 清空对话历史
    async fn clear(&self) -> Result<()>;
}

pub enum AgentMode {
    Chat,   // 已实现
    Work,   // 预留
}

/// 运维上下文（Chat 模式用）
pub struct Context {
    pub session_id: Option<String>,
    pub terminal_output: Option<String>,    // 终端最近 N 行
    pub include_context: bool,
}
```

### 4.3 LLM 客户端抽象（Rust）

```rust
/// LLM 客户端 trait —— 屏蔽不同 Provider 差异
pub trait LlmClient: Send + Sync {
    /// 流式对话
    async fn chat_stream(
        &self,
        messages: &[Message],
        on_token: &dyn Fn(String),
    ) -> Result<()>;
}

pub struct Message {
    pub role: Role,         // System/User/Assistant
    pub content: String,
}
```

### 4.4 前端会话类型（TypeScript）

```typescript
/** 会话类型 */
type SessionKind = 'ssh' | 'rdp' | 'host' | 'chat'

/** 标签数据 */
interface TabItem {
  id: string
  kind: SessionKind
  title: string
  sessionId?: string
}
```

### 4.5 Tauri Commands 接口（前后端契约）

| 命令 | 参数 | 返回 | 说明 |
|------|------|------|------|
| `session_connect` | SSH 配置 | sessionId | 建立 SSH 会话 |
| `session_input` | sessionId, data | - | 终端输入 |
| `session_resize` | sessionId, cols, rows | - | 终端尺寸变更 |
| `desktop_connect` | 远程桌面配置 | sessionId | 建立远程桌面（按 platform 路由） |
| `desktop_input` | sessionId, 事件 | - | 鼠标/键盘输入 |
| `ai_chat_send` | sessionId, msg, ctx | - | AI 发送（流式响应走 event） |
| `ai_chat_abort` | sessionId | - | 中断生成 |
| `sftp_list` / `sftp_upload` / `sftp_download` | ... | ... | SFTP 操作 |
| `tunnel_start` / `tunnel_stop` | ... | ... | 端口转发 |

### 4.6 Tauri Events（后端 → 前端）

| 事件 | payload | 说明 |
|------|---------|------|
| `terminal_output` | `{ sessionId, data }` | 终端输出（base64） |
| `desktop_frame` | `{ sessionId, frame }` | 远程桌面帧（RGBA） |
| `ai_token` | `{ sessionId, token }` | AI 流式 token |
| `ai_done` | `{ sessionId }` | AI 响应结束 |
| `transfer_progress` | `{ taskId, progress }` | SFTP 传输进度 |

---

## 五、关键数据流设计

### 5.1 终端输出流（防 OOM 三层联动）

详见架构 §2.6。核心：后端有界 channel 背压 + 传输层节流批处理 + 前端 scrollback 环形缓冲 + 超大输出转存。

```
远端 SSH → ssh-core 读循环 → [有界 mpsc 64] → desktop 节流(16ms/16kb)
  → Tauri event → 前端 xterm.js(scrollback 环形) → Canvas 渲染可视区
超 10MB → 转存本地 .log → 终端显示前 N 行 + 提示
```

### 5.2 远程桌面帧流（双路径）

**路径 A（RDP，Win/Linux）**：
```
被控端 RDP 服务(3389) → ironrdp 接收图形指令 → 解码 RGBA → 节流 60fps → event → Canvas
```

**路径 B（Host，macOS）**：
```
被控服务 ScreenCaptureKit → VP9 硬编 → 自研协议(TLS) → HostClient 解码 RGBA → event → Canvas
```

### 5.3 AI 流式响应

```
前端 invoke(ai_chat_send) → ai-core 组装 prompt(+可选上下文)
  → LlmClient 调用 LLM SSE 流 → 逐 token 回调 → Tauri event(ai_token)
  → 前端 markdown-it 增量渲染
```

### 5.4 SFTP 传输

```
拖拽 → 创建传输任务 → tokio 任务池并发分块 → 进度 event → 前端进度条
断点续传：offset 持久化到 SQLite
```

---

## 六、存储设计

### 6.1 SQLite Schema（config.db）

```sql
-- 会话配置
CREATE TABLE sessions (
  id          TEXT PRIMARY KEY,        -- UUID
  kind        TEXT NOT NULL,           -- ssh|rdp|host
  name        TEXT NOT NULL,           -- 显示名
  group_id    TEXT,                    -- 分组
  host        TEXT,
  port        INTEGER,
  username    TEXT,
  auth_type   TEXT,                    -- password|key
  key_id      TEXT,                    -- 引用 keys 表
  extra       TEXT,                    -- JSON: 平台特定配置(RDP 分辨率/Host 指纹等)
  created_at  INTEGER,
  updated_at  INTEGER
);

-- 密钥
CREATE TABLE keys (
  id          TEXT PRIMARY KEY,
  name        TEXT,
  type        TEXT,                    -- ed25519|rsa|ecdsa
  public_key  TEXT,
  path        TEXT,                    -- 私钥文件路径(本身加密)
  created_at  INTEGER
);

-- known_hosts
CREATE TABLE known_hosts (
  host        TEXT,
  port        INTEGER,
  key_type    TEXT,
  fingerprint TEXT,                    -- SHA256
  PRIMARY KEY (host, port, key_type)
);

-- 端口转发
CREATE TABLE tunnels (
  id          TEXT PRIMARY KEY,
  session_id  TEXT,
  type        TEXT,                    -- local|remote|dynamic
  local_addr  TEXT,
  local_port  INTEGER,
  remote_addr TEXT,
  remote_port INTEGER,
  enabled     INTEGER
);

-- SFTP 断点续传
CREATE TABLE transfer_tasks (
  id          TEXT PRIMARY KEY,
  session_id  TEXT,
  direction   TEXT,                    -- upload|download
  local_path  TEXT,
  remote_path TEXT,
  offset      INTEGER,
  total       INTEGER,
  status      TEXT
);
```

### 6.2 凭据存储（OS Keyring）

| 数据 | Key | 存储位置 |
|------|-----|---------|
| SSH 密码 | `ssh:<sessionId>` | OS Keyring |
| 私钥 passphrase | `key:<keyId>` | OS Keyring |
| RDP 密码 | `rdp:<sessionId>` | OS Keyring |
| Host 预共享密钥 | `host:<sessionId>` | OS Keyring |
| LLM API Key | `llm:<provider>` | OS Keyring |

**原则**：永不落盘明文，永不外传。

---

## 七、安全设计

| 维度 | 措施 |
|------|------|
| 凭据 | OS Keyring 存储，配置库可选 AES-256-GCM 加密 |
| SSH | known_hosts 校验、算法白名单、rekey |
| RDP | NLA/CredSSP 认证、证书校验 |
| Host 服务 | TLS 1.3 全链路、预共享密钥 + 设备指纹 |
| AI | API Key 存 Keyring、调用显式发起、上下文采集默认关闭 |
| 隐私 | 本地 Ollama 模式数据不出本机 |

---

## 八、构建与发布

### 8.1 开发环境构建

```powershell
# 安装前端依赖
pnpm install

# 开发模式（启动 Tauri + 前端热更新）
cargo tauri dev

# 仅构建前端
pnpm build

# 仅编译 Rust workspace
cargo build
```

### 8.2 生产构建

```powershell
# 构建当前平台安装包
cargo tauri build

# 产物位置
# Windows: src-tauri/target/release/bundle/{msi,nsis}/
# macOS:   src-tauri/target/release/bundle/{dmg,app}/
# Linux:   src-tauri/target/release/bundle/{deb,appimage}/
```

### 8.3 macOS 被控服务构建

```powershell
cd host-service
cargo build --release
# 生成 .pkg 安装包（含 launchd.plist）
```

### 8.4 CI（GitHub Actions）

- `on: push` → `cargo build` + `pnpm build` + 单元测试
- `on: tag` → 三平台交叉编译 + 发布 Release

---

## 九、测试策略

### 9.1 测试分层

| 层级 | 工具 | 范围 |
|------|------|------|
| 单元测试 | `cargo test` | ssh-core / remote-desktop-core / ai-core 核心逻辑 |
| 前端单元 | Vitest | composables / stores / utils |
| 集成测试 | `cargo test` + Tauri 测试 | commands ↔ core 联动 |
| E2E | 手动 + tauri-driver | 关键用户流程 |

### 9.2 核心测试点

- ssh-core：连接认证、PTY 读写、背压（有界 channel 满阻塞）
- remote-desktop-core：帧解码、输入坐标映射
- ai-core：LlmClient 各 Provider 适配、流式 token 推送、上下文组装
- 终端大输出：模拟 GB 级输出，验证前端不 OOM

---

## 十、开发规范

### 10.1 代码规范

- **Rust**：遵循 `rustfmt` + `clippy`，禁用 `unwrap`/`expect`（测试除外），统一错误类型
- **TypeScript**：严格模式（`strict: true`），禁用 `any`（必要时用 `unknown`）
- **注释**：函数级注释、类级注释必备（复杂业务需详细描述）
- **错误处理**：所有 IO/网络操作需捕获并转化为统一错误类型
- **性能**：异步用 Tokio，避免阻塞式 IO；大对象避免无谓拷贝

### 10.2 提交规范

- 约定式提交：`feat:` / `fix:` / `docs:` / `refactor:` / `test:` / `chore:`
- 作用域：`feat(ssh-core):` / `fix(desktop):` / `feat(ai-core):`

### 10.3 分支策略

- `main`：稳定，受保护
- `develop`：集成
- `feature/*` / `fix/*`：功能/修复分支

---

## 十一、开发环境搭建

### 11.1 前置依赖

| 工具 | 版本 | 用途 |
|------|------|------|
| Rust | 1.75+ | 后端 |
| Node.js | 20+ | 前端 |
| pnpm | 8+ | 前端包管理 |
| Tauri CLI | 2.x | 随 cargo 安装 |
| 平台依赖 | - | Win: WebView2；macOS: Xcode；Linux: webkit2gtk |

### 11.2 初始化步骤

```powershell
# 1. 克隆仓库
git clone <repo>
cd SmileX-Remote-Terminal

# 2. 安装前端依赖
pnpm install

# 3. 首次编译（下载 Rust 依赖）
cargo build

# 4. 启动开发
cargo tauri dev
```

### 11.3 测试账号准备

- 一台测试 SSH 服务器（本机起 sshd 或 VPS）
- 一台测试 RDP 服务器（Win 开启远程桌面 或 Linux 装 xrdp）
- 一个 LLM Provider（OpenAI API Key 或本地 Ollama）

---

## 十二、首批实施任务

> 详见架构 §11，此处给出可执行的任务分解。

| # | 任务 | 产出 |
|---|------|------|
| 1 | 初始化 monorepo | Cargo workspace + pnpm workspace + 目录骨架 |
| 2 | 搭建 ssh-core | connection/terminal，最小可用 SSH + PTY |
| 3 | 搭建 remote-desktop-core | session/rdp，最小可用 RDP 连接 + 帧接收 |
| 4 | 搭建 ai-core | provider/chat，LlmClient(OpenAI 兼容) + ChatProvider + 流式 |
| 5 | 搭建 desktop(Tauri) | 窗口 + SessionRegistry + commands/{session,desktop,ai}.rs |
| 6 | 搭建前端 | Vue3 + xterm + Canvas + ChatPanel + invoke 封装 |
| 7 | 打通端到端 | SSH + RDP + AI 三条链路可用 |

---

## 十三、附录

### 13.1 关联文档

- [系统架构设计方案](../../.trae/documents/smilex-remote-terminal-architecture.md)
- [需求分析文档](./requirements.md)

### 13.2 常见问题

**Q: desktop 和 app 的区别？**
A: 同一个 Tauri 应用的两层。`desktop`（Rust）是后端主进程，`app`（TS/Vue）是 WebView 前端，通过 IPC 通信。

**Q: 为什么 macOS 不用 VNC？**
A: VNC 传像素矩形，Retina 屏单帧 24MB，必然卡顿。改用自研被控服务（ScreenCaptureKit + VP9 硬编）根治。

**Q: AI Work 模式什么时候做？**
A: 接口已 trait 预留，MVP 不实现，未来结合 function calling 自主运维。

### 13.3 变更记录

| 版本 | 日期 | 变更 | 作者 |
|------|------|------|------|
| v1.0 | 2026-07-17 | 初始版本，基于架构文档生成 | - |
