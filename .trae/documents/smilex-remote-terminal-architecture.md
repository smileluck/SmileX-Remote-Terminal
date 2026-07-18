# SmileX-Remote-Terminal 系统架构设计方案 (SSH + 远程桌面 + AI 运维工具)

## Context

这是一个 **综合运维工具**，融合 SSH 终端（类似 Xshell）、远程桌面（类似 mstsc）、AI 运维助手三大核心能力。

**产品定位**：SSH 命令行 + 远程桌面图形操作 + AI 智能助手 三合一
**核心特性**：
- SSH 模块：直连目标机 SSH 服务（**无需被控服务**），命令行终端
- 远程桌面模块：**混合协议方案**，按被控平台选择最优路径
  - **Windows/Linux**：接入标准 **RDP** 协议，被控端利用系统自带服务（Win 自带 / Linux xrdp），**零额外部署**
  - **macOS**：部署轻量 **被控服务**（ScreenCaptureKit 采集 + VP9 编码 + 自定义流协议），**根治 VNC 卡顿**
- **AI Agent 模块**：智能运维助手，两种模式
  - **Chat 模式**：用户提问 → LLM 回答；**接入当前会话上下文**（可读取终端输出/报错，结合运维场景作答，如"这个报错怎么解决"）
  - **Work 模式**：**预留接口**（AgentProvider trait），未来实现自主执行运维任务的 AI Agent（如自动诊断、自动修复）
  - **LLM 接入**：可配置多 Provider（OpenAI / Claude / 本地 Ollama 等），用户自配 API Key
- 凭据纯本地管理（**无需服务端中转/云端**）
- 支持 Windows/macOS/Linux 三大桌面平台（纯桌面应用）

**协议选型决策**（macOS VNC 卡顿根因分析）：
- **VNC 协议本质缺陷**：传输像素矩形，Retina 屏单帧未压缩 ~24MB，10Mbps 回线传一帧需 20 秒；即使 Tight/ZRLE 编码也无法质变
- **macOS 系统屏幕共享对第三方 VNC 不友好**：标准 VNC 客户端连接会退化到 Raw 编码
- **Windows/Linux 用 RDP 完美**：RDP 传输图形指令而非像素，效率最高，系统自带/成熟
- **macOS 用自研被控服务**：ScreenCaptureKit (macOS 12.3+) 可获取 GPU 加速帧 + VP9 硬编，流畅度远超 VNC
- **混合方案是运维场景最优解**：被控端多为 Win/Linux 服务器（RDP 完美），macOS 作为少数场景用被控服务解决

**术语约定**（避免混淆）：
- **被控服务 (Host Service)**：部署在被控端 macOS 上的轻量程序，负责屏幕捕获+编码+传输，**不叫 Agent**（避免与 AI Agent 混淆）
- **控制端**：本应用（Tauri 桌面程序），发起连接并显示远端桌面
- **AI Agent**：本应用内置的智能运维助手模块（Chat/Work 两种模式）

用户优先级：多标签终端+SSH ≈ 远程桌面 > AI Chat > SFTP > 端口转发 > 密钥管理。

---

## 一、设计原则

1. **三核心协同**：SSH、远程桌面、AI Agent 三大模块独立建设，通过会话上下文打通
2. **分层解耦**：表现层(Vue3) / 应用服务(Rust) / 领域核心(SSH+远程桌面+AI) / 基础设施(存储) 分离
3. **聚焦桌面**：基于 Tauri 单一桌面技术栈，前端与 Rust 后端通过 Tauri IPC 直接通信
4. **按平台选最优路径**：Win/Linux 用标准 RDP（零部署），macOS 用自研被控服务（流畅优先）
5. **AI 可扩展**：LLM 多 Provider 可配；Work 模式 trait 预留，不破坏接口即可扩展
6. **隐私优先**：凭据存 OS 原生钥匙串，配置文件加密，无任何外传；LLM 调用显式用户发起
7. **性能优先**：终端大输出不卡顿（背压/流控），SFTP 并发传输，远程桌面自适应码率

---

## 二、MVP 架构设计

### 2.1 整体分层架构

```
┌─────────────────────────────────────────────────────────────┐
│                    表现层 (Presentation)                      │
│   Vue3 + xterm.js (SSH终端) + Canvas (远程桌面)               │
│   + Chat 面板 (Markdown 渲染) + Naive UI                     │
│   (运行于 Tauri WebView)                                     │
├─────────────────────────────────────────────────────────────┤
│                    应用服务层 (Application)                   │
│   Tauri Commands (Rust) - 通过 IPC 对接前端                  │
│   ├── SSH 命令组 (session/sftp/tunnel/key)                   │
│   ├── 远程桌面命令组 (connect/input/frame/clipboard)         │
│   │   └─ 路由: 按 platform 选 RdpSession / HostSession       │
│   └── AI 命令组 (chat_send/chat_stream/context_provider)     │
├─────────────────────────────────────────────────────────────┤
│                    领域核心层 (Domain Core)                   │
│   ┌────────────────────┐  ┌────────────────────────────┐    │
│   │ ssh-core (Rust)    │  │ remote-desktop-core (Rust) │    │
│   │ ├── SSH (russh)    │  │ ├── RDP 客户端 (ironrdp)   │    │
│   │ ├── PTY 终端       │  │ │   (Win/Linux 被控端)     │    │
│   │ ├── SFTP           │  │ ├── Host 协议客户端         │    │
│   │ ├── 端口转发       │  │ │   (连 macOS 被控服务)    │    │
│   │ └── 密钥管理       │  │ ├── 帧解码 + 缩放          │    │
│   └────────────────────┘  │ ├── 输入事件映射           │    │
│                           │ └── 剪贴板同步             │    │
│                           └────────────────────────────┘    │
│   ┌─────────────────────────────────────────────────────┐   │
│   │ ai-core (Rust crate)                                │   │
│   │ ├── AgentProvider trait (Chat/Work 统一抽象)        │   │
│   │ ├── chat/   Chat 模式实现 (已实现)                  │   │
│   │ │   ├── LLM Client (多 Provider: OpenAI/Claude/Ollama) │
│   │ │   ├── 会话上下文采集 (读终端输出/报错)            │   │
│   │ │   └── 流式响应 (SSE → Tauri event)                │   │
│   │ └── work/  Work 模式 (trait 预留, 未来实现)         │   │
│   └─────────────────────────────────────────────────────┘   │
├─────────────────────────────────────────────────────────────┤
│           被控端 (按平台不同，部署形态不同)                   │
│   ├── Windows/Linux: 系统自带 RDP 服务 (3389端口)            │
│   └── macOS: 自研被控服务 (独立项目 host-service)            │
│       ├── ScreenCaptureKit 屏幕捕获 (GPU加速)                │
│       ├── VP9 硬件编码                                       │
│       ├── 自定义流协议 (TCP+TLS)                             │
│       └── 输入事件注入 (CGEvent)                             │
├─────────────────────────────────────────────────────────────┤
│                    基础设施层 (Infrastructure)                │
│   ├── 配置存储 (SQLite + 加密)                               │
│   ├── 凭据存储 (OS Keyring，含 LLM API Key)                  │
│   └── 日志系统                                               │
└─────────────────────────────────────────────────────────────┘
```

**会话统一调度与上下文打通**：
- SSH 会话、远程桌面会话、AI 会话在应用层由统一的 `SessionRegistry` 管理
- AI Chat 模式可读取当前活跃会话（SSH 终端输出/报错）作为上下文，实现"结合运维场景"的智能问答
- 远程桌面会话按目标平台自动路由：Windows/Linux → RdpSession，macOS → HostSession
- 前端 Tab 按 `kind: 'ssh'|'rdp'|'host'|'chat'` 渲染不同视图组件

