# SmileX-Remote-Terminal 开发文档

| 项目 | 说明 |
|------|------|
| 文档版本 | v1.1 |
| 创建日期 | 2026-07-17 |
| 最后更新 | 2026-09-15 |
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
│   ├── chat.rs           # ChatProvider（流式生成 + CancellationToken 取消）
│   ├── context.rs        # ContextProvider（采集终端上下文）+ 系统提示词（run/plan 命令执行协议）
│   ├── history.rs        # 对话历史（多轮上下文）
│   ├── llm/              # LlmClient trait + 各 Provider 适配器（OpenAI/Claude/Ollama）
│   └── work.rs           # WorkProvider 占位（stub，见 §13.6 后续阶段）
├── safety/               # 命令安全分类（classify_command，安全闸门唯一事实来源）
└── error.rs
```

**关键设计**：

- **双模式抽象**：`AgentProvider` trait 统一 Chat/Work，Work 未来实现不改 trait。
- **多 Provider**：`LlmClient` trait 屏蔽 OpenAI/Claude/Ollama 差异。
- **上下文采集**：`ContextProvider` 通过应用层 SessionRegistry 读 ssh-core 终端 buffer 尾部。
- **run/plan 命令执行协议**：系统提示词由 `context.rs` 组装，约定 LLM 输出 ```run / ```plan 块（详见 §13.1）。
- **安全分类唯一事实来源**：`safety::classify_command` → `CommandRisk::{read_only, modify, danger}`（详见 §13.2）。
- **流式取消**：`ChatProvider` 持有 `CancellationToken`，abort 真实中断 SSE 读流（详见 §13.4）。

### 3.4 desktop（Tauri 应用层）

```
desktop/src/
├── main.rs             # Tauri 入口
├── commands/           # Tauri invoke 处理器
│   ├── registry.rs     # 统一会话注册表（SSH+RDP+Host+Chat）
│   ├── session.rs      # SSH 命令
│   ├── desktop.rs      # 远程桌面命令（按 platform 路由）
│   ├── ai.rs           # AI 对话命令（发送/中断/清空）
│   ├── ai_exec.rs      # AI 命令执行安全闸门（预检/执行/白名单/审计，见 §13.2）
│   ├── agent_chat.rs   # Agent 会话与消息持久化命令（含消息 meta 更新）
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
├── stores/             # Pinia（tabs/session/config/transfer/agent 等；agent 含闸门时序与计划状态机）
├── views/              # 主视图（Terminal/RemoteDesktop/AiAssistant/Sftp/...）
├── components/
│   ├── terminal/       # xterm.js 封装
│   ├── desktop/        # Canvas 渲染
│   ├── ai/             # ChatPanel/MessageBubble/...
│   ├── settings/       # 设置页分区（含 AiSecurity「AI 命令授权」，见 §13.5）
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
| `ai_classify_command` | command | CommandRisk | 纯分类（前端即时 UI 提示，不过闸不审计） |
| `ai_exec_prepare` | chatId, command, source, approved | ExecPrepareResult | 闸门预检（PTY 可见路径，见 §13.2） |
| `ai_exec_finish` | chatId, command, risk, source, exitCode, durationMs | - | PTY 路径执行完成后补记审计 |
| `ai_exec_command` | chatId, command, source, approved, remember | ExecResult | 非交互 exec 通道一站式（闸门+执行+审计） |
| `ai_allowlist_add` / `ai_allowlist_remove` / `ai_allowlist_list` / `ai_allowlist_all` | ... | ... | AI 命令白名单管理（scope: chat/profile/global） |
| `ai_audit_list` | chatId?, limit?, offset? | 审计条目 | AI 命令审计分页查询 |
| `agent_chat_message_set_meta` | chatId, messageId, meta | bool | 更新消息 meta（plan 步骤状态机持久化） |
| `sftp_list` / `sftp_upload` / `sftp_download` | ... | ... | SFTP 操作 |
| `tunnel_start` / `tunnel_stop` | ... | ... | 端口转发 |

### 4.6 Tauri Events（后端 → 前端）

