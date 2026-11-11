//! iOS 宿主的连接活动与设备形态。

// iOS 的安装与观察入口
#[cfg(target_os = "ios")]
#[path = "ios/mod.rs"]
mod platform;

#[cfg(target_os = "ios")]
// 重新导出安装与观察接口
pub(super) use platform::{form_factor, install, observe};

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