### 2.2 多标签终端管理架构

```
TabManager (前端 Pinia store)
  └── Tab[]
        ├── id, title, sessionId, status
        └── TerminalView (xterm.js 实例)
              │
              │  用户键入 → Tauri invoke
              ▼
        SessionManager (Rust 应用层)
          └── SshSession[] (来自 ssh-core)
                ├── channel (russh Channel - PTY)
                ├── 读循环 → 数据回流 → xterm 渲染
                └── 写接口 → 接收前端输入
```

- 每个标签 = 一个独立的 SSH 会话 + 一个 xterm.js 实例
- 后端按 sessionId 路由数据流，实现多会话并发
- 终端输出走 **Tauri Event 推送**（非轮询），带背压控制

### 2.3 SFTP 文件传输架构

```
前端: 文件管理器 UI (树+列表, 拖拽)
  │ 拖拽上传 / 下载请求
  ▼
应用层: SftpService
  └── ssh-core::SftpClient (russh-sftp)
        ├── 并发传输队列 (tokio 任务池, 限流)
        ├── 断点续传 (记录 offset)
        └── 进度回调 → 前端进度条
```

### 2.5 端口转发/隧道架构

```
隧道类型:
  - Local  (-L): 本地端口 → SSH → 远端目标
  - Remote (-R): 远端端口 → SSH → 本地目标
  - Dynamic(-D): SOCKS5 代理, 任意远端目标

实现: ssh-core::TunnelEngine
  ├── TcpListener (本地/远端监听)
  ├── 每个连接 → russh channel (direct-tcpip / forwarded-tcpip)
  └── 双向数据桥接 (tokio::io::copy)
```

### 2.5 SSH 密钥管理架构

```
KeyManager
  ├── 密钥生成 (Ed25519/RSA/ECDSA)
  ├── 密钥导入 (解析 OpenSSH 格式)
  ├── 私钥加密存储 (OS Keyring, 永不落盘明文)
  ├── 公钥分发辅助 (ssh-copy-id 逻辑)
  └── Agent Forwarding (转发 ssh-agent 请求)
```

### 2.6 大输出防 OOM 与虚拟渲染架构

**问题场景**：在终端执行 `cat huge.log`、`find /`、`yes` 等命令时，远端可能推送 GB 级数据。若全部缓存到内存会导致前端 WebView OOM 崩溃。

**核心思路**：三层联动 —— 后端限流背压 + 传输层节流批量 + 前端环形缓冲（可视区虚拟化）+ 超大输出转存文件。

```
┌──────────────────────────────────────────────────────────────┐
│                  数据流与背压控制 (端到端)                     │
├──────────────────────────────────────────────────────────────┤
│                                                                │
│  [远端 SSH channel]                                            │
│       │ 流式数据 (可能 GB)                                      │
│       ▼                                                        │
│  [ssh-core 读循环]  ◄── 有界 mpsc channel (容量如 64)          │
│   ├─ 每帧 ≤ 16KB                                               │
│   ├─ channel 满则暂停读取 russh (天然背压)                     │
│   └─ 超过会话阈值 (如累计 10MB) → 触发"转存模式"               │
│       │                                                        │
│       ▼                                                        │
│  [desktop 节流批处理]                                          │
│   ├─ 合并 16ms 内的多帧为单个 Tauri event (≈60fps)             │
│   ├─ 避免 byte 级 IPC 风暴                                     │
│   └─ 转存模式时：写入本地 .log 文件，event 只发摘要            │
│       │                                                        │
│       ▼                                                        │
│  [前端 xterm.js]                                               │
│   ├─ scrollback 环形缓冲 (默认 10000 行，可配)                 │
│   ├─ 超出上限自动丢弃最旧行 (xterm 内置行为，内存恒定)         │
│   ├─ Canvas 渲染：只绘制可视行 (非 DOM，天然虚拟化)            │
│   └─ 用户回滚时从 scrollback 读，已丢弃的行从转存日志读        │
│                                                                │
└──────────────────────────────────────────────────────────────┘
```

**关键设计点**：

1. **后端有界 channel 背压**
   - 读循环写入 `tokio::sync::mpsc` 有界通道（容量 64 帧）
   - 通道满时 `await` 阻塞，自动暂停读取 russh channel
   - SSH 协议层 window 调整会反向施压远端，从源头限流

2. **传输层节流批处理**
   - 合并时间窗口（16ms）或大小窗口（16KB）内的数据为单次 Tauri event
   - 避免每字节触发一次 IPC（IPC 开销远大于数据本身）

3. **前端 scrollback 环形缓冲（xterm.js 内置）**
   - xterm.js 的 `scrollback` 选项设为有限值（默认 10000，可配）
   - 超出后自动丢弃最旧的行，**内存占用恒定**（这是"虚拟树"思想在终端的体现）
   - xterm.js 用 Canvas/WebGL 渲染，**只绘制可视区域**，不是 DOM 长列表

4. **超大输出转存（防丢数据）**
   - 当单次命令累计输出超过阈值（默认 10MB，可配），切换到"转存模式"
   - 数据写入本地文件 `~/.smilex/logs/<session>-<timestamp>.log`
   - 终端显示前 N 行 + 提示条：「输出过大已转存，完整内容保存到 xxx.log，点击打开」
   - 用户可后续用日志查看器（带虚拟滚动）打开，不占终端内存

5. **内存预算监控**
   - desktop 端定期检查会话缓冲堆积量
   - 前端监控 xterm buffer 行数
   - 超警戒线时自动降低 scrollback 或触发转存

**为什么不直接用"虚拟树长列表"组件？**
- 终端不是结构化行列表，是带 ANSI 颜色/光标控制的字符流，必须由终端模拟器（xterm.js）解析渲染
- xterm.js 本身已用 Canvas 实现可视区虚拟渲染 + 环形缓冲，等价于"虚拟树"
- 真正的 OOM 风险在"保留全量行"，由 scrollback 上限 + 转存机制解决

### 2.7 远程桌面架构 (混合方案: RDP + macOS 被控服务)

**核心策略**：按被控平台选最优路径，统一抽象、对前端透明。

```
┌──────────────────────────────────────────────────────────────┐
│                    前端 (Tauri WebView)                       │
│   RemoteDesktopView.vue (Canvas 统一渲染)                     │
│   不感知后端用哪种协议，只接收 RGBA 帧并绘制                   │
└────────────────────────┬─────────────────────────────────────┘
                         │ Tauri event (帧) + invoke (输入)
┌────────────────────────▼─────────────────────────────────────┐
│              desktop (remote-desktop-core)                    │
│   RemoteDesktopSession trait (统一抽象)                       │
│   ┌─────────────────────────────────────────────────────┐    │
│   │ 按目标平台路由                                        │    │
│   │  ├── Windows/Linux → RdpSession (ironrdp)            │    │
│   │  └── macOS         → HostSession (自研协议客户端)    │    │
│   └─────────────────────────────────────────────────────┘    │
└────────────────────────┬─────────────────────────────────────┘
                         │
        ┌────────────────┴────────────────┐
        │                                 │
┌───────▼───────────┐           ┌─────────▼──────────────────┐
│ 路径 A: RDP 直连   │           │ 路径 B: macOS 被控服务      │
│ (Win/Linux)       │           │ (macOS)                    │
│                   │           │                            │
│ 控制端 ──RDP──► 被控端 │      │ 控制端 ──自研协议──► 被控服务 │
│         系统自带    │           │         (host-service)     │
│         3389端口    │           │         独立部署            │
│                   │           │                            │
│ ✓ 零额外部署       │           │ ✓ ScreenCaptureKit GPU捕获 │
│ ✓ 图形指令高效     │           │ ✓ VP9 硬编 (VideoToolbox)  │
│ ✓ NLA认证          │           │ ✓ CGEvent 输入注入         │
│                   │           │ ✓ 自定义流协议 (TLS加密)    │
│                   │           │ ✓ 根治 VNC 卡顿             │
└───────────────────┘           └────────────────────────────┘
```