| 事件 | payload | 说明 |
|------|---------|------|
| `terminal_output` | `{ sessionId, data }` | 终端输出（base64） |
| `desktop_frame` | `{ sessionId, frame }` | 远程桌面帧（RGBA） |
| `ai_token` | `{ sessionId, token }` | AI 流式 token |
| `ai_done` | `{ sessionId, success, error }` | AI 响应结束（成功/失败/取消统一经此事件；错误分 cancelled/limit/其他，见 §13.4） |
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

取消与上限：`ai_chat_abort` 经 `CancellationToken` 真实中断 SSE 读流；前端累计输出超 256KB 自动中止；成功/失败/取消统一由 `ai_done` 收尾（详见 §13.4）。

### 5.4 SFTP 传输

```
拖拽 → 创建传输任务 → tokio 任务池并发分块 → 进度 event → 前端进度条
断点续传：offset 持久化到 SQLite
```

### 5.5 AI 命令执行（安全闸门）

```
前端 runGated(stores/agent.ts) → ai_exec_prepare 预检（分类 + 绑定校验 + 白名单）
  → rejected      展示原因（已记审计）
  → needs_approval 弹窗确认（approved=true 重新过闸，可选"记住授权"写白名单）
  → allowed       PTY 可见路径写 session_input 执行（写入失败回退 ai_exec_command 非交互通道）
  → ai_exec_finish 补记审计 → 结果截断 8KB 保尾回传 LLM → advancePlan 推进计划
```

用户正常打字的 `session_input` 不过闸、不受影响。详见 §13.2。

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

-- Agent 助手会话与消息（Chat 面板多会话 Tab）
CREATE TABLE agent_chats (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL DEFAULT '',   -- 空串 = 未命名（首条用户消息落库时自动生成）
  profile_id  TEXT NOT NULL DEFAULT '',   -- 关联的会话配置 ID（按终端会话区分聊天）
  created_at  INTEGER,
  updated_at  INTEGER
);
CREATE INDEX idx_agent_chats_updated ON agent_chats(updated_at DESC);

-- meta：附加元数据 JSON（plan 块步骤状态机等；空串 = 无），旧库经幂等 ALTER 补列
CREATE TABLE agent_chat_messages (
  id          TEXT PRIMARY KEY,
  chat_id     TEXT NOT NULL,
  role        TEXT NOT NULL,              -- user|assistant|tool
  content     TEXT NOT NULL,
  error       INTEGER NOT NULL DEFAULT 0,
  meta        TEXT NOT NULL DEFAULT '',
  created_at  INTEGER
);
CREATE INDEX idx_agent_chat_messages_chat ON agent_chat_messages(chat_id, created_at);

-- AI 命令白名单（安全闸门，见 §13.2；danger 级不允许入表）
CREATE TABLE ai_command_allowlist (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  pattern     TEXT NOT NULL,
  risk        TEXT NOT NULL,              -- read_only|modify（加白时的分类）
  scope       TEXT NOT NULL,              -- chat|profile|global（global 时 scope_id 为 NULL）
  scope_id    TEXT,
  created_at  INTEGER
);
CREATE INDEX idx_ai_allowlist_scope ON ai_command_allowlist(scope, scope_id);

-- AI 命令审计日志（放行/拒绝/执行结果全量留痕）
CREATE TABLE ai_audit_log (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  ts          INTEGER NOT NULL,
  chat_id     TEXT,
  profile_id  TEXT,
  command     TEXT NOT NULL,
  risk        TEXT NOT NULL,
  decision    TEXT NOT NULL,              -- auto|approved|rejected|failed（授权决策，非执行成败）
  source      TEXT NOT NULL,              -- manual|auto|plan
  exit_code   INTEGER,
  duration_ms INTEGER
);
CREATE INDEX idx_ai_audit_log_ts   ON ai_audit_log(ts DESC);
CREATE INDEX idx_ai_audit_log_chat ON ai_audit_log(chat_id, ts DESC);
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
| AI | API Key 存 Keyring、调用显式发起、上下文采集默认关闭；AI 发起的命令统一经安全闸门（三级分类 + 白名单 + 审计，见 §13.2） |
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

