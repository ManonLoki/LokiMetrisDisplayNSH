//! 系统应用活动状态,可在 NSH 的 JavaScript 引擎线程上安全读取。
//!
//! AppKit 与 GTK 控件属于平台线程。macOS 使用系统只读
//! 查询;Linux 在其主循环上安排 GTK 读取。iOS 在运行器启动之前,
//! 于主线程上观察 UIKit。缺少 GUI 会话或
//! 查询没有结论时视为未知,而不是凭空制造生命周期事件。

/// `os` 是 NSH 上报的平台;与编译目标不同的模拟平台
/// 没有系统事实,保持为未知。
// os 是 NSH 报告的平台；与编译目标不同的模拟平台没有系统事实，保持未知
pub fn foreground(os: &str) -> Option<bool> {
    // 不是当前操作系统
    if os != std::env::consts::OS {
        // 返回 None
        return None;
    }
    // Android：由启动模块提供
    #[cfg(target_os = "android")]
    return super::bootstrap::foreground();
    // 其余桌面与 iOS 平台：由各自实现提供
    #[cfg(any(
        target_os = "ios",
        // macOS
        target_os = "macos",
        // Windows
        target_os = "windows",
        // Linux
        target_os = "linux"
    ))]
    // 返回平台前台状态
    return platform::foreground();
    // 未知平台
    #[cfg(not(any(
        target_os = "android",
        // iOS
        target_os = "ios",
        // macOS
        target_os = "macos",
        // Windows
        target_os = "windows",
        // Linux
        target_os = "linux"
    )))]
    // 其余平台没有结果
    None
}

// 各平台实现
#[cfg(target_os = "macos")]
#[path = "macos.rs"]
mod platform;

#[cfg(target_os = "windows")]
#[path = "windows.rs"]
// 平台实现
mod platform;

#[cfg(target_os = "linux")]
#[path = "linux.rs"]
// 平台实现
mod platform;

// 测试模块
#[cfg(test)]
#[path = "lifecycle_test.rs"]
mod lifecycle_test;

// iOS 的安装与观察入口
#[cfg(target_os = "ios")]
#[path = "ios/mod.rs"]
mod platform;

#[cfg(target_os = "ios")]
// 重新导出安装与观察接口
pub(super) use platform::{install, observe};

#[cfg(target_os = "ios")]
// 活动观察者
pub(super) use ios_observers::ActivityObserver;

// iOS 观察者实现，测试也编译
#[cfg(any(target_os = "ios", test))]
#[path = "ios/observers.rs"]
mod ios_observers;

#[cfg(any(target_os = "ios", test))]
// iOS 活动状态机
#[path = "ios/state.rs"]
mod ios_state;