**关键设计点**：

1. **统一会话抽象，对前端透明**
   - `remote-desktop-core` 定义 `RemoteDesktopSession` trait
   - `RdpSession`（Win/Linux）和 `HostSession`（macOS）各自实现
   - 应用层根据会话配置的 `platform` 字段自动路由
   - 前端只通过 sessionId 操作，不感知协议差异

2. **路径 A: RDP 直连（Windows/Linux）**
   - 基于 ironrdp，协议握手 + NLA/CredSSP 认证
   - 接收 RDP 图形指令（位图/文字/绘制命令），解码为 RGBA
   - 输入事件打包为 RDP 格式发送
   - 被控端零部署（Win 自带 / Linux 装 xrdp）

3. **路径 B: macOS 被控服务（自研轻量服务）**
   - 独立项目 `host-service`，部署在被控 macOS 上
   - **屏幕捕获**：ScreenCaptureKit (macOS 12.3+)，GPU 加速，零拷贝取帧
   - **视频编码**：VP9 硬件编码（VideoToolbox），码率自适应
   - **传输协议**：自定义二进制流协议 over TCP + TLS 1.3
   - **输入注入**：CGEvent API 注入鼠标/键盘事件
   - **认证**：预共享密钥 + 设备指纹（凭据从控制端 OS Keyring 取）
   - **部署形态**：launchd 守护进程，开机自启，静默运行

4. **帧传输优化（双路径共用）**
   - 后端解码为 RGBA 后节流推送（60fps 上限）
   - 大帧用 base64 或 SharedArrayBuffer 传输（视 Tauri 能力）
   - 前端 Canvas 2D 渲染（MVP）；未来可升级 WebGL 纹理渲染

5. **自适应画质（双路径共用）**
   - 实时监测带宽和延迟
   - RDP: 调整 color_depth、分辨率、禁用壁纸
   - 被控服务: 调整 VP9 码率、帧率、捕获区域

6. **输入事件映射（双路径共用）**
   - 前端监听 Canvas 上的 mousemove/down/up/wheel/keypress
   - 坐标按缩放比例反算回远端分辨率
   - RDP 路径：打包为 RDP 输入事件
   - 被控服务路径：打包为自定义协议的输入消息

7. **会话生命周期**
   - 连接成功后持续推送帧直到断开
   - 断线自动重连（可配次数和间隔）
   - 标签关闭时立即释放会话资源（RDP 连接 / 被控服务连接）

**独立标签与会话平级**：
- 前端 Tab 增加 `kind: 'ssh' | 'rdp' | 'host'` 字段（rdp 和 host 在 UI 上统一显示为"远程桌面"，仅后端路由不同）
- ConnectManager 中会话配置区分类型（SSH / 远程桌面-Win&Linux / 远程桌面-macOS）
- Tab 渲染时按 kind 切换 TerminalView / RemoteDesktopView 组件

### 2.8 AI Agent 模块架构 (Chat 已实现 + Work 预留)

**双模式设计**：Chat 模式即问即答（结合运维上下文），Work 模式预留接口未来做自主执行。

```
┌──────────────────────────────────────────────────────────────┐
│                    前端 (Tauri WebView)                       │
│   ChatPanel.vue (侧边栏/独立标签)                             │
│   ├── 消息列表 (Markdown 渲染 + 代码块高亮)                   │
│   ├── 输入框 + 发送 + "附带当前会话上下文"开关                 │
│   └── Provider/模型选择下拉框                                 │
└────────────────────────┬─────────────────────────────────────┘
                         │ Tauri invoke (发送) + event (流式响应)
┌────────────────────────▼─────────────────────────────────────┐
│              desktop (ai-core)                                │
│   AgentProvider trait (统一抽象, Chat/Work 都实现此接口)      │
│   ┌──────────────────────────────────────────────────────┐   │
│   │ 按模式路由                                            │   │
│   │  ├── Chat 模式 → ChatProvider (已实现)               │   │
│   │  └── Work 模式 → WorkProvider (trait 预留, 未实现)   │   │
│   └──────────────────────────────────────────────────────┘   │
└────────────────────────┬─────────────────────────────────────┘
                         │
              ┌──────────┴──────────┐
              │                     │
┌─────────────▼─────────┐ ┌─────────▼──────────────────────────┐
│ Chat 模式数据流(已实现) │ │ Work 模式(预留)                     │
│                       │ │                                    │
│ 1. 接收用户问题        │ │ 未来:                              │
│ 2. (可选)采集上下文    │ │  - 自主调用 SSH 命令               │
│    ┌───────────────┐  │ │  - 自主操作远程桌面                │
│    │ ContextProvider│  │ │  - 自动诊断/修复                   │
│    │ 读取当前活跃   │  │ │  - 工具调用 (function calling)     │
│    │ SSH 终端最近   │  │ │ 接口已预留, 实现时不改 trait       │
│    │ N 行输出/报错  │  │ │                                    │
│    └───────────────┘  │ │                                    │
│ 3. 组装 prompt        │ │                                    │
│ 4. 调用 LLM (流式)    │ │                                    │
│ 5. SSE → event 推送   │ │                                    │
└───────────────────────┘ └────────────────────────────────────┘
```

**关键设计点**：

1. **AgentProvider trait —— Chat/Work 统一抽象**
   - 定义统一的会话生命周期与消息收发接口
   - `ChatProvider` 现已实现；`WorkProvider` 未来实现时无需改 trait
   - 支持 streaming（流式响应）和 non-streaming 两种调用

2. **LLM Client —— 多 Provider 可配**
   - 抽象 `LlmClient` trait，屏蔽不同 Provider 差异
   - 内置适配器：OpenAI、Anthropic Claude、Ollama（本地）
   - 用户在设置页配置 Provider + 模型 + API Key（Key 存 OS Keyring）
   - 支持自定义 BaseURL（兼容 OpenAI 协议的第三方服务，如 DeepSeek、智谱等）

3. **会话上下文采集 (ContextProvider)**
   - Chat 发送时可勾选"附带当前会话上下文"
   - 上下文来源：当前活跃 SSH 会话的终端最近 N 行输出（含报错）
   - 上下文来源：远程桌面会话暂不采集（隐私+无文本）
   - 采集逻辑在 desktop 层实现，通过 SessionRegistry 按 sessionId 取终端 buffer 尾部
   - 上下文随用户问题一起组装进 prompt，让 AI 结合实际运维场景作答

4. **流式响应**
   - 后端用 reqwest 的 streaming 能力接收 LLM 的 SSE 流
   - 每收到一个 token 立即通过 Tauri event 推送到前端
   - 前端逐字渲染（Markdown 增量解析），体验流畅

5. **对话历史管理**
   - 每个会话（sessionId）维护独立的对话历史（多轮上下文）
   - 历史存内存，会话关闭即清；可选持久化到 SQLite
   - 超长历史自动截断（保留最近 N 轮 + 系统提示）

