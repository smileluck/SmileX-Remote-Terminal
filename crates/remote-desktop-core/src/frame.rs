//! # 帧数据结构
//!
//! 远程桌面画面的最小传输单元。后端解码为 RGBA 后推送给前端 Canvas。

/// 一帧远程桌面画面
#[derive(Debug, Clone)]
pub struct DesktopFrame {
    /// 帧序号（单调递增，用于乱序检测）
    pub seq: u64,
    /// 时间戳（毫秒）
    pub timestamp_ms: u64,
    /// 画面宽度
    pub width: u32,
    /// 画面高度
    pub height: u32,
    /// RGBA 像素数据（长度 = width * height * 4）
    pub rgba: Vec<u8>,
    /// 是否为关键帧（完整画面）vs 增量帧
    pub key_frame: bool,
}

impl DesktopFrame {
    /// 创建占位空帧（用于测试）
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            seq: 0,
            timestamp_ms: 0,
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
            key_frame: true,
        }
    }
}
