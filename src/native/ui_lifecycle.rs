//! 面向 NSH UI 的只读系统活动与输入形态事实。
// 引入启动配置与生命周期
use super::{bootstrap, lifecycle};
// 引入 nsh 模块宏
use nsh_core::prelude::*;

/// 只读的 NSH 应用活动与输入形态事实。
// 声明 nsh 原生模块
#[module]
// 只读的应用活动与输入形态
pub struct UiLifecycle;

// 模块方法
#[module]
impl UiLifecycle {
    /// 当前平台无法确定前台可见性时返回未知。
    // 前台状态；当前平台无法确定时返回 None
    pub fn foreground(sys: System) -> Option<bool> {
        // 按操作系统取前台状态
        lifecycle::foreground(sys.os.name())
    }

    /// 识别电视输入,而不暴露已退役的系统扩展能力。
    // 输入形态，用于识别电视
    pub fn form_factor() -> String {
        // 取设备形态
        bootstrap::form_factor()
    }
}