6. **安全与隐私**
   - LLM 调用**显式由用户发起**（点发送才调用），无后台自动调用
   - API Key 存 OS Keyring，永不落盘明文
   - 上下文采集需用户勾选确认，默认不附带（避免误传敏感信息）
   - 本地 Ollama 模式数据完全不出本机

**独立标签**：
- 前端 Tab 增加 `kind: 'chat'`，AI 面板作为独立标签平级管理
- 也可作为侧边栏常驻（与当前 SSH/远程桌面会话联动上下文）

---

## 三、技术选型分析

### 3.1 SSH 协议库

| 候选 | 评价 | 选择 |
|------|------|------|
| **russh** | 纯 Rust、Tokio 原生异步、内置 SFTP/端口转发/Agent、用 ring 不依赖 OpenSSL(Windows 编译无痛)、Tabby 作者维护 | **✅ 选定** |
| libssh (FFI) | C 库绑定，需系统依赖，Windows 编译复杂 | ❌ |
| 调用系统 ssh 命令 | 跨平台行为不一致、无法精细控制 | ❌ |
| thrussh | russh 前身，已停更 | ❌ |

### 3.2 终端模拟器

| 候选 | 评价 | 选择 |
|------|------|------|
| **xterm.js** | 事实标准、VSCode/Hyper 都用、性能好、ANSI 完整支持 | **✅ 选定** |
| tabby-terminal | 与 Tabby 耦合 | ❌ |

### 3.3 前端框架与 UI

| 层面 | 选择 | 理由 |
|------|------|------|
| 框架 | **Vue 3 + TypeScript** | Composition API 适合复杂状态、生态成熟 |
| UI 库 | **Naive UI** | 原生 TS、暗色模式、组件丰富、无 CSS-in-JS 运行时开销 |
| 状态 | **Pinia** | Vue3 官方推荐，TS 友好 |
| 构建 | **Vite** | 极速 HMR |

### 3.4 桌面端壳

| 选择 | 理由 |
|------|------|
| **Tauri 2.x** | 包体小(<20MB)、Rust 后端可直接调 russh、原生 WebView 性能好、跨三平台 |

### 3.5 配置与凭据存储

| 数据类型 | 方案 | 理由 |
|---------|------|------|
| 会话配置(主机/端口/用户名/选项) | **SQLite (rusqlite)** | 结构化查询、事务、单文件易备份 |
| 密码/私钥 passphrase | **OS Keyring (keyring crate)** | 调用 Win Credential Store / macOS Keychain / Linux libsecret，永不落盘明文 |
| 私钥文件本身 | 加密文件 + 密钥存 Keyring | 或直接引用用户 `~/.ssh/` 已有私钥 |
| known_hosts | **SQLite 表** | 校验远端主机指纹 |

**否决方案**：SQLite 明文存密码（不安全）、Stronghold（需额外主密码，体验差）。

### 3.6 远程桌面协议库与被控服务

#### RDP 协议库（控制端，用于连 Win/Linux 被控端）

| 候选 | 评价 | 选择 |
|------|------|------|
| **ironrdp** | Devolutions 开源、纯 Rust、Tokio 异步、活跃维护、支持 NLA/CredSSP/RDP 10.x、组件化（可只用需要的模块） | **✅ 选定** |
| rdp-rs | 更新缓慢、协议版本支持旧 | ❌ |
| 调用 FreeRDP (FFI) | C 库依赖大、Windows 编译复杂 | ❌ |

#### macOS 被控服务技术栈（独立项目 host-service）

| 能力 | 技术选型 | 说明 |
|------|---------|------|
| **屏幕捕获** | ScreenCaptureKit (macOS 12.3+) | 苹果新一代 API，GPU 加速、零拷贝，性能远超 CGWindowListCreateImage |
| **视频编码** | VideoToolbox (VP9 硬编) | 硬件编码，CPU 占用低；VP9 压缩率优于 H.264，适合屏幕内容 |
| **网络传输** | Tokio + rustls (TLS 1.3) | 纯 Rust 异步网络 + TLS 加密 |
| **输入注入** | CGEvent API | 苹果原生事件注入，支持鼠标/键盘/滚轮 |
| **部署形态** | launchd 守护进程 | 开机自启、崩溃重启、静默运行 |
| **权限申请** | 屏幕录制权限 + 辅助功能权限 | 首次运行引导用户授权 |

#### 控制端连接 macOS 被控服务的协议客户端（在 remote-desktop-core 内）

| 能力 | 技术选型 | 说明 |
|------|---------|------|
| **协议客户端** | 自研 HostClient (Tokio + rustls) | 与 host-service 的自定义协议对接 |
| **帧解码** | VP9 解码 (dav1d/libvpx via FFI 或 rustls 配套) | 解码被控服务推送的 VP9 帧 |

#### 前端渲染（双路径共用）

| 候选 | 评价 | 选择 |
|------|------|------|
| **Canvas 2D** | 兼容性最好、实现简单、对中等画质够用 | **✅ MVP 选定** |
| WebGL 纹理渲染 | 性能更高、支持 YUV 直接渲染，但复杂 | 未来优化 |

**说明**：MVP 用 Canvas 2D 渲染后端解码好的 RGBA 帧足够；未来若需更高帧率/更低延迟，可升级到 WebGL 纹理渲染。VNC 方案已彻底放弃（macOS VNC 卡顿无法根治，Linux 用 RDP 更优）。

### 3.7 AI/LLM 技术选型

#### LLM HTTP 客户端库

| 候选 | 评价 | 选择 |
|------|------|------|
| **reqwest + eventsource-stream** | 纯 Rust、Tokio 异步、支持 SSE 流式解析、生态成熟 | **✅ 选定** |
| async-openai | 专为 OpenAI 封装，但耦合单一 Provider，扩展性差 | ❌ |
| 各 Provider 官方 SDK | 多语言混乱、无统一抽象 | ❌ |

**设计**：自研 `LlmClient` trait + 各 Provider 适配器，统一在 reqwest 之上实现，支持 streaming。

#### LLM Provider 适配（用户可配）

| Provider | 协议 | 适用场景 | 实现 |
|----------|------|---------|------|
| **OpenAI 兼容** | OpenAI Chat Completions API | GPT 系列、DeepSeek、智谱、通义千问等（均兼容此协议） | ✅ 内置适配器 |
| **Anthropic Claude** | Claude Messages API | Claude 系列（协议不同，单独适配） | ✅ 内置适配器 |
| **Ollama (本地)** | Ollama REST API | 本地模型、零费用、数据不出本机 | ✅ 内置适配器 |
| 自定义 | 用户填 BaseURL + 协议类型 | 任意兼容服务 | ✅ 支持配置 |

#### 前端 Markdown 渲染

| 候选 | 评价 | 选择 |
|------|------|------|
| **markdown-it + highlight.js** | 解析快、插件生态丰富、代码块高亮好（AI 回答常含代码） | **✅ 选定** |
| marked | 轻量但扩展性弱 | ❌ |
| 预渲染(后端渲染 HTML) | 流式体验差，需前端再渲染 | ❌ |

**说明**：AI 回答多为 Markdown + 代码块，前端用 markdown-it 解析 + highlight.js 高亮，支持流式增量渲染。

#### 对话历史存储

| 方案 | 评价 | 选择 |
|------|------|------|
| **内存 (HashMap<sessionId, Vec<Message>>)** | 快、简单、会话关闭即清 | ✅ MVP 选定 |
| SQLite 持久化 | 跨重启保留历史，但增加复杂度 | 未来可选 |

---

## 四、模块划分