## 十三、Agent 助手 Harness 化（P1，2026-09）

> 本期把"AI 发起命令"从纯前端约定升级为后端强制管线：提示词协议、安全闸门、计划状态机、审计与白名单。

### 13.1 run/plan 提示词协议

- LLM 以 fenced 代码块输出指令：```run（一条非交互 shell 命令）与 ```plan（分步骤执行计划）。
- 系统提示词由 `crates/ai-core/src/provider/context.rs` 的 `Context::to_system_prompt` 组装，含**三级执行策略**：
  - 只读查询类（ls、cat、grep、ps、df、ss 等）自动执行并回传，获取信息优先用简洁精准的只读命令；
  - 修改类需经用户确认后才执行，输出此类命令前必须先用文字说明其影响；
  - 危险操作（rm -rf、dd、重启等破坏性命令）不会被自动执行，必须显式警告用户风险；
  - 每次回复最多一个 ```run 块；禁止需要交互输入的命令（如需输密码的 sudo）。
- **计划模式**（`Context.plan_mode`，不依赖运维上下文）：收到任务先输出 ```plan 块（每步写明目的与将执行的命令，并简述风险点），此阶段禁止输出 ```run 块；用户确认后每次回复只输出一个 ```run 块对应当前步骤，待执行结果回传后再继续下一步；某步执行失败即停止输出后续步骤，等待用户指示重试、跳过或终止；纯咨询问答直接回答，不必出计划。

### 13.2 安全闸门（Capability Gate）

- **唯一事实来源**：分类函数 `ai_core::safety::classify_command`（crates/ai-core/src/safety/）→ `CommandRisk::{read_only, modify, danger}`（snake_case 序列化）。分级规则 Danger 黑名单 > Modify 兜底（未知命令一律 Modify）> ReadOnly 白名单；管道/串联命令按最高风险段定级。
- **所有 AI 发起的执行都必须过闸**，两条路径：
  - PTY 可见路径：`ai_exec_prepare` 预检 → 写 `session_input` 执行（终端可见）→ `ai_exec_finish` 补记审计；
  - 非交互 exec 通道：`ai_exec_command` 一站式（闸门 + `SshSession::exec_with_status` + 审计，超时 120s）。
  用户正常打字的 `session_input` 不受影响。
- **闸门决策**：read_only 恒放行；modify 命中白名单或 `approved=true` 放行，否则 `needs_approval`（软拒绝、不记审计，弹窗确认后带 `approved=true` 重新过闸）；danger 永不自动执行、永不入白名单，必须显式 `approved=true` 二次确认，未批准为 `rejected` 硬拒绝。
- **绑定校验**：chat → profile → 活跃 SSH 会话（`user@host:port` 身份匹配），未绑定/断连一律拒绝并记审计，绝不静默打到别的机器。
- **白名单**：modify/read_only 可入，scope = chat（单会话）/ profile（整台主机档案）/ global（全局），可随时删除撤销；命中按"相等或前缀+空白"匹配且条目 risk 必须与当前分类一致。
- **审计**：执行完成与被硬拒绝的尝试全量落 `ai_audit_log`；`decision` = auto/approved/rejected/failed 表示授权决策（非远端执行成败，非零退出码不算 failed）；`source` = manual/auto/plan；needs_approval 待确认状态不记审计。
- **Tauri 命令族**：`ai_classify_command / ai_exec_prepare / ai_exec_finish / ai_exec_command / ai_allowlist_add / ai_allowlist_remove / ai_allowlist_list / ai_allowlist_all / ai_audit_list / agent_chat_message_set_meta`（实现于 crates/desktop/src/commands/ai_exec.rs、agent_chat.rs）。

### 13.3 计划步骤状态机（前端 stores/agent.ts）

