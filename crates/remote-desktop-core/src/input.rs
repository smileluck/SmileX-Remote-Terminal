//! # 输入事件
//!
//! 前端 Canvas 收集的鼠标/键盘/滚轮事件，转发给后端发送到被控端。
//! 坐标已按缩放比例反算为被控端原始分辨率坐标。

/// 鼠标按键
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

/// 鼠标事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum MouseAction {
    Down,
    Up,
    Move,
}

/// 修饰键状态
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool, // Win/Command
}

/// 远程桌面输入事件（统一抽象，RDP/Host 各自映射到协议格式）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind")]
pub enum InputEvent {
    /// 鼠标事件
    Mouse {
        button: MouseButton,
        action: MouseAction,
        /// 被控端坐标 X
        x: i32,
        /// 被控端坐标 Y
        y: i32,
        modifiers: Modifiers,
    },
    /// 滚轮事件
    Wheel {
        /// 横向滚动量
        delta_x: f32,
        /// 纵向滚动量
        delta_y: f32,
        x: i32,
        y: i32,
        modifiers: Modifiers,
    },
    /// 键盘按下
    KeyDown {
        /// 键码（前端 KeyboardEvent.code）
        code: String,
        modifiers: Modifiers,
    },
    /// 键盘抬起
    KeyUp {
        code: String,
        modifiers: Modifiers,
    },
    /// 文本输入（用于 IME 合成）
    TextInput {
        text: String,
    },
    /// 剪贴板同步
    Clipboard {
        text: String,
    },
}