```
ssh-core (领域核心, Rust crate)
├── connection/    SSH 连接管理 (russh Session 生命周期)
├── terminal/      PTY 会话 (channel + 读循环 + 写接口)
├── sftp/          SFTP 客户端 (目录遍历/上传/下载)
├── tunnel/        端口转发引擎 (Local/Remote/Dynamic)
├── keys/          密钥管理 (生成/导入/Agent Forwarding)
└── error/         统一错误类型

remote-desktop-core (领域核心, Rust crate)
├── session/       远程桌面会话抽象 (RemoteDesktopSession trait)
│   ├── rdp/       RDP 实现 (ironrdp 封装，连 Win/Linux)
│   └── host/      macOS 被控服务协议客户端 (自研协议，连 host-service)
├── frame/         帧解码 (协议格式 → RGBA)
├── input/         输入事件映射 (本地事件 → 协议格式)
├── adaptive/      自适应码率/画质控制
├── clipboard/     剪贴板同步
└── error/         统一错误类型

host-service (独立项目, 部署在被控 macOS 上)
├── capture/       屏幕捕获 (ScreenCaptureKit 封装)
├── encoder/       视频编码 (VideoToolbox VP9 硬编)
├── protocol/      自定义流协议 (服务端实现)
├── input/         输入注入 (CGEvent)
├── auth/          认证 (预共享密钥 + 设备指纹)
└── launcher/      launchd 守护进程配置与安装

ai-core (领域核心, Rust crate)
├── provider/      AgentProvider trait (Chat/Work 统一抽象)
│   ├── chat/      Chat 模式实现 (已实现)
│   │   ├── llm/   LLM 客户端 (LlmClient trait + 各 Provider 适配器)
│   │   ├── context/ 会话上下文采集 (ContextProvider, 读终端输出)
│   │   └── history/ 对话历史管理 (多轮上下文, 内存)
│   └── work/      Work 模式 (trait 预留, 未来实现自主运维)
└── error/         统一错误类型

desktop (Tauri 应用)
├── commands/      Tauri invoke 处理器 (对接前端)
│   ├── session_cmd     SSH 会话命令
│   ├── sftp_cmd        SFTP 命令
│   ├── tunnel_cmd      隧道命令
│   ├── key_cmd         密钥命令
│   ├── desktop_cmd     远程桌面命令 (按 platform 路由 RDP/Host)
│   ├── ai_cmd          AI 命令 (chat_send/chat_stream/clear_history)
│   └── registry        统一会话注册表 (SSH + RDP + Host + Chat 共用 sessionId 空间)
├── events/        Tauri event 推送 (终端输出/传输进度/桌面帧/AI 流式响应)
└── storage/       SQLite + Keyring 封装 (含 LLM API Key)

frontend (Vue3, 运行于 Tauri WebView)
├── views/         会话列表/终端/远程桌面/SFTP/AI 助手/设置
├── components/
│   ├── terminal/     xterm.js 终端组件
│   ├── desktop/      远程桌面组件 (Canvas 渲染/工具栏)
│   ├── ai/           AI 助手组件 (ChatPanel/消息列表/Markdown 渲染)
│   ├── sftp/         文件管理
│   ├── tunnel/       隧道管理
│   └── common/       通用组件
├── stores/        tabs/session/config/transfer (Pinia, tabs 含 kind: ssh|rdp|host|chat)
├── composables/   useTerminal/useDesktop/useChat/useSftp/useTunnel
└── services/      Tauri IPC 封装 (SSH 组 + 远程桌面组 + AI 组)
```

**依赖方向**：
- frontend → Tauri IPC → desktop → (ssh-core | remote-desktop-core | ai-core) → (russh | ironrdp | HostClient | reqwest/LLM)
- ai-core 的 ContextProvider 通过 desktop 层的 SessionRegistry 读取 ssh-core 的终端 buffer（跨模块协作）
- host-service 独立部署在被控 macOS，通过自研协议与 remote-desktop-core 的 HostClient 通信

---

## 五、数据流程设计

### 5.1 SSH 连接建立流程

```
用户点"连接"
  │ 前端 invoke('session_connect', config)
  ▼
desktop::session_cmd::connect
  │ 从 Keyring 取密码/密钥passphrase
  │ 加载私钥
  ▼
ssh-core::connection::connect
  │ russh handshake
  │ known_hosts 校验 (首次询问/自动拒绝)
  │ 算法协商 (curve25519-sha256/chacha20-poly1305)
  ▼
成功 → 开启 PTY channel (xterm-256color, 尺寸)
  │ 保存 sessionId → SessionManager
  ▼
返回 sessionId 给前端 → 打开新 Tab → 创建 xterm 实例
```

### 5.2 终端数据交互流程

```
[输入] 用户键入 'l','s','\n'
  │ xterm onData
  ▼ invoke('session_input', sessionId, data)
desktop → ssh-core::terminal::write(sessionId, data)
  │ russh channel.data()
  ▼ 远端 shell 处理

[输出] 远端回显 'file1 file2\r\n'
  │ russh channel.data() 回调
  ▼ ssh-core 读循环 → desktop events.emit('term_data', sessionId, chunk)
前端监听 event → xterm.write(chunk) → 渲染
```

**背压**：前端缓冲超过阈值时，通知后端暂停读取，避免大输出 OOM。

### 5.3 SFTP 文件传输流程

```
浏览远端目录
  │ invoke('sftp_list', sessionId, path)
  ▼ ssh-core::sftp::read_dir → 返回 entry[]
前端渲染文件列表

上传文件 (拖拽)
  │ invoke('sftp_upload', sessionId, localPath, remotePath)
  ▼ ssh-core 创建 tokio 任务
     ├─ 分块读取本地文件
     ├─ sftp.put(offset+=chunk)
     └─ 进度回调 → event('transfer_progress')
前端显示进度条, 完成后刷新目录

下载文件 (逆向, sftp.get → 本地写入)
```

### 5.4 端口转发工作流程

```
用户配置 Local: localhost:8080 → 远端 db.internal:3306
  │ invoke('tunnel_open', {type:'local', local:8080, remote:'db.internal:3306'})
  ▼ ssh-core::tunnel::open_local
     ├─ tokio TcpListener bind 127.0.0.1:8080
     └─ 每个接入连接:
        ├─ russh channel.open_direct_tcpip('db.internal', 3306)
        └─ 双向 copy bidirectional (本地socket ↔ channel)

Dynamic (SOCKS5): 同上但解析 SOCKS5 协议获取目标地址
Remote: SSH 服务端监听, 通过 forwarded-tcpip 请求回到本地
```

---

## 六、安全架构

### 6.1 凭据本地加密存储

| 数据 | 存储位置 | 加密方式 |
|------|---------|---------|
| 会话主机/端口/用户名 | SQLite (config.db) | 应用层 AES-256-GCM 加密整个 db 文件(可选) |
| 登录密码 | OS Keyring | 由 OS 保护 (用户登录密码派生) |
| 私钥 passphrase | OS Keyring | 同上 |
| 私钥文件 | `~/.smilex/keys/` 或引用 `~/.ssh/` | OpenSSH 格式(本身加密) |

**原则**：私钥/passphrase **永不落盘明文**，永不外传网络。

### 6.2 SSH 协议安全

- **known_hosts 校验**：首次连接展示指纹(SHA256)，用户确认后入库；后续连接比对，不匹配告警拒绝(防中间人)
- **算法白名单**：默认禁用过时算法(rsa-sha1, diffie-hellman-group1)，优先 curve25519/chacha20-poly1305/ed25519
- **密钥轮换**：SSH 协议自带 rekey，russh 自动处理