- 单步 `PlanStep.status`：`pending / running / done / failed / skipped`；计划级 `PlanStatus`：`pending_confirm / running / paused_failed / done / cancelled`（app/src/types/ai.ts）。
- 确认计划 → 解析 ```plan 块为结构化步骤、置 running，并把计划原文回传 LLM 开始逐步执行；每步完成经 `advancePlan`：成功标 done 并推进，失败标 failed 且计划暂停为 `paused_failed`，支持重试（`retryPlanStep`）/ 跳过（`skipPlanStep`）/ 终止（`terminatePlan`）。
- 状态序列化进所属消息的 `meta` JSON 列持久化（`agent_chat_message_set_meta`）；历史恢复时 running 降级为 `paused_failed`（running 步标 failed），避免重启后自动续跑。
- `MAX_AUTO_CHAIN = 8` 仍为全局自动执行预算，超出提示剩余命令手动执行。
- 闸门时序（`runGated`）：`ai_exec_prepare` → rejected（展示原因）/ needs_approval（弹窗，可勾选"记住授权"写白名单）/ allowed → PTY 执行（写入失败回退 `ai_exec_command`）→ `ai_exec_finish` 审计 → `advancePlan`。

### 13.4 输出打印与流控

- **流式取消真实生效**：`ChatProvider` 持有 `CancellationToken`（tokio-util），abort 即中断 SSE 读流，ai-core 三个 LLM 客户端（openai/anthropic/ollama）均在读循环内响应取消。
- **`ai_done` 错误分类**：payload `{ sessionId, success, error }`；前端区分 cancelled（用户主动取消，安静收尾不显示错误样式）/ limit（输出超限，保留已生成内容标错）/ 其他（humanize 展示）。
- **执行结果回传截断**：8KB 保尾（报错通常在末尾），超出加 `[已截断 N 字节]` 前缀；前端 `MAX_EXEC_OUTPUT` 与后端 `OUTPUT_CAP` 均为 8KB，char 边界安全。
- **LLM → 前端流上限**：单轮累计超 256KB（`MAX_STREAM_BYTES`）自动调用 abort 中止，防输出失控撑爆 UI。

### 13.5 设置页「AI 命令授权」

`app/src/components/settings/AiSecurity.vue`：白名单管理（查看全部条目、按 scope 展示、支持删除撤销）+ 最近 50 条审计只读列表（`ai_audit_list`）。

### 13.6 后续阶段（P2 路线图）

借鉴 DeepSeek Harness 的运行时设计，逐步把"前端驱动循环"下沉为后端 Agent 运行时：

- **WorkProvider 落地**（crates/ai-core/src/provider/work.rs 目前为 stub）：Turn/Step 驱动器——Inbox 支持 followup / steer / inject 三类注入，kick 主循环驱动逐步执行，步数 / token / 墙钟三类预算约束。
- **Session Event Log**：追加式事件表作为唯一事实来源，`deriveMessages` 投影生成 LLM 上下文；支持压缩遮蔽旧事件与会话 Fork。
- **能力接缝下沉**：命令执行 / 终端读写 / 指标查询抽象为 capability，审批策略（本期安全闸门）固定在管线层，不再散落 UI。
- **前端退化**：从驱动循环退化为渲染事件流 + 提交审批决定。

---

## 十四、附录

### 14.1 关联文档

- [系统架构设计方案](../../.trae/documents/smilex-remote-terminal-architecture.md)
- [需求分析文档](./requirements.md)

### 14.2 常见问题

**Q: desktop 和 app 的区别？**
A: 同一个 Tauri 应用的两层。`desktop`（Rust）是后端主进程，`app`（TS/Vue）是 WebView 前端，通过 IPC 通信。

**Q: 为什么 macOS 不用 VNC？**
A: VNC 传像素矩形，Retina 屏单帧 24MB，必然卡顿。改用自研被控服务（ScreenCaptureKit + VP9 硬编）根治。

**Q: AI Work 模式什么时候做？**
A: 接口已 trait 预留（WorkProvider 为 stub），规划见 §13.6 后续阶段（P2 路线图）。

### 14.3 变更记录

| 版本 | 日期 | 变更 | 作者 |
|------|------|------|------|
| v1.0 | 2026-07-17 | 初始版本，基于架构文档生成 | - |
| v1.1 | 2026-09-15 | 同步 Agent 助手 Harness 化 P1：run/plan 协议、安全闸门、计划状态机、审计与白名单、新增 SQLite 表；附录顺延为第十四章 | - |