---

## 七、项目目录结构

```
SmileX-Remote-Terminal/
├── Cargo.toml                    # Rust workspace 根
├── package.json                  # 前端 workspace 根
├── README.md
├── .github/workflows/            # CI (build/release)
├── docs/
│   ├── architecture.md
│   ├── ssh-protocol.md
│   └── development.md
│
├── crates/                       # Rust 工作空间
│   ├── ssh-core/          # 领域核心 (Rust crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── connection/       # SSH 连接管理
│   │       │   ├── mod.rs
│   │       │   ├── manager.rs    # SessionManager
│   │       │   └── session.rs    # 单个 SSH 会话封装
│   │       ├── terminal/         # PTY 终端
│   │       │   ├── mod.rs
│   │       │   ├── pty.rs        # PTY channel 管理
│   │       │   └── stream.rs     # 输入输出流 + 背压
│   │       ├── sftp/             # SFTP 文件传输
│   │       │   ├── mod.rs
│   │       │   ├── client.rs     # SFTP 操作封装
│   │       │   └── transfer.rs   # 传输队列 + 断点续传
│   │       ├── tunnel/           # 端口转发引擎
│   │       │   ├── mod.rs
│   │       │   ├── local.rs      # Local 转发
│   │       │   ├── remote.rs     # Remote 转发
│   │       │   └── dynamic.rs    # SOCKS5 动态转发
│   │       ├── keys/             # 密钥管理
│   │       │   ├── mod.rs
│   │       │   ├── generate.rs   # 密钥生成
│   │       │   ├── parse.rs      # OpenSSH 格式解析
│   │       │   └── agent.rs      # Agent Forwarding
│   │       ├── provider/         # 协议提供者抽象 (扩展用)
│   │       │   └── mod.rs        # ProtocolProvider trait
│   │       └── error.rs          # 统一错误类型
│   │
│   ├── remote-desktop-core/      # 远程桌面核心 (Rust crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── session/          # 远程桌面会话抽象
│   │       │   ├── mod.rs        # RemoteDesktopSession trait
│   │       │   ├── rdp/          # RDP 实现 (连 Win/Linux 被控端)
│   │       │   │   ├── mod.rs
│   │       │   │   ├── client.rs # ironrdp 客户端封装
│   │       │   │   └── nla.rs    # NLA/CredSSP 认证
│   │       │   └── host/         # macOS 被控服务协议客户端
│   │       │       ├── mod.rs
│   │       │       ├── client.rs # 自研协议客户端 (连 host-service)
│   │       │       └── decode.rs # VP9 帧解码
│   │       ├── frame/            # 帧解码
│   │       │   ├── mod.rs
│   │       │   ├── decoder.rs    # 协议帧 → RGBA
│   │       │   └── scaler.rs     # 分辨率缩放
│   │       ├── input/            # 输入事件映射
│   │       │   ├── mod.rs
│   │       │   ├── mouse.rs      # 鼠标事件 → 协议格式
│   │       │   └── keyboard.rs   # 键盘事件 → 协议格式
│   │       ├── adaptive.rs       # 自适应码率/画质控制
│   │       ├── clipboard.rs      # 剪贴板同步
│   │       └── error.rs          # 统一错误类型
│   │
│   ├── ai-core/                   # AI Agent 核心 (Rust crate)
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── provider/          # AgentProvider 统一抽象
│   │       │   ├── mod.rs         # AgentProvider trait + AgentMode(Chat/Work)
│   │       │   ├── chat/          # Chat 模式 (已实现)
│   │       │   │   ├── mod.rs
│   │       │   │   ├── provider.rs   # ChatProvider 实现 AgentProvider
│   │       │   │   ├── llm/       # LLM 客户端
│   │       │   │   │   ├── mod.rs    # LlmClient trait
│   │       │   │   │   ├── openai.rs # OpenAI 兼容适配器
│   │       │   │   │   ├── claude.rs # Anthropic Claude 适配器
│   │       │   │   │   └── ollama.rs # 本地 Ollama 适配器
│   │       │   │   ├── context.rs # ContextProvider (采集终端上下文)
│   │       │   │   └── history.rs # 对话历史 (多轮上下文)
│   │       │   └── work/          # Work 模式 (trait 预留)
│   │       │       └── mod.rs     # WorkProvider 占位 (未来实现)
│   │       └── error.rs           # 统一错误类型
│   │
│   └── desktop/           # Tauri 桌面应用
│       ├── Cargo.toml
│       ├── tauri.conf.json
│       └── src/
│           ├── main.rs           # Tauri 入口
│           ├── commands/         # Tauri invoke 处理器
│           │   ├── mod.rs
│           │   ├── registry.rs   # 统一会话注册表 (SSH+RDP+Host+Chat)
│           │   ├── session.rs    # SSH 会话命令
│           │   ├── sftp.rs       # SFTP 命令
│           │   ├── tunnel.rs     # 隧道命令
│           │   ├── key.rs        # 密钥命令
│           │   ├── desktop.rs    # 远程桌面命令 (按 platform 路由 RDP/Host)
│           │   └── ai.rs         # AI 命令 (chat_send/stream/clear_history)
│           ├── events.rs         # 事件推送封装
│           └── storage/
│               ├── mod.rs
│               ├── sqlite.rs     # 配置存储
│               └── keyring.rs    # 凭据存储 (含 LLM API Key)
│
├── host-service/                 # macOS 被控服务 (独立项目, 部署在被控端)
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs               # 服务入口
│       ├── capture/              # 屏幕捕获
│       │   ├── mod.rs
│       │   └── screencapturekit.rs # ScreenCaptureKit 封装 (Swift FFI)
│       ├── encoder/              # 视频编码
│       │   ├── mod.rs
│       │   └── videotoolbox.rs   # VideoToolbox VP9 硬编 (Swift FFI)
│       ├── protocol/             # 自定义流协议服务端
│       │   ├── mod.rs
│       │   ├── server.rs         # TCP+TLS 监听
│       │   ├── frame.rs          # 帧封包/解包
│       │   └── message.rs        # 消息类型定义 (帧/输入/剪贴板/控制)
│       ├── input/                # 输入注入
│       │   ├── mod.rs
│       │   ├── mouse.rs          # CGEvent 鼠标注入
│       │   └── keyboard.rs       # CGEvent 键盘注入
│       ├── auth/                 # 认证
│       │   ├── mod.rs
│       │   └── psk.rs            # 预共享密钥 + 设备指纹
│       └── launcher/             # 部署相关
│           ├── launchd.plist     # launchd 守护进程配置
│           └── installer.rs      # 安装/卸载脚本
│
├── app/                          # 前端 (运行于 Tauri WebView)
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   ├── index.html
│   └── src/
│       ├── main.ts
│       ├── App.vue
│       ├── router/index.ts
│       ├── services/             # Tauri IPC 封装
│       │   ├── invoke.ts         # invoke 调用封装 (按命令分组)
│       │   └── event.ts          # event 监听封装
│       ├── stores/               # Pinia
│       │   ├── tabs.ts           # 标签管理 (kind: ssh|rdp|host|chat)
│       │   ├── session.ts        # 会话状态
│       │   ├── config.ts         # 配置 (含 LLM Provider 配置)
│       │   └── transfer.ts       # 传输进度
│       ├── views/
│       │   ├── ConnectManager.vue   # 会话管理/连接 (含 SSH + 远程桌面配置)
│       │   ├── Terminal.vue         # 终端主视图(多标签)
│       │   ├── RemoteDesktop.vue    # 远程桌面主视图(多标签, RDP/Host 统一)
│       │   ├── AiAssistant.vue      # AI 助手主视图 (Chat/Work 模式切换)
│       │   ├── Sftp.vue             # 文件管理
│       │   ├── Tunnels.vue          # 隧道管理
│       │   ├── Keys.vue             # 密钥管理
│       │   └── Settings.vue         # 设置 (含 LLM Provider/模型/Key 配置)
│       ├── components/
│       │   ├── terminal/
│       │   │   ├── TerminalTab.vue
│       │   │   └── TerminalView.vue # xterm.js 封装
│       │   ├── desktop/             # 远程桌面组件
│       │   │   ├── DesktopView.vue  # Canvas 渲染主组件
│       │   │   ├── DesktopToolbar.vue # 工具栏 (全屏/缩放/剪贴板/截图)
│       │   │   └── QualityIndicator.vue # 网络质量指示器
│       │   ├── ai/                  # AI 助手组件
│       │   │   ├── ChatPanel.vue    # Chat 面板 (消息列表+输入框+上下文开关)
│       │   │   ├── MessageBubble.vue # 单条消息 (Markdown 渲染+代码高亮)
│       │   │   ├── ContextToggle.vue # "附带当前会话上下文"开关
│       │   │   └── ProviderSelect.vue # LLM Provider/模型选择
│       │   ├── sftp/
│       │   │   ├── FileExplorer.vue
│       │   │   ├── TransferQueue.vue
│       │   │   └── PathBar.vue
│       │   ├── tunnel/
│       │   │   └── TunnelCard.vue
│       │   ├── viewer/
│       │   │   └── LogViewer.vue     # 大日志文件查看器(虚拟滚动)
│       │   └── common/
│       │       ├── HostInput.vue
│       │       └── KeyPicker.vue
│       ├── composables/
│       │   ├── useTerminal.ts
│       │   ├── useDesktop.ts        # 远程桌面 Canvas + 输入处理
│       │   ├── useChat.ts           # AI Chat 流式响应处理
│       │   ├── useSftp.ts
│       │   └── useTunnel.ts
│       ├── types/                # TS 类型定义
│       │   ├── session.ts        # 含 SessionKind: ssh|rdp|host|chat
│       │   ├── desktop.ts        # 远程桌面类型 (帧/输入事件)
│       │   ├── ai.ts             # AI 类型 (消息/Provider/模式)
│       │   ├── sftp.ts
│       │   └── tunnel.ts
│       └── styles/
│
└── scripts/
    ├── build-desktop.sh          # 打包桌面端 (三平台)
    └── dev.sh                    # 开发模式启动
```

---

## 八、扩展性设计

### 8.1 协议提供者抽象（为远程桌面/其他协议预留）

```rust
/// 协议提供者 trait - 不同协议(SSH/RDP/Host/...)实现此接口
/// 已实现 SshProvider + RdpProvider + HostProvider(macOS 被控服务)
pub trait ProtocolProvider: Send + Sync {
    /// 建立连接
    async fn connect(&self, config: &ConnectionConfig) -> Result<Box<dyn Connection>>;
    /// 协议类型标识
    fn protocol_type(&self) -> ProtocolType;
}

/// 连接抽象 - SSH会话/远程桌面会话的统一接口
pub trait Connection: Send {
    /// 创建一个交互通道(终端/桌面流)
    async fn open_channel(&self, kind: ChannelKind) -> Result<Box<dyn Channel>>;
    async fn close(&self) -> Result<()>;
}

/// 通道种类枚举 - 已覆盖 SSH 与远程桌面
pub enum ChannelKind {
    Terminal,       // SSH PTY
    Sftp,
    Tunnel,
    RemoteDesktop,  // RDP(Win/Linux) 或 Host(macOS 被控服务) 桌面流 (已实现)
}
```

已实现 `SshProvider` + `RdpProvider` + `HostProvider`，未来可继续扩展 SerialProvider 等。

### 8.2 会话种类（已落地）

```typescript
// 前端 Tab 已支持四种会话类型，平级管理
type SessionKind = 'ssh' | 'rdp' | 'host' | 'chat'
// 已实现；rdp 连 Win/Linux，host 连 macOS 被控服务，chat 为 AI 助手；未来可加 'serial'
```

### 8.3 插件化架构（轻量预留）

- 前端：`services/` 下的 invoke/event 封装按命令分组，便于插件扩展自定义命令
- 后端：`ssh-core` / `remote-desktop-core` / `ai-core` 暴露 `register_provider()` 动态注册
- 暂不实现插件加载器，只保证接口可扩展

### 8.4 AI Agent 扩展性（Chat 已实现，Work trait 预留）

```rust
/// AI Agent 提供者 trait - Chat/Work 模式统一抽象
/// 已实现 ChatProvider；WorkProvider 未来实现，无需改此 trait
pub trait AgentProvider: Send + Sync {
    /// 模式标识
    fn mode(&self) -> AgentMode;
    /// 发送消息（流式响应通过回调推送）
    async fn send(&self, msg: &str, ctx: &Context, on_token: Box<dyn Fn(String)>) -> Result<()>;
    /// 中断当前生成
    async fn abort(&self) -> Result<()>;
    /// 清空对话历史
    async fn clear(&self) -> Result<()>;
}

/// Agent 模式枚举
pub enum AgentMode {
    Chat,   // 已实现：即问即答 + 运维上下文
    Work,   // 预留：未来实现自主执行运维任务
}

/// 运维上下文（Chat 模式用）
pub struct Context {
    pub session_id: Option<String>,         // 关联的 SSH/远程桌面会话
    pub terminal_output: Option<String>,    // 终端最近 N 行输出
    pub include_context: bool,              // 用户是否勾选附带上下文
}
```

**扩展方式**：
- 新增 LLM Provider：实现 `LlmClient` trait，在设置页注册
- 实现 Work 模式：实现 `AgentProvider` trait 的 `WorkProvider`，注册到 ai-core
- Work 模式未来可结合 function calling 自主调用 SSH 命令/远程桌面操作

---

## 九、MVP 开发路线图

### 阶段 1: 基础骨架 + 单会话打通 (核心)
- 搭建 monorepo (Cargo workspace + pnpm workspace)
- `ssh-core::connection` 实现 russh 连接/认证
- `remote-desktop-core::session::rdp` 实现 ironrdp 连接/认证
- `desktop` Tauri 工程 + 基础窗口 + 统一会话注册表
- 前端 `services/invoke` + TerminalView (xterm.js) + DesktopView (Canvas)
- 实现一个硬编码 SSH 会话 + 一个硬编码 RDP 会话
- **交付**：能 SSH 敲命令 + 能 RDP 看到远端桌面

### 阶段 2: 会话配置管理 + AI Chat 基础
- SQLite schema (sessions/known_hosts，含 kind 字段区分 ssh/rdp/host/chat)
- OS Keyring 封装 (存密码/passphrase/被控服务预共享密钥/LLM API Key)
- ConnectManager UI (增删改查 SSH + RDP + Host 会话、连接测试)
- known_hosts 首次确认/校验（SSH）；RDP 证书校验；Host 服务指纹校验
- **AI Chat 基础**：`ai-core::provider::chat` 实现 LlmClient(OpenAI 兼容) + ChatProvider；前端 ChatPanel + markdown 渲染
- 设置页：LLM Provider/模型/BaseURL/API Key 配置
- **交付**：可视化配置多类会话 + AI 能基础问答

### 阶段 3: 多标签 + 终端/桌面体验优化 + AI 上下文打通
- TabManager (Pinia，含 kind 字段) + 多 xterm/Canvas 实例
- 终端尺寸同步 (resize → window-change)；RDP 分辨率协商
- 终端背压控制；RDP 帧节流推送
- 主题/字体/配色设置；远程桌面缩放模式 (contain/stretch/1:1)
- **AI 上下文打通**：ContextProvider 实现读终端最近 N 行输出；Chat 流式响应(SSE→event)；多轮对话历史
- **交付**：流畅多标签终端 + 流畅 RDP 远程桌面 + AI 结合运维上下文作答

### 阶段 4: SFTP 文件传输 + 剪贴板同步
- `ssh-core::sftp` 实现
- FileExplorer UI (双面板: 本地/远端)
- 拖拽上传/下载 + 进度条 + 队列 + 断点续传
- RDP 剪贴板双向同步
- **交付**：完整文件管理 + RDP 跨端剪贴板

### 阶段 5: 端口转发 + 密钥管理 + macOS 被控服务 + Work 接口预留
- `ssh-core::tunnel` (Local/Remote/Dynamic)
- Tunnels UI (启停/状态)
- 密钥生成/导入/分发辅助
- **macOS 被控服务（host-service）开发**：
  - ScreenCaptureKit 屏幕捕获 + VideoToolbox VP9 硬编
  - 自定义流协议服务端 + CGEvent 输入注入
  - launchd 守护进程 + 权限引导（屏幕录制/辅助功能）
  - 控制端 `remote-desktop-core::session::host` 协议客户端实现
- **AI Work 接口预留**：定义 `WorkProvider` trait 占位，不实现（未来自主运维）
- **交付**：功能完整的 MVP（SSH 全功能 + RDP + macOS 被控服务 + AI Chat，全平台流畅）

---

## 十、关键技术难点及解决方案

| 难点 | 解决方案 |
|------|---------|
| 终端大输出 OOM/卡顿 | 详见 §2.6：后端有界 channel 背压 + 节流批处理 + xterm scrollback 环形缓冲 + 超大输出转存文件 |
| 多标签会话状态管理 | 后端统一 SessionRegistry 用 HashMap<sessionId, Box<dyn Connection>>，前端 Pinia tabs store (含 kind) 映射 |
| 私钥跨平台加载 | russh-keys crate 解析 OpenSSH 格式，兼容 Ed25519/RSA/ECDSA；passphrase 从 Keyring 取 |
| known_hosts 校验体验 | 首次连接弹窗显示指纹 SHA256，用户确认入库；后续静默校验，不匹配拒绝并高亮告警 |
| SFTP 大文件传输效率 | 分块并发(tokio 任务池) + 断点续传(offset 持久化) + 限流(避免占满带宽) |
| 端口转发的连接泄漏 | 每个转发连接独立 task + 超时 + 主动 cancel；隧道关闭时取消所有子 task |
| Tauri 后端长任务管理 | 用 tokio + tauri::async_runtime，长连接持有在全局 AppState，避免被请求作用域回收 |
| RDP 帧传输 IPC 瓶颈 | 后端解码为 RGBA 后节流推送(60fps)；大帧用 base64/SharedArrayBuffer；未来可升级 WebGL 前端直接渲染 |
| RDP NLA 认证复杂 | ironrdp 已封装 CredSSP/NTLM/Kerberos；凭据从 Keyring 取；证书校验同 known_hosts 逻辑 |
| 远程桌面输入事件坐标映射 | Canvas 监听事件 → 按缩放比例反算 → 打包协议格式发送；键盘事件按 scancode 映射 |
| SSH 与远程桌面会话统一管理 | 统一 sessionId 空间 + SessionRegistry trait object；前端 Tab 按 kind 分发到不同 View 组件 |
| macOS 被控服务权限申请 | 首次启动引导用户授予"屏幕录制"+"辅助功能"权限；检测权限缺失时弹窗指引系统设置 |
| ScreenCaptureKit 与 Rust 集成 | Swift 编写捕获层，通过 FFI 暴露 C 接口给 Rust；或用 objc2 crate 直接调 ObjC API |
| VP9 硬件编码帧率/码率控制 | VideoToolbox 实时调整码率；根据网络反馈动态降帧率/降分辨率 |
| 自研流协议设计 | 帧消息(序号+时间戳+VP9数据) + 输入消息 + 控制消息(画质/重连)；TLS 1.3 加密全链路 |
| macOS 被控服务部署体验 | 提供 .pkg 安装包 + launchd 自启 + 控制端自动发现（可选 mDNS）|
| AI 流式响应 IPC | 后端 reqwest SSE 流逐 token 解析 → Tauri event 推送；前端 markdown-it 增量渲染 |
| 多 LLM Provider 协议差异 | LlmClient trait 屏蔽差异；OpenAI 兼容(含 DeepSeek/智谱)、Claude、Ollama 各自适配器 |
| AI 会话上下文采集 | ContextProvider 通过 SessionRegistry 取 ssh-core 终端 buffer 尾部 N 行；用户勾选才附带 |
| LLM API Key 安全存储 | 存 OS Keyring，永不落盘明文；调用显式用户发起，无后台自动调用 |
| AI 隐私(误传敏感信息) | 上下文采集默认关闭需用户勾选；本地 Ollama 模式数据不出本机 |
| Work 模式接口预留稳定性 | AgentProvider trait 统一抽象，WorkProvider 未来实现不改 trait；function calling 留扩展点 |

---

## 十一、实施计划 (当前步骤)

基于"MVP 注重基础架构"，第一批落地（不含 macOS 被控服务和 AI Work 模式，那在阶段 5）：

1. **初始化 monorepo 结构** — 创建 Cargo workspace + 前端 workspace + 上述目录骨架
2. **搭建 `ssh-core`** — connection/terminal 模块，基于 russh 实现最小可用的 SSH 连接 + PTY 会话
3. **搭建 `remote-desktop-core`** — session/rdp 模块，基于 ironrdp 实现最小可用的 RDP 连接 + 帧接收
4. **搭建 `ai-core`** — provider/chat 模块，实现 LlmClient(OpenAI 兼容) + ChatProvider + 流式响应
5. **搭建 `desktop` (Tauri)** — 基础窗口 + 统一会话注册表 + commands/{session,desktop,ai}.rs
6. **搭建前端** — Vue3 + xterm.js (TerminalView) + Canvas (DesktopView) + ChatPanel + Tauri invoke 封装
7. **打通端到端** — SSH 测试会话 + RDP 测试会话 + AI 基础问答 → 终端交互 + 桌面显示 + AI 对话

注：`host-service` (macOS 被控服务) 和 AI Work 模式在阶段 5 开发，初期用 RDP 覆盖 Win/Linux + Chat 基础问答已足够验证架构。

### 验证方式
- `cargo build` (workspace) 编译通过
- `pnpm dev` (前端) 启动通过
- `cargo tauri dev` 启动桌面应用：
  - 能连接一台测试 SSH 服务器并交互命令
  - 能连接一台测试 RDP 服务器（Win 自带或 xrdp）并看到桌面、操作鼠标键盘
  - 能在 AI Chat 面板提问并收到流式回答（配置一个 LLM Provider 后）
- 单元测试：ssh-core、remote-desktop-core、ai-core 核心模块测试通过
- （阶段 3 验证）AI Chat 勾选"附带当前会话上下文"后，能结合终端输出回答运维问题
- （阶段 5 验证）`host-service` 在测试 Mac 上部署后，控制端能流畅连接并操作（对比 VNC 体验提升）
